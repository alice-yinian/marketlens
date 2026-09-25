/**
 * 采集结果逐条展示。
 *
 * 四种 `SeriesStatus` 视觉各异：Ok 正常 / Unavailable 灰（不可得）/
 * Failed 红（失败）/ Skipped 蓝（续传跳过，**这是好事**，不画成红的）。
 *
 * `reason` 只要有值就显示——包括 Ok 状态下的「数据在到达起始时间前已耗尽，
 * 该时段可能不完整」这类提示，它和失败一样影响复盘结论。
 */
import { S } from "../../lib/strings";
import type { SeriesReport } from "../../lib/types";
import { seriesStatusView } from "./format";

export function SeriesReportList({ reports }: { reports: SeriesReport[] }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-medium text-neutral-200">{S.review.report.title}</h2>
      {reports.map((report) => {
        const view = seriesStatusView(report.status);
        return (
          <div
            key={report.key}
            className={`rounded-lg border bg-neutral-900/40 p-3 ${view.row}`}
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm text-neutral-200">{report.label}</span>
              <span
                className={`rounded border px-1.5 py-0.5 text-xs ${view.badge}`}
              >
                {view.label}
              </span>
              <span className="ml-auto font-mono text-xs text-neutral-500">
                {S.review.report.rows} {report.rows} · {S.review.report.pages} {report.pages}
              </span>
            </div>
            {report.reason === null ? null : (
              <p
                className={`mt-1 text-xs ${
                  view.tone === "failed" ? "text-red-300/90" : "text-neutral-400"
                }`}
              >
                {report.reason}
              </p>
            )}
          </div>
        );
      })}
    </section>
  );
}
