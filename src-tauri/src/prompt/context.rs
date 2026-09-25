//! 提示词上下文装配（设计文档 §6.7）。
//!
//! 装配分两段：
//!   1. 把领域类型摊平成模板友好的 JSON（`build_*`）
//!   2. 应用隐私分级（`privacy::apply`）
//!
//! **顺序不能反**：隐私分级必须发生在装配之后、渲染之前，这样用户自写模板
//! 也拿不到未脱敏的数据——泄漏只可能在装配阶段发生，而装配阶段是我们控制的代码。
//!
//! 另一条规则：**市场数据（价格、资金费率、指标）不是隐私**，原样保留。
//! 只有账户与仓位相关的数值走 [`Sensitive`]。把行情也脱敏只会让提示词失去意义。

use serde_json::{Value, json};

use crate::market::live::{LiveSnapshot, MarketState};
use crate::position::{AccountOverview, Position};
use crate::prompt::privacy::{self, PrivacyLevel, Sensitive};
use crate::review::ReviewContext;

/// 上下文 schema 版本。
///
/// 模板契约会随版本演进（加字段、改结构），把它写进上下文，
/// 用户保存的模板出问题时至少能看出是「按哪个版本写的」。
pub const SCHEMA_VERSION: &str = "1";

/// 一次装配的结果：渲染输入 + 给用户看的信息。
pub struct Assembled {
    pub context: Value,
    pub warnings: Vec<String>,
}

// ------------------------------------------------------------------ 实盘上下文

/// 装配实盘提示词上下文。
///
/// `account` / `positions` 为 `None` 表示**账户数据拿不到**（未配置凭据、接口失败）。
/// 这仍然是一个合法上下文——用户可能只想让 AI 分析行情——所以这里不报错，
/// 而是在上下文里显式声明「账户数据不可得」及原因。
pub fn live(
    snapshot: &LiveSnapshot,
    account: Option<&AccountOverview>,
    positions: Option<&[Position]>,
    level: PrivacyLevel,
    account_reason: Option<&str>,
) -> Assembled {
    let equity = account.map(|overview| overview.total_eq_usd);

    let mut warnings = snapshot.warnings.clone();
    if account.is_none() {
        warnings.push(account_reason.unwrap_or("账户数据不可得").to_string());
    }

    let account_value = match account {
        Some(overview) => account_block(overview),
        None => unavailable_account_block(account_reason.unwrap_or("账户数据不可得")),
    };

    let position_values: Vec<Value> = positions
        .unwrap_or(&[])
        .iter()
        .map(position_block)
        .collect();

    let market_values: Vec<Value> = snapshot.instruments.iter().map(market_block).collect();

    let mut context = json!({
        "meta": meta_block(level, &warnings, snapshot.ts),
        "account": account_value,
        "positions": position_values,
        "market": market_values,
    });

    privacy::apply(&mut context, level, equity);

    // 权益量级区间**始终**提供（L0 下也提供，尽管那时精确权益本来就在）。
    //
    // 这不是冗余：模板契约必须与隐私等级**无关**。如果这个字段只在 L1/L2 存在，
    // 那么引用它的模板在 L0 下会直接渲染失败——而用户切隐私等级只是想换个脱敏程度，
    // 不该让模板突然坏掉。
    if let Some(equity) = equity {
        context["account"]["equity_magnitude"] = Value::from(privacy::magnitude_bucket(equity));
    }

    Assembled { context, warnings }
}

fn meta_block(level: PrivacyLevel, warnings: &[String], generated_at: i64) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "generated_at": generated_at,
        "data_source": "OKX",
        "privacy": {
            "level": level.as_str(),
            "note": level.note(),
        },
        "warnings": warnings,
    })
}

