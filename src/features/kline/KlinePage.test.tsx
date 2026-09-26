// @vitest-environment jsdom
/**
 * 行情页行为测试：空关注列表的引导、默认参数、合计上限的门控、指标行的增删与去重、
 * 计划预估展示、取数进度的 `plan_id` 过滤（进度是全局广播）、结果与提示词生成、
 * 模板库按 kind 过滤、以及后端错误的原样展示。
 *
 * 只 mock IPC 出口（lib/ipc），页面、hook 与纯函数全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  BarOption,
  ExecutionReport,
  KlinePlan,
  Progress,
  PromptOutput,
  PromptTemplate,
} from "../../lib/types";

const callMock = vi.fn();
const onFetchProgressMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
  onFetchProgress: (...args: unknown[]) => onFetchProgressMock(...args),
}));

import { S } from "../../lib/strings";
import { formatDurationMs } from "../review/format";
import { DEFAULT_PERIOD, INDICATOR_LABEL, MAX_INDICATORS, MAX_TOTAL_CANDLES } from "./format";
import { KlinePage } from "./KlinePage";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

// ---------------------------------------------------------------------------
// 夹具：形状照 src/lib/types.ts，字段名不猜
// ---------------------------------------------------------------------------

const INSTRUMENTS = ["BTC-USDT-SWAP", "ETH-USDT-SWAP"];

const BARS: BarOption[] = [
  { value: "15m", label: "15 分钟" },
  { value: "1H", label: "1 小时" },
  { value: "4H", label: "4 小时" },
  { value: "1D", label: "1 天" },
];

/** 三种 kind 各一个：行情页只该显示 market 那一个 */
const TEMPLATES: PromptTemplate[] = [
  templateOf({ id: "tpl-live", name: "实盘盯盘模板", kind: "live" }),
  templateOf({ id: "tpl-review", name: "复盘归因模板", kind: "review" }),
  templateOf({ id: "tpl-market", name: "行情结构模板", kind: "market" }),
];

const WARNING = { metric: "4 小时", reason: "最近可用的 4 小时 K 线不足 200 根" };

function templateOf(overrides: Partial<PromptTemplate> = {}): PromptTemplate {
  return {
    id: "tpl-1",
    name: "行情模板",
    description: "把多周期 K 线与指标交给 AI",
    kind: "market",
    body: "# 行情\n{{ candles }}",
    builtin: true,
    updated_at: 1_700_000_000_000,
    ...overrides,
  };
}

function planOf(overrides: Partial<KlinePlan> = {}): KlinePlan {
  return {
    id: "plan-k1",
    inst_id: "BTC-USDT-SWAP",
    bars: [
      {
        bar: "1H",
        label: "1 小时",
        candle_count: 200,
        from: 1_700_000_000_000,
        to: 1_700_086_400_000,
        est_pages: 3,
        series_key: "candles|BTC-USDT-SWAP|1H",
      },
      {
        bar: "4H",
        label: "4 小时",
        candle_count: 137,
        from: 1_600_000_000_000,
        to: 1_700_086_400_000,
        est_pages: 2,
        series_key: "candles|BTC-USDT-SWAP|4H",
      },
    ],
    series: [
      {
        key: "candles|BTC-USDT-SWAP|1H",
        label: "BTC-USDT-SWAP 1H K 线",
        kind: "Candles",
        inst_id: "BTC-USDT-SWAP",
        ccy: null,
        priority: 1,
        est_pages: 3,
      },
    ],
    est_requests: 12,
    est_duration_ms: 9000,
    est_tokens: 3456,
    warnings: [WARNING],
    ...overrides,
  };
}

function reportOf(overrides: Partial<ExecutionReport> = {}): ExecutionReport {
  return {
    plan_id: "plan-k1",
    series: [
      {
        key: "candles|BTC-USDT-SWAP|1H",
        label: "BTC-USDT-SWAP 1H K 线",
        status: "Ok",
        rows: 200,
        pages: 3,
        reason: null,
      },
      {
        key: "candles|BTC-USDT-SWAP|4H",
        label: "BTC-USDT-SWAP 4H K 线",
        status: "Failed",
        rows: 0,
        pages: 0,
        reason: "网络请求失败：超时",
      },
    ],
    done_requests: 12,
    elapsed_ms: 12_500,
    cancelled: false,
    ...overrides,
  };
}

