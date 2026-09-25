import { useQuery } from "@tanstack/react-query";

import { LivePage } from "./features/live/LivePage";
import { call } from "./lib/ipc";
import { S } from "./lib/strings";

/**
 * 页脚：紧凑的版本信息（M0 验收路径保留为常驻诊断信息）。
 * 它同时是「IPC 通道是否可用」的最轻量探针，因此失败时只做静默降级，不打扰主流程。
 */
function VersionFooter() {
  const info = useQuery({
    queryKey: ["app_info"],
    queryFn: () => call("app_info"),
  });

  return (
    <footer className="border-t border-neutral-900 px-4 py-3 text-center text-xs text-neutral-600 sm:px-6">
      {info.isPending ? (
        <span>{S.footer.loading}</span>
      ) : info.isError ? (
        <span className="text-neutral-500">{S.footer.fail}</span>
      ) : (
        <span className="inline-flex flex-wrap justify-center gap-x-3 gap-y-1">
          <span className="text-neutral-500">{S.appName}</span>
          <span>{S.footer.version(info.data.app_version)}</span>
          <span>{S.footer.core(info.data.core_version)}</span>
          <span>{S.footer.schema(info.data.schema_version)}</span>
          <span>{S.footer.targetOs(info.data.target_os)}</span>
        </span>
      )}
    </footer>
  );
}

export default function App() {
  return (
    <div className="flex min-h-full flex-col">
      <div className="flex-1">
        <LivePage />
      </div>
      <VersionFooter />
    </div>
  );
}
