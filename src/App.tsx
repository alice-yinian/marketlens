import { useQuery } from "@tanstack/react-query";

import { call } from "./lib/ipc";
import { S } from "./lib/strings";

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-6 border-b border-neutral-800 py-2 last:border-b-0">
      <span className="text-sm text-neutral-400">{label}</span>
      <span className="font-mono text-sm text-neutral-100">{value}</span>
    </div>
  );
}

export default function App() {
  const info = useQuery({
    queryKey: ["app_info"],
    queryFn: () => call("app_info"),
  });

  return (
    <main className="mx-auto flex min-h-full max-w-2xl flex-col justify-center gap-6 p-6">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">{S.appName}</h1>
        <p className="text-sm text-neutral-400">{S.tagline}</p>
      </header>

      <section className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-5">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-base font-medium">{S.boot.title}</h2>
          {info.isPending ? null : info.isError ? (
            <span className="rounded-full bg-red-950 px-3 py-1 text-xs text-red-300">
              {S.boot.ipcFail}
            </span>
          ) : (
            <span className="rounded-full bg-emerald-950 px-3 py-1 text-xs text-emerald-300">
              {S.boot.ipcOk}
            </span>
          )}
        </div>

        {info.isPending ? (
          <p className="py-4 text-sm text-neutral-500">{S.boot.loading}</p>
        ) : info.isError ? (
          <div className="space-y-3 py-2">
            <p className="font-mono text-xs break-all text-red-300">{String(info.error)}</p>
            <button
              type="button"
              onClick={() => void info.refetch()}
              className="rounded-md bg-neutral-800 px-3 py-1.5 text-sm hover:bg-neutral-700"
            >
              {S.boot.retry}
            </button>
          </div>
        ) : (
          <div>
            <InfoRow label={S.boot.appVersion} value={info.data.app_version} />
            <InfoRow label={S.boot.coreVersion} value={info.data.core_version} />
            <InfoRow label={S.boot.schemaVersion} value={String(info.data.schema_version)} />
            <InfoRow label={S.boot.targetOs} value={info.data.target_os} />
          </div>
        )}
      </section>

      <p className="text-center text-xs text-neutral-600">{S.boot.migrationNote}</p>
    </main>
  );
}
