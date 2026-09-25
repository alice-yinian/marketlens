//! 当前仓位与权益（设计文档 §6.4.1）。

use std::collections::HashMap;

use crate::error::{AppError, AppResult};
use crate::okx::client::OkxClient;
use crate::okx::credentials::Credentials;
use crate::okx::endpoints::{RateGroup, inst_type, private, public};
use crate::okx::models::{AccountBalance, AccountConfig, Instrument, OkxPosition};
use crate::position::trace::{self, TraceCoverage};
use crate::position::{AccountOverview, AccountSnapshot, CurrencyBalance, Position};
use crate::storage::Db;

/// 拉取账户概览与全部持仓，并**顺带**写本地留痕（零额外网络请求）。
pub async fn fetch(
    client: &OkxClient,
    db: &Db,
    credentials: &Credentials,
) -> AppResult<AccountSnapshot> {
    let mut warnings = Vec::new();

    // 账户配置是权限与持仓模式的来源。它失败通常意味着凭据本身有问题（过期、口令错、
    // 环境不匹配），因此直接上报而不是降级——否则用户会看到一堆「权益获取失败」，
    // 却不知道根因是密钥。
    let config = client
        .get_private::<AccountConfig>(
            credentials,
            private::ACCOUNT_CONFIG,
            RateGroup::Account,
            &[],
        )
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Okx {
            code: "empty".to_string(),
            msg: "account/config 返回了空数据".to_string(),
        })?;

    // 余额、持仓、合约信息互不依赖，并发拉取。
    let (balance, positions, instruments) = tokio::join!(
        client.get_private::<AccountBalance>(
            credentials,
            private::ACCOUNT_BALANCE,
            RateGroup::Account,
            &[]
        ),
        client.get_private::<OkxPosition>(
            credentials,
            private::ACCOUNT_POSITIONS,
            RateGroup::Account,
            &[("instType", inst_type::SWAP)]
        ),
        client.get_public::<Instrument>(
            public::INSTRUMENTS,
            RateGroup::Reference,
            &[("instType", inst_type::SWAP)]
        ),
    );

    let balance = match balance {
        Ok(rows) => rows.into_iter().next(),
        Err(err) => {
            warnings.push(format!("账户权益获取失败：{err}"));
            None
        }
    };

    let instruments: Vec<Instrument> = match instruments {
        Ok(rows) => rows,
        Err(err) => {
            // 缺了合约信息只是算不出币数量，不该让整个刷新失败
            warnings.push(format!("合约信息获取失败，币数量将不可得：{err}"));
            Vec::new()
        }
    };
    let by_inst: HashMap<&str, &Instrument> = instruments
        .iter()
        .map(|instrument| (instrument.inst_id.as_str(), instrument))
        .collect();

    let raw_positions = match positions {
        Ok(rows) => rows,
        Err(err) => {
            warnings.push(format!("持仓获取失败：{err}"));
            Vec::new()
        }
    };

    let normalized: Vec<Position> = raw_positions
        .iter()
        .map(|raw| normalize(raw, by_inst.get(raw.inst_id.as_str()).copied()))
        .collect();

    let overview = build_overview(&config, balance.as_ref());

    let now = crate::storage::now_ms();
    let coverage = match trace::record(db, &normalized, now).await {
        Ok(coverage) => coverage,
        Err(err) => {
            // 留痕失败不影响本次展示，但必须让用户知道历史数据没记上
            warnings.push(format!("本地留痕写入失败：{err}"));
            TraceCoverage {
                records: 0,
                has_gaps: false,
                max_gap_ms: 0,
                last_trace_at: None,
                note: "本次留痕未写入。".to_string(),
            }
        }
    };

    Ok(AccountSnapshot {
        overview,
        positions: normalized,
        trace: coverage,
        warnings,
    })
}

