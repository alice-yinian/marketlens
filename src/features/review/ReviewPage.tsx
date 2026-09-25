/**
 * 复盘采集页（M3）。
 *
 * 流程：选时段与粒度 → 「生成计划」（**不联网**，只看预估与可得性）
 * → 确认后「开始采集」→ 进度条 + 取消 → 结果逐条展示。
 *
 * 页面本身不含业务逻辑：数据全部来自 Rust（`review_plan` / `review_fetch`），
 * 进度来自 `fetch://progress` 事件（经 `lib/ipc` 封装），文案全部来自 `lib/strings.ts`。
 */
import { useState } from "react";

import { ErrorPanel } from "../account/ErrorPanel";
import { errorPayloadOf } from "../live/errors";
import { S } from "../../lib/strings";
import type { ExecutionReport } from "../../lib/types";
import { PlanPreview } from "./PlanPreview";
import { ProgressBar } from "./ProgressBar";
import { RangePicker } from "./RangePicker";
import { SeriesReportList } from "./SeriesReportList";
import { formatDurationMs } from "./format";
import { useReviewFetch } from "./useReviewFetch";
import { useReviewPlan } from "./useReviewPlan";

const DAY_MS = 24 * 60 * 60 * 1000;

function RangeTooLargePanel() {
  return (
    <section className="rounded-xl border border-amber-900/60 bg-amber-950/30 p-4">
      <h2 className="text-sm font-medium text-amber-300">{S.review.errors.rangeTooLargeTitle}</h2>
      <p className="mt-2 text-xs text-amber-200/90">{S.review.errors.rangeTooLarge}</p>
    </section>
  );
}

function ReportSection({ report }: { report: ExecutionReport }) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium text-neutral-200">{S.review.report.title}</h2>
        <span className="flex flex-wrap gap-x-3 text-xs text-neutral-500">
          <span>{S.review.report.elapsed(formatDurationMs(report.elapsed_ms))}</span>
          <span>{S.review.report.doneRequests(report.done_requests)}</span>
        </span>
      </div>
      {report.cancelled ? (
        <p className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3 text-xs text-amber-200">
          {S.review.report.cancelled}
        </p>
      ) : null}
      <SeriesReportList reports={report.series} />
    </section>
  );
}

export function ReviewPage() {
  const [range, setRange] = useState(() => {
    const now = Date.now();
    return { from: now - 7 * DAY_MS, to: now, bar: "1H" };
  });

  const planMutation = useReviewPlan();
  const fetchState = useReviewFetch();

  const plan = planMutation.data ?? null;
  const planError = planMutation.error;

  const handleGenerate = () => {
    fetchState.reset();
    planMutation.mutate({ from: range.from, to: range.to, bar: range.bar });
  };

  const payload = planError == null ? null : errorPayloadOf(planError);

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
      <header>
        <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{S.review.title}</h1>
        <p className="mt-0.5 text-sm text-neutral-400">{S.review.subtitle}</p>
      </header>

      <RangePicker
        from={range.from}
        to={range.to}
        bar={range.bar}
        onChange={(next) => setRange((prev) => ({ ...prev, ...next }))}
        onGenerate={handleGenerate}
        generating={planMutation.isPending}
        disabled={fetchState.isFetching}
      />

      {payload === null ? null : payload.code === "RangeTooLarge" ? (
        <RangeTooLargePanel />
      ) : (
        <ErrorPanel
          title={S.review.errors.planTitle}
          error={planError}
          onRetry={handleGenerate}
          busy={planMutation.isPending}
        />
      )}

      {plan === null ? null : (
        <PlanPreview
          plan={plan}
          onStart={() => fetchState.start(plan.id)}
          disabled={fetchState.isFetching}
        />
      )}

      {fetchState.isFetching ? (
        <ProgressBar
          progress={fetchState.progress}
          cancelling={fetchState.isCancelling}
          cancelNote={fetchState.cancelNote}
          onCancel={() => {
            if (plan !== null) fetchState.cancel(plan.id);
          }}
        />
      ) : null}

      {fetchState.error == null ? null : (
        <ErrorPanel
          title={S.review.errors.fetchTitle}
          error={fetchState.error}
          onRetry={plan === null ? undefined : () => fetchState.start(plan.id)}
          busy={fetchState.isFetching}
        />
      )}

      {fetchState.report === null ? null : <ReportSection report={fetchState.report} />}
    </main>
  );
}
