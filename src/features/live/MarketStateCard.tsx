/**
 * 单标的市场状态卡片。
 *
 * 展示顺序刻意固定：身份与价格 → 状态徽章 → 指标网格 → 触发信号（为什么）
 * → 数据不可得（缺什么、为什么缺）。
 *
 * 架构铁律 4：任何拿不到的指标都走 `orUnavailable()` 显示「数据不可得」，
 * 不留空白——空白会被读成「没有风险」。
 */
import { S } from "../../lib/strings";
import type { MarketState } from "../../lib/types";
import {
  formatLocalTime,
  formatPct,
  formatPrice,
  formatRatio,
  formatSignedPct,
  formatUsd,
  orUnavailable,
} from "./format";
import { RegimeBadges } from "./RegimeBadges";

type ValueTone = "up" | "down" | "plain" | "muted";

const TONE_TEXT: Record<ValueTone, string> = {
  up: "text-emerald-400",
  down: "text-red-400",
  plain: "text-neutral-100",
  muted: "text-neutral-500",
};

interface Metric {
  label: string;
  /** null 表示数据不可得 */
  value: string | null;
  tone?: ValueTone;
}

/** 带符号数值（涨跌、资金费率、基差）按正负着色；null 视为不可得 */
function signedTone(value: number | null): ValueTone {
  if (value === null || !Number.isFinite(value)) return "muted";
  if (value > 0) return "up";
  if (value < 0) return "down";
  return "plain";
}

function MetricCell({ metric }: { metric: Metric }) {
  const tone: ValueTone = metric.value === null ? "muted" : (metric.tone ?? "plain");
  return (
    <div className="min-w-0">
      <div className="truncate text-xs text-neutral-500">{metric.label}</div>
      <div className={`font-mono text-sm tabular-nums ${TONE_TEXT[tone]}`}>
        {orUnavailable(metric.value)}
      </div>
    </div>
  );
}

export function MarketStateCard({ state }: { state: MarketState }) {
  const metrics: Metric[] = [
    { label: S.live.labels.high24h, value: formatPrice(state.high_24h) },
    { label: S.live.labels.low24h, value: formatPrice(state.low_24h) },
    { label: S.live.labels.volume24h, value: formatUsd(state.volume_24h_usd) },
    {
      label: S.live.labels.fundingRate,
      value: formatSignedPct(state.funding_rate, 4),
      tone: signedTone(state.funding_rate),
    },
    {
      label: S.live.labels.fundingAnnualized,
      value: formatSignedPct(state.funding_annualized),
      tone: signedTone(state.funding_annualized),
    },
    {
      label: S.live.labels.nextFundingRate,
      value: formatSignedPct(state.next_funding_rate, 4),
      tone: signedTone(state.next_funding_rate),
    },
    { label: S.live.labels.nextFundingTime, value: formatLocalTime(state.next_funding_time) },
    { label: S.live.labels.openInterest, value: formatUsd(state.open_interest_usd) },
    {
      label: S.live.labels.basis,
      value: formatSignedPct(state.basis_pct),
      tone: signedTone(state.basis_pct),
    },
    { label: S.live.labels.longShortRatio, value: formatRatio(state.long_short_ratio) },
    { label: S.live.labels.takerBuySellRatio, value: formatRatio(state.taker_buy_sell_ratio) },
    { label: S.live.labels.ema20, value: formatPrice(state.ema20) },
    { label: S.live.labels.ema60, value: formatPrice(state.ema60) },
    { label: S.live.labels.ema200, value: formatPrice(state.ema200) },
    { label: S.live.labels.rsi14, value: formatRatio(state.rsi14, 1) },
    { label: S.live.labels.atr, value: formatPct(state.atr_pct) },
    { label: S.live.labels.realizedVol, value: formatPct(state.realized_vol) },
  ];

  const changeTone = signedTone(state.change_pct);

  return (
    <article className="flex flex-col gap-4 rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
      <header className="flex items-start justify-between gap-4">
        <h3 className="font-mono text-base font-semibold break-all text-neutral-100">
          {state.inst_id}
        </h3>
        <div className="shrink-0 text-right">
          <div className="text-xs text-neutral-500">{S.live.labels.last}</div>
          <div className="font-mono text-lg tabular-nums text-neutral-100">
            {orUnavailable(formatPrice(state.last))}
          </div>
          <div className="text-xs text-neutral-500">
            {S.live.labels.change24h}{" "}
            <span className={`font-mono tabular-nums ${TONE_TEXT[changeTone]}`}>
              {orUnavailable(formatSignedPct(state.change_pct))}
            </span>
          </div>
        </div>
      </header>

      <RegimeBadges trend={state.trend} vol={state.vol} crowding={state.crowding} />

      <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
        {metrics.map((metric) => (
          <MetricCell key={metric.label} metric={metric} />
        ))}
      </div>

      <section>
        <h4 className="mb-1.5 text-xs font-medium text-neutral-400">
          {S.live.sections.signals}
        </h4>
        {state.signals.length === 0 ? (
          <p className="text-xs text-neutral-600">{S.live.noSignals}</p>
        ) : (
          <ul className="space-y-1">
            {state.signals.map((signal, index) => (
              <li key={index} className="flex gap-2 text-xs text-neutral-300">
                <span aria-hidden="true" className="text-amber-400">
                  •
                </span>
                <span>{signal}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {state.unavailable.length > 0 && (
        <section className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-2.5">
          <h4 className="mb-1.5 text-xs font-medium text-amber-300">
            {S.live.sections.unavailable}
          </h4>
          <ul className="space-y-1">
            {state.unavailable.map((reason, index) => (
              <li key={index} className="text-xs text-amber-200/90">
                {reason}
              </li>
            ))}
          </ul>
        </section>
      )}
    </article>
  );
}
