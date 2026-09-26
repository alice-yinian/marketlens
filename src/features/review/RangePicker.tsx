/**
 * 时段与粒度选择。
 *
 * 90 天上限**在界面上说明**并前置校验：超限时禁用「生成计划」并给出原因，
 * 而不是让后端返回错误码再解释——更不会静默把时段改小。
 */
import { S } from "../../lib/strings";
import {
  BAR_OPTIONS,
  barLabel,
  localInputToMs,
  msToLocalInput,
  rangeDays,
  rangeError,
  rangeProblemText,
} from "./format";

const DAY_MS = 24 * 60 * 60 * 1000;

const QUICK_RANGES: readonly { label: string; spanMs: number }[] = [
  { label: S.review.range.last24h, spanMs: DAY_MS },
  { label: S.review.range.last7d, spanMs: 7 * DAY_MS },
  { label: S.review.range.last30d, spanMs: 30 * DAY_MS },
];

const inputClass =
  "rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100";

export function RangePicker({
  from,
  to,
  bar,
  onChange,
  onGenerate,
  generating,
  disabled,
}: {
  from: number;
  to: number;
  bar: string;
  onChange: (next: { from?: number; to?: number; bar?: string }) => void;
  onGenerate: () => void;
  generating: boolean;
  disabled: boolean;
}) {
  const problem = rangeError(from, to);

  return (
    <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.review.range.title}</h2>

      <div className="mt-3 flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 text-xs text-neutral-400">
          {S.review.range.from}
          <input
            type="datetime-local"
            className={inputClass}
            value={msToLocalInput(from)}
            onChange={(event) => {
              const ms = localInputToMs(event.target.value);
              if (ms !== null) onChange({ from: ms });
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-neutral-400">
          {S.review.range.to}
          <input
            type="datetime-local"
            className={inputClass}
            value={msToLocalInput(to)}
            onChange={(event) => {
              const ms = localInputToMs(event.target.value);
              if (ms !== null) onChange({ to: ms });
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-neutral-400">
          {S.review.range.bar}
          <select
            className={inputClass}
            value={bar}
            onChange={(event) => onChange({ bar: event.target.value })}
          >
            {BAR_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {barLabel(option)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <span className="text-xs text-neutral-500">{S.review.range.quick}</span>
        {QUICK_RANGES.map((quick) => (
          <button
            key={quick.label}
            type="button"
            onClick={() => {
              const now = Date.now();
              onChange({ from: now - quick.spanMs, to: now });
            }}
            className="rounded-md border border-neutral-700 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
          >
            {quick.label}
          </button>
        ))}
        {from < to ? (
          <span className="text-xs text-neutral-600">
            {S.review.range.rangeDays(rangeDays(from, to))}
          </span>
        ) : null}
      </div>

      <p className="mt-3 text-xs text-neutral-500">{S.review.range.maxRangeNote}</p>
      {problem === null ? null : (
        <p className="mt-1 text-xs text-amber-300">{rangeProblemText(problem)}</p>
      )}

      <button
        type="button"
        onClick={onGenerate}
        disabled={problem !== null || generating || disabled}
        className="mt-3 rounded-md bg-neutral-800 px-4 py-2 text-sm font-medium text-neutral-100 hover:bg-neutral-700 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {generating ? S.review.plan.generating : S.review.plan.generate}
      </button>
      <p className="mt-2 text-xs text-neutral-500">{S.review.plan.offlineNote}</p>
    </section>
  );
}
