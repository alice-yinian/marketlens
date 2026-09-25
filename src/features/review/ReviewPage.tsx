/**
 * 复盘页（M3 采集 + M4 装配）。
 *
 * 流程：选时段与粒度 → 「生成计划」（**不联网**，只看预估与可得性）
 * → 确认后「开始采集」→ 进度条 + 取消 → 采集结果 → 「装配复盘」（`review_context`）
 * → 统计 / 分组 / 逐笔 / 可信度 / warnings。
 *
 * 页面本身不含业务逻辑：数据全部来自 Rust（`review_plan` / `review_fetch` /
 * `review_context`），进度来自 `fetch://progress` 事件（经 `lib/ipc` 封装），
 * 文案全部来自 `lib/strings.ts`。
 *
 * 装配**不依赖采集是否跑过**：没采集也能用已落库数据装配（缺失项会出现在
 * warnings 里），所以装配区始终可见，只是会提示「先采集能拿到更完整的数据」。
 */
import { useState } from "react";
import type { UseQueryResult } from "@tanstack/react-query";

import { ErrorPanel } from "../account/ErrorPanel";
import { useCredentials } from "../account/useAccountSnapshot";
import { errorPayloadOf } from "../live/errors";
import { S } from "../../lib/strings";
import type { CredentialMeta, ExecutionReport, ReviewContext } from "../../lib/types";
import { PlanPreview } from "./PlanPreview";
import { ProgressBar } from "./ProgressBar";
import { RangePicker } from "./RangePicker";
import { ReviewResult } from "./ReviewResult";
import { SeriesReportList } from "./SeriesReportList";
import { formatDurationMs, rangeError } from "./format";
import { useReviewContext } from "./useReviewContext";
import { useReviewFetch } from "./useReviewFetch";
import { useReviewPlan } from "./useReviewPlan";

const DAY_MS = 24 * 60 * 60 * 1000;