function outputOf(overrides: Partial<PromptOutput> = {}): PromptOutput {
  return {
    text: "## 行情\nBTC-USDT-SWAP 1 小时 200 根",
    token_estimate: 1234,
    char_count: 5678,
    privacy: "L0",
    template_id: "tpl-market",
    template_name: "行情结构模板",
    warnings: [],
    generated_at: 1_700_000_000_000,
    ...overrides,
  };
}

function defaultMock(cmd: string): Promise<unknown> {
  switch (cmd) {
    case "kline_bars":
      return Promise.resolve(BARS);
    case "watchlist_get":
      return Promise.resolve(INSTRUMENTS);
    case "template_list":
      return Promise.resolve(TEMPLATES);
    // 其余命令（kline_plan / kline_fetch / kline_build…）由各用例自行分派
    default:
      return Promise.resolve(null);
  }
}

// ---------------------------------------------------------------------------
// 渲染与交互辅助（与 ReviewPage / SettingsPage 的测试同款）
// ---------------------------------------------------------------------------

let root: Root | undefined;
let container: HTMLDivElement | undefined;

async function flush() {
  await act(async () => {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  });
}

async function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const host = document.createElement("div");
  document.body.appendChild(host);
  container = host;
  root = createRoot(host);
  await act(async () => {
    root?.render(
      <QueryClientProvider client={client}>
        <KlinePage />
      </QueryClientProvider>,
    );
  });
  await flush();
  return host;
}

function button(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) => b.textContent === text);
  if (found === undefined) throw new Error(`按钮未渲染：${text}`);
  return found as HTMLButtonElement;
}

function hasButton(host: HTMLElement, text: string): boolean {
  return [...host.querySelectorAll("button")].some((b) => b.textContent === text);
}

/** 取第 index 行（0 基）；缺失直接失败，而不是把 undefined 往下传 */
function rowAt<T>(rows: T[], index: number): T {
  const row = rows[index];
  if (row === undefined) throw new Error(`第 ${index + 1} 行未渲染`);
  return row;
}

/** 模板行的按钮里除了模板名还有「内置 / 自定义」与类型角标，所以按包含匹配 */
function templateRow(host: HTMLElement, name: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) =>
    (b.textContent ?? "").includes(name),
  );
  if (found === undefined) throw new Error(`模板行未渲染：${name}`);
  return found as HTMLButtonElement;
}

async function click(target: HTMLElement) {
  await act(async () => {
    target.click();
  });
  await flush();
}

/** React 19 受控 input：必须走原生 setter + 事件派发 */
async function setNumberValue(input: HTMLInputElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await flush();
}

/** 受控 `<select>`：换种类走的是 change 事件 */
async function setSelectValue(select: HTMLSelectElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
    setter?.call(select, value);
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await flush();
}

/** 页面上唯一的编号输入框就是「每个周期根数」（指标周期在各自的行里） */
function countInput(host: HTMLElement): HTMLInputElement {
  const found = host.querySelector('input[type="number"]');
  if (found === null) throw new Error("根数输入框未渲染");
  return found as HTMLInputElement;
}

/** 指标行 = 带下拉框的 li；用 select 认行，避免误抓提示词里的 li */
function indicatorRows(host: HTMLElement): HTMLLIElement[] {
  return [...host.querySelectorAll("li")].filter(
    (row) => row.querySelector("select") !== null,
  ) as HTMLLIElement[];
}

function indicatorKinds(host: HTMLElement): HTMLSelectElement[] {
  return indicatorRows(host).map((row) => {
    const select = row.querySelector("select");
    if (select === null) throw new Error("指标种类下拉框未渲染");
    return select as HTMLSelectElement;
  });
}

