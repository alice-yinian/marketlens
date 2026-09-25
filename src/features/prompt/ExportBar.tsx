/**
 * 导出：复制到剪贴板 / 保存为文件。
 *
 * 导出方式选了**浏览器 Blob 下载**（`a[download]`）而不是 Tauri 的
 * `plugin-dialog` + `plugin-fs`：本仓库未安装这两个插件，为了不给 M5 引入
 * 新的原生依赖（还要同时照顾 Windows 与 Android 的路径差异），用标准 Web
 * 能力兜底即可满足「另存为 .md」的需求。
 *
 * L0 导出前必须二次确认：它会把完整金额写进文件，用户得先知道这件事。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import type { PrivacyLevel, PromptOutput } from "../../lib/types";
import { exportFileName } from "./format";

type ExportAction = "copy" | "file";

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

function downloadText(fileName: string, text: string) {
  const blob = new Blob([text], { type: "text/markdown;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

export function ExportBar({
  output,
  privacy,
  templateName,
}: {
  output: PromptOutput | null;
  privacy: PrivacyLevel;
  templateName: string;
}) {
  const [pending, setPending] = useState<ExportAction | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);

  const ready = output !== null;

  const run = async (action: ExportAction) => {
    if (output === null) return;
    setPending(null);
    if (action === "copy") {
      const ok = await copyText(output.text);
      setFeedback(ok ? S.prompt.export.copied : S.prompt.export.copyFailed);
      return;
    }
    const fileName = exportFileName(templateName, privacy, Date.now());
    downloadText(fileName, output.text);
    setFeedback(S.prompt.export.savedFile(fileName));
  };

  const request = (action: ExportAction) => {
    setFeedback(null);
    if (privacy === "L0") {
      // L0 会带出完整金额：先确认，再导出。
      setPending(action);
      return;
    }
    void run(action);
  };

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.prompt.export.title}</h2>

      {pending === null ? null : (
        <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
          <p className="text-xs font-medium text-amber-300">{S.prompt.export.l0WarningTitle}</p>
          <p className="mt-1 text-xs text-amber-200/90">{S.prompt.export.l0Warning}</p>
          <div className="mt-2 flex gap-2">
            <button
              type="button"
              onClick={() => void run(pending)}
              className="rounded-md bg-amber-800/80 px-3 py-1.5 text-xs text-amber-50 hover:bg-amber-800"
            >
              {S.prompt.export.confirm}
            </button>
            <button
              type="button"
              onClick={() => setPending(null)}
              className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs text-neutral-300 hover:bg-neutral-800"
            >
              {S.prompt.export.cancel}
            </button>
          </div>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={!ready}
          onClick={() => request("copy")}
          className="rounded-md bg-indigo-800 px-4 py-2 text-sm font-medium text-indigo-50 hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {S.prompt.export.copy}
        </button>
        <button
          type="button"
          disabled={!ready}
          onClick={() => request("file")}
          className="rounded-md border border-neutral-700 px-4 py-2 text-sm text-neutral-200 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {S.prompt.export.saveFile}
        </button>
        {feedback === null ? null : (
          <span className="text-xs text-emerald-300">{feedback}</span>
        )}
        {ready ? null : (
          <span className="text-xs text-neutral-500">{S.prompt.export.noPreview}</span>
        )}
      </div>
    </section>
  );
}
