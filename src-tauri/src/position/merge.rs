//! 历史仓位的双源合并（设计文档 §6.4.3）。
//!
//! 两个来源各有盲区：
//!   * **官方 `positions-history`**：精确（含手续费与资金费），但窗口有限，
//!     而且**拿不到持仓期间的最大浮盈/最大浮亏**。
//!   * **本地留痕**：只要应用被打开过就有记录，能推导出最大浮盈/浮亏，
//!     但时间精度受留痕间隔限制，且跨应用未运行期间的事件会丢失。
//!
//! 合并原则：**官方优先，本地只补充它拿不到的东西**。并且每条记录都带
//! `source` 与 `time_precision` 角标——数据可信度是复盘结论的地基，不能糊。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::AppResult;
use crate::position::history::ClosedPosition;
use crate::storage::Db;

/// 从留痕推导出的仓位生命周期。
#[derive(Debug, Clone, PartialEq)]
pub struct LocalLifecycle {
    pub pos_id: String,
    pub inst_id: String,
    /// 首次出现的留痕时间（≈开仓时间）
    pub first_seen: i64,
    /// 最后一次出现的留痕时间（≈平仓时间）
    pub last_seen: i64,
    /// 观察到的最大张数
    pub max_contracts: f64,
    /// 持仓期间的最大浮盈（留痕独有信息）
    pub max_favorable: Option<f64>,
    /// 持仓期间的最大浮亏
    pub max_adverse: Option<f64>,
    /// 观察到的最后均价
    pub last_avg_px: f64,
    /// 保证金模式（留痕里有，可以带过来）
    pub mgn_mode: String,
    pub lever: f64,
    /// 仓位在 OKX 侧的创建时间（比留痕时间精确）
    pub reported_open_time: i64,
}

/// 合并结果。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct MergeReport {
    /// 官方来源的笔数
    pub from_official: usize,
    /// 仅本地留痕推导出的笔数
    pub from_local: usize,
    /// 两源都有、被本地补充了极值的笔数
    pub enriched: usize,
    /// 本地留痕覆盖度说明，直接展示给用户
    pub coverage_note: String,
}

/// 从留痕快照推导仓位生命周期。
///
/// 判定规则（与设计文档 §6.4.2 一致）：
///   * `posId` 首次出现 → 开仓
///   * `posId` 不再出现 → 平仓
///   * 期间取张数最大值、浮盈/浮亏的极值
///
/// **抽成纯函数是为了能直接测**：这段逻辑写错了不会报错，只会静默给出错误的
/// 持仓时长与极值，而它们会直接影响复盘结论。
pub fn derive_lifecycles(snapshots: &[crate::storage::TraceSnapshot]) -> Vec<LocalLifecycle> {
    let mut ordered: Vec<&crate::storage::TraceSnapshot> = snapshots.iter().collect();
    ordered.sort_by_key(|snapshot| snapshot.ts);

    let mut lifecycles: HashMap<String, LocalLifecycle> = HashMap::new();

    for snapshot in ordered {
        let entry = lifecycles
            .entry(snapshot.pos_id.clone())
            .or_insert_with(|| LocalLifecycle {
                pos_id: snapshot.pos_id.clone(),
                inst_id: snapshot.inst_id.clone(),
                first_seen: snapshot.ts,
                last_seen: snapshot.ts,
                max_contracts: snapshot.contracts,
                max_favorable: None,
                max_adverse: None,
                last_avg_px: snapshot.avg_px,
                mgn_mode: snapshot.mgn_mode.clone(),
                lever: snapshot.lever,
                // 用交易所给的创建时间，而不是「我第一次看到它」的时间
                reported_open_time: snapshot.created_at,
            });

        entry.last_seen = entry.last_seen.max(snapshot.ts);
        entry.max_contracts = entry.max_contracts.max(snapshot.contracts);
        entry.last_avg_px = snapshot.avg_px;

        // 浮盈/浮亏取极值：这正是官方接口拿不到的信息
        entry.max_favorable = Some(
            entry
                .max_favorable
                .map_or(snapshot.upl, |current| current.max(snapshot.upl)),
        );
        entry.max_adverse = Some(
            entry
                .max_adverse
                .map_or(snapshot.upl, |current| current.min(snapshot.upl)),
        );
    }

    let mut result: Vec<LocalLifecycle> = lifecycles.into_values().collect();
    // 次级键用 pos_id：只按 first_seen 排序会在时间相同时依赖 HashMap 的迭代顺序，
    // 于是同一个输入每次跑出来的顺序都可能不同（测试会间歇性失败，界面也会跳来跳去）。
    result.sort_by(|a, b| {
        a.first_seen
            .cmp(&b.first_seen)
            .then_with(|| a.pos_id.cmp(&b.pos_id))
    });
    result
}