function indicatorPeriods(host: HTMLElement): HTMLInputElement[] {
  return indicatorRows(host).map((row) => {
    const input = row.querySelector('input[type="number"]');
    if (input === null) throw new Error("指标周期输入框未渲染");
    return input as HTMLInputElement;
  });
}

/** 行尾的「种类 + 周期」角标（如 EMA20）；这是用户核对「和提示词表里的列对得上」的依据 */
function indicatorBadges(host: HTMLElement): string[] {
  return indicatorRows(host).map((row) => {
    const badge = row.querySelector("span.font-mono");
    if (badge === null) throw new Error("指标角标未渲染");
    return badge.textContent ?? "";
  });
}

/** 计划表格的逐行文本（一行一个周期） */
function planRowTexts(host: HTMLElement): string[] {
  return [...host.querySelectorAll("tbody tr")].map((row) => row.textContent ?? "");
}

/** 推一条进度事件；页面还没订阅就说明状态机没接上，直接失败 */
async function pushProgress(handler: ((p: Progress) => void) | undefined, snapshot: Progress) {
  if (handler === undefined) throw new Error("页面尚未订阅取数进度");
  await act(async () => {
    handler(snapshot);
  });
  await flush();
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  callMock.mockReset();
  callMock.mockImplementation((cmd: string) => defaultMock(cmd));
  onFetchProgressMock.mockReset();
  onFetchProgressMock.mockResolvedValue(vi.fn());
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container?.remove();
  root = undefined;
  container = undefined;
});

// ---------------------------------------------------------------------------

describe("KlinePage 参数区", () => {
  it("关注列表为空时只显示提示，不渲染参数表单", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "watchlist_get" ? Promise.resolve([]) : defaultMock(cmd),
    );
    const host = await renderPage();

    expect(host.textContent).toContain(S.kline.instrumentEmpty);
    expect(host.textContent).toContain(S.kline.instrumentEmptyHint);
    // 没有标的可选：连下拉框都没有
    expect(host.querySelector("select")).toBeNull();

    // 参数表单整块不渲染：否则用户会白填一遍（周期、根数、指标都填好了），
    // 最后才发现「生成计划」永远是灰的
    expect(hasButton(host, S.kline.plan)).toBe(false);
    expect(host.textContent).not.toContain(S.kline.count);
    expect(host.textContent).not.toContain(S.kline.noIndicators);
  });

  it("关注列表读取失败时如实报错，不退化成「关注列表为空」", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "watchlist_get"
        ? Promise.reject({ code: "Database", message: "数据库操作失败：disk I/O error", retryable: false })
        : defaultMock(cmd),
    );
    const host = await renderPage();

    expect(host.textContent).toContain(S.kline.instrumentErrorTitle);
    expect(host.textContent).toContain("disk I/O error");
    // 「空」是一个结论，「读失败」是另一个——混在一起会让用户去设置页白找一圈
    expect(host.textContent).not.toContain(S.kline.instrumentEmpty);
  });

  it("周期列表读取失败时给出错误与重试，而不是一片空白", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "kline_bars"
        ? Promise.reject({ code: "Config", message: "Command kline_bars not found", retryable: false })
        : defaultMock(cmd),
    );
    const host = await renderPage();

    expect(host.textContent).toContain(S.kline.barsErrorTitle);
    // 没有周期可选时不能假装「没有周期」也不给任何线索
    expect(host.textContent).not.toContain(S.kline.barsEmpty);
  });

  it("默认参数：选中关注列表第一个标的、周期默认 1H、根数默认 200，并显示合计", async () => {
    const host = await renderPage();

    const instruments = host.querySelector("select") as HTMLSelectElement;
    expect([...instruments.options].map((option) => option.value)).toEqual(INSTRUMENTS);
    expect(instruments.value).toBe("BTC-USDT-SWAP");

    expect(button(host, "1 小时").getAttribute("aria-pressed")).toBe("true");
    expect(button(host, "4 小时").getAttribute("aria-pressed")).toBe("false");

    expect(countInput(host).value).toBe("200");
    expect(host.textContent).toContain(S.kline.totalNote(200, MAX_TOTAL_CANDLES));
    // 参数合法：可以生成计划
    expect(button(host, S.kline.plan).disabled).toBe(false);
    expect(host.textContent).not.toContain(S.kline.errors.emptyInstrument);
  });

  it("合计根数压线放行，超出一根就拦截并给出合计", async () => {
    const host = await renderPage();
    await setNumberValue(countInput(host), "500");
    // 1 × 500：合法
    expect(button(host, S.kline.plan).disabled).toBe(false);

    // 3 × 500 = 1500：刚好等于上限，仍合法
    await click(button(host, "15 分钟"));
    await click(button(host, "4 小时"));
    expect(host.textContent).toContain(S.kline.totalNote(3 * 500, MAX_TOTAL_CANDLES));
    expect(button(host, S.kline.plan).disabled).toBe(false);

    // 4 × 500 = 2000 > 1500：拦住，并说清超了多少
    await click(button(host, "1 天"));
    expect(host.textContent).toContain(S.kline.errors.tooManyCandles(4 * 500, MAX_TOTAL_CANDLES));
    expect(button(host, S.kline.plan).disabled).toBe(true);
  });
});

