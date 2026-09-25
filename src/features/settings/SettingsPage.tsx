/**
 * 设置页（M6）：缓存管理 + 诊断导出 + 版本信息。
 *
 * 页面本身不含业务逻辑：统计、清理、导出全部来自 Rust
 * （`cache_stats` / `cache_cleanup` / `diagnostics_export`），
 * 文案来自 `lib/strings.ts`，组件不直接碰 Tauri API。
 *
 * 两条硬要求体现在这里：
 * 1. 清理是破坏性的——「清理缓存」只是展开确认区，**确认后**才真正调用命令，
 *    且确认文案逐条列出会删什么；
 * 2. 诊断包必须把「它到底遮了什么」摊开给用户核对，而不是只让用户相信。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import type { DiagnosticExport } from "../../lib/types";
import { ErrorPanel } from "../account/ErrorPanel";
import { formatLocalTime } from "../live/format";
import {
  cleanupDeletedLines,
  cleanupFreedText,
  formatBytes,
  formatCount,
  redactionLines,
  retentionText,
} from "./format";
import { useCacheCleanup, useCacheStats, useDiagnosticsExport } from "./useSettings";

async function copyText(text: string): Promise<boolean> {
  const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
  if (clipboard === undefined) return false;
  try {
    await clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

function Card({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <div>
        <h2 className="text-sm font-medium text-neutral-200">{title}</h2>
        {subtitle === undefined ? null : (
          <p className="mt-0.5 text-xs text-neutral-500">{subtitle}</p>
        )}
      </div>
      {children}
    </section>
  );
}

/** 缓存管理：统计表 + 二次确认清理 + 清理结果。 */
function CacheCard() {
  const stats = useCacheStats();
  const cleanup = useCacheCleanup();
  const [confirming, setConfirming] = useState(false);

  const data = stats.data;

  return (
    <Card title={S.settings.cache.title} subtitle={S.settings.cache.subtitle}>
      {stats.isPending ? (
        <p className="text-sm text-neutral-400">{S.settings.cache.loading}</p>
      ) : stats.isError && data === undefined ? (
        <ErrorPanel
          title={S.settings.cache.errorTitle}
          error={stats.error}
          onRetry={() => void stats.refetch()}
          busy={stats.isFetching}
        />
      ) : data === undefined ? null : (
        <>
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
            <span className="text-xs text-neutral-400">{S.settings.cache.databaseSize}</span>
            <span className="text-lg font-semibold text-neutral-100">
              {formatBytes(data.database_bytes)}
            </span>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-neutral-800 text-xs text-neutral-500">
                  <th className="py-2 pr-3 font-medium">{S.settings.cache.columns.label}</th>
                  <th className="py-2 pr-3 text-right font-medium">
                    {S.settings.cache.columns.rows}
                  </th>
                  <th className="py-2 font-medium">{S.settings.cache.columns.retention}</th>
                </tr>
              </thead>
              <tbody>
                {data.tables.map((table) => (
                  <tr key={table.name} className="border-b border-neutral-900/80 last:border-0">
                    <td className="py-2 pr-3 text-neutral-200">{table.label}</td>
                    <td className="py-2 pr-3 text-right font-mono text-neutral-300">
                      {formatCount(table.rows)}
                    </td>
                    <td className="py-2 text-neutral-400">
                      {retentionText(table.retention)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={stats.isFetching}
          onClick={() => void stats.refetch()}
          className="rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stats.isFetching ? S.settings.cache.refreshing : S.settings.cache.refresh}
        </button>
        <button
          type="button"
          disabled={cleanup.isPending}
          onClick={() => setConfirming(true)}
          className="rounded-md bg-red-900/70 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {cleanup.isPending ? S.settings.cache.cleaning : S.settings.cache.cleanup}
        </button>
      </div>

      {confirming ? (
        <div className="rounded-lg border border-red-900/60 bg-red-950/30 p-3">
          <p className="text-xs font-medium text-red-300">{S.settings.cache.confirmTitle}</p>
          <p className="mt-1 text-xs text-red-200/90">{S.settings.cache.confirmBody}</p>
          <div className="mt-2 flex gap-2">
            <button
              type="button"
              onClick={() => {
                setConfirming(false);
                cleanup.mutate();
              }}
              className="rounded-md bg-red-900/70 px-3 py-1.5 text-xs text-red-100 hover:bg-red-900"
            >
              {S.settings.cache.confirm}
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs text-neutral-300 hover:bg-neutral-800"
            >
              {S.settings.cache.cancel}
            </button>
          </div>
        </div>
      ) : null}

      {cleanup.isError ? (
        <ErrorPanel
          title={S.settings.cache.cleanupErrorTitle}
          error={cleanup.error}
          onRetry={() => cleanup.mutate()}
          busy={cleanup.isPending}
        />
      ) : null}

      {cleanup.data === undefined ? null : (
        <div className="rounded-lg border border-neutral-800 bg-neutral-950/60 p-3">
          <p className="text-xs font-medium text-neutral-300">{S.settings.cache.reportTitle}</p>
          <ul className="mt-1 flex flex-col gap-0.5 text-xs text-neutral-400">
            {cleanupDeletedLines(cleanup.data.deleted, data?.tables ?? []).map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          <p className="mt-1 text-xs text-neutral-400">
            {cleanupFreedText(cleanup.data.bytes_before, cleanup.data.bytes_after)}
          </p>
        </div>
      )}
    </Card>
  );
}

/** 诊断导出：结果 + 脱敏说明 + 脱敏明细 + 凭据概览 + 日志尾部。 */
function DiagnosticsCard() {
  const exportMutation = useDiagnosticsExport();
  const [feedback, setFeedback] = useState<string | null>(null);
  const [logOpen, setLogOpen] = useState(false);

  const result: DiagnosticExport | undefined = exportMutation.data;
  const bundle = result?.bundle;

  const copy = async (text: string) => {
    const ok = await copyText(text);
    setFeedback(ok ? S.settings.diagnostics.copied : S.settings.diagnostics.copyFailed);
  };

  return (
    <Card title={S.settings.diagnostics.title} subtitle={S.settings.diagnostics.subtitle}>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={exportMutation.isPending}
          onClick={() => {
            setFeedback(null);
            exportMutation.mutate();
          }}
          className="rounded-md bg-indigo-800 px-4 py-2 text-sm font-medium text-indigo-50 hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {exportMutation.isPending ? S.settings.diagnostics.exporting : S.settings.diagnostics.export}
        </button>
        {feedback === null ? null : (
          <span className="text-xs text-emerald-300">{feedback}</span>
        )}
      </div>

      {exportMutation.isError ? (
        <ErrorPanel
          title={S.settings.diagnostics.errorTitle}
          error={exportMutation.error}
          onRetry={() => exportMutation.mutate()}
          busy={exportMutation.isPending}
        />
      ) : null}

      {result === undefined || bundle === undefined ? null : (
        <>
          <div className="rounded-lg border border-neutral-800 bg-neutral-950/60 p-3 text-xs">
            <p className="font-medium text-neutral-300">{S.settings.diagnostics.resultTitle}</p>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <span className="text-neutral-500">{S.settings.diagnostics.path}</span>
              <code className="font-mono break-all text-neutral-300">{result.path}</code>
              <button
                type="button"
                onClick={() => void copy(result.path)}
                className="rounded-md border border-neutral-700 px-2 py-0.5 text-neutral-300 hover:bg-neutral-800"
              >
                {S.settings.diagnostics.copyPath}
              </button>
            </div>
            <p className="mt-1 text-neutral-400">{S.settings.diagnostics.size(formatBytes(result.bytes))}</p>
            <p className="mt-0.5 text-neutral-400">
              {S.settings.diagnostics.generatedAt(formatLocalTime(bundle.generated_at) ?? "—")}
            </p>
            {bundle.log_file === null ? null : (
              <p className="mt-0.5 break-all text-neutral-500">
                {S.settings.diagnostics.logFile(bundle.log_file)}
              </p>
            )}
          </div>

          {/* 脱敏说明：必须具体，让用户知道「为什么可以放心发出去」 */}
          <div className="rounded-lg border border-emerald-900/50 bg-emerald-950/20 p-3">
            <p className="text-xs font-medium text-emerald-300">
              {S.settings.diagnostics.redactionTitle}
            </p>
            <p className="mt-1 text-xs text-emerald-200/90">
              {S.settings.diagnostics.redactionIntro}
            </p>
            <ul className="mt-1 flex list-disc flex-col gap-0.5 pl-5 text-xs text-emerald-200/80">
              <li>{S.settings.diagnostics.redactionNoKeys}</li>
              <li>{S.settings.diagnostics.redactionNoRows}</li>
              <li>{S.settings.diagnostics.redactionLogs}</li>
            </ul>
          </div>

          {/* 本次脱敏明细：让用户能自己核对，而不是只能相信 */}
          <div className="rounded-lg border border-neutral-800 bg-neutral-950/60 p-3">
            <p className="text-xs font-medium text-neutral-300">
              {S.settings.diagnostics.redactionsTitle}
            </p>
            <ul className="mt-1 flex flex-col gap-0.5 text-xs text-neutral-400">
              {redactionLines(bundle.redactions).map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </div>

          {/* 凭据概览：只有数量与属性 */}
          <div className="rounded-lg border border-neutral-800 bg-neutral-950/60 p-3">
            <p className="text-xs font-medium text-neutral-300">
              {S.settings.diagnostics.credentialsTitle}
            </p>
            <p className="mt-1 text-xs text-neutral-400">
              {S.settings.diagnostics.credentialsCount(bundle.credentials.count)}
            </p>
            {bundle.credentials.entries.length === 0 ? (
              <p className="text-xs text-neutral-500">
                {S.settings.diagnostics.credentialsNone}
              </p>
            ) : (
              <ul className="mt-1 flex flex-wrap gap-2 text-xs text-neutral-300">
                {bundle.credentials.entries.map((entry) => (
                  <li key={entry} className="rounded border border-neutral-800 px-2 py-0.5 font-mono">
                    {entry}
                  </li>
                ))}
              </ul>
            )}
            <p className="mt-1 text-xs text-neutral-500">
              {S.settings.diagnostics.credentialsNote}
            </p>
          </div>

          {/* 日志尾部：默认折叠，展开后限高滚动 */}
          <div className="rounded-lg border border-neutral-800 bg-neutral-950/60 p-3">
            <div className="flex items-center justify-between gap-2">
              <p className="text-xs font-medium text-neutral-300">
                {S.settings.diagnostics.logTitle}
              </p>
              <button
                type="button"
                onClick={() => setLogOpen((prev) => !prev)}
                className="rounded-md border border-neutral-700 px-2 py-0.5 text-xs text-neutral-300 hover:bg-neutral-800"
              >
                {logOpen ? S.settings.diagnostics.logHide : S.settings.diagnostics.logShow}
              </button>
            </div>
            {logOpen ? (
              <pre className="mt-2 max-h-64 overflow-auto rounded-md bg-neutral-950 p-2 font-mono text-xs break-all whitespace-pre-wrap text-neutral-400">
                {bundle.log_tail.join("\n")}
              </pre>
            ) : null}
          </div>

          <div>
            <button
              type="button"
              onClick={() => void copy(JSON.stringify(bundle, null, 2))}
              className="rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            >
              {S.settings.diagnostics.copyJson}
            </button>
          </div>

          {/* 版本信息（诊断包里的 app 快照，与页脚同源） */}
          <div className="rounded-lg border border-neutral-800 bg-neutral-950/60 p-3">
            <p className="text-xs font-medium text-neutral-300">
              {S.settings.diagnostics.appTitle}
            </p>
            <dl className="mt-1 grid grid-cols-1 gap-x-4 gap-y-0.5 text-xs sm:grid-cols-2">
              <div className="flex justify-between gap-2 sm:justify-start">
                <dt className="text-neutral-500">{S.settings.diagnostics.appVersion}</dt>
                <dd className="font-mono text-neutral-300">{bundle.app.app_version}</dd>
              </div>
              <div className="flex justify-between gap-2 sm:justify-start">
                <dt className="text-neutral-500">{S.settings.diagnostics.coreVersion}</dt>
                <dd className="font-mono text-neutral-300">{bundle.app.core_version}</dd>
              </div>
              <div className="flex justify-between gap-2 sm:justify-start">
                <dt className="text-neutral-500">{S.settings.diagnostics.schemaVersion}</dt>
                <dd className="font-mono text-neutral-300">{bundle.app.schema_version}</dd>
              </div>
              <div className="flex justify-between gap-2 sm:justify-start">
                <dt className="text-neutral-500">{S.settings.diagnostics.targetOs}</dt>
                <dd className="font-mono text-neutral-300">{bundle.app.target_os}</dd>
              </div>
            </dl>
          </div>
        </>
      )}
    </Card>
  );
}

export function SettingsPage() {
  return (
    <main className="mx-auto flex w-full max-w-5xl flex-col gap-5 p-4 sm:p-6">
      <header>
        <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{S.settings.title}</h1>
        <p className="mt-0.5 text-sm text-neutral-400">{S.settings.subtitle}</p>
      </header>

      <CacheCard />
      <DiagnosticsCard />
    </main>
  );
}
