/**
 * 纯格式化函数：不含任何副作用、不读全局状态，输入相同则输出相同
 * （相对时间显式接收 `now`，因此可以在单元测试里固定时间）。
 *
 * 单位约定（见 lib/types.ts）：
 * - `change_pct` / `basis_pct` / `atr_pct` / `realized_vol` 是小数比例 → ×100 后带 %。
 * - `volume_24h_usd` / `open_interest_usd` 是美元金额 → K/M/B 缩写。
 * - 毫秒时间戳 → 本地时间 / 相对时间。
 *
 * 返回 `null` 表示数据不可得；调用方用 `orUnavailable()` 转成「数据不可得」，
 * 绝不静默留空（架构铁律 4）。
 */
import { S } from "../../lib/strings";

/** 数值是否可用于展示（null / NaN / Infinity 都算不可得） */
function isRenderable(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function groupThousands(intPart: string): string {
  return intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** 美元金额缩写：$1.23B / $456.78M / $12.34K / $9.99 */
export function formatUsd(value: number | null): string | null {
  if (!isRenderable(value)) return null;
  const sign = value < 0 ? "-" : "";
  const abs = Math.abs(value);
  if (abs >= 1e9) return `${sign}$${(abs / 1e9).toFixed(2)}B`;
  if (abs >= 1e6) return `${sign}$${(abs / 1e6).toFixed(2)}M`;
  if (abs >= 1e3) return `${sign}$${(abs / 1e3).toFixed(2)}K`;
  return `${sign}$${abs.toFixed(2)}`;
}

/** 价格：按量级自适应小数位，带千分位（0.00001234 这类小币价不会退化成 0.00） */
export function formatPrice(value: number | null): string | null {
  if (!isRenderable(value)) return null;
  const abs = Math.abs(value);
  const digits = abs >= 1 ? 2 : abs >= 0.01 ? 4 : 6;
  const [intPart = "0", frac] = abs.toFixed(digits).split(".");
  const sign = value < 0 ? "-" : "";
  const grouped = groupThousands(intPart);
  return frac === undefined ? `${sign}${grouped}` : `${sign}${grouped}.${frac}`;
}

/** 小数比例 → 百分比，带符号：0.0123 → "+1.23%" */
export function formatSignedPct(ratio: number | null, digits = 2): string | null {
  if (!isRenderable(ratio)) return null;
  const pct = ratio * 100;
  const sign = pct > 0 ? "+" : pct < 0 ? "-" : "";
  return `${sign}${Math.abs(pct).toFixed(digits)}%`;
}

/** 小数比例 → 百分比，不带符号：0.0123 → "1.23%" */
export function formatPct(ratio: number | null, digits = 2): string | null {
  if (!isRenderable(ratio)) return null;
  return `${(ratio * 100).toFixed(digits)}%`;
}

/** 纯比值（多空账户比 / 主动买卖比 / RSI）：1.24 → "1.24" */
export function formatRatio(value: number | null, digits = 2): string | null {
  if (!isRenderable(value)) return null;
  return value.toFixed(digits);
}

/** Unix 毫秒 → 本地时间 "2026-09-25 14:30"（不含时区后缀，界面按本机时区显示） */
export function formatLocalTime(ms: number): string;
export function formatLocalTime(ms: null): null;
export function formatLocalTime(ms: number | null): string | null;
export function formatLocalTime(ms: number | null): string | null {
  if (!isRenderable(ms)) return null;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return null;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

/**
 * Unix 毫秒 → 相对时间「X 分钟前」。
 *
 * `now` 显式传入：既保证函数纯粹（可测试），也让界面能在时钟滴答时重新渲染，
 * 避免「缓存于 3 分钟前」永远停在 3 分钟前。
 */
export function formatRelativeTime(ms: number, now: number): string {
  if (!Number.isFinite(ms) || !Number.isFinite(now)) return S.live.na;
  const diff = now - ms;
  if (diff < 1000) return S.live.time.justNow;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return S.live.time.secondsAgo(seconds);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return S.live.time.minutesAgo(minutes);
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return S.live.time.hoursAgo(hours);
  return S.live.time.daysAgo(Math.floor(hours / 24));
}

/** 可选数值的统一出口：拿不到就明确写「数据不可得」，绝不留空 */
export function orUnavailable(text: string | null): string {
  return text ?? S.live.na;
}