describe("KlinePage 指标区", () => {
  it("默认 EMA20；重复项被静默去重；换种类带出该种类的默认周期；删空回到说明文案", async () => {
    const host = await renderPage();
    expect(host.textContent).toContain(S.kline.noIndicators);

    await click(button(host, S.kline.addIndicator));
    expect(indicatorBadges(host)).toEqual([`${INDICATOR_LABEL.ema}${DEFAULT_PERIOD.ema}`]);

    // 再点一次：同一个「种类 + 周期」不会变成两行，而且要**说清发生了什么**——
    // 否则用户看到的是「按钮点了没反应」
    await click(button(host, S.kline.addIndicator));
    expect(indicatorRows(host)).toHaveLength(1);
    expect(host.textContent).toContain(S.kline.indicatorDuplicate);

    // 换成 RSI：周期必须跟着变成 14（EMA 的 20 对 RSI 没意义），提示随之消失
    await setSelectValue(rowAt(indicatorKinds(host), 0), "rsi");
    expect(rowAt(indicatorKinds(host), 0).value).toBe("rsi");
    expect(rowAt(indicatorPeriods(host), 0).value).toBe(String(DEFAULT_PERIOD.rsi));
    expect(indicatorBadges(host)).toEqual([`${INDICATOR_LABEL.rsi}${DEFAULT_PERIOD.rsi}`]);
    expect(host.textContent).not.toContain(S.kline.indicatorDuplicate);

    await click(button(host, S.kline.removeIndicator));
    expect(indicatorRows(host)).toHaveLength(0);
    expect(host.textContent).toContain(S.kline.noIndicators);
  });

  it("加到 MAX_INDICATORS 行后「添加指标」封顶", async () => {
    const host = await renderPage();

    // 每次「添加指标」加的都是 EMA20，会被去重；所以加完先把当行周期改掉，
    // 下一次添加才是一条新行（这也顺带证明去重不是「什么都不加」）
    for (let n = 1; n <= MAX_INDICATORS; n += 1) {
      await click(button(host, S.kline.addIndicator));
      expect(indicatorRows(host)).toHaveLength(n);
      await setNumberValue(rowAt(indicatorPeriods(host), n - 1), String(10 + n));
    }

    expect(host.textContent).toContain(`${MAX_INDICATORS} / ${MAX_INDICATORS}`);
    expect(button(host, S.kline.addIndicator).disabled).toBe(true);
  });

  /**
   * 周期输入框在按键后不能被重建。
   *
   * 这条守的是一个真实缺陷：行的 `key` 里一旦含 `period`，每敲一个字符都会换 key，
   * React 就把整行换成新节点——输入框随之失焦，第二位数字根本打不进去，
   * 而界面上看起来一切正常（只是「打不进去」）。
   */
  it("周期输入框在按键后不被重建：两位数能连续输入", async () => {
    const host = await renderPage();
    await click(button(host, S.kline.addIndicator));

    const input = rowAt(indicatorPeriods(host), 0);
    input.focus();
    expect(document.activeElement).toBe(input);

    // 「20 → 2」正是最危险的中间态：值一变，含 period 的 key 就会失效
    await setNumberValue(input, "2");

    expect(host.contains(input)).toBe(true);
    expect(document.activeElement).toBe(input);
  });
});

