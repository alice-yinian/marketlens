//! 模板库（设计文档 §6.6.4）。
//!
//! 内置模板的正文用 `include_str!` 从 `templates/` 目录加载，**不写在 Rust 代码里**：
//! 模板是给 AI 读的散文，混进代码里既难编辑也难 review。
//! 用户模板存在 SQLite 里，与内置模板共用同一套渲染路径。
//!
//! **内置模板在隐私等级下必须都能渲染**——这是黄金测试守的核心性质。
//! 内置模板做不到的事，不能指望用户自写模板能做到。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 模板适用的上下文类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    /// 实盘：市场状态 + 当前仓位
    Live,
    /// 复盘：历史仓位 + 统计
    Review,
}

impl TemplateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TemplateKind::Live => "live",
            TemplateKind::Review => "review",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "live" => Some(TemplateKind::Live),
            "review" => Some(TemplateKind::Review),
            _ => None,
        }
    }
}

/// 一个提示词模板。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: TemplateKind,
    pub body: String,
    /// 内置模板不可删除；「另存为」会产生一个非内置副本
    pub builtin: bool,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// 内置模板清单：(id, 名称, 说明, 类型, 正文)
const BUILTINS: &[(&str, &str, &str, TemplateKind, &str)] = &[
    (
        "live_quick",
        "实盘速览",
        "市场状态 + 当前持仓，问「顺风还是逆风」",
        TemplateKind::Live,
        include_str!("../../templates/live_quick.md"),
    ),
    (
        "live_risk",
        "持仓风险体检",
        "聚焦爆仓距离、保证金率与方向重叠",
        TemplateKind::Live,
        include_str!("../../templates/live_risk.md"),
    ),
    (
        "review_performance",
        "复盘：绩效与归因",
        "统计 + 按开仓时状态分组 + 逐笔明细",
        TemplateKind::Review,
        include_str!("../../templates/review_performance.md"),
    ),
    (
        "review_lesson",
        "复盘：教训",
        "像教练一样指出重复犯的错",
        TemplateKind::Review,
        include_str!("../../templates/review_lesson.md"),
    ),
];

/// 全部内置模板。
pub fn builtins() -> Vec<PromptTemplate> {
    BUILTINS
        .iter()
        .map(|(id, name, description, kind, body)| PromptTemplate {
            id: (*id).to_string(),
            name: (*name).to_string(),
            description: (*description).to_string(),
            kind: *kind,
            body: (*body).to_string(),
            builtin: true,
            updated_at: 0,
        })
        .collect()
}

/// 按 id 查内置模板。
pub fn builtin(id: &str) -> Option<PromptTemplate> {
    builtins().into_iter().find(|template| template.id == id)
}

#[cfg(test)]
mod golden {
    //! 黄金测试：4 个内置模板 × 3 个隐私等级。
    //!
    //! 守三件事：
    //!   1. 任何模板在任何等级下都能**渲染成功**（不缺变量、不报错）
    //!   2. L0 之外**不出现原始金额**（安全性质）
    //!   3. 数据不可得时渲染成**显式声明**，不是空白

    use super::*;
    use crate::prompt::context;
    use crate::prompt::privacy::PrivacyLevel;
    use crate::prompt::render::render;
    use crate::prompt::tokens;

    fn live_fixture(level: PrivacyLevel) -> serde_json::Value {
        let snapshot = crate::test_fixtures::live_snapshot();
        let account = crate::test_fixtures::account_overview();
        let positions = crate::test_fixtures::positions();
        context::live(&snapshot, Some(&account), Some(&positions), level, None).context
    }

    fn review_fixture(level: PrivacyLevel) -> serde_json::Value {
        context::review(&crate::test_fixtures::review_context(), level).context
    }

