/**
 * 复盘采集页的纯格式化与状态映射：无副作用、不读全局状态。
 *
 * 单位约定：所有时间戳是 Unix 毫秒；`est_duration_ms` 在 Rust 里是 `u64`
 * （ts-rs 声明为 bigint），但 IPC 走 JSON，运行时可能是 number——
 * 所以时长格式化同时接受两者。
 *
 * 返回 `null` 表示无法计算；状态映射对未知值不静默留空。
 */
import { S } from "../../lib/strings";
import type { RegimeSnapshot, ReviewStats, SeriesKind, SeriesStatus } from "../../lib/types";
import { formatPct, formatSignedPct, formatUsd } from "../live/format";

/** 与 Rust `fetch::plan::MAX_RANGE_MS` 一致的 90 天硬上限 */
export const MAX_RANGE_MS = 90 * 24 * 60 * 60 * 1000;

const DAY_MS = 24 * 60 * 60 * 1000;

/** 可选的 K 线粒度（与后端 `bar_millis` 接受的取值一致） */
export const BAR_OPTIONS: readonly string[] = ["15m", "1H", "4H", "1D"];

export type RangeProblem = "invalid" | "order" | "tooLarge";

/**
 * 校验时段。返回 null 表示可以生成计划。
 *
 * 上限检查放在 UI 侧是为了**提前**告诉用户为什么不能继续，
 * 而不是等后端返回 `RangeTooLarge`——但后端仍会独立校验（不信任前端）。
 */
export function rangeError(from: number, to: number): RangeProblem | null {
  if (!Number.isFinite(from) || !Number.isFinite(to)) return "invalid";
  if (to <= from) return "order";
  if (to - from > MAX_RANGE_MS) return "tooLarge";
  return null;
}

/** 时段天数（向上取整，用于提示「当前约 N 天」） */
export function rangeDays(from: number, to: number): number {
  if (!Number.isFinite(from) || !Number.isFinite(to) || to <= from) return 0;
  return Math.ceil((to - from) / DAY_MS);
}

/** Unix 毫秒 → `<input type="datetime-local">` 的本地时间字符串（YYYY-MM-DDTHH:mm） */
export function msToLocalInput(ms: number): string {
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

/** `<input type="datetime-local">` 的值 → Unix 毫秒；无法解析返回 null */
export function localInputToMs(value: string): number | null {
  if (value.length === 0) return null;
  const ms = new Date(value).getTime();
  return Number.isFinite(ms) ? ms : null;
}

/**
 * 进度百分比：`done_series / total_series`，钳制到 0–100。
 * `total_series` 为 0（计划为空或进度尚未到）时返回 0。
 */
export function progressPercent(done: number, total: number): number {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((done / total) * 100)));
}

/** 时长：不足 1 秒 / 约 N 秒 / 约 N 分钟。 */
export function formatDurationMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 1000) return S.review.duration.underSecond;
  if (ms < 60_000) return S.review.duration.seconds(Math.round(ms / 1000));
  return S.review.duration.minutes(Number((ms / 60_000).toFixed(1)));
}

/** `SeriesKind`（Rust 变体名）→ 中文 */
export function seriesKindLabel(kind: SeriesKind): string {
  return S.review.seriesKind[kind];
}

/** `bar` → 中文展示（15m → 15 分钟） */
export function barLabel(bar: string): string {
  const labels: Record<string, string> = {
    "15m": "15 分钟",
    "1H": "1 小时",
    "4H": "4 小时",
    "1D": "1 天",
  };
  return labels[bar] ?? bar;
}

export type SeriesTone = "ok" | "unavailable" | "failed" | "skipped";

export interface SeriesStatusView {
  tone: SeriesTone;
  label: string;
  /** 徽章样式 */
  badge: string;
  /** 整行边框样式 */
  row: string;
}

const TONES: Record<SeriesStatus, SeriesTone> = {
  Ok: "ok",
  Unavailable: "unavailable",
  Failed: "failed",
  Skipped: "skipped",
};

const BADGES: Record<SeriesTone, string> = {
  ok: "border-emerald-800 bg-emerald-950/50 text-emerald-300",
  unavailable: "border-neutral-700 bg-neutral-800/60 text-neutral-400",
  failed: "border-red-900 bg-red-950/50 text-red-300",
  skipped: "border-sky-900 bg-sky-950/50 text-sky-300",
};