/// 账户不可得时的账户块。
///
/// **键必须与 [`account_block`] 完全一致**，只是值都是「不可得」。
/// 否则「没配凭据」这一种情况就会让所有引用 `account.*` 的模板渲染失败——
/// 而用户此时最需要的恰恰是「让 AI 只分析行情」这种能用的提示词。
fn unavailable_account_block(reason: &str) -> Value {
    let sensitive = || Sensitive::unavailable(reason);
    json!({
        "available": false,
        "reason": reason,
        "equity": sensitive(),
        "equity_magnitude": format!("[数据不可得：{reason}]"),
        "adjusted_equity": sensitive(),
        "available_equity": sensitive(),
        "unrealized_pnl": sensitive(),
        "notional": sensitive(),
        "margin_ratio": null,
        "initial_margin": sensitive(),
        "maintenance_margin": sensitive(),
        "position_mode": null,
        "currencies": [],
    })
}

fn account_block(overview: &AccountOverview) -> Value {
    let currencies: Vec<Value> = overview
        .currencies
        .iter()
        .map(|balance| {
            json!({
                "ccy": balance.ccy,
                "equity": Sensitive::usd(balance.eq_usd),
                "available": Sensitive::usd(balance.avail_bal),
            })
        })
        .collect();

    json!({
        "available": true,
        "equity": Sensitive::usd(overview.total_eq_usd),
        "adjusted_equity": Sensitive::usd(overview.adj_eq_usd),
        "available_equity": Sensitive::usd(overview.avail_eq_usd),
        "unrealized_pnl": Sensitive::usd(overview.upl),
        "notional": overview.notional_usd.map(Sensitive::usd),
        "margin_ratio": overview.mgn_ratio,
        "initial_margin": overview.imr.map(Sensitive::usd),
        "maintenance_margin": overview.mmr.map(Sensitive::usd),
        "position_mode": overview.pos_mode,
        "currencies": currencies,
    })
}

fn position_block(position: &Position) -> Value {
    json!({
        "inst_id": position.inst_id,
        "pos_side": position.pos_side,
        "margin_mode": position.mgn_mode,
        "leverage": position.lever,
        // 张数可结合价格推算仓位规模，按敏感值处理
        "contracts": Sensitive::count(position.contracts),
        "size_base": position.size_base.map(Sensitive::count),
        "avg_price": position.avg_px,
        "mark_price": position.mark_px,
        "liquidation_price": position.liq_px,
        "unrealized_pnl": Sensitive::usd(position.upl),
        // 收益率不是金额：它单独看不出账户规模，且是判断风险最直接的量
        "unrealized_pnl_ratio": position.upl_ratio,
        "margin_ratio": position.mgn_ratio,
        "notional": Sensitive::usd(position.notional_usd),
        "fee": Sensitive::usd(position.fee),
        "funding_fee": Sensitive::usd(position.funding_fee),
        "realized_pnl": Sensitive::usd(position.realized_pnl),
        "created_at": position.created_at,
        "updated_at": position.updated_at,
    })
}

fn market_block(state: &MarketState) -> Value {
    json!({
        "inst_id": state.inst_id,
        "last": state.last,
        "change_24h_pct": state.change_pct,
        "high_24h": state.high_24h,
        "low_24h": state.low_24h,
        "volume_24h_usd": state.volume_24h_usd,
        "funding_rate": state.funding_rate,
        "funding_annualized": state.funding_annualized,
        "next_funding_rate": state.next_funding_rate,
        "next_funding_time": state.next_funding_time,
        "open_interest_usd": state.open_interest_usd,
        "basis_pct": state.basis_pct,
        "long_short_ratio": state.long_short_ratio,
        "taker_buy_sell_ratio": state.taker_buy_sell_ratio,
        "ema20": state.ema20,
        "ema60": state.ema60,
        "ema200": state.ema200,
        "rsi14": state.rsi14,
        "atr_pct": state.atr_pct,
        "realized_vol": state.realized_vol,
        "trend": state.trend.map(trend_label),
        "volatility": state.vol.map(vol_label),
        "crowding": crowding_label(state.crowding),
        "signals": state.signals,
        // 拿不到的指标及原因：模板必须渲染成「数据不可得」，不能留空
        "unavailable": state.unavailable,
    })
}