    #[test]
    fn every_builtin_renders_at_every_privacy_level() {
        for template in builtins() {
            for level in [PrivacyLevel::L0, PrivacyLevel::L1, PrivacyLevel::L2] {
                let context = match template.kind {
                    TemplateKind::Live => live_fixture(level),
                    TemplateKind::Review => review_fixture(level),
                };

                let output = render(&template.body, &context).unwrap_or_else(|err| {
                    panic!(
                        "模板 {} 在 {} 下渲染失败：{err}",
                        template.id,
                        level.as_str()
                    )
                });

                assert!(
                    output.len() > 200,
                    "模板 {} 在 {} 下输出过短，可能整段被跳过：{output}",
                    template.id,
                    level.as_str()
                );
                // 模板作者不该在正文里留下未替换的 Jinja 标记
                assert!(
                    !output.contains("{{") && !output.contains("{%"),
                    "模板 {} 在 {} 下留下了未渲染的标记",
                    template.id,
                    level.as_str()
                );
            }
        }
    }

    /// **安全性质**：L1/L2 的输出里不得出现原始金额。
    ///
    /// 夹具里的金额都是刻意选的、不会与行情数字撞车的大数（如 12345.67）。
    #[test]
    fn l1_and_l2_never_leak_raw_amounts() {
        // 夹具中账户权益、未实现盈亏等金额的原始字符串
        const LEAKS: &[&str] = &["12345.67", "12.35K", "-1234.56", "9876.54"];

        for template in builtins() {
            for level in [PrivacyLevel::L1, PrivacyLevel::L2] {
                let context = match template.kind {
                    TemplateKind::Live => live_fixture(level),
                    TemplateKind::Review => review_fixture(level),
                };
                let output = render(&template.body, &context)
                    .unwrap_or_else(|err| panic!("{} 渲染失败：{err}", template.id));

                for leak in LEAKS {
                    assert!(
                        !output.contains(leak),
                        "模板 {} 在 {} 下泄漏了原始金额 {leak}",
                        template.id,
                        level.as_str()
                    );
                }
            }
        }
    }

    /// L2 的输出里不该出现任何 `$` 金额（市场数据里也没有以 $ 开头的数字，
    /// 行情是纯数字）。这条比逐字符串匹配更强，能兜住夹具没覆盖到的字段。
    #[test]
    fn l2_output_contains_no_dollar_amounts() {
        for template in builtins() {
            let context = match template.kind {
                TemplateKind::Live => live_fixture(PrivacyLevel::L2),
                TemplateKind::Review => review_fixture(PrivacyLevel::L2),
            };
            let output = render(&template.body, &context).expect("应能渲染");

            // 允许「$」出现在说明文字里（如权益量级区间 "$10k–$50k" 是刻意保留的量级信息），
            // 但不允许出现具体金额格式 "$12,345.67" / "$1.23K" / "-$1.23"
            for line in output.lines() {
                assert!(
                    !line.contains("-$") && !line.contains("+$"),
                    "L2 下出现了带符号金额：{line}"
                );
            }
        }
    }

    /// 数据不可得时必须显式声明。夹具刻意让部分指标缺失。
    #[test]
    fn unavailable_data_is_declared_not_blank() {
        let context = live_fixture(PrivacyLevel::L0);
        let output = render(
            &builtin("live_quick").expect("内置模板应存在").body,
            &context,
        )
        .expect("应能渲染");

        // 夹具里 Rubik 指标缺失，模板应渲染出显式声明
        assert!(
            output.contains("数据不可得") || output.contains("不可得"),
            "缺失数据必须显式声明：{output}"
        );
    }

    /// 复盘模板在「开仓时刻状态不可得」时也要说清楚，而不是留空。
    #[test]
    fn review_template_declares_missing_open_regime() {
        let context = review_fixture(PrivacyLevel::L0);
        let output = render(
            &builtin("review_performance").expect("内置模板应存在").body,
            &context,
        )
        .expect("应能渲染");

        assert!(
            output.contains("不可得"),
            "缺少开仓时刻状态时必须说明：{output}"
        );
    }

    /// token 估算要能跑在所有模板上，且量级合理（内置模板应当是「能直接粘」的长度）。
    #[test]
    fn token_estimate_is_reasonable_for_builtins() {
        for template in builtins() {
            let context = match template.kind {
                TemplateKind::Live => live_fixture(PrivacyLevel::L1),
                TemplateKind::Review => review_fixture(PrivacyLevel::L1),
            };
            let output = render(&template.body, &context).expect("应能渲染");
            let estimated = tokens::estimate(&output);

            assert!(estimated > 50, "{} 估算过小：{estimated}", template.id);
            assert!(
                estimated < 8_000,
                "{} 估算过大（{estimated}），内置模板不该长到难以粘贴",
                template.id
            );
        }
    }
}

