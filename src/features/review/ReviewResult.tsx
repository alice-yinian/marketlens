/**
 * 复盘结果展示（M4）。
 *
 * 组装顺序刻意与「结论可信度」的重要性对齐：
 *   a) 统计概览（费用侵蚀突出）→ b) 按开仓时市场状态分组（产品核心价值）
 *   → c) 逐笔仓位（带来源/精度角标与归因）→ d) 数据可信度 → e) warnings。
 *
 * 纯展示：所有数值都由 `format.ts` 的纯函数映射，组件不读全局状态、不做计算。
 * 数据不可得一律显式写出原因（架构铁律 4/5）。
 */
import { S } from "../../lib/strings";
import type {
  ClosedPosition,
  ReviewContext,
  StatGroup,
} from "../../lib/types";
import { barLabel } from "./format";
import {
  PNL_TONE_CLASS,
  directionView,
  extremeText,
  feeDragIsHigh,
  feeDragText,
  formatHoldDuration,
  formatSignedUsd,
  pnlTone,
  precisionView,
  profitFactorText,
  regimeView,
  sourceView,
  winRateText,
} from "./format";
import { formatLocalTime, formatPrice } from "../live/format";

function Badge({ view }: { view: { label: string; badge: string } }) {
  return (
    <span
      className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs ${view.badge}`}
    >
      {view.label}
    </span>
  );
}

function Stat({
  label,
  value,
  valueClass = "text-neutral-100",
  hint,
  emphasis = false,
}: {
  label: string;
  value: string;
  valueClass?: string;
  hint?: string;
  emphasis?: boolean;
}) {
  return (
    <div
      className={`rounded-lg border px-3 py-2 ${
        emphasis
          ? "border-amber-800/70 bg-amber-950/20"
          : "border-neutral-800 bg-neutral-900/60"
      }`}
    >
      <div className="text-xs text-neutral-500">{label}</div>
      <div className={`mt-0.5 font-mono text-sm ${valueClass}`}>{value}</div>
      {hint === undefined ? null : (
        <div className="mt-1 text-xs text-neutral-500">{hint}</div>
      )}
    </div>
  );
}

function OverviewSection({ context }: { context: ReviewContext }) {
  const { stats } = context;
  const feeHigh = feeDragIsHigh(stats.fee_drag);

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-medium text-neutral-200">{S.review.result.overviewTitle}</h2>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-6">
        <Stat
          label={S.review.result.winRate}
          value={winRateText(stats.win_rate)}
          hint={S.review.result.groupCount(stats.total)}
        />
        <Stat
          label={S.review.result.profitFactor}
          value={profitFactorText(stats)}
        />
        <Stat
          label={S.review.result.expectancy}
          value={formatSignedUsd(stats.expectancy) ?? S.live.na}
          valueClass={PNL_TONE_CLASS[pnlTone(stats.expectancy)]}
        />
        <Stat
          label={S.review.result.totalPnl}
          value={formatSignedUsd(stats.total_realized_pnl) ?? S.live.na}
          valueClass={PNL_TONE_CLASS[pnlTone(stats.total_realized_pnl)]}
        />
        <Stat
          label={S.review.result.avgHold}
          value={formatHoldDuration(stats.avg_hold_ms)}
        />
        {/* 费用侵蚀是复盘里最容易被忽略、又最容易改进的一项，刻意突出 */}
        <Stat
          label={S.review.result.feeDrag}
          value={feeDragText(stats.fee_drag)}
          valueClass={feeHigh ? "text-amber-300" : "text-neutral-100"}
          hint={S.review.result.feeDragHint}
          emphasis
        />
      </div>
      {feeHigh ? (
        <p className="text-xs text-amber-300">{S.review.result.feeDragWarning}</p>
      ) : null}
    </section>
  );
}

function GroupRow({ group }: { group: StatGroup }) {
  const unattributed = group.is_unattributed;
  return (
    <tr className={unattributed ? "bg-neutral-900/60" : undefined}>
      <td className="px-3 py-2">
        <span className={unattributed ? "text-neutral-400" : "text-neutral-200"}>
          {group.key.length > 0 ? group.key : S.review.result.groupUnattributed}
        </span>
        {unattributed ? (
          <span className="ml-2 text-xs text-amber-300/90">
            {S.review.result.groupUnattributedNote}
          </span>
        ) : null}
      </td>
      <td className="px-3 py-2 font-mono text-neutral-300">{group.count}</td>
      <td className="px-3 py-2 font-mono text-neutral-300">{winRateText(group.win_rate)}</td>
      <td
        className={`px-3 py-2 font-mono ${PNL_TONE_CLASS[pnlTone(group.total_pnl)]}`}
      >
        {formatSignedUsd(group.total_pnl) ?? S.live.na}
      </td>
    </tr>
  );
}

function TrendSection({ groups }: { groups: StatGroup[] }) {
  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-sm font-medium text-neutral-200">{S.review.result.trendTitle}</h2>
        <p className="mt-1 text-xs text-neutral-500">{S.review.result.trendHint}</p>
      </div>
      {groups.length === 0 ? (
        <p className="text-sm text-neutral-400">{S.review.result.empty}</p>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-neutral-800">
          <table className="w-full text-sm">
            <thead className="bg-neutral-900/80 text-xs text-neutral-500">
              <tr>
                <th className="px-3 py-2 text-left font-medium">
                  {S.review.result.colGroup}
                </th>
                <th className="px-3 py-2 text-left font-medium">
                  {S.review.result.colCount}
                </th>
                <th className="px-3 py-2 text-left font-medium">
                  {S.review.result.colWinRate}
                </th>
                <th className="px-3 py-2 text-left font-medium">{S.review.result.colPnl}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-neutral-800">
              {groups.map((group) => (
                <GroupRow key={group.key} group={group} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function Field({ label, value, valueClass }: { label: string; value: string; valueClass?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-xs text-neutral-500">{label}</span>
      <span className={`font-mono text-xs ${valueClass ?? "text-neutral-200"}`}>{value}</span>
    </div>
  );
}

function RegimeBlock({ position }: { position: ClosedPosition }) {
  const regime = regimeView(position.regime);
  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900/40 p-3">
      <div className="text-xs font-medium text-neutral-400">
        {S.review.result.regimeTitle}
      </div>
      {regime.available ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          <Badge
            view={{
              label: regime.trend ?? S.live.na,
              badge: "border-neutral-700 bg-neutral-800/60 text-neutral-300",
            }}
          />
          <Badge
            view={{
              label: regime.vol ?? S.live.na,
              badge: "border-neutral-700 bg-neutral-800/60 text-neutral-300",
            }}
          />
          <Badge
            view={{
              label: regime.crowding ?? S.live.na,
              badge: "border-neutral-700 bg-neutral-800/60 text-neutral-300",
            }}
          />
        </div>
      ) : (
        <p className="mt-1 text-xs text-amber-300/90">{regime.unavailableNote}</p>
      )}
      <div className="mt-2">
        <Field
          label={S.review.result.change24h}
          value={regime.change24h ?? S.live.na}
          valueClass={
            regime.change24h === null
              ? "text-neutral-500"
              : regime.change24h.startsWith("-")
                ? PNL_TONE_CLASS.down
                : PNL_TONE_CLASS.up
          }
        />
      </div>
    </div>
  );
}

function PositionCard({ position }: { position: ClosedPosition }) {
  const source = sourceView(position.source);
  const precision = precisionView(position.time_precision);
  const direction = directionView(position.direction);

  return (
    <article
      data-testid="position-card"
      className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4"
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-sm text-neutral-100">{position.inst_id}</span>
        <Badge view={direction} />
        <Badge view={source} />
        <Badge view={precision} />
        <span className="text-xs text-neutral-500">
          {S.review.result.lever} {position.lever}×
        </span>
        <span
          className={`ml-auto font-mono text-sm ${PNL_TONE_CLASS[pnlTone(position.realized_pnl)]}`}
        >
          {formatSignedUsd(position.realized_pnl) ?? S.live.na}
        </span>
      </div>

      <div className="grid gap-x-6 gap-y-1.5 sm:grid-cols-2">
        <Field
          label={S.review.result.openTime}
          value={formatLocalTime(position.open_time) ?? S.live.na}
        />
        <Field
          label={S.review.result.closeTime}
          value={formatLocalTime(position.close_time) ?? S.live.na}
        />
        <Field
          label={S.review.result.openPx}
          value={formatPrice(position.open_avg_px) ?? S.live.na}
        />
        <Field
          label={S.review.result.closePx}
          value={formatPrice(position.close_avg_px) ?? S.live.na}
        />
        <Field label={S.review.result.realizedPnl} value={
          formatSignedUsd(position.realized_pnl) ?? S.live.na
        } />
        <Field label={S.review.result.fee} value={formatSignedUsd(position.fee) ?? S.live.na} />
        <Field
          label={S.review.result.fundingFee}
          value={formatSignedUsd(position.funding_fee) ?? S.live.na}
        />
      </div>

      <RegimeBlock position={position} />

      <div className="rounded-lg border border-neutral-800 bg-neutral-900/40 p-3">
        <div className="text-xs font-medium text-neutral-400">
          {S.review.result.extremeTitle}
        </div>
        <div className="mt-2 grid gap-x-6 gap-y-1.5 sm:grid-cols-2">
          <Field
            label={S.review.result.maxFavorable}
            value={extremeText(position.max_favorable)}
            valueClass={
              position.max_favorable === null ? "text-neutral-500" : PNL_TONE_CLASS.up
            }
          />
          <Field
            label={S.review.result.maxAdverse}
            value={extremeText(position.max_adverse)}
            valueClass={
              position.max_adverse === null ? "text-neutral-500" : PNL_TONE_CLASS.down
            }
          />
        </div>
      </div>
    </article>
  );
}

function TrustSection({ context }: { context: ReviewContext }) {
  const { merge, trace, stats } = context;
  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.review.result.trustTitle}</h2>

      <div>
        <div className="text-xs text-neutral-500">{S.review.result.mergeLabel}</div>
        <p className="mt-0.5 text-xs text-neutral-300">
          {S.review.result.mergeCounts(
            merge.from_official,
            merge.from_local,
            merge.enriched,
          )}
        </p>
        <p className="mt-1 text-xs break-all text-neutral-400">{merge.coverage_note}</p>
      </div>

      <div>
        <div className="text-xs text-neutral-500">{S.review.result.traceLabel}</div>
        <p className="mt-1 text-xs break-all text-neutral-400">{trace.note}</p>
      </div>

      {stats.unattributed > 0 ? (
        <p className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-2 text-xs text-amber-200">
          {S.review.result.unattributedNote(stats.unattributed)}
        </p>
      ) : null}
    </section>
  );
}

function WarningsPanel({ warnings }: { warnings: string[] }) {
  if (warnings.length === 0) return null;
  return (
    <section className="rounded-xl border border-amber-900/60 bg-amber-950/30 p-4">
      <h2 className="text-sm font-medium text-amber-300">{S.review.result.warningsTitle}</h2>
      <p className="mt-1 text-xs text-amber-200/70">{S.review.result.warningsHint}</p>
      <ul className="mt-2 space-y-1.5">
        {warnings.map((warning, index) => (
          <li key={index} className="text-xs break-all text-amber-200/90">
            {warning}
          </li>
        ))}
      </ul>
    </section>
  );
}

export function ReviewResult({ context }: { context: ReviewContext }) {
  return (
    <section className="flex flex-col gap-5">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-base font-semibold text-neutral-100">{S.review.result.title}</h2>
        <span className="flex flex-wrap gap-x-3 text-xs text-neutral-500">
          <span>
            {S.review.result.range(
              `${formatLocalTime(context.from) ?? S.live.na} → ${
                formatLocalTime(context.to) ?? S.live.na
              }`,
            )}
          </span>
          <span>{S.review.result.bar(barLabel(context.bar))}</span>
          <span>{S.review.result.positionsCount(context.positions.length)}</span>
        </span>
      </div>

      <WarningsPanel warnings={context.warnings} />

      {context.stats.total === 0 ? (
        <section className="rounded-xl border border-dashed border-neutral-800 bg-neutral-900/40 p-8 text-center">
          <p className="text-sm text-neutral-300">{S.review.result.empty}</p>
          <p className="mt-1 text-xs text-neutral-500">{S.review.result.emptyHint}</p>
        </section>
      ) : (
        <>
          <OverviewSection context={context} />
          <TrendSection groups={context.stats.by_trend} />
          <section className="flex flex-col gap-3">
            <h2 className="text-sm font-medium text-neutral-200">
              {S.review.result.positionsTitle}
            </h2>
            <div className="grid gap-4 xl:grid-cols-2">
              {context.positions.map((position) => (
                <PositionCard key={position.pos_id} position={position} />
              ))}
            </div>
          </section>
        </>
      )}

      <TrustSection context={context} />
    </section>
  );
}
