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
    let template = resolve_template(
        &db,
        request.template_id.as_deref(),
        request.body,
        TemplateKind::Live,
    )
    .await?;

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
    let template = resolve_template(
        &db,
        request.template_id.as_deref(),
        request.body,
        TemplateKind::Review,
    )
    .await?;

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
///
/// `kind` 只在 `body` 分支用到：命令层已经知道自己在生成实盘还是复盘，
/// 用它给「未保存的模板」一个正确的类型，否则复盘预览会因类型校验被误拒。
async fn resolve_template(
    db: &Db,
    template_id: Option<&str>,
    body: Option<String>,
    kind: TemplateKind,
) -> AppResult<PromptTemplate> {
    if let Some(body) = body {
        return Ok(PromptTemplate {
            id: String::new(),
            name: "（未保存的模板）".to_string(),
            description: String::new(),
            kind,
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

#[cfg(test)]
mod tests {
    //! 这一组测试的直接动因是一个**真实漏过的 bug**：
    //!
    //! `resolve_template` 的 body 分支曾硬编码 `kind = Live`，而
    //! `prompt_build_review` 拿到模板后会校验类型——于是「复盘模板 + 编辑器实时预览」
    //! 必然报「这是实盘模板」。后端自测没覆盖，因为它只测了 `template_id` 那条路径，
    //! 而前端**编辑后总是传 body**。
    //!
    //! 教训：预览路径（body）和已保存路径（template_id）是两条路，两条都要测。

    use super::*;

    async fn memory_db() -> Db {
        Db::open_in_memory().await.expect("内存库创建失败")
    }

    /// 预览未保存的正文时，类型必须来自**调用方**，否则命令层的类型校验会误拒。
    #[tokio::test]
    async fn preview_body_takes_the_callers_kind() {
        let db = memory_db().await;

        let review = resolve_template(
            &db,
            None,
            Some("# 复盘模板".to_string()),
            TemplateKind::Review,
        )
        .await
        .expect("应能解析");
        assert_eq!(
            review.kind,
            TemplateKind::Review,
            "复盘预览必须得到 Review 类型，否则会被 prompt_build_review 的类型校验误拒"
        );

        let live = resolve_template(
            &db,
            None,
            Some("# 实盘模板".to_string()),
            TemplateKind::Live,
        )
        .await
        .expect("应能解析");
        assert_eq!(live.kind, TemplateKind::Live);
    }

    /// 已保存的模板（内置或用户）类型来自自身，**不该被调用方的兜底值覆盖**。
    /// 否则一个实盘模板会被当成复盘模板渲染，用户看到的是莫名其妙的错误。
    #[tokio::test]
    async fn saved_templates_keep_their_own_kind() {
        let db = memory_db().await;

        let builtin = resolve_template(
            &db,
            Some("review_performance"),
            None,
            TemplateKind::Live, // 故意传错：不该生效
        )
        .await
        .expect("内置模板应存在");
        assert_eq!(
            builtin.kind,
            TemplateKind::Review,
            "内置模板的类型应来自自身，而不是调用方传的兜底值"
        );
    }

    #[tokio::test]
    async fn missing_template_is_an_error_with_the_id() {
        let db = memory_db().await;
        let err = resolve_template(&db, Some("nope"), None, TemplateKind::Live)
            .await
            .expect_err("不存在的模板应当报错");
        assert!(err.to_string().contains("nope"), "错误里应带上 id：{err}");
    }

    /// 用户模板的数据库往返：写入 → 读回 → 列出 → 删除。
    #[tokio::test]
    async fn user_templates_round_trip_through_the_database() {
        let db = memory_db().await;

        let row = crate::storage::PromptTemplateRow {
            id: "user_test_1".to_string(),
            name: "我的模板".to_string(),
            description: Some("说明".to_string()),
            scope: "review".to_string(),
            body: "{{ period.bar }}".to_string(),
            updated_at: 1_700_000_000_000,
        };
        db.upsert_prompt_template(&row).await.expect("写入失败");

        let loaded = resolve_template(&db, Some("user_test_1"), None, TemplateKind::Live)
            .await
            .expect("应能读回");
        assert_eq!(loaded.name, "我的模板");
        assert_eq!(loaded.kind, TemplateKind::Review);
        assert!(!loaded.builtin, "用户模板不该被标成内置");

        let listed = db.list_prompt_templates().await.expect("列出失败");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "user_test_1");

        // 覆盖保存：created_at 不该跟着 updated_at 一起往后跑
        let updated = crate::storage::PromptTemplateRow {
            name: "改过的名字".to_string(),
            updated_at: 1_700_000_999_000,
            ..row.clone()
        };
        db.upsert_prompt_template(&updated).await.expect("覆盖失败");
        let reloaded = db
            .get_prompt_template("user_test_1")
            .await
            .expect("读取失败")
            .expect("应当存在");
        assert_eq!(reloaded.name, "改过的名字");
        assert_eq!(reloaded.updated_at, 1_700_000_999_000);

        assert!(
            db.delete_prompt_template("user_test_1")
                .await
                .expect("删除失败")
        );
        assert!(
            !db.delete_prompt_template("user_test_1")
                .await
                .expect("重复删除不应报错"),
            "第二次删除应返回 false 而不是报错"
        );
    }
}
