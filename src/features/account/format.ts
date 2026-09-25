/**
 * 账户 / 仓位的纯格式化与标签映射：无副作用、不读全局状态。
 *
 * 单位约定沿用 `lib/types.ts`：金额字段是美元，`upl_ratio` / `mgn_ratio` 是小数比例。
 * 通用的价格 / 百分比格式化直接复用 M1 的 `features/live/format`，
 * 这里只补账户场景特有的部分（完整两位小数的金额、张数、Rust 字符串 → 中文）。
 */
import { S } from "../../lib/strings";

/** 数值是否可用于展示（null / NaN / Infinity 都算不可得） */
function isRenderable(value: number | null): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function groupThousands(intPart: string): string {
  return intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * 美元金额：完整两位小数 + 千分位。
 *
 * 刻意不做 K / M 缩写（与 `live/format.formatUsd` 不同）：账户权益、保证金这类数字
 * 需要精确值，缩写会把 12,345 和 12,999 都显示成 $12.34K。
 */
export function formatAmount(value: number | null): string | null {
  if (!isRenderable(value)) return null;
  const sign = value < 0 ? "-" : "";
  const [intPart = "0", frac = "00"] = Math.abs(value).toFixed(2).split(".");
  return `${sign}$${groupThousands(intPart)}.${frac}`;
}

/** 张数：整数带千分位；小数最多 4 位并去掉尾随 0 */
export function formatContracts(value: number | null): string | null {
  if (!isRenderable(value)) return null;
  if (Number.isInteger(value)) return groupThousands(String(value));
  return value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
}

/** 杠杆：10 → "10x" */
export function formatLever(lever: number | null): string | null {
  if (!isRenderable(lever)) return null;
  return `${lever}x`;
}

const POS_MODE_LABELS: Record<string, string> = {
  net_mode: S.account.posMode.net_mode,
  long_short_mode: S.account.posMode.long_short_mode,
};

const POS_SIDE_LABELS: Record<string, string> = {
  long: S.account.posSide.long,
  short: S.account.posSide.short,
  net: S.account.posSide.net,
};

const MGN_MODE_LABELS: Record<string, string> = {
  cross: S.account.mgnMode.cross,
  isolated: S.account.mgnMode.isolated,
};

/** `net_mode` | `long_short_mode` → 中文；未知值不静默留空 */
export function posModeLabel(mode: string | null): string | null {
  if (mode === null || mode.length === 0) return null;
  return POS_MODE_LABELS[mode] ?? S.account.unknown;
}

/** `long` | `short` | `net` → 中文 */
export function posSideLabel(side: string | null): string | null {
  if (side === null || side.length === 0) return null;
  return POS_SIDE_LABELS[side] ?? S.account.unknown;
}

/** `cross` | `isolated` → 全仓 / 逐仓 */
export function mgnModeLabel(mode: string | null): string | null {
  if (mode === null || mode.length === 0) return null;
  return MGN_MODE_LABELS[mode] ?? S.account.unknown;
}

export type ValueTone = "up" | "down" | "plain" | "muted";

/** 带符号数值按正负着色；null / 非有限数视为不可得 */
export function signedTone(value: number | null): ValueTone {
  if (!isRenderable(value)) return "muted";
  if (value > 0) return "up";
  if (value < 0) return "down";
  return "plain";
}

/** 各 tone 对应的文字颜色类 */
export const TONE_TEXT: Record<ValueTone, string> = {
  up: "text-emerald-400",
  down: "text-red-400",
  plain: "text-neutral-100",
  muted: "text-neutral-500",
};
