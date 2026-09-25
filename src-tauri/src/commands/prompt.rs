//! 提示词命令（设计文档 §6.8）。

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::fetch::cache::LiveCache;
use crate::market::live;
use crate::okx::client::OkxClient;
use crate::position::current;
use crate::prompt::privacy::PrivacyLevel;
use crate::prompt::templates::{self, PromptTemplate, TemplateKind};
use crate::prompt::{context, render, tokens};
use crate::settings;
use crate::storage::Db;
use crate::vault::Vault;

/// 一次提示词生成的结果。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct PromptOutput {
    pub text: String,
    /// token 估算（保守高估，见 `prompt::tokens`）
    pub token_estimate: usize,
    pub char_count: usize,
    pub privacy: PrivacyLevel,
    /// 使用的模板（预览未保存的正文时为 `None`）
    pub template_id: Option<String>,
    pub template_name: String,
    /// 装配阶段的问题（数据缺失、L1 退化等）。**必须展示给用户**，
    /// 否则他会以为提示词里包含了全部信息。
    pub warnings: Vec<String>,
    #[ts(type = "number")]
    pub generated_at: i64,
}

/// 列出全部模板：内置在前，用户模板在后。
#[tauri::command]
pub async fn template_list(db: State<'_, Db>) -> AppResult<Vec<PromptTemplate>> {
    let mut list = templates::builtins();

    for row in db.list_prompt_templates().await? {
        list.push(row_to_template(row)?);
    }

    Ok(list)
}

#[derive(Debug, Deserialize)]
pub struct SaveTemplateRequest {
    /// 给了 id 且不是内置模板 → 覆盖；否则新建
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub kind: TemplateKind,
    pub body: String,
}

/// 保存模板。
///
/// **内置模板不可覆盖**：传内置 id 会被当作「另存为」，生成一份用户副本。
/// 否则用户一改内置模板，升级后就分不清「这是我的改动还是新版内置」了。
#[tauri::command(rename_all = "snake_case")]
pub async fn template_save(
    db: State<'_, Db>,
    request: SaveTemplateRequest,
) -> AppResult<PromptTemplate> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(AppError::Config("模板名称不能为空".to_string()));
    }
    if request.body.trim().is_empty() {
        return Err(AppError::Config("模板正文不能为空".to_string()));
    }

    // 保存前先渲染一次：语法错误或引用了不存在的变量，此时就该拦住，
    // 而不是等用户点了「生成提示词」才发现。
    if let Err(err) = render::render(&request.body, &serde_json::json!({})) {
        // 空上下文必然缺变量——只把**语法错误**当致命问题，变量缺失留给实际生成时报。
        let message = err.to_string();
        if message.contains("模板语法错误") {
            return Err(err);
        }
    }

    let now = crate::storage::now_ms();
    let id = match request.id.as_deref() {
        Some(id) if !id.is_empty() && templates::builtin(id).is_none() => id.to_string(),
        _ => format!("user_{now}"),
    };
    let description = request.description.unwrap_or_default();
    let kind = request.kind.as_str();

    db.upsert_prompt_template(&crate::storage::PromptTemplateRow {
        id: id.clone(),
        name: name.to_string(),
        description: Some(description.clone()),
        scope: kind.to_string(),
        body: request.body.clone(),
        updated_at: now,
    })
    .await?;

    tracing::info!(template_id = %id, "模板已保存");

    Ok(PromptTemplate {
        id,
        name: name.to_string(),
        description,
        kind: request.kind,
        body: request.body,
        builtin: false,
        updated_at: now,
    })
}

/// 删除用户模板。内置模板不可删除（会明确报错，而不是静默忽略）。
#[tauri::command]
pub async fn template_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    if templates::builtin(&id).is_some() {
        return Err(AppError::Config(
            "内置模板不可删除，请用「另存为」创建副本".to_string(),
        ));
    }

    if !db.delete_prompt_template(&id).await? {
        return Err(AppError::Config(format!("模板不存在：{id}")));
    }
    Ok(())
}

// ------------------------------------------------------------------ 生成提示词

#[derive(Debug, Deserialize)]
pub struct BuildLiveRequest {
    /// 模板 id；给了 `body` 时可省略（预览未保存的正文）
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub privacy: Option<PrivacyLevel>,
    #[serde(default)]
    pub credential_id: Option<String>,
    /// 跳过 30 秒缓存
    #[serde(default)]
    pub force: Option<bool>,
}