// ------------------------------------------------------------------ 复盘上下文

/// 装配复盘提示词上下文。
pub fn review(context: &ReviewContext, level: PrivacyLevel) -> Assembled {
    let equity = None; // 复盘上下文不携带账户权益，见下方注释

    let position_values: Vec<Value> = context
        .positions
        .iter()
        .map(|position| {
            json!({
                "inst_id": position.inst_id,
                "direction": position.direction,
                "margin_mode": position.mgn_mode,
                "leverage": position.lever,
                "max_contracts": Sensitive::count(position.max_contracts),
                "open_price": position.open_avg_px,
                "close_price": position.close_avg_px,
                "price_pnl": Sensitive::usd(position.pnl),
                "realized_pnl": Sensitive::usd(position.realized_pnl),
                "pnl_ratio": position.pnl_ratio,
                "fee": Sensitive::usd(position.fee),
                "funding_fee": Sensitive::usd(position.funding_fee),
                "open_time": position.open_time,
                "close_time": position.close_time,
                "hold_ms": position.close_time - position.open_time,
                "source": position.source,
                "time_precision": position.time_precision,
                "max_favorable": position.max_favorable.map(Sensitive::usd),
                "max_adverse": position.max_adverse.map(Sensitive::usd),
                "regime": position.regime.as_ref().map(regime_block),
            })
        })
        .collect();

    let stats = &context.stats;
    let by_trend: Vec<Value> = stats
        .by_trend
        .iter()
        .map(|group| {
            json!({
                "key": group.key,
                "count": group.count,
                "win_rate": group.win_rate,
                "total_pnl": Sensitive::usd(group.total_pnl),
                "profit_factor": group.profit_factor,
                "is_unattributed": group.is_unattributed,
            })
        })
        .collect();
    let by_direction: Vec<Value> = stats
        .by_direction
        .iter()
        .map(|group| {
            json!({
                "key": group.key,
                "count": group.count,
                "win_rate": group.win_rate,
                "total_pnl": Sensitive::usd(group.total_pnl),
                "profit_factor": group.profit_factor,
                "is_unattributed": group.is_unattributed,
            })
        })
        .collect();
    let by_instrument: Vec<Value> = stats
        .by_instrument
        .iter()
        .map(|group| {
            json!({
                "key": group.key,
                "count": group.count,
                "win_rate": group.win_rate,
                "total_pnl": Sensitive::usd(group.total_pnl),
                "profit_factor": group.profit_factor,
                "is_unattributed": group.is_unattributed,
            })
        })
        .collect();

    let mut warnings = context.warnings.clone();
    if level == PrivacyLevel::L1 {
        // 复盘上下文没有账户权益（它可能覆盖几个月，那时的权益和现在不是一回事，
        // 拿现在的权益去除历史盈亏会算出误导性的比例），因此 L1 在这里退化为 L2 行为。
        warnings.push(
            "隐私等级 L1 需要账户权益才能百分比化，而复盘上下文不携带权益，\
             本次按结构脱敏处理"
                .to_string(),
        );
    }

    let mut value = json!({
        "meta": meta_block(level, &warnings, context.to),
        "period": {
            "from": context.from,
            "to": context.to,
            "bar": context.bar,
        },
        "positions": position_values,
        "stats": {
            "total": stats.total,
            "win_rate": stats.win_rate,
            "profit_factor": stats.profit_factor,
            "expectancy": Sensitive::usd(stats.expectancy),
            "total_realized_pnl": Sensitive::usd(stats.total_realized_pnl),
            "avg_hold_ms": stats.avg_hold_ms,
            "fee_drag": stats.fee_drag,
            "by_trend": by_trend,
            "by_direction": by_direction,
            "by_instrument": by_instrument,
            // 拿不到开仓时状态的笔数：直接影响「按状态分组」的可信度
            "unattributed": stats.unattributed,
        },
        "data_quality": {
            "from_official": context.merge.from_official,
            "from_local": context.merge.from_local,
            "enriched": context.merge.enriched,
            "note": context.merge.coverage_note,
            "trace_note": context.trace.note,
        },
    });

    // 复盘上下文没有权益可依，L1 只能退化为「按结构脱敏」——
    // 宁可少给数据，也不能因为算不出比例就把原始金额放出去。
    let effective = match level {
        PrivacyLevel::L1 => PrivacyLevel::L2,
        other => other,
    };
    privacy::apply(&mut value, effective, equity);

    Assembled {
        context: value,
        warnings,
    }
}

