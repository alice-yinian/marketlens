/**
 * 行情页的纯函数：参数校验与展示格式化。
 *
 * 与 Rust 侧的上限保持一致（`MAX_BARS_PER_PLAN = 6`、`MAX_CANDLES_PER_BAR = 500`、
 * `MAX_TOTAL_CANDLES = 1500`、`MAX_INDICATORS = 8`、`MAX_INDICATOR_PERIOD = 500`）。
 *
 * 前端先挡一遍是为了**即时反馈**（用户不用等到点按钮才知道超限），
 * 后端的校验才是安全边界——两边都不能省，而这里错了一致性由后端的错误信息兜底。
 */
import type { IndicatorKind, IndicatorSpec } from "../../lib/types";

export const MAX_BARS_PER_PLAN = 6;
export const MAX_CANDLES_PER_BAR = 500;
export const MAX_TOTAL_CANDLES = 1500;
export const MAX_INDICATORS = 8;
export const MAX_INDICATOR_PERIOD = 500;

/** 界面上可选的指标种类，顺序即下拉框顺序。 */
export const INDICATOR_KINDS: readonly IndicatorKind[] = [
  "ema",
  "rsi",
  "atr_pct",
  "realized_vol",
];

/**
 * 每个指标的默认周期。
 *
 * 用最常用的档位（EMA20 / RSI14 / ATR14 / 24 根算波动率）而不是 0 或其他占位值：
 * 默认值本身就是个可用的选择，用户不必先想「该填几」。
 */
export const DEFAULT_PERIOD: Record<IndicatorKind, number> = {
  ema: 20,
  rsi: 14,
  atr_pct: 14,
  realized_vol: 24,
};

/**
 * 指标的短标签，与 Rust `IndicatorKind::label()` 一一对应。
 *
 * 对应关系是刻意的：提示词表格里的列名由 Rust 生成（`EMA20` / `ATR%14`），
 * 界面上的名字与它们不一致的话，用户没法把「我勾的」和「表里的列」对上号。
 * 用 `Record` 而不是 `switch`：漏一个分支时前者编译期就报错。
 */
export const INDICATOR_LABEL: Record<IndicatorKind, string> = {
  ema: "EMA",
  rsi: "RSI",
  atr_pct: "ATR%",
  realized_vol: "RVol",
};

/** 去掉重复的「种类 + 周期」组合，保留首次出现的顺序。 */
export function dedupeIndicators(list: IndicatorSpec[]): IndicatorSpec[] {
  const seen = new Set<string>();
  const unique: IndicatorSpec[] = [];
  for (const spec of list) {
    const key = `${spec.kind}:${spec.period}`;
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(spec);
  }
  return unique;
}

/**
 * 指标清单里第一个问题；全部合格时返回 `null`。
 *
 * 只返回**一条**：同时列五条会让人不知道先改哪个。
 */
export function indicatorProblem(list: IndicatorSpec[]): string | null {
  if (list.length > MAX_INDICATORS) {
    return "too_many";
  }
  for (const spec of list) {
    if (!Number.isInteger(spec.period) || spec.period < 1 || spec.period > MAX_INDICATOR_PERIOD) {
      return "period_range";
    }
  }
  return null;
}

/** 选择参数里第一个问题；全部合格时返回 `null`。 */
export function selectionProblem(input: {
  instId: string | null;
  bars: string[];
  candleCount: number;
}): "inst" | "bars" | "count" | "total" | "too_many_bars" | null {
  if (input.instId === null || input.instId.trim().length === 0) return "inst";
  if (input.bars.length === 0) return "bars";
  if (input.bars.length > MAX_BARS_PER_PLAN) return "too_many_bars";
  if (
    !Number.isInteger(input.candleCount) ||
    input.candleCount < 1 ||
    input.candleCount > MAX_CANDLES_PER_BAR
  ) {
    return "count";
  }
  if (input.candleCount * input.bars.length > MAX_TOTAL_CANDLES) return "total";
  return null;
}