/// 生成实盘提示词。
#[tauri::command(rename_all = "snake_case")]
pub async fn prompt_build_live(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    cache: State<'_, LiveCache>,
    vault: State<'_, Vault>,
    request: BuildLiveRequest,
) -> AppResult<PromptOutput> {
    let level = request.privacy.unwrap_or_default();
    let template = resolve_template(&db, request.template_id.as_deref(), request.body).await?;

    if template.kind != TemplateKind::Live {
        return Err(AppError::Config(format!(
            "模板「{}」是复盘模板，不能用于实盘",
            template.name
        )));
    }

    // 市场状态：命中 30 秒缓存就不重复联网
    let snapshot = match cache
        .get_fresh()
        .filter(|_| !request.force.unwrap_or(false))
    {
        Some(snapshot) => snapshot,
        None => {
            let watchlist = settings::read_watchlist(&db).await?;
            let (instruments, warnings) = live::fetch_snapshot(&client, &watchlist).await?;
            let now = crate::storage::now_ms();
            let snapshot = live::LiveSnapshot {
                ts: now,
                cache_hit: false,
                fetched_at: now,
                watchlist,
                instruments,
                warnings,
            };
            cache.store(&snapshot);
            snapshot
        }
    };

    // 账户与仓位：拿不到不是错误，而是上下文里的一条显式声明
    let (account, positions, reason) = match crate::credentials::credentials_for_optional(
        &db,
        &vault,
        request.credential_id.as_deref(),
    )
    .await?
    {
        Some(credentials) => match current::fetch(&client, &db, &credentials).await {
            Ok(snapshot) => (Some(snapshot.overview), Some(snapshot.positions), None),
            Err(err) => {
                tracing::warn!(error = %err, "账户数据拉取失败，按不可得处理");
                (None, None, Some(err.to_string()))
            }
        },
        None => (None, None, Some("尚未配置凭据".to_string())),
    };

    let assembled = context::live(
        &snapshot,
        account.as_ref(),
        positions.as_deref(),
        level,
        reason.as_deref(),
    );

    finish(&template, &assembled.context, assembled.warnings, level)
}

#[derive(Debug, Deserialize)]
pub struct BuildReviewRequest {
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub privacy: Option<PrivacyLevel>,
    #[serde(default)]
    pub credential_id: Option<String>,
    pub from: i64,
    pub to: i64,
    #[serde(default)]
    pub bar: Option<String>,
}

/// 生成复盘提示词。
///
/// 会先同步官方历史仓位（与「装配复盘」走同一条路径），所以首次调用可能慢几秒。
#[tauri::command(rename_all = "snake_case")]
pub async fn prompt_build_review(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    vault: State<'_, Vault>,
    request: BuildReviewRequest,
) -> AppResult<PromptOutput> {
    if request.to <= request.from {
        return Err(AppError::Config("结束时间必须晚于开始时间".to_string()));
    }
    if request.to - request.from > crate::fetch::plan::MAX_RANGE_MS {
        return Err(AppError::RangeTooLarge);
    }

    let level = request.privacy.unwrap_or_default();
    let template = resolve_template(&db, request.template_id.as_deref(), request.body).await?;

    if template.kind != TemplateKind::Review {
        return Err(AppError::Config(format!(
            "模板「{}」是实盘模板，不能用于复盘",
            template.name
        )));
    }

    let bar = request.bar.unwrap_or_else(|| "1H".to_string());
    let credentials =
        crate::credentials::credentials_for_optional(&db, &vault, request.credential_id.as_deref())
            .await?
            .ok_or_else(|| AppError::Config("复盘需要读取历史仓位，必须先配置凭据".to_string()))?;

    let review = crate::review::build_context(
        &client,
        &db,
        Some(&credentials),
        request.from,
        request.to,
        &bar,
    )
    .await?;

    let assembled = context::review(&review, level);

    finish(&template, &assembled.context, assembled.warnings, level)
}

/// 解析要用的模板：显式 `body`（编辑器预览）优先，否则按 id 查库、再查内置。
async fn resolve_template(
    db: &Db,
    template_id: Option<&str>,
    body: Option<String>,
) -> AppResult<PromptTemplate> {
    if let Some(body) = body {
        return Ok(PromptTemplate {
            id: String::new(),
            name: "（未保存的模板）".to_string(),
            description: String::new(),
            // 预览时类型由调用方的命令决定，这里给一个占位；
            // 命令层已在上游校验过类型，不会用到这个值。
            kind: TemplateKind::Live,
            body,
            builtin: false,
            updated_at: 0,
        });
    }

    let id =
        template_id.ok_or_else(|| AppError::Config("必须提供 template_id 或 body".to_string()))?;

    if let Some(builtin) = templates::builtin(id) {
        return Ok(builtin);
    }

    let row = db
        .get_prompt_template(id)
        .await?
        .ok_or_else(|| AppError::Config(format!("模板不存在：{id}")))?;

    row_to_template(row)
}

fn row_to_template(row: crate::storage::PromptTemplateRow) -> AppResult<PromptTemplate> {
    let kind = TemplateKind::parse(&row.scope)
        .ok_or_else(|| AppError::Config(format!("模板类型无法识别：{}", row.scope)))?;
    Ok(PromptTemplate {
        id: row.id,
        name: row.name,
        description: row.description.unwrap_or_default(),
        kind,
        body: row.body,
        builtin: false,
        updated_at: row.updated_at,
    })
}

fn finish(
    template: &PromptTemplate,
    context: &serde_json::Value,
    warnings: Vec<String>,
    level: PrivacyLevel,
) -> AppResult<PromptOutput> {
    let text = render::render(&template.body, context)?;

    Ok(PromptOutput {
        token_estimate: tokens::estimate(&text),
        char_count: tokens::char_count(&text),
        text,
        privacy: level,
        template_id: if template.id.is_empty() {
            None
        } else {
            Some(template.id.clone())
        },
        template_name: template.name.clone(),
        warnings,
        generated_at: crate::storage::now_ms(),
    })
}