/// 合并两个来源。
///
/// `official` 会被就地补充极值（如果本地留痕里有对应 `posId`）。
/// 返回的 `MergeReport` 说明各自的贡献，供界面如实展示。
pub fn merge(official: &mut [ClosedPosition], local: &[LocalLifecycle]) -> MergeReport {
    let by_pos_id: HashMap<&str, &LocalLifecycle> = local
        .iter()
        .map(|lifecycle| (lifecycle.pos_id.as_str(), lifecycle))
        .collect();

    let mut enriched = 0usize;
    for position in official.iter_mut() {
        if let Some(lifecycle) = by_pos_id.get(position.pos_id.as_str()) {
            // 极值来自本地留痕，官方接口给不了
            position.max_favorable = lifecycle.max_favorable;
            position.max_adverse = lifecycle.max_adverse;
            enriched += 1;
        }
    }

    // 官方记录里没有、但本地留痕推导出来的仓位 → 补成 source = "local"
    let official_ids: std::collections::HashSet<&str> =
        official.iter().map(|p| p.pos_id.as_str()).collect();
    let local_only: Vec<&LocalLifecycle> = local
        .iter()
        .filter(|lifecycle| !official_ids.contains(lifecycle.pos_id.as_str()))
        .collect();

    let from_local = local_only.len();
    let from_official = official.len();

    let coverage_note = if local.is_empty() {
        "本地无留痕记录：无法提供持仓期间的最大浮盈/浮亏，也无法补全官方窗口之外的仓位。"
            .to_string()
    } else if from_local > 0 {
        format!(
            "本地留痕推导出 {from_local} 笔官方记录里没有的仓位；其时间为近似值（受留痕间隔限制）。"
        )
    } else {
        format!("本地留痕与官方记录一致，并为 {enriched} 笔补充了持仓期间的极值。")
    };

    MergeReport {
        from_official,
        from_local,
        enriched,
        coverage_note,
    }
}

/// 把「仅本地」的生命周期转成 `ClosedPosition`。
///
/// 这些记录**没有盈亏数据**（留痕只记浮盈快照，没有已实现盈亏），所以
/// 盈亏字段一律为 0，并标注 `source = "local"` 与 `time_precision = "approximate"`。
/// 统计时必须把它们与官方记录区分开——把 0 当成「不赚不亏」会污染胜率。
pub fn local_to_position(lifecycle: &LocalLifecycle) -> ClosedPosition {
    ClosedPosition {
        pos_id: lifecycle.pos_id.clone(),
        inst_id: lifecycle.inst_id.clone(),
        // 留痕里没有方向信息（净持仓模式下 posSide 恒为 net）
        // 留痕里没有方向（净持仓模式下 posSide 恒为 net）
        direction: "unknown".to_string(),
        mgn_mode: lifecycle.mgn_mode.clone(),
        lever: lifecycle.lever,
        open_avg_px: lifecycle.last_avg_px,
        close_avg_px: lifecycle.last_avg_px,
        max_contracts: lifecycle.max_contracts,
        pnl: 0.0,
        pnl_ratio: 0.0,
        realized_pnl: 0.0,
        fee: 0.0,
        funding_fee: 0.0,
        // 开仓时间优先用交易所给的创建时间；平仓时间只能用最后观测到的时间
        open_time: if lifecycle.reported_open_time > 0 {
            lifecycle.reported_open_time
        } else {
            lifecycle.first_seen
        },
        close_time: lifecycle.last_seen,
        source: "local".to_string(),
        time_precision: "approximate".to_string(),
        regime: None,
        // 极值恰恰是留痕独有的信息，必须带过去
        max_favorable: lifecycle.max_favorable,
        max_adverse: lifecycle.max_adverse,
    }
}