const ROWS: Record<SeriesTone, string> = {
  ok: "border-neutral-800",
  unavailable: "border-neutral-800",
  failed: "border-red-900/60",
  skipped: "border-sky-900/50",
};

/** `SeriesStatus` → 视觉呈现（四态各不相同，Skipped 是好事，不画成红的） */
export function seriesStatusView(status: SeriesStatus): SeriesStatusView {
  const tone = TONES[status];
  return { tone, label: S.review.status[status], badge: BADGES[tone], row: ROWS[tone] };
}

// ---------------------------------------------------------------------------
// 复盘结果展示（M4）的映射与格式化
//
// 约定：映射类函数对**未知取值**不静默留空，而是回落到明确的「未知/不可得」，
// 因为数据可信度是复盘结论的地基（架构铁律 4/5）。
// ---------------------------------------------------------------------------

/** 角标：文案 + 样式。样式集中在 format.ts，组件只负责摆放。 */
export interface BadgeView {
  label: string;
  badge: string;
}

const BADGE_NEUTRAL = "border-neutral-700 bg-neutral-800/60 text-neutral-300";
const BADGE_SOURCE_OKX = "border-sky-900 bg-sky-950/50 text-sky-300";
const BADGE_SOURCE_LOCAL = "border-violet-900 bg-violet-950/50 text-violet-300";
const BADGE_SOURCE_MERGED = "border-emerald-900 bg-emerald-950/50 text-emerald-300";
const BADGE_PRECISION_EXACT = "border-neutral-700 bg-neutral-800/60 text-neutral-300";
const BADGE_PRECISION_APPROX = "border-amber-900 bg-amber-950/40 text-amber-300";

const SOURCE_BADGES: Record<string, string> = {
  okx: BADGE_SOURCE_OKX,
  local: BADGE_SOURCE_LOCAL,
  merged: BADGE_SOURCE_MERGED,
};

/**
 * 仓位来源角标：`okx` / `local` / `merged` 三态视觉各异。
 *
 * 未知取值不隐藏，回落成「来源未知」+ 中性灰——隐藏会让用户误以为数据可信。
 */
export function sourceView(source: string): BadgeView {
  const labels: Record<string, string> = {
    okx: S.review.result.source.okx,
    local: S.review.result.source.local,
    merged: S.review.result.source.merged,
  };
  return {
    label: labels[source] ?? S.review.result.source.unknown,
    badge: SOURCE_BADGES[source] ?? BADGE_NEUTRAL,
  };
}

/** 时间精度角标：`exact` / `approximate`。未知取值回落成「精度未知」。 */
export function precisionView(precision: string): BadgeView {
  if (precision === "exact") {
    return { label: S.review.result.precision.exact, badge: BADGE_PRECISION_EXACT };
  }
  if (precision === "approximate") {
    return { label: S.review.result.precision.approximate, badge: BADGE_PRECISION_APPROX };
  }
  return { label: S.review.result.precision.unknown, badge: BADGE_NEUTRAL };
}

/** 方向文案：`long`→多 / `short`→空 / `unknown`→未知（任何其它取值也归为未知） */
export function directionLabel(direction: string): string {
  const labels: Record<string, string> = {
    long: S.review.result.direction.long,
    short: S.review.result.direction.short,
  };
  return labels[direction] ?? S.review.result.direction.unknown;
}

/** 方向角标样式：多→绿、空→红、未知→暗灰（未知绝不画成中性「无方向」） */
export function directionView(direction: string): BadgeView {
  const tones: Record<string, string> = {
    long: "border-emerald-900 bg-emerald-950/50 text-emerald-300",
    short: "border-red-900 bg-red-950/50 text-red-300",
  };
  return {
    label: directionLabel(direction),
    badge: tones[direction] ?? "border-neutral-700 bg-neutral-800/60 text-neutral-400",
  };
}

/**
 * 盈亏比文案。`null` 表示没有盈利笔、没有亏损笔，或亏损合计为 0——此时
 * **不能**显示 0 或 ∞。现有字段足以区分前两种原因，因此给出精确文案：
 * - `total === 0` → 无交易记录
 * - `win_rate === 0` → 无盈利笔
 * - `win_rate === 1` → 无亏损笔
 * - 其余（理论上只剩「亏损合计为 0」）→ 无法计算
 */
