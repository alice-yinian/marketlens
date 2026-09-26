/**
 * 行情页：单标的 × 多周期的原始 K 线 + 逐根指标 → 给 AI 的分析提示词。
 *
 * 流程与复盘页同构：调参数 → 「生成计划」（**不联网**，先看「要拉多少、多久、
 * 提示词多长」）→ 「开始取数」（进度 + 可取消）→ 选模板 → 「生成提示词」→ 预览 / 导出。
 *
 * 与其它页面的三点不同：
 * 1. **不带账户、不带仓位**：这是纯行情上下文，不需要 API Key 也能用；
 * 2. **没有隐私等级选择器**：行情是公开数据，三个等级产出完全相同的内容，
 *    显示选择器只会让人以为「切到 L2 能脱敏价格」；
 * 3. **改指标不用重新取数**：指标是装配阶段的参数（`kline_build`），
 *    所以「换个指标看看」是纯本地操作。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import type { IndicatorSpec, PromptOutput } from "../../lib/types";
import { ErrorPanel } from "../account/ErrorPanel";
import { ExportBar } from "../prompt/ExportBar";
import { formatTimestamp } from "../prompt/format";
import { PreviewPanel } from "../prompt/PreviewPanel";
import { TemplateLibrary } from "../prompt/TemplateLibrary";
import { useTemplates } from "../prompt/useTemplates";
import { usePrivacy } from "../settings/usePrivacy";
import { ProgressBar } from "../review/ProgressBar";
import { SeriesReportList } from "../review/SeriesReportList";
import { formatDurationMs } from "../review/format";
import { IndicatorEditor } from "./IndicatorEditor";
import {
  MAX_BARS_PER_PLAN,
  MAX_CANDLES_PER_BAR,
  MAX_TOTAL_CANDLES,
  selectionProblem,
} from "./format";
import {
  useKlineBars,
  useKlineBuild,
  useKlineFetch,
  useKlinePlan,
  useWatchlist,
} from "./useKline";

/** 行情页只认行情模板：实盘/复盘模板引用的是账户字段，这里根本没有。 */
const MARKET_KINDS = ["market"] as const;

const INPUT_CLASS =
  "rounded-md border border-neutral-700 bg-neutral-900 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-neutral-500";

const BUTTON_CLASS =
  "rounded-md bg-neutral-100 px-4 py-2 text-sm font-medium text-neutral-900 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50";