fn build_overview(config: &AccountConfig, balance: Option<&AccountBalance>) -> AccountOverview {
    let currencies = balance
        .map(|balance| {
            balance
                .details
                .iter()
                .map(|detail| CurrencyBalance {
                    ccy: detail.ccy.clone(),
                    eq: detail.eq,
                    eq_usd: detail.eq_usd,
                    avail_bal: detail.avail_bal,
                    cash_bal: detail.cash_bal,
                })
                .collect()
        })
        .unwrap_or_default();

    AccountOverview {
        total_eq_usd: balance.map_or(0.0, |b| b.total_eq),
        iso_eq_usd: balance.map_or(0.0, |b| b.iso_eq),
        adj_eq_usd: balance.map_or(0.0, |b| b.adj_eq),
        avail_eq_usd: balance.map_or(0.0, |b| b.avail_eq),
        upl: balance.map_or(0.0, |b| b.upl),
        mgn_ratio: balance.and_then(|b| b.mgn_ratio),
        imr: balance.and_then(|b| b.imr),
        mmr: balance.and_then(|b| b.mmr),
        notional_usd: balance.and_then(|b| b.notional_usd),
        pos_mode: config.pos_mode.clone(),
        currencies,
        fetched_at: crate::storage::now_ms(),
    }
}

fn normalize(raw: &OkxPosition, instrument: Option<&Instrument>) -> Position {
    Position {
        inst_id: raw.inst_id.clone(),
        pos_id: raw.pos_id.clone(),
        pos_side: raw.pos_side.clone(),
        mgn_mode: raw.mgn_mode.clone(),
        lever: raw.lever,
        contracts: raw.pos,
        size_base: size_base(raw.pos, instrument),
        avg_px: raw.avg_px,
        mark_px: raw.mark_px,
        liq_px: raw.liq_px,
        upl: raw.upl,
        upl_ratio: raw.upl_ratio,
        mgn_ratio: raw.mgn_ratio,
        notional_usd: raw.notional_usd,
        imr: raw.imr,
        mmr: raw.mmr,
        fee: raw.fee,
        funding_fee: raw.funding_fee,
        realized_pnl: raw.realized_pnl,
        created_at: raw.c_time,
        updated_at: raw.u_time,
    }
}

