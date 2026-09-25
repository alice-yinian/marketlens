/**
 * 采集计划预览：预估请求数 / 耗时、标的集与粒度、以及**逐条**的可得性提示。
 *
 * `warnings` 是产品可信度的核心——用户必须知道哪些数据在本时段拿不到，
 * 所以用醒目的琥珀色面板逐条展示，而不是折叠或缩略。
 */
import { S } from "../../lib/strings";
import type { FetchPlan } from "../../lib/types";
import { barLabel, formatDurationMs } from "./format";

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900/60 px-3 py-2">
      <div className="text-xs text-neutral-500">{label}</div>
      <div className="mt-0.5 font-mono text-sm text-neutral-100">{value}</div>
    </div>
  );
}

export function PlanPreview({
  plan,
  onStart,
  disabled,
}: {
  plan: FetchPlan;
  onStart: () => void;
  disabled: boolean;
}) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium text-neutral-200">{S.review.plan.title}</h2>
        <span className="text-xs text-neutral-500">{S.review.plan.offlineNote}</span>
      </div>

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat
          label={S.review.plan.estRequests}
          value={S.review.plan.estRequestsValue(plan.est_requests)}
        />
        <Stat
          label={S.review.plan.estDuration}
          value={formatDurationMs(plan.est_duration_ms)}
        />
        <Stat
          label={S.review.plan.insts}
          value={S.review.plan.instsCount(plan.inst_ids.length)}
        />
        <Stat label={S.review.plan.barLabel} value={barLabel(plan.bar)} />
      </div>

      <div>
        <div className="text-xs text-neutral-500">{S.review.plan.seriesCount(plan.series.length)}</div>
        <div className="mt-1 flex flex-wrap gap-1.5">
          {plan.inst_ids.map((instId) => (
            <span
              key={instId}
              className="rounded border border-neutral-800 bg-neutral-900 px-2 py-0.5 font-mono text-xs text-neutral-300"
            >
              {instId}
            </span>
          ))}
        </div>
      </div>

      {plan.warnings.length === 0 ? null : (
        <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
          <h3 className="text-sm font-medium text-amber-300">{S.review.plan.warningsTitle}</h3>
          <p className="mt-1 text-xs text-amber-200/70">{S.review.plan.warningsHint}</p>
          <ul className="mt-2 space-y-1.5">
            {plan.warnings.map((note, index) => (
              <li key={index} className="text-xs">
                <span className="font-medium text-amber-200">{note.metric}</span>
                <span className="ml-1.5 break-all text-amber-200/80">{note.reason}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div>
        <button
          type="button"
          onClick={onStart}
          disabled={disabled}
          className="rounded-md bg-emerald-800 px-4 py-2 text-sm font-medium text-emerald-50 hover:bg-emerald-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {S.review.plan.start}
        </button>
      </div>
    </section>
  );
}