export function KlinePage() {
  const bars = useKlineBars();
  const watchlist = useWatchlist();
  const templates = useTemplates();
  // 行情上下文里没有敏感数值，等级对它没有实际影响；这里显示它只是为了让
  // 「隐私等级在哪设」这个问题在每个生成页都有答案。
  const privacy = usePrivacy();

  const planMutation = useKlinePlan();
  const fetchState = useKlineFetch();
  const buildMutation = useKlineBuild();

  const [pickedInst, setPickedInst] = useState<string | null>(null);
  const [pickedBars, setPickedBars] = useState<string[]>(["1H"]);
  const [candleCount, setCandleCount] = useState(200);
  const [indicators, setIndicators] = useState<IndicatorSpec[]>([]);
  const [pickedTemplate, setPickedTemplate] = useState<string | null>(null);

  const instruments = watchlist.data ?? [];
  // 选中的标的被移出关注列表时回落到第一个；不做额外 effect 同步。
  const instId =
    pickedInst !== null && instruments.includes(pickedInst)
      ? pickedInst
      : (instruments[0] ?? null);

  const marketTemplates = (templates.data ?? []).filter(
    (template) => template.kind === "market",
  );
  const templateId =
    pickedTemplate !== null && marketTemplates.some((t) => t.id === pickedTemplate)
      ? pickedTemplate
      : (marketTemplates[0]?.id ?? null);

  const problem = selectionProblem({ instId, bars: pickedBars, candleCount });
  const problemText =
    problem === "inst"
      ? S.kline.errors.emptyInstrument
      : problem === "bars"
        ? S.kline.errors.emptyBars
        : problem === "too_many_bars"
          ? S.kline.errors.tooManyBars(MAX_BARS_PER_PLAN)
          : problem === "count"
            ? S.kline.errors.countRange(MAX_CANDLES_PER_BAR)
            : problem === "total"
              ? S.kline.errors.tooManyCandles(pickedBars.length * candleCount, MAX_TOTAL_CANDLES)
              : null;

  const plan = planMutation.data ?? null;
  const report = fetchState.report;
  const output: PromptOutput | null = buildMutation.data ?? null;

  const generatePlan = () => {
    if (instId === null || problem !== null) return;
    fetchState.reset();
    buildMutation.reset();
    planMutation.mutate({
      inst_id: instId,
      bars: pickedBars,
      candle_count: candleCount,
    });
  };

  const runFetch = () => {
    if (plan === null) return;
    buildMutation.reset();
    fetchState.start(plan.id);
  };

  const buildPrompt = () => {
    if (plan === null) return;
    buildMutation.mutate({
      plan_id: plan.id,
      template_id: templateId,
      indicators,
    });
  };

  const toggleBar = (value: string) => {
    setPickedBars((prev) =>
      prev.includes(value) ? prev.filter((bar) => bar !== value) : [...prev, value],
    );
  };

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
      <header>
        <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{S.kline.title}</h1>
        <p className="mt-0.5 text-sm text-neutral-400">{S.kline.subtitle}</p>
      </header>

      <section className="flex flex-col gap-4 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
        {watchlist.isPending ? (
          <p className="text-xs text-neutral-500">{S.kline.instrumentLoading}</p>
        ) : watchlist.isError ? (
          <ErrorPanel
            title={S.kline.instrumentErrorTitle}
            error={watchlist.error}
            onRetry={() => void watchlist.refetch()}
            busy={watchlist.isFetching}
          />
        ) : instruments.length === 0 ? (
          <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
            <p className="text-sm text-amber-200">{S.kline.instrumentEmpty}</p>
            <p className="mt-1 text-xs text-amber-300/80">{S.kline.instrumentEmptyHint}</p>
          </div>
        ) : (
          <label className="flex flex-wrap items-center gap-2 text-sm text-neutral-300">
            {S.kline.instrument}
            <select
              className={INPUT_CLASS}
              value={instId ?? ""}
              disabled={fetchState.isFetching}
              onChange={(event) => setPickedInst(event.target.value)}
            >
              {instruments.map((inst) => (
                <option key={inst} value={inst}>
                  {inst}
                </option>
              ))}
            </select>
          </label>
        )}

        {/* 没有标的就没什么可配的：继续渲染表单只会让用户白填一遍——
            他填完才会发现「生成计划」永远是灰的。这里直接不渲染。 */}
        {instId === null ? null : (
          <>
            <div className="flex flex-col gap-2">
              <span className="text-sm text-neutral-300">{S.kline.bars}</span>
              {bars.isPending ? (
                <p className="text-xs text-neutral-500">{S.kline.barsLoading}</p>
              ) : bars.isError ? (
                <ErrorPanel
                  title={S.kline.barsErrorTitle}
                  error={bars.error}
                  onRetry={() => void bars.refetch()}
                  busy={bars.isFetching}
                />
              ) : (bars.data ?? []).length === 0 ? (
                <p className="text-xs text-amber-300">{S.kline.barsEmpty}</p>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {(bars.data ?? []).map((option) => {
                    const active = pickedBars.includes(option.value);
                    return (
                      <button
                        key={option.value}
                        type="button"
                        aria-pressed={active}
                        disabled={fetchState.isFetching}
                        onClick={() => toggleBar(option.value)}
                        className={`rounded-full border px-3 py-1 text-xs ${
                          active
                            ? "border-neutral-100 bg-neutral-100 text-neutral-900"
                            : "border-neutral-700 text-neutral-300 hover:bg-neutral-800"
                        } disabled:cursor-not-allowed disabled:opacity-50`}
                      >
                        {option.label}
                      </button>
                    );
                  })}
                </div>
              )}
              <p className="text-xs text-neutral-500">{S.kline.barsHint}</p>
            </div>

            <label className="flex flex-wrap items-center gap-2 text-sm text-neutral-300">
              {S.kline.count}
              <input
                className={`${INPUT_CLASS} w-28`}
                type="number"
                min={1}
                max={MAX_CANDLES_PER_BAR}
                value={candleCount}
                disabled={fetchState.isFetching}
                onChange={(event) =>
                  setCandleCount(Number.parseInt(event.target.value, 10) || 0)
                }
              />
              <span className="text-xs text-neutral-500">
                {S.kline.totalNote(pickedBars.length * candleCount, MAX_TOTAL_CANDLES)}
              </span>
            </label>

            <div className="flex flex-col gap-2">
              <span className="text-sm text-neutral-300">{S.kline.indicators}</span>
              <IndicatorEditor
                specs={indicators}
                onChange={setIndicators}
                disabled={fetchState.isFetching}
              />
            </div>

            {problemText === null ? null : (
              <p className="text-sm text-amber-300">{problemText}</p>
            )}

            <div className="flex flex-wrap items-center gap-3">
              <button
                type="button"
                className={BUTTON_CLASS}
                disabled={problem !== null || planMutation.isPending || fetchState.isFetching}
                onClick={generatePlan}
              >
                {planMutation.isPending ? S.kline.planning : S.kline.plan}
              </button>
              <span className="text-xs text-neutral-500">{S.kline.fetchHint}</span>
            </div>
          </>
        )}
      </section>

      {planMutation.isError ? (
        <ErrorPanel
          title={S.kline.planErrorTitle}
          error={planMutation.error}
          onRetry={generatePlan}
          busy={planMutation.isPending}
        />
      ) : null}

      {plan === null ? null : (
        <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
          <h2 className="text-sm font-medium text-neutral-200">{S.kline.planTitle}</h2>

          <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-neutral-400">
            <span>{S.kline.planRequests(plan.est_requests)}</span>
            <span>{S.kline.planDuration(formatDurationMs(plan.est_duration_ms))}</span>
            <span>{S.kline.planTokens(plan.est_tokens)}</span>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-neutral-800 text-xs text-neutral-500">
                  <th className="py-2 pr-3 font-medium">{S.kline.planBars}</th>
                  <th className="py-2 pr-3 text-right font-medium">{S.kline.planRows}</th>
                  <th className="py-2 pr-3 text-right font-medium">{S.kline.planPages}</th>
                  <th className="py-2 font-medium">{S.kline.planRange}</th>
                </tr>
              </thead>
              <tbody>
                {plan.bars.map((item) => (
                  <tr key={item.bar} className="border-b border-neutral-900/80 last:border-0">
                    <td className="py-2 pr-3 text-neutral-200">{item.label}</td>
                    <td className="py-2 pr-3 text-right font-mono text-neutral-300">
                      {item.candle_count}
                    </td>
                    <td className="py-2 pr-3 text-right font-mono text-neutral-400">
                      {item.est_pages}
                    </td>
                    <td className="py-2 text-neutral-400">
                      {`${formatTimestamp(item.from)} → ${formatTimestamp(item.to)}`}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {plan.warnings.length === 0 ? null : (
            <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
              <p className="text-xs font-medium text-amber-200">{S.kline.planWarnings}</p>
              <ul className="mt-1 list-disc pl-4 text-xs text-amber-200/90">
                {plan.warnings.map((warning) => (
                  <li key={warning.metric}>{`${warning.metric}：${warning.reason}`}</li>
                ))}
              </ul>
            </div>
          )}

          <div className="flex flex-wrap items-center gap-3">
            <button
              type="button"
              className={BUTTON_CLASS}
              disabled={fetchState.isFetching}
              onClick={() => runFetch()}
            >
              {fetchState.isFetching ? S.kline.fetching : S.kline.fetch}
            </button>
          </div>
        </section>
      )}

      {fetchState.error == null ? null : (
        <ErrorPanel
          title={S.review.errors.fetchTitle}
          error={fetchState.error}
          onRetry={() => runFetch()}
          busy={fetchState.isFetching}
        />
      )}

      {fetchState.isFetching ? (
        <ProgressBar
          progress={fetchState.progress}
          cancelling={fetchState.isCancelling}
          cancelNote={fetchState.cancelNote}
          onCancel={() => {
            if (plan !== null) fetchState.cancel(plan.id);
          }}
        />
      ) : null}

      {report === null ? null : (
        <SeriesReportList reports={report.series} />
      )}

      {report === null || plan === null ? null : (
        <section className="flex flex-col gap-4 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
          <TemplateLibrary
            templates={marketTemplates}
            kinds={MARKET_KINDS}
            loading={templates.isPending}
            error={templates.error}
            onRetry={() => void templates.refetch()}
            selectedId={templateId}
            onSelect={setPickedTemplate}
          />

          <p className="text-xs text-neutral-500">
            {`${S.kline.privacyNote}（当前：${S.settings.privacy[privacy].label}，在设置页统一调整）`}
          </p>

          <div className="flex flex-wrap items-center gap-3">
            <button
              type="button"
              className={BUTTON_CLASS}
              disabled={templateId === null || buildMutation.isPending}
              onClick={buildPrompt}
            >
              {buildMutation.isPending ? S.kline.building : S.kline.build}
            </button>
            <span className="text-xs text-neutral-500">{S.kline.cachedNote}</span>
          </div>
        </section>
      )}

      <PreviewPanel
        output={output}
        error={buildMutation.isError ? buildMutation.error : null}
        isFetching={buildMutation.isPending}
        isPending={buildMutation.isPending && output === null}
        enabled={report !== null}
        templateName={
          marketTemplates.find((template) => template.id === templateId)?.name ?? S.kline.title
        }
      />

      <ExportBar
        output={output}
        // 行情不受隐私分级影响：这里传 L0 只是走既有签名，导出文件名里不会带等级含义
        privacy="L0"
        templateName={
          marketTemplates.find((template) => template.id === templateId)?.name ?? S.kline.title
        }
      />
    </main>
  );
}
