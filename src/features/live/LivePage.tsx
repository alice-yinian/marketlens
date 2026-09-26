/**
 * 实盘市场状态页（M1）。
 *
 * 组装顺序：标题与刷新 → 数据新鲜度（硬要求，绝不让陈旧数据静默展示）
 * → 本次刷新的非致命问题（warnings）→ 标的卡片网格 / 空态 / 错误态。
 *
 * 页面本身不含业务逻辑：所有数据来自 Rust（`live_refresh`），
 * 所有文案来自 `lib/strings.ts`。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import type { LiveSnapshot } from "../../lib/types";
import { useCredentials } from "../account/useAccountSnapshot";
import { LiveContextPanel } from "../prompt/LiveContextPanel";
import { PromptPanel } from "../prompt/PromptPanel";
import { errorPayloadOf } from "./errors";
import { formatLocalTime, formatRelativeTime } from "./format";
import { MarketStateCard } from "./MarketStateCard";
import { useForceRefresh, useLiveSnapshot, useNow } from "./useLiveSnapshot";

/** 新鲜度提示：命中缓存 → 「缓存于 X 前」；未命中 → 「刚刚拉取」 */
function FreshnessStrip({ snapshot, now }: { snapshot: LiveSnapshot; now: number }) {
  const cached = snapshot.cache_hit;
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
      <span
        className={`rounded-full px-2.5 py-1 ring-1 ring-inset ${
          cached
            ? "bg-amber-950 text-amber-300 ring-amber-900"
            : "bg-emerald-950 text-emerald-300 ring-emerald-900"
        }`}
      >
        {cached ? S.live.freshness.badgeCached : S.live.freshness.badgeFresh}
      </span>
      <span className={cached ? "text-amber-300" : "text-emerald-300"}>
        {cached
          ? S.live.freshness.cached(formatRelativeTime(snapshot.fetched_at, now))
          : S.live.freshness.fresh}
      </span>
      {cached ? <span className="text-neutral-500">{S.live.freshness.cachedNote}</span> : null}
      <span className="text-neutral-500">
        {S.live.freshness.fetchedAt(formatLocalTime(snapshot.fetched_at))}
      </span>
    </div>
  );
}

function WarningsPanel({ warnings }: { warnings: string[] }) {
  return (
    <section className="rounded-xl border border-amber-900/60 bg-amber-950/30 p-4">
      <h2 className="text-sm font-medium text-amber-300">{S.live.sections.warnings}</h2>
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

function ErrorPanel({
  title,
  error,
  hint,
  onRetry,
  busy = false,
}: {
  title: string;
  error: unknown;
  hint?: string;
  onRetry?: () => void;
  busy?: boolean;
}) {
  const payload = errorPayloadOf(error);
  return (
    <section className="rounded-xl border border-red-900/60 bg-red-950/30 p-4">
      <h2 className="text-sm font-medium text-red-300">{title}</h2>
      <p className="mt-2 font-mono text-xs break-all text-red-200/90">{payload.message}</p>
      <p className="mt-1 text-xs text-red-300/70">{S.live.errorCode(payload.code)}</p>
      {hint === undefined ? null : <p className="mt-1 text-xs text-neutral-400">{hint}</p>}
      {payload.retryable && onRetry !== undefined ? (
        <button
          type="button"
          onClick={onRetry}
          disabled={busy}
          className="mt-3 rounded-md bg-red-900/70 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {S.live.retry}
        </button>
      ) : null}
      {payload.retryable ? null : (
        <p className="mt-2 text-xs text-red-300/70">{S.live.errorNotRetryable}</p>
      )}
    </section>
  );
}

function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="rounded-xl border border-dashed border-neutral-800 bg-neutral-900/40 p-8 text-center">
      <p className="text-sm text-neutral-300">{title}</p>
      <p className="mt-1 text-xs text-neutral-500">{hint}</p>
    </div>
  );
}

export function LivePage({ onOpenLibrary }: { onOpenLibrary?: () => void } = {}) {
  const query = useLiveSnapshot();
  const refresh = useForceRefresh();
  const now = useNow();

  // 生成提示词的上下文：凭据（可空 → 只分析行情）+ 是否绕过行情缓存。
  // 这两个值同时被「生成」与「刷新」用到，所以放在页面级状态里而不是面板内部。
  const credentialsQuery = useCredentials();
  const [credentialId, setCredentialId] = useState<string | null>(null);
  const [force, setForce] = useState(false);

  const snapshot = query.data;
  const fatalError = query.isError && snapshot === undefined ? query.error : undefined;

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{S.live.title}</h1>
          <p className="mt-0.5 text-sm text-neutral-400">{S.live.subtitle}</p>
        </div>
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          className="rounded-md bg-neutral-800 px-4 py-2 text-sm font-medium text-neutral-100 hover:bg-neutral-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {refresh.isPending ? S.live.refreshing : S.live.refresh}
        </button>
      </header>

      {snapshot === undefined ? null : <FreshnessStrip snapshot={snapshot} now={now} />}

      {refresh.error === null ? null : (
        <ErrorPanel
          title={S.live.refreshFailedTitle}
          error={refresh.error}
          hint={snapshot === undefined ? undefined : S.live.refreshFailedHint}
          onRetry={() => refresh.mutate()}
          busy={refresh.isPending}
        />
      )}

      {fatalError !== undefined ? (
        <ErrorPanel
          title={S.live.errorTitle}
          error={fatalError}
          onRetry={() => void query.refetch()}
          busy={query.isFetching}
        />
      ) : query.isPending ? (
        <Placeholder title={S.live.loading} hint={S.live.loadingHint} />
      ) : snapshot === undefined ? null : (
        <>
          {snapshot.warnings.length === 0 ? null : <WarningsPanel warnings={snapshot.warnings} />}
          {snapshot.instruments.length === 0 ? (
            snapshot.warnings.length === 0 ? (
              <Placeholder title={S.live.empty} hint={S.live.emptyHint} />
            ) : (
              <p className="text-sm text-neutral-400">{S.live.noInstruments}</p>
            )
          ) : (
            <>
              <p className="text-xs text-neutral-500">
                {S.live.watchlistCount(snapshot.instruments.length)}
              </p>
              <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
                {snapshot.instruments.map((instrument) => (
                  <MarketStateCard key={instrument.inst_id} state={instrument} />
                ))}
              </div>
            </>
          )}
        </>
      )}

      <LiveContextPanel
        credentials={{
          list: credentialsQuery.data ?? [],
          isPending: credentialsQuery.isPending,
          isError: credentialsQuery.isError,
        }}
        credentialId={credentialId}
        onCredential={setCredentialId}
        force={force}
        onForce={setForce}
      />

      <PromptPanel
        context={{ kind: "live", credentialId, force }}
        onOpenLibrary={onOpenLibrary}
      />
    </main>
  );
}