function RangeTooLargePanel({ message }: { message: string }) {
  return (
    <section className="rounded-xl border border-amber-900/60 bg-amber-950/30 p-4">
      <h2 className="text-sm font-medium text-amber-300">{S.review.errors.rangeTooLargeTitle}</h2>
      <p className="mt-2 text-xs text-amber-200/90">{message}</p>
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

/**
 * 装配区：凭据选择 + 装配按钮 + 加载态。
 *
 * 凭据列表读失败时不阻塞整页（采集不依赖凭据），只在这里如实说明。
 */
function AssemblePanel({
  credentialId,
  credentials,
  onPickCredential,
  onRun,
  running,
  canRun,
  onConfigure,
}: {
  credentialId: string | null;
  credentials: UseQueryResult<CredentialMeta[], Error>;
  onPickCredential: (id: string) => void;
  onRun: () => void;
  running: boolean;
  canRun: boolean;
  onConfigure?: () => void;
}) {
  const list = credentials.data ?? [];

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <div>
        <h2 className="text-sm font-medium text-neutral-200">{S.review.assemble.title}</h2>
        <p className="mt-0.5 text-xs text-neutral-400">{S.review.assemble.subtitle}</p>
      </div>
      <p className="rounded-lg border border-sky-900/50 bg-sky-950/20 p-2 text-xs text-sky-200/90">
        {S.review.assemble.note}
      </p>

      {credentials.isPending ? (
        <p className="text-xs text-neutral-500">{S.review.assemble.credentialLoading}</p>
      ) : credentials.isError ? (
        <div className="rounded-lg border border-red-900/60 bg-red-950/30 p-3">
          <p className="text-xs text-red-300">{S.review.assemble.credentialError}</p>
          <button
            type="button"
            onClick={() => void credentials.refetch()}
            disabled={credentials.isFetching}
            className="mt-2 rounded-md bg-red-900/70 px-3 py-1.5 text-xs text-red-100 hover:bg-red-900 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {S.review.assemble.retryCredentials}
          </button>
        </div>
      ) : list.length === 0 ? (
        <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
          <p className="text-xs text-amber-200">{S.review.assemble.needCredential}</p>
          <p className="mt-1 text-xs text-amber-200/80">{S.review.assemble.needCredentialHint}</p>
          {onConfigure === undefined ? null : (
            <button
              type="button"
              onClick={onConfigure}
              className="mt-2 rounded-md bg-neutral-800 px-3 py-1.5 text-xs text-neutral-100 hover:bg-neutral-700"
            >
              {S.review.assemble.addCredential}
            </button>
          )}
        </div>
      ) : list.length === 1 ? (
        <p className="text-xs text-neutral-500">
          {S.account.credential}
          <span className="ml-2 font-mono text-neutral-300">{list[0]?.label}</span>
          <span className="ml-2 font-mono text-neutral-500">{list[0]?.api_key_masked}</span>
        </p>
      ) : (
        <label className="flex flex-wrap items-center gap-2 text-xs text-neutral-400">
          {S.account.credential}
          <select
            className="rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100"
            value={credentialId ?? ""}
            onChange={(event) => onPickCredential(event.target.value)}
          >
            {list.map((meta) => (
              <option key={meta.id} value={meta.id}>
                {`${meta.label} · ${meta.api_key_masked} · ${
                  meta.env === "demo" ? S.account.envDemo : S.account.envLive
                }`}
              </option>
            ))}
          </select>
        </label>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={onRun}
          disabled={!canRun || running}
          className="rounded-md bg-indigo-800 px-4 py-2 text-sm font-medium text-indigo-50 hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {running ? S.review.assemble.running : S.review.assemble.run}
        </button>
        <span className="text-xs text-neutral-500">{S.review.assemble.rangeNote}</span>
      </div>
      {running ? (
        <p className="text-xs text-indigo-300/90">{S.review.assemble.runningHint}</p>
      ) : null}
    </section>
  );
}

export function ReviewPage({ onConfigure }: { onConfigure?: () => void } = {}) {
  const [range, setRange] = useState(() => {
    const now = Date.now();
    return { from: now - 7 * DAY_MS, to: now, bar: "1H" };
  });
  const [pickedId, setPickedId] = useState<string | null>(null);

  const planMutation = useReviewPlan();
  const fetchState = useReviewFetch();
  const contextMutation = useReviewContext();

  const credentials = useCredentials();
  const credentialList = credentials.data ?? [];
  // 选中的凭据被删除 / 列表尚未加载时，回落到第一条；不做额外 effect 同步。
  const selectedId =
    pickedId !== null && credentialList.some((meta) => meta.id === pickedId)
      ? pickedId
      : (credentialList[0]?.id ?? null);

  const plan = planMutation.data ?? null;
  const planError = planMutation.error;

  const handleGenerate = () => {
    fetchState.reset();
    planMutation.mutate({ from: range.from, to: range.to, bar: range.bar });
  };

  const handleAssemble = () => {
    if (selectedId === null) return;
    contextMutation.mutate({
      credential_id: selectedId,
      from: range.from,
      to: range.to,
      bar: range.bar,
    });
  };

  const planPayload = planError == null ? null : errorPayloadOf(planError);
  const contextError = contextMutation.error;
  const contextPayload = contextError == null ? null : errorPayloadOf(contextError);
  const context: ReviewContext | null = contextMutation.data ?? null;

  const rangeOk = rangeError(range.from, range.to) === null;

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

      {planPayload === null ? null : planPayload.code === "RangeTooLarge" ? (
        <RangeTooLargePanel message={S.review.errors.rangeTooLarge} />
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

      <AssemblePanel
        credentialId={selectedId}
        credentials={credentials}
        onPickCredential={setPickedId}
        onRun={handleAssemble}
        running={contextMutation.isPending}
        canRun={selectedId !== null && rangeOk}
        onConfigure={onConfigure}
      />

      {contextPayload === null ? null : contextPayload.code === "RangeTooLarge" ? (
        <RangeTooLargePanel message={S.review.errors.assembleRangeTooLarge} />
      ) : (
        <ErrorPanel
          title={S.review.errors.assembleTitle}
          error={contextError}
          onRetry={handleAssemble}
          busy={contextMutation.isPending}
        />
      )}

      {context === null ? null : <ReviewResult context={context} />}
    </main>
  );
}
