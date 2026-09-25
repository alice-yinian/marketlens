/**
 * 单个持仓卡片。
 *
 * 刻意同时展示 `contracts`（张数）与 `size_base`（币数量）：换算过程容易出错，
 * 两个数都摆出来，用户才能一眼核对换算是否合理（`size_base` 为 null 表示合约信息没取到）。
 * 强平价为 null 表示无强平价（全仓模式常见）→ 显示「无」，而不是「数据不可得」。
 */
import { S } from "../../lib/strings";
import type { Position } from "../../lib/types";
import { formatPct, formatPrice, formatSignedPct } from "../live/format";
import {
  TONE_TEXT,
  formatAmount,
  formatContracts,
  formatLever,
  mgnModeLabel,
  posSideLabel,
  signedTone,
  type ValueTone,
} from "./format";

interface Metric {
  label: string;
  value: string | null;
  tone?: ValueTone;
}

function MetricCell({ metric }: { metric: Metric }) {
  const tone: ValueTone = metric.value === null ? "muted" : (metric.tone ?? "plain");
  return (
    <div className="min-w-0">
      <div className="truncate text-xs text-neutral-500">{metric.label}</div>
      <div className={`font-mono text-sm tabular-nums ${TONE_TEXT[tone]}`}>
        {metric.value ?? S.account.na}
      </div>
    </div>
  );
}

export function PositionCard({ position }: { position: Position }) {
  const uplTone = signedTone(position.upl);
  const metrics: Metric[] = [
    { label: S.account.position.side, value: posSideLabel(position.pos_side) },
    { label: S.account.position.mgnMode, value: mgnModeLabel(position.mgn_mode) },
    { label: S.account.position.lever, value: formatLever(position.lever) },
    { label: S.account.position.contracts, value: formatContracts(position.contracts) },
    { label: S.account.position.sizeBase, value: formatPrice(position.size_base) },
    { label: S.account.position.avgPx, value: formatPrice(position.avg_px) },
    { label: S.account.position.markPx, value: formatPrice(position.mark_px) },
    {
      label: S.account.position.liqPx,
      value: position.liq_px === null ? S.account.liqNone : formatPrice(position.liq_px),
    },
    { label: S.account.position.notional, value: formatAmount(position.notional_usd) },
    { label: S.account.position.upl, value: formatAmount(position.upl), tone: uplTone },
    {
      label: S.account.position.uplRatio,
      value: formatSignedPct(position.upl_ratio),
      tone: uplTone,
    },
    { label: S.account.position.mgnRatio, value: formatPct(position.mgn_ratio) },
  ];

  return (
    <article className="flex flex-col gap-4 rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
      <header className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h3 className="font-mono text-base font-semibold break-all text-neutral-100">
            {position.inst_id}
          </h3>
          <p className="mt-0.5 text-xs text-neutral-500">
            {posSideLabel(position.pos_side) ?? S.account.na}
            <span className="mx-2">·</span>
            {mgnModeLabel(position.mgn_mode) ?? S.account.na}
            <span className="mx-2">·</span>
            {formatLever(position.lever) ?? S.account.na}
          </p>
        </div>
        <div className="shrink-0 text-right">
          <div className="text-xs text-neutral-500">{S.account.position.upl}</div>
          <div className={`font-mono text-lg tabular-nums ${TONE_TEXT[uplTone]}`}>
            {formatAmount(position.upl) ?? S.account.na}
          </div>
          <div className={`font-mono text-xs tabular-nums ${TONE_TEXT[uplTone]}`}>
            {formatSignedPct(position.upl_ratio) ?? S.account.na}
          </div>
        </div>
      </header>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
        {metrics.map((metric) => (
          <MetricCell key={metric.label} metric={metric} />
        ))}
      </div>
    </article>
  );
}
