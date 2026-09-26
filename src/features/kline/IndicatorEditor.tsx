/**
 * 指标行编辑器（行情页专用）。
 *
 * 每一行是「种类 + 周期」——周期可以自由填（开放自定义周期），所以校验必须在
 * 输入处就给出反馈：`0` 会让后端的纯函数走进除零分支，超长的周期则会让整列变成 `—`。
 *
 * 去重放在这里做而不是提示用户「请不要重复」：重复提交同一个指标没有意义，
 * 而静默去重的结果与用户预期一致（他想要的就是一列 EMA20）。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import type { IndicatorKind, IndicatorSpec } from "../../lib/types";
import {
  DEFAULT_PERIOD,
  INDICATOR_KINDS,
  INDICATOR_LABEL,
  MAX_INDICATOR_PERIOD,
  MAX_INDICATORS,
  dedupeIndicators,
  indicatorProblem,
} from "./format";

const INPUT_CLASS =
  "rounded-md border border-neutral-700 bg-neutral-900 px-2 py-1.5 text-sm text-neutral-100 outline-none focus:border-neutral-500";

export function IndicatorEditor({
  specs,
  onChange,
  disabled,
}: {
  specs: IndicatorSpec[];
  onChange: (next: IndicatorSpec[]) => void;
  disabled: boolean;
}) {
  // 「添加指标」总加 EMA20，而 EMA20 已在列表里时会被去重掉——
  // 表现为「按钮点了没反应」。用一条会自己消失的提示说清发生了什么。
  const [addIgnored, setAddIgnored] = useState(false);

  const problem = indicatorProblem(specs);

  const problemText =
    problem === "too_many"
      ? S.kline.indicatorLimit(MAX_INDICATORS)
      : problem === "period_range"
        ? S.kline.indicatorPeriodRange(MAX_INDICATOR_PERIOD)
        : null;

  const replace = (index: number, next: IndicatorSpec) => {
    setAddIgnored(false);
    const list = specs.map((spec, at) => (at === index ? next : spec));
    onChange(dedupeIndicators(list));
  };

  const add = () => {
    const candidate: IndicatorSpec = { kind: "ema", period: DEFAULT_PERIOD.ema };
    const next = dedupeIndicators([...specs, candidate]);
    setAddIgnored(next.length === specs.length);
    onChange(next);
  };

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs text-neutral-500">{S.kline.indicatorsHint}</p>

      {specs.length === 0 ? (
        <p className="text-xs text-neutral-500">{S.kline.noIndicators}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {specs.map((spec, index) => (
            // 用「序号」而不是「种类 + 周期」当 key：后者在周期变化时会换 key，
            // React 会**重建整个行**，输入框在每次按键后失焦——多位数周期根本打不进去。
            // 行内所有控件都是受控的，复用 DOM 节点不会残留旧状态。
            <li key={index} className="flex flex-wrap items-center gap-2">
              <label className="flex items-center gap-2 text-xs text-neutral-400">
                <span className="sr-only">
                  {S.kline.indicatorKind}
                </span>
                <select
                  className={INPUT_CLASS}
                  value={spec.kind}
                  disabled={disabled}
                  onChange={(event) => {
                    const kind = event.target.value as IndicatorKind;
                    // 换种类时把周期带回该种类的常用档位：EMA 的 20 对 RSI 没意义
                    replace(index, { kind, period: DEFAULT_PERIOD[kind] });
                  }}
                >
                  {INDICATOR_KINDS.map((kind) => (
                    <option key={kind} value={kind}>
                      {S.kline.indicatorKinds[kind]}
                    </option>
                  ))}
                </select>
              </label>

              <label className="flex items-center gap-2 text-xs text-neutral-400">
                <span>{S.kline.indicatorPeriod}</span>
                <input
                  className={`${INPUT_CLASS} w-20`}
                  type="number"
                  min={1}
                  max={MAX_INDICATOR_PERIOD}
                  value={spec.period}
                  disabled={disabled}
                  onChange={(event) =>
                    replace(index, {
                      kind: spec.kind,
                      // 空输入按 0 处理交给校验提示，而不是悄悄变成上一个值
                      period: Number.parseInt(event.target.value, 10) || 0,
                    })
                  }
                />
              </label>

              <span className="rounded border border-neutral-700 px-1.5 py-0.5 font-mono text-xs text-neutral-400">
                {`${INDICATOR_LABEL[spec.kind]}${spec.period}`}
              </span>

              <button
                type="button"
                disabled={disabled}
                onClick={() => onChange(specs.filter((_, at) => at !== index))}
                className="rounded-md border border-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {S.kline.removeIndicator}
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          disabled={disabled || specs.length >= MAX_INDICATORS}
          onClick={add}
          className="rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {S.kline.addIndicator}
        </button>
        <span className="text-xs text-neutral-500">
          {`${specs.length} / ${MAX_INDICATORS}`}
        </span>
      </div>

      {problemText === null ? null : (
        <p className="text-xs text-amber-300">{problemText}</p>
      )}

      {addIgnored ? (
        <p className="text-xs text-neutral-400">{S.kline.indicatorDuplicate}</p>
      ) : null}
    </div>
  );
}