fn regime_block(regime: &crate::position::history::RegimeSnapshot) -> Value {
    json!({
        "trend": regime.trend.map(trend_label),
        "volatility": regime.vol.map(vol_label),
        "crowding": regime.crowding.map(crowding_label),
        "change_24h_pct": regime.change_24h,
        "funding_annualized": regime.funding_annualized,
        "basis_pct": regime.basis_pct,
    })
}

// -------------------------------------------------------------- 枚举 → 中文标签
// 复用 stats / regime 模块已有的映射，避免同一套文案维护两份。

fn trend_label(trend: crate::market::regime::TrendRegime) -> &'static str {
    crate::position::stats::trend_label(trend)
}

fn vol_label(vol: crate::market::regime::VolRegime) -> &'static str {
    crate::market::regime::vol_label(vol)
}

fn crowding_label(crowding: crate::market::regime::Crowding) -> &'static str {
    crate::market::regime::crowding_label(crowding)
}

#[cfg(test)]
mod contract_tests {
    //! 模板契约的稳定性。这些测试守的不是「值对不对」，而是
    //! **同一个模板在任何隐私等级、任何数据可得性下都能渲染成功**。

    use super::*;
    use crate::prompt::privacy::PrivacyLevel;
    use crate::prompt::render::render;

    /// 引用全部实盘字段的模板。任何字段在某个等级下缺失都会让这个测试失败。
    const LIVE_PROBE: &str = "\
{{ meta.schema_version }}{{ meta.generated_at | ts }}{{ meta.data_source }}
{{ meta.privacy.level }}{{ meta.privacy.note }}{{ meta.warnings | length }}
{{ account.available }}{{ account.equity | money }}{{ account.equity_magnitude }}
{{ account.adjusted_equity | money }}{{ account.available_equity | money }}
{{ account.unrealized_pnl | money }}{{ account.notional | money }}
{{ account.margin_ratio | na(\"无\") }}{{ account.initial_margin | money }}
{{ account.maintenance_margin | money }}{{ account.position_mode | na(\"无\") }}
{{ account.currencies | length }}
{{ positions | length }}{{ market | length }}";

    fn empty_snapshot() -> LiveSnapshot {
        LiveSnapshot {
            ts: 1_700_000_000_000,
            cache_hit: false,
            fetched_at: 1_700_000_000_000,
            watchlist: vec![],
            instruments: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn account_unavailable_still_renders_every_field() {
        // 没有账户数据时，引用 account.* 的模板仍须渲染成功
        for level in [PrivacyLevel::L0, PrivacyLevel::L1, PrivacyLevel::L2] {
            let assembled = live(&empty_snapshot(), None, None, level, Some("尚未配置凭据"));
            let output = render(LIVE_PROBE, &assembled.context).unwrap_or_else(|err| {
                panic!("{} 下账户不可得时模板渲染失败：{err}", level.as_str())
            });
            assert!(output.contains("尚未配置凭据"), "应说明原因：{output}");
        }
    }

    #[test]
    fn privacy_level_does_not_change_the_template_contract() {
        // 关键性质：切隐私等级不能让模板突然坏掉
        let account = AccountOverview {
            total_eq_usd: 12345.0,
            iso_eq_usd: 0.0,
            adj_eq_usd: 12345.0,
            avail_eq_usd: 12000.0,
            upl: 45.0,
            mgn_ratio: Some(2.5),
            imr: Some(100.0),
            mmr: Some(10.0),
            notional_usd: Some(5000.0),
            pos_mode: "net_mode".to_string(),
            currencies: vec![],
            fetched_at: 1_700_000_000_000,
        };

        let mut rendered = Vec::new();
        for level in [PrivacyLevel::L0, PrivacyLevel::L1, PrivacyLevel::L2] {
            let assembled = live(&empty_snapshot(), Some(&account), None, level, None);
            let output = render(LIVE_PROBE, &assembled.context)
                .unwrap_or_else(|err| panic!("{} 下渲染失败：{err}", level.as_str()));
            rendered.push((level, output));
        }

        // L0 出现原始金额；L1/L2 都不出现
        assert!(rendered[0].1.contains("$12.35K"), "L0 应含原始金额");
        assert!(
            !rendered[1].1.contains("12.35K"),
            "L1 不该含原始金额：{}",
            rendered[1].1
        );
        assert!(!rendered[2].1.contains("12.35K"), "L2 不该含原始金额");

        // L1 有权益可依，金额应当**百分比化**（不是变成不可得）
        assert!(
            rendered[1].1.contains("%"),
            "L1 应把金额转成占权益百分比：{}",
            rendered[1].1
        );
        assert!(
            !rendered[1].1.contains("数据不可得"),
            "L1 有权益时不该把金额标成不可得：{}",
            rendered[1].1
        );

        // L2 无权益可依也无需百分比，一律显式声明不可得，绝不能留空
        assert!(
            rendered[2].1.contains("数据不可得"),
            "L2 应显式声明不可得：{}",
            rendered[2].1
        );

        // 权益量级区间在任何等级下都可用
        for (level, output) in &rendered {
            assert!(
                output.contains("$10k–$50k"),
                "{} 应给出权益量级",
                level.as_str()
            );
        }
    }

    #[test]
    fn unavailable_market_indicators_render_as_explicit_notes() {
        // 行情指标缺失（旧时段、K 线不足）走同一条出口
        let mut snapshot = empty_snapshot();
        snapshot.instruments.push(crate::market::live::MarketState {
            inst_id: "BTC-USDT-SWAP".to_string(),
            ts: 1_700_000_000_000,
            last: 100_000.0,
            change_pct: 0.0123,
            high_24h: 101_000.0,
            low_24h: 99_000.0,
            volume_24h_usd: 1_500_000_000.0,
            funding_rate: 0.0001,
            funding_annualized: 0.1095,
            next_funding_rate: None,
            next_funding_time: None,
            open_interest_usd: None,
            basis_pct: None,
            long_short_ratio: None,
            taker_buy_sell_ratio: None,
            ema20: None,
            ema60: None,
            ema200: None,
            rsi14: None,
            atr_pct: None,
            realized_vol: None,
            trend: None,
            vol: None,
            crowding: crate::market::regime::Crowding::Balanced,
            signals: vec![],
            unavailable: vec!["RSI14：K 线不足".to_string()],
        });

        let assembled = live(&snapshot, None, None, PrivacyLevel::L0, None);
        let body = "{{ market[0].trend | na(\"趋势不可判定\") }}|\
                    {{ market[0].atr_pct | na(\"K 线不足\") | pct }}|\
                    {{ market[0].unavailable | join(\";\") }}";
        let output = render(body, &assembled.context).expect("应能渲染");

        assert_eq!(
            output,
            "[数据不可得：趋势不可判定]|[数据不可得：K 线不足]|RSI14：K 线不足"
        );
    }
}
