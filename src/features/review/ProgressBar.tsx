/**
 * 采集进度条。
 *
 * 进度 = `done_series / total_series`；同时显示「已完成 X / Y 条序列」
 * 与当前正在拉的序列（`current`）。旁边是「取消」按钮。
 *
 * `progress` 为 null 表示首个进度事件还没到（计划刚启动），此时显示启动态，
 * 不假装进度是 0%。
 */
import { S } from "../../lib/strings";
import type { Progress } from "../../lib/types";
import { progressPercent } from "./format";

export function ProgressBar({
  progress,
  cancelling,
  cancelNote,
  onCancel,
}: {
  progress: Progress | null;
  cancelling: boolean;
  cancelNote: string | null;
  onCancel: () => void;
}) {
  const percent = progress === null ? 0 : progressPercent(progress.done_series, progress.total_series);

  return (
    <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-medium text-neutral-200">{S.review.progress.title}</h2>
        <button
          type="button"
          onClick={onCancel}
          disabled={cancelling}
          className="rounded-md border border-red-900 px-3 py-1.5 text-xs text-red-300 hover:bg-red-950/50 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {cancelling ? S.review.progress.cancelling : S.review.progress.cancel}
        </button>
      </div>

      <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-neutral-800">
        <div
          className="h-full rounded-full bg-emerald-600 transition-[width] duration-300"
          style={{ width: `${percent}%` }}
        />
      </div>

      {progress === null ? (
        <p className="mt-2 text-xs text-neutral-400">{S.review.progress.starting}</p>
      ) : (
        <>
          <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
            <span className="text-xs text-neutral-300">
              {S.review.progress.seriesProgress(progress.done_series, progress.total_series)}
            </span>
            <span className="font-mono text-xs text-neutral-500">{percent}%</span>
          </div>
          <p className="mt-1 text-xs text-neutral-400">
            {progress.current.length === 0
              ? S.review.progress.starting
              : S.review.progress.current(progress.current)}
          </p>
          <p className="mt-0.5 font-mono text-xs text-neutral-500">
            {S.review.progress.requests(progress.done_requests, progress.total_requests)}
          </p>
        </>
      )}

      {cancelNote === null ? null : (
        <p className="mt-2 text-xs text-amber-300">{cancelNote}</p>
      )}
    </section>
  );
}