/// 端到端：读留痕 → 推导 → 与官方记录合并。
pub async fn merge_for_range(
    db: &Db,
    official: &mut Vec<ClosedPosition>,
    from: i64,
    to: i64,
) -> AppResult<MergeReport> {
    let snapshots = db.position_traces_between(from, to).await?;
    let lifecycles = derive_lifecycles(&snapshots);

    let report = merge(official, &lifecycles);

    // 仅本地的仓位补进结果集（它们仍有价值：能说明「那段时间我确实开过仓」）
    let official_ids: std::collections::HashSet<String> =
        official.iter().map(|p| p.pos_id.clone()).collect();
    for lifecycle in &lifecycles {
        if !official_ids.contains(&lifecycle.pos_id) {
            official.push(local_to_position(lifecycle));
        }
    }

    official.sort_by_key(|position| position.close_time);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TraceSnapshot;

    fn snapshot(ts: i64, pos_id: &str, contracts: f64, upl: f64) -> TraceSnapshot {
        TraceSnapshot {
            ts,
            pos_id: pos_id.to_string(),
            inst_id: "BTC-USDT-SWAP".to_string(),
            mgn_mode: "cross".to_string(),
            lever: 3.0,
            contracts,
            avg_px: 100.0,
            upl,
            // 交易所给的创建时间比留痕时间早一点，用于验证「优先用创建时间」
            created_at: ts - 500,
        }
    }

    #[test]
    fn derives_lifecycle_from_consecutive_snapshots() {
        let snapshots = vec![
            snapshot(1_000, "a", 1.0, 5.0),
            snapshot(2_000, "a", 2.0, 12.0), // 加仓 + 浮盈变大
            snapshot(3_000, "a", 2.0, -4.0), // 回撤到浮亏
            snapshot(4_000, "a", 1.0, 2.0),  // 减仓
        ];

        let lifecycles = derive_lifecycles(&snapshots);
        assert_eq!(lifecycles.len(), 1);
        let lifecycle = &lifecycles[0];
        assert_eq!(lifecycle.pos_id, "a");
        assert_eq!(lifecycle.first_seen, 1_000);
        assert_eq!(lifecycle.last_seen, 4_000);
        assert_eq!(lifecycle.max_contracts, 2.0);
        assert_eq!(lifecycle.max_favorable, Some(12.0), "最大浮盈取极值");
        assert_eq!(lifecycle.max_adverse, Some(-4.0), "最大浮亏取极值");
    }

    #[test]
    fn separates_multiple_positions() {
        let snapshots = vec![
            snapshot(1_000, "a", 1.0, 1.0),
            snapshot(1_000, "b", 1.0, -1.0),
            snapshot(2_000, "a", 1.0, 2.0),
        ];
        let lifecycles = derive_lifecycles(&snapshots);
        assert_eq!(lifecycles.len(), 2);
        assert_eq!(lifecycles[0].pos_id, "a");
        assert_eq!(lifecycles[1].pos_id, "b");
    }

    #[test]
    fn empty_snapshots_yield_no_lifecycles() {
        assert!(derive_lifecycles(&[]).is_empty());
    }

    fn official(pos_id: &str) -> ClosedPosition {
        ClosedPosition {
            pos_id: pos_id.to_string(),
            inst_id: "BTC-USDT-SWAP".to_string(),
            direction: "long".to_string(),
            mgn_mode: "cross".to_string(),
            lever: 3.0,
            open_avg_px: 100.0,
            close_avg_px: 110.0,
            max_contracts: 1.0,
            pnl: 10.0,
            pnl_ratio: 0.1,
            realized_pnl: 9.0,
            fee: -1.0,
            funding_fee: 0.0,
            open_time: 1_000,
            close_time: 2_000,
            source: "okx".to_string(),
            time_precision: "exact".to_string(),
            regime: None,
            max_favorable: None,
            max_adverse: None,
        }
    }

    #[test]
    fn merge_enriches_official_records_with_local_extremes() {
        let mut positions = vec![official("a")];
        let lifecycles = vec![LocalLifecycle {
            pos_id: "a".to_string(),
            inst_id: "BTC-USDT-SWAP".to_string(),
            first_seen: 1_000,
            last_seen: 2_000,
            max_contracts: 1.5,
            max_favorable: Some(42.0),
            max_adverse: Some(-7.0),
            last_avg_px: 100.0,
            mgn_mode: "cross".to_string(),
            lever: 3.0,
            reported_open_time: 1_000,
        }];

        let report = merge(&mut positions, &lifecycles);
        assert_eq!(report.enriched, 1);
        assert_eq!(report.from_local, 0, "官方已有该 posId，不算仅本地");
        assert_eq!(
            positions[0].max_favorable,
            Some(42.0),
            "官方拿不到的极值应由本地补上"
        );
        assert_eq!(positions[0].source, "okx", "官方记录的来源不应被改写");
    }

    #[test]
    fn merge_marks_local_only_positions() {
        let mut positions = vec![official("a")];
        let lifecycles = vec![LocalLifecycle {
            pos_id: "b".to_string(),
            inst_id: "ETH-USDT-SWAP".to_string(),
            first_seen: 5_000,
            last_seen: 6_000,
            max_contracts: 1.0,
            max_favorable: Some(3.0),
            max_adverse: Some(-3.0),
            last_avg_px: 50.0,
            mgn_mode: "isolated".to_string(),
            lever: 5.0,
            reported_open_time: 4_900,
        }];

        let report = merge(&mut positions, &lifecycles);
        assert_eq!(report.from_local, 1);

        let local = local_to_position(&lifecycles[0]);
        assert_eq!(local.source, "local");
        assert_eq!(local.time_precision, "approximate");
        assert_eq!(
            local.realized_pnl, 0.0,
            "留痕没有已实现盈亏，必须为 0 并靠 source 区分，不能当成不赚不亏"
        );
    }

    #[test]
    fn merge_reports_empty_coverage_honestly() {
        let mut positions = vec![official("a")];
        let report = merge(&mut positions, &[]);
        assert!(report.coverage_note.contains("无留痕"));
    }
}