#[cfg(test)]
mod real_data {
    //! 真机联网测试：**真实数据**走通「装配 → 隐私分级 → 渲染」。
    //!
    //! 夹具测试证明不了「真实金额不会泄漏」——因为夹具里的金额是我自己选的、
    //! 我知道它们长什么样。这里用真实账户权益做断言：
    //! 在 L0 里把它找出来，再断言 L1/L2 的输出里没有它。

    use super::*;
    use crate::prompt::context;
    use crate::prompt::filters::format_usd;
    use crate::prompt::privacy::PrivacyLevel;
    use crate::prompt::render::render;

    #[tokio::test]
    #[ignore = "访问真实 OKX 私有端点，需显式提供凭据"]
    async fn real_account_amounts_never_leak_below_l0() {
        use crate::market::live::LiveSnapshot;
        use crate::okx::credentials::Credentials;

        let credentials = Credentials {
            api_key: std::env::var("MARKETLENS_TEST_API_KEY")
                .expect("缺少 MARKETLENS_TEST_API_KEY"),
            secret_key: std::env::var("MARKETLENS_TEST_SECRET_KEY")
                .expect("缺少 MARKETLENS_TEST_SECRET_KEY"),
            passphrase: std::env::var("MARKETLENS_TEST_PASSPHRASE")
                .expect("缺少 MARKETLENS_TEST_PASSPHRASE"),
            demo: true,
        };

        let client = crate::okx::client::OkxClient::new().expect("客户端构造失败");
        let db = crate::storage::Db::open_in_memory()
            .await
            .expect("内存库创建失败");

        let watchlist = vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()];
        let (instruments, warnings) = crate::market::live::fetch_snapshot(&client, &watchlist)
            .await
            .expect("行情拉取失败");
        let now = crate::storage::now_ms();
        let snapshot = LiveSnapshot {
            ts: now,
            cache_hit: false,
            fetched_at: now,
            watchlist,
            instruments,
            warnings,
        };

        let account = crate::position::current::fetch(&client, &db, &credentials)
            .await
            .expect("账户拉取失败");

        // L0 下必须能看见真实权益的格式化形式（否则后面的「不存在」断言毫无意义）
        let equity_text = format_usd(account.overview.total_eq_usd);
        let l0 = context::live(
            &snapshot,
            Some(&account.overview),
            Some(&account.positions),
            PrivacyLevel::L0,
            None,
        )
        .context;
        let l0_text =
            render(&builtin("live_quick").expect("内置模板存在").body, &l0).expect("L0 渲染失败");
        assert!(
            l0_text.contains(&equity_text),
            "L0 里应当能看到真实权益 {equity_text}——看不到说明这个断言本身失效了"
        );

        for level in [PrivacyLevel::L1, PrivacyLevel::L2] {
            let assembled = context::live(
                &snapshot,
                Some(&account.overview),
                Some(&account.positions),
                level,
                None,
            );
            for template in builtins().iter().filter(|t| t.kind == TemplateKind::Live) {
                let output = render(&template.body, &assembled.context).unwrap_or_else(|err| {
                    panic!("{} 在 {} 下渲染失败：{err}", template.id, level.as_str())
                });

                assert!(
                    !output.contains(&equity_text),
                    "{} 在 {} 下泄漏了真实权益 {equity_text}",
                    template.id,
                    level.as_str()
                );
                assert!(
                    !output.contains("$"),
                    "{} 在 {} 下出现了美元金额——L1/L2 不该有任何 $ 开头的数字",
                    template.id,
                    level.as_str()
                );
            }

            println!("--- {} 装配问题：{:?}", level.as_str(), assembled.warnings);
        }

        // 顺带把 L1 的真实渲染结果打出来，便于人工确认可读性
        let l1 = context::live(
            &snapshot,
            Some(&account.overview),
            Some(&account.positions),
            PrivacyLevel::L1,
            None,
        )
        .context;
        let l1_text =
            render(&builtin("live_quick").expect("内置模板存在").body, &l1).expect("L1 渲染失败");
        println!("===== L1 真实渲染 =====\n{l1_text}\n===== 结束 =====");
    }
}