describe("KlinePage 计划与取数", () => {
  it("生成计划：参数原样提交，并展示预估、逐周期根数与提醒", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "kline_plan" ? Promise.resolve(planOf()) : defaultMock(cmd),
    );
    const host = await renderPage();
    await click(button(host, S.kline.plan));

    expect(callMock).toHaveBeenCalledWith("kline_plan", {
      request: { inst_id: "BTC-USDT-SWAP", bars: ["1H"], candle_count: 200 },
    });

    expect(host.textContent).toContain(S.kline.planRequests(12));
    expect(host.textContent).toContain(S.kline.planTokens(3456));
    expect(host.textContent).toContain(S.kline.planDuration(formatDurationMs(9000)));

    // 每个周期一行：标签与根数都来自后端，前端不复算
    const rows = planRowTexts(host);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toContain("1 小时");
    expect(rows[0]).toContain("200");
    expect(rows[1]).toContain("4 小时");
    expect(rows[1]).toContain("137");

    expect(host.textContent).toContain(WARNING.metric);
    expect(host.textContent).toContain(WARNING.reason);
    expect(hasButton(host, S.kline.fetch)).toBe(true);
  });

  it("取数：进度只认匹配 plan_id 的事件，别的计划的事件被丢弃", async () => {
    let progressHandler: ((p: Progress) => void) | undefined;
    onFetchProgressMock.mockImplementation((handler: (p: Progress) => void) => {
      progressHandler = handler;
      return Promise.resolve(vi.fn());
    });

    let resolveFetch: ((report: ExecutionReport) => void) | undefined;
    const fetchPromise = new Promise<ExecutionReport>((resolve) => {
      resolveFetch = resolve;
    });
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "kline_plan") return Promise.resolve(planOf());
      if (cmd === "kline_fetch") return fetchPromise;
      return defaultMock(cmd);
    });

    const host = await renderPage();
    await click(button(host, S.kline.plan));
    await click(button(host, S.kline.fetch));

    expect(callMock).toHaveBeenCalledWith("kline_fetch", { plan_id: "plan-k1" });
    // 首个进度事件还没到：显示启动态，而不是假装 0%
    expect(host.textContent).toContain(S.review.progress.starting);

    // 进度是全局广播：别的计划的事件必须被丢掉
    await pushProgress(progressHandler, {
      plan_id: "plan-other",
      done_series: 9,
      total_series: 10,
      done_requests: 9,
      total_requests: 10,
      current: "别的计划的序列",
    });
    expect(host.textContent).not.toContain(S.review.progress.seriesProgress(9, 10));
    expect(host.textContent).not.toContain(S.review.progress.current("别的计划的序列"));

    // 匹配当前计划的事件才推进进度条
    await pushProgress(progressHandler, {
      plan_id: "plan-k1",
      done_series: 1,
      total_series: 3,
      done_requests: 4,
      total_requests: 12,
      current: "BTC-USDT-SWAP 1H K 线",
    });
    expect(host.textContent).toContain(S.review.progress.seriesProgress(1, 3));
    expect(host.textContent).toContain(S.review.progress.current("BTC-USDT-SWAP 1H K 线"));
    expect(host.textContent).toContain("33%");
    expect(button(host, S.kline.fetching).disabled).toBe(true);

    await act(async () => {
      resolveFetch?.(reportOf());
    });
    await flush();
    // 结束后进度条收起，换成结果区
    expect(host.textContent).not.toContain(S.review.progress.title);
    expect(host.textContent).toContain(S.review.report.title);
  });

  it("取数完成后逐序列展示结果，并给出「生成提示词」入口", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "kline_plan") return Promise.resolve(planOf());
      if (cmd === "kline_fetch") return Promise.resolve(reportOf());
      return defaultMock(cmd);
    });
    const host = await renderPage();
    await click(button(host, S.kline.plan));
    await click(button(host, S.kline.fetch));

    expect(host.textContent).toContain(S.review.report.title);
    expect(host.textContent).toContain("BTC-USDT-SWAP 1H K 线");
    expect(host.textContent).toContain(S.review.status.Ok);
    expect(host.textContent).toContain("BTC-USDT-SWAP 4H K 线");
    expect(host.textContent).toContain(S.review.status.Failed);
    expect(host.textContent).toContain("网络请求失败：超时");
    expect(host.textContent).toContain(S.review.report.rows);
    expect(host.textContent).toContain(S.review.report.pages);

    expect(button(host, S.kline.build).disabled).toBe(false);
  });
});