export function profitFactorText(
  stats: Pick<ReviewStats, "profit_factor" | "total" | "win_rate">,
): string {
  const profitFactor = stats.profit_factor;
  if (profitFactor !== null && Number.isFinite(profitFactor)) {
    return profitFactor.toFixed(2);
  }
  if (stats.total === 0) return S.review.result.profitFactorNoTrades;
  if (stats.win_rate === 0) return S.review.result.profitFactorNoWins;
  if (stats.win_rate === 1) return S.review.result.profitFactorNoLosses;
  return S.review.result.profitFactorUnavailable;
}

/** 胜率（小数）→ 百分比；非有限数回落到「数据不可得」 */
export function winRateText(winRate: number): string {
  if (!Number.isFinite(winRate)) return S.live.na;
  return formatPct(winRate) ?? S.live.na;
}

/** 费用侵蚀（小数）→ 百分比；`null` 表示无已实现盈亏，明确说明而非显示 0 */
export function feeDragText(feeDrag: number | null): string {
  if (feeDrag === null || !Number.isFinite(feeDrag)) {
    return S.review.result.feeDragUnavailable;
  }
  return formatPct(feeDrag) ?? S.review.result.feeDragUnavailable;
}

/** 费用侵蚀是否高到值得警示（≥ 20% 的利润被成本吃掉） */
export function feeDragIsHigh(feeDrag: number | null): boolean {
  return feeDrag !== null && Number.isFinite(feeDrag) && feeDrag >= 0.2;
}

/** 带符号美元金额：正数显式加 `+`，负数由 `formatUsd` 带 `-` */
export function formatSignedUsd(value: number | null): string | null {
  if (value === null || !Number.isFinite(value)) return null;
  const text = formatUsd(value);
  if (text === null) return null;
  return value > 0 ? `+${text}` : text;
}

/** 盈亏配色：正绿负红，零/不可得中性 */
export type PnlTone = "up" | "down" | "flat";

export function pnlTone(value: number): PnlTone {
  if (!Number.isFinite(value) || value === 0) return "flat";
  return value > 0 ? "up" : "down";
}

export const PNL_TONE_CLASS: Record<PnlTone, string> = {
  up: "text-emerald-400",
  down: "text-red-400",
  flat: "text-neutral-400",
};

/** 平均持仓时长：天/小时/分钟；非法输入回落到「不足 1 秒」 */
export function formatHoldDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return S.review.duration.underSecond;
  const days = Math.floor(ms / 86_400_000);
  const hours = Math.floor((ms % 86_400_000) / 3_600_000);
  const minutes = Math.floor((ms % 3_600_000) / 60_000);
  if (days > 0) {
    return hours > 0 ? S.review.hold.daysHours(days, hours) : S.review.hold.days(days);
  }
  if (hours > 0) {
    return minutes > 0
      ? S.review.hold.hoursMinutes(hours, minutes)
      : S.review.hold.hours(hours);
  }
  return formatDurationMs(ms);
}

/**
 * 开仓时刻市场状态的展示视图。
 *
 * `regime` 为 `null` 时**不隐藏**，而是给出「开仓时状态不可得」；
 * 单个字段为 `null`（如旧仓位的多空比）也在该字段上显示「数据不可得」。
 */
export interface RegimeView {
  available: boolean;
  /** null 表示该字段不可得 */
  trend: string | null;
  vol: string | null;
  crowding: string | null;
  change24h: string | null;
  /** regime 整体不可得时的说明文案 */
  unavailableNote: string | null;
}

export function regimeView(regime: RegimeSnapshot | null): RegimeView {
  if (regime === null) {
    return {
      available: false,
      trend: null,
      vol: null,
      crowding: null,
      change24h: null,
      unavailableNote: S.review.result.regimeUnavailable,
    };
  }
  return {
    available: true,
    trend: regime.trend === null ? null : S.live.trend[regime.trend],
    vol: regime.vol === null ? null : S.live.vol[regime.vol],
    crowding: regime.crowding === null ? null : S.live.crowding[regime.crowding],
    change24h: formatSignedPct(regime.change_24h),
    unavailableNote: null,
  };
}

/** 持仓极值（最大浮盈/浮亏）：`null` 说明只有本地留痕能给，明确写出原因 */
export function extremeText(value: number | null): string {
  const text = formatSignedUsd(value);
  return text ?? S.review.result.extremeUnavailable;
}
