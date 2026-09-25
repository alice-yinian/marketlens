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
import type { SeriesKind, SeriesStatus } from "../../lib/types";

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
