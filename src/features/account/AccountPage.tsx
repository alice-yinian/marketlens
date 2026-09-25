/**
 * 账户 / 仓位页（M2）。
 *
 * 组装顺序：标题与刷新 → 凭据选择 → 概览卡片 → 本地留痕覆盖度 → warnings
 * → 持仓列表 / 空态 / 错误态。
 *
 * 页面本身不含业务逻辑：数据全部来自 Rust（`account_snapshot`），
 * 文案全部来自 `lib/strings.ts`。刻意不做自动轮询，只有点「刷新」才重新拉取。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import { formatLocalTime } from "../live/format";
import { AccountOverviewCard } from "./AccountOverviewCard";
import { ErrorPanel } from "./ErrorPanel";
import { PositionCard } from "./PositionCard";
import { useAccountSnapshot, useCredentials } from "./useAccountSnapshot";

function Placeholder({
  title,
  hint,
  actionLabel,
  onAction,
}: {
  title: string;
  hint: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div className="rounded-xl border border-dashed border-neutral-800 bg-neutral-900/40 p-8 text-center">
      <p className="text-sm text-neutral-300">{title}</p>
      <p className="mt-1 text-xs text-neutral-500">{hint}</p>
      {actionLabel === undefined || onAction === undefined ? null : (
        <button
          type="button"
          onClick={onAction}
          className="mt-4 rounded-md bg-neutral-800 px-4 py-2 text-sm font-medium text-neutral-100 hover:bg-neutral-700"
        >
          {actionLabel}
        </button>
      )}
    </div>
  );
}

function WarningsPanel({ warnings }: { warnings: string[] }) {
  return (
    <section className="rounded-xl border border-amber-900/60 bg-amber-950/30 p-4">
      <h2 className="text-sm font-medium text-amber-300">{S.account.warningsTitle}</h2>
      <ul className="mt-2 space-y-1">
        {warnings.map((warning, index) => (
          <li key={index} className="text-xs break-all text-amber-200/90">
            {warning}
          </li>
        ))}
      </ul>
    </section>
  );
}

export function AccountPage({ onConfigure }: { onConfigure: () => void }) {
  const credentials = useCredentials();
  const [pickedId, setPickedId] = useState<string | null>(null);

  const list = credentials.data ?? [];
  // 选中的凭据被删除 / 列表尚未加载时，回落到第一条；不做额外 effect 同步。
  const selectedId =
    pickedId !== null && list.some((meta) => meta.id === pickedId)
      ? pickedId
      : (list[0]?.id ?? null);

  const snapshot = useAccountSnapshot(selectedId);
  const data = snapshot.data;
  const fatalError = snapshot.isError && data === undefined ? snapshot.error : undefined;
  const refreshError = snapshot.isError && data !== undefined ? snapshot.error : undefined;

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{S.account.title}</h1>
          <p className="mt-0.5 text-sm text-neutral-400">{S.account.subtitle}</p>
        </div>
        <button
          type="button"
          onClick={() => void snapshot.refetch()}
          disabled={snapshot.isFetching || selectedId === null}
          className="rounded-md bg-neutral-800 px-4 py-2 text-sm font-medium text-neutral-100 hover:bg-neutral-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {snapshot.isFetching ? S.account.refreshing : S.account.refresh}
        </button>
      </header>

      {credentials.isPending ? (
        <Placeholder title={S.account.loading} hint={S.account.loadingHint} />
      ) : credentials.isError ? (
        <ErrorPanel
          title={S.account.errorTitle}
          error={credentials.error}
          onRetry={() => void credentials.refetch()}
          busy={credentials.isFetching}
        />
      ) : list.length === 0 ? (
        <Placeholder
          title={S.account.noCredentials}
          hint={S.account.noCredentialsHint}
          actionLabel={S.account.addCredential}
          onAction={onConfigure}
        />
      ) : (
        <>
          {list.length === 1 ? (
            <p className="text-xs text-neutral-500">
              {S.account.credential}
              <span className="ml-2 font-mono text-neutral-300">{list[0]?.label}</span>
              <span className="ml-2 font-mono text-neutral-500">{list[0]?.api_key_masked}</span>
              <span className="ml-2">
                {list[0]?.env === "demo" ? S.account.envDemo : S.account.envLive}
              </span>
            </p>
          ) : (
            <label className="flex flex-wrap items-center gap-2 text-sm text-neutral-300">
              {S.account.credential}
              <select
                className="rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100"
                value={selectedId ?? ""}
                onChange={(event) => setPickedId(event.target.value)}
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

          {refreshError === undefined ? null : (
            <ErrorPanel
              title={S.account.refreshFailedTitle}
              error={refreshError}
              hint={S.account.refreshFailedHint}
              onRetry={() => void snapshot.refetch()}
              busy={snapshot.isFetching}
            />
          )}

          {fatalError !== undefined ? (
            <ErrorPanel
              title={S.account.errorTitle}
              error={fatalError}
              onRetry={() => void snapshot.refetch()}
              busy={snapshot.isFetching}
            />
          ) : snapshot.isPending ? (
            <Placeholder title={S.account.loading} hint={S.account.loadingHint} />
          ) : data === undefined ? null : (
            <>
              <AccountOverviewCard overview={data.overview} />
              <p className="text-xs text-neutral-500">
                {S.account.fetchedAt(formatLocalTime(data.overview.fetched_at) ?? S.account.na)}
              </p>

              <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
                <h2 className="text-sm font-medium text-neutral-200">{S.account.traceTitle}</h2>
                <p className="mt-1 text-xs text-neutral-400">{data.trace.note}</p>
                {data.trace.has_gaps ? (
                  <p className="mt-1 text-xs text-amber-300">{S.account.traceGap}</p>
                ) : null}
              </section>

              {data.warnings.length === 0 ? null : <WarningsPanel warnings={data.warnings} />}

              <section className="flex flex-col gap-4">
                <div className="flex items-baseline justify-between">
                  <h2 className="text-sm font-medium text-neutral-200">
                    {S.account.positionsTitle}
                  </h2>
                  <span className="text-xs text-neutral-500">
                    {S.account.positionsCount(data.positions.length)}
                  </span>
                </div>
                {data.positions.length === 0 ? (
                  <p className="text-sm text-neutral-400">{S.account.noPositions}</p>
                ) : (
                  <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
                    {data.positions.map((position) => (
                      <PositionCard key={position.pos_id} position={position} />
                    ))}
                  </div>
                )}
              </section>
            </>
          )}
        </>
      )}
    </main>
  );
}