/// 张数 → 币数量。
///
/// 必须用合约元数据换算，不能假设「1 张 = 1 币」或「1 张 = 计价币数量」：
/// 实测 BTC-USDT-SWAP 的 `ctVal` 是 0.01 BTC，而 SOL-USDT-SWAP 是 1 SOL。
/// 币本位合约的 `ctValCcy` 还可能是非计价币。**换算错了会直接给用户
/// （以及后续喂给 AI 的提示词）一个数量级级别的错数字。**
///
/// 拿不到合约信息时返回 `None`——宁可显式「不可得」，也不要猜一个数出来。
fn size_base(contracts: f64, instrument: Option<&Instrument>) -> Option<f64> {
    let instrument = instrument?;
    Some(contracts * instrument.ct_val * instrument.ct_mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instrument(inst_id: &str, ct_val: f64, ct_mult: f64) -> Instrument {
        Instrument {
            inst_id: inst_id.to_string(),
            ct_val,
            ct_val_ccy: "X".to_string(),
            ct_mult,
            settle_ccy: "USDT".to_string(),
            state: "live".to_string(),
        }
    }

    #[test]
    fn converts_contracts_using_real_contract_values() {
        // 真机值：BTC-USDT-SWAP ctVal=0.01；SOL-USDT-SWAP ctVal=1
        let btc = instrument("BTC-USDT-SWAP", 0.01, 1.0);
        let sol = instrument("SOL-USDT-SWAP", 1.0, 1.0);

        assert!((size_base(5.0, Some(&btc)).unwrap() - 0.05).abs() < 1e-12);
        assert!((size_base(5.0, Some(&sol)).unwrap() - 5.0).abs() < 1e-12);
    }

    /// 真机交叉验证：SOL 持仓 5 张、markPx 117.44、notionalUsd 587.100176。
    /// 换算出的币数量乘以标记价应当与 OKX 给出的名义价值吻合。
    #[test]
    fn conversion_agrees_with_okx_notional_value() {
        let sol = instrument("SOL-USDT-SWAP", 1.0, 1.0);
        let size = size_base(5.0, Some(&sol)).expect("应可换算");
        let computed_notional = size * 117.44;

        assert!(
            (computed_notional - 587.100176).abs() < 0.5,
            "换算出的名义价值 {computed_notional} 应与 OKX 的 587.100176 接近"
        );
    }

    #[test]
    fn multiplier_is_applied() {
        let multiplied = instrument("X-USDT-SWAP", 0.01, 10.0);
        assert!((size_base(2.0, Some(&multiplied)).unwrap() - 0.2).abs() < 1e-12);
    }

    #[test]
    fn missing_instrument_yields_none_instead_of_a_guess() {
        assert_eq!(
            size_base(5.0, None),
            None,
            "拿不到合约信息时必须显式不可得，不能猜"
        );
    }

    /// 对**真实 OKX 私有端点**的端到端验证。
    ///
    /// 凭据从环境变量读取——**绝不写进仓库**。默认 `#[ignore]`：
    ///
    /// ```text
    /// MARKETLENS_TEST_API_KEY=... MARKETLENS_TEST_SECRET_KEY=... \
    /// MARKETLENS_TEST_PASSPHRASE=... \
    /// cargo test --lib okx_private -- --ignored --nocapture
    /// ```
    ///
    /// 测试本身不打印任何凭据内容，只打印接口返回的业务字段。
    #[tokio::test]
    #[ignore = "访问真实 OKX 私有端点，需显式提供凭据"]
    async fn okx_private_account_snapshot_is_sane() {
        let credentials = Credentials {
            api_key: std::env::var("MARKETLENS_TEST_API_KEY")
                .expect("缺少 MARKETLENS_TEST_API_KEY"),
            secret_key: std::env::var("MARKETLENS_TEST_SECRET_KEY")
                .expect("缺少 MARKETLENS_TEST_SECRET_KEY"),
            passphrase: std::env::var("MARKETLENS_TEST_PASSPHRASE")
                .expect("缺少 MARKETLENS_TEST_PASSPHRASE"),
            demo: true,
        };

        let client = OkxClient::new().expect("客户端构造失败");
        let db = Db::open_in_memory().await.expect("内存库创建失败");

        let snapshot = fetch(&client, &db, &credentials)
            .await
            .expect("账户拉取失败");

        println!("=== 账户概览 ===");
        println!("总权益      ${:.2}", snapshot.overview.total_eq_usd);
        println!("可用权益    ${:.2}", snapshot.overview.avail_eq_usd);
        println!("未实现盈亏  ${:.4}", snapshot.overview.upl);
        println!("持仓模式    {}", snapshot.overview.pos_mode);
        println!("保证金率    {:?}", snapshot.overview.mgn_ratio);
        println!("名义价值    {:?}", snapshot.overview.notional_usd);
        println!("币种明细    {} 条", snapshot.overview.currencies.len());

        println!("\n=== 持仓（{} 个）===", snapshot.positions.len());
        for position in &snapshot.positions {
            println!(
                "{} {} {}倍 {}张 → {:?} 币 | 均价 {} 标记价 {} 强平 {:?} | 名义 ${:.2} | 未实现 {:+.4}",
                position.inst_id,
                position.pos_side,
                position.lever,
                position.contracts,
                position.size_base,
                position.avg_px,
                position.mark_px,
                position.liq_px,
                position.notional_usd,
                position.upl,
            );
        }

        println!("\n=== 本地留痕 ===");
        println!("{}", snapshot.trace.note);
        println!("警告        {:?}", snapshot.warnings);

        assert!(
            snapshot.warnings.is_empty(),
            "不应有警告：{:#?}",
            snapshot.warnings
        );
        assert!(snapshot.overview.total_eq_usd > 0.0, "总权益应为正");
        assert!(
            !snapshot.overview.pos_mode.is_empty(),
            "持仓模式必须能读出来——它决定 posSide 的解释方式"
        );

        // 留痕必须真的写进去了
        assert!(
            snapshot.trace.records >= 1,
            "本次刷新应当写入至少一条留痕，实际 {}",
            snapshot.trace.records
        );

        // 有持仓时，验证换算与 OKX 自己的名义价值是否吻合
        for position in &snapshot.positions {
            if let Some(size_base) = position.size_base {
                let computed = size_base * position.mark_px;
                let ratio =
                    (computed - position.notional_usd).abs() / position.notional_usd.max(1.0);
                assert!(
                    ratio < 0.02,
                    "{} 的换算名义价值 {computed:.2} 与 OKX 的 {:.2} 相差 {:.2}%，\
                     说明 ctVal 换算有问题",
                    position.inst_id,
                    position.notional_usd,
                    ratio * 100.0
                );
            }
        }
    }
}