describe("KlinePage 提示词与模板", () => {
  it("模板库只列行情模板：实盘 / 复盘模板不出现在行情页", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "kline_plan") return Promise.resolve(planOf());
      if (cmd === "kline_fetch") return Promise.resolve(reportOf());
      return defaultMock(cmd);
    });
    const host = await renderPage();
    await click(button(host, S.kline.plan));
    await click(button(host, S.kline.fetch));

    // 只出现 market 那一个（名字与分组标题各一处）
    expect(host.textContent).toContain("行情结构模板");
    expect(host.textContent).not.toContain("实盘盯盘模板");
    expect(host.textContent).not.toContain("复盘归因模板");
    // 只能选中行情模板，且默认就已选中它
    expect(templateRow(host, "行情结构模板").getAttribute("aria-pressed")).toBe("true");
  });

  it("生成提示词：计划、模板与当时勾选的指标一起提交，预览显示 token 与正文", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "kline_plan") return Promise.resolve(planOf());
      if (cmd === "kline_fetch") return Promise.resolve(reportOf());
      if (cmd === "kline_build") return Promise.resolve(outputOf());
      return defaultMock(cmd);
    });
    const host = await renderPage();

    // 指标是装配阶段的参数：取数前设好，取数后不必重新联网
    await click(button(host, S.kline.addIndicator));
    await setSelectValue(rowAt(indicatorKinds(host), 0), "rsi");

    await click(button(host, S.kline.plan));
    await click(button(host, S.kline.fetch));
    await click(button(host, S.kline.build));

    expect(callMock).toHaveBeenCalledWith("kline_build", {
      request: {
        plan_id: "plan-k1",
        template_id: "tpl-market",
        body: null,
        indicators: [{ kind: "rsi", period: DEFAULT_PERIOD.rsi }],
      },
    });

    const output = outputOf();
    expect(host.textContent).toContain(output.text);
    expect(host.textContent).toContain(S.prompt.preview.tokens(output.token_estimate));
    expect(host.textContent).toContain(S.prompt.preview.chars(output.char_count));
    expect(host.textContent).toContain(S.prompt.preview.templateName("行情结构模板"));
  });

  it("kline_build 失败时原样展示后端消息（模板语法错误是用户唯一的线索）", async () => {
    const message = "模板语法错误：第 3 行 {{ ohlc_json }} 之后缺少 `}}`";
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "kline_plan") return Promise.resolve(planOf());
      if (cmd === "kline_fetch") return Promise.resolve(reportOf());
      if (cmd === "kline_build") {
        return Promise.reject({ code: "Template", message, retryable: false });
      }
      return defaultMock(cmd);
    });
    const host = await renderPage();
    await click(button(host, S.kline.plan));
    await click(button(host, S.kline.fetch));
    await click(button(host, S.kline.build));

    expect(host.textContent).toContain(message);
    expect(host.textContent).toContain(S.prompt.preview.errorTitle);
    expect(host.textContent).toContain(S.account.errorCode("Template"));
    // 顺带给出「照着报错改模板」的指引，而不是只丢一行错误
    expect(host.textContent).toContain(S.prompt.preview.errorHint);
  });
});
