// @vitest-environment jsdom
/**
 * 复盘采集页的行为测试：生成计划的预估与可得性提示、90 天上限的 UI 门控、
 * RangeTooLarge 的友好展示、进度事件驱动进度条、取消语义、结果四态渲染。
 *
 * 只 mock IPC 出口（lib/ipc），页面与 hook 逻辑全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CredentialMeta,
  ExecutionReport,
  FetchPlan,
  Progress,
  PromptOutput,
  PromptTemplate,
  ReviewContext,
} from "../../lib/types";

const callMock = vi.fn();
const onFetchProgressMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
  onFetchProgress: (...args: unknown[]) => onFetchProgressMock(...args),
}));

import { S } from "../../lib/strings";
import { MAX_RANGE_MS, localInputToMs, msToLocalInput } from "./format";
import { ReviewPage } from "./ReviewPage";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

function planOf(overrides: Partial<FetchPlan> = {}): FetchPlan {
  return {
    id: "plan-1",
    from: 1_700_000_000_000,
    to: 1_700_086_400_000,
    inst_ids: ["BTC-USDT-SWAP", "ETH-USDT-SWAP"],
    bar: "1H",
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
    warnings: [
      { metric: "多空账户比", reason: "OKX 的 Rubik 统计只提供最近约 48 小时的数据" },
    ],
    ...overrides,
  };
}

function reportOf(overrides: Partial<ExecutionReport> = {}): ExecutionReport {
  return {
    plan_id: "plan-1",
    series: [
      { key: "a", label: "BTC-USDT-SWAP 1H K 线", status: "Ok", rows: 168, pages: 3, reason: null },
      {
        key: "b",
        label: "BTC 多空账户比",
        status: "Unavailable",
        rows: 0,
        pages: 0,
        reason: "该时段没有数据",
      },
      {
        key: "c",
        label: "BTC 主动买卖量",
        status: "Skipped",
        rows: 0,
        pages: 0,
        reason: "上次已采集完成，跳过",
      },
      {
        key: "d",
        label: "BTC-USDT-SWAP 资金费率",
        status: "Failed",
        rows: 0,
        pages: 0,
        reason: "网络请求失败：超时",
      },
    ],
    done_requests: 11,
    elapsed_ms: 12_500,
    cancelled: false,
    ...overrides,
  };
}

function credentialOf(overrides: Partial<CredentialMeta> = {}): CredentialMeta {
  return {
    id: "cred-1",
    label: "主账户只读",
    env: "live",
    api_key_masked: "abcd****ef12",
    permissions: "read_only",
    uid_masked: "1234****",
    last_ok_at: null,
    last_error: null,
    created_at: 1_700_000_000_000,
    ...overrides,
  };
}

function reviewTemplateOf(overrides: Partial<PromptTemplate> = {}): PromptTemplate {
  return {
    id: "review_performance",
    name: "复盘：绩效与归因",
    description: "统计 + 归因",
    kind: "review",
    body: "REVIEW BODY {{ stats.win_rate }}",
    builtin: true,
    updated_at: 0,
    ...overrides,
  };
}

function promptOutputOf(overrides: Partial<PromptOutput> = {}): PromptOutput {
  return {
    text: "PROMPT TEXT",
    token_estimate: 123,
    char_count: 456,
    privacy: "L1",
    template_id: "review_performance",
    template_name: "复盘：绩效与归因",
    warnings: [],
    generated_at: 1_700_000_000_000,
    ...overrides,
  };
}

function contextOf(overrides: Partial<ReviewContext> = {}): ReviewContext {
  return {
    from: 1_700_000_000_000,
    to: 1_700_086_400_000,
    bar: "1H",
    positions: [
      {
        pos_id: "p1",
        inst_id: "BTC-USDT-SWAP",
        direction: "long",
        mgn_mode: "cross",
        lever: 5,
        open_avg_px: 60_000,
        close_avg_px: 61_000,
        max_contracts: 1,
        pnl: 100,
        pnl_ratio: 0.01,
        realized_pnl: 12.5,
        fee: -1.2,
        funding_fee: -0.3,
        open_time: 1_700_000_000_000,
        close_time: 1_700_003_600_000,
        source: "merged",
        time_precision: "approximate",
        regime: null,
        max_favorable: 30,
        max_adverse: -4,
      },
    ],
    stats: {
      total: 1,
      win_rate: 1,
      profit_factor: null,
      expectancy: 12.5,
      total_realized_pnl: 12.5,
      avg_hold_ms: 3_600_000,
      fee_drag: 0.12,
      by_trend: [
        {
          key: "未归因",
          count: 1,
          win_rate: 1,
          total_pnl: 12.5,
          profit_factor: null,
          is_unattributed: true,
        },
      ],
      by_direction: [
        {
          key: "多",
          count: 1,
          win_rate: 1,
          total_pnl: 12.5,
          profit_factor: null,
          is_unattributed: false,
        },
      ],
      by_instrument: [
        {
          key: "BTC-USDT-SWAP",
          count: 1,
          win_rate: 1,
          total_pnl: 12.5,
          profit_factor: null,
          is_unattributed: false,
        },
      ],
      unattributed: 1,
    },
    merge: {
      from_official: 1,
      from_local: 0,
      enriched: 0,
      coverage_note: "本地无留痕记录：无法提供持仓期间的最大浮盈/浮亏。",
    },
    trace: {
      records: 0,
      has_gaps: false,
      max_gap_ms: 0,
      last_trace_at: null,
      note: "最近 30 天没有本地留痕。",
    },
    warnings: ["官方历史仓位在到达起始时间前已耗尽，该时段可能不完整"],
    ...overrides,
  };
}

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
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(
      <QueryClientProvider client={client}>
        <ReviewPage />
      </QueryClientProvider>,
    );
  });
  await flush();
  return container;
}

function button(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) => b.textContent === text);
  if (found === undefined) throw new Error(`按钮未渲染：${text}`);
  return found as HTMLButtonElement;
}

function hasButton(host: HTMLElement, text: string): boolean {
  return [...host.querySelectorAll("button")].some((b) => b.textContent === text);
}

async function click(target: HTMLElement) {
  await act(async () => {
    target.click();
  });
  await flush();
}

/** React 19 受控 input：必须走原生 setter + 事件派发 */
function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  callMock.mockReset();
  onFetchProgressMock.mockReset();
  onFetchProgressMock.mockResolvedValue(vi.fn());
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container?.remove();
  root = undefined;
  container = undefined;
});

describe("ReviewPage", () => {
  it("生成计划后展示预估、标的集与可得性提示，且「生成计划」不联网的说明可见", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "review_plan" ? Promise.resolve(planOf()) : Promise.reject(new Error("unexpected")),
    );
    const host = await renderPage();
    await click(button(host, S.review.plan.generate));

    expect(callMock).toHaveBeenCalledWith("review_plan", {
      request: { from: expect.any(Number), to: expect.any(Number), bar: "1H" },
    });
    expect(host.textContent).toContain(S.review.plan.offlineNote);
    expect(host.textContent).toContain(S.review.plan.estRequestsValue(12));
    expect(host.textContent).toContain(S.review.duration.seconds(9));
    expect(host.textContent).toContain("BTC-USDT-SWAP");
    expect(host.textContent).toContain("ETH-USDT-SWAP");
    expect(host.textContent).toContain(S.review.plan.seriesCount(1));
    // warnings 逐条醒目展示
    expect(host.textContent).toContain(S.review.plan.warningsTitle);
    expect(host.textContent).toContain("多空账户比");
    expect(host.textContent).toContain("OKX 的 Rubik 统计只提供最近约 48 小时的数据");
    expect(hasButton(host, S.review.plan.start)).toBe(true);
  });

  it("时段超 90 天时禁用「生成计划」并给出原因", async () => {
    callMock.mockRejectedValue(new Error("不应被调用"));
    const host = await renderPage();

    const inputs = [...host.querySelectorAll('input[type="datetime-local"]')];
    const fromInput = inputs[0];
    if (!(fromInput instanceof HTMLInputElement)) throw new Error("未找到开始时间输入");

    await act(async () => {
      setInputValue(fromInput, msToLocalInput(Date.now() - MAX_RANGE_MS - 24 * 60 * 60 * 1000));
    });
    await flush();

    expect(host.textContent).toContain(S.review.range.tooLarge);
    expect(button(host, S.review.plan.generate).disabled).toBe(true);
    expect(callMock).not.toHaveBeenCalledWith("review_plan", expect.anything());
  });

  it("RangeTooLarge 错误展示为友好提示，而不是原始错误码", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "review_plan"
        ? Promise.reject({ code: "RangeTooLarge", message: "时段超出上限：最多 90 天", retryable: false })
        : Promise.reject(new Error("unexpected")),
    );
    const host = await renderPage();
    await click(button(host, S.review.plan.generate));

    expect(host.textContent).toContain(S.review.errors.rangeTooLargeTitle);
    expect(host.textContent).toContain(S.review.errors.rangeTooLarge);
    expect(host.textContent).not.toContain("RangeTooLarge");
  });

  it("采集期间进度事件驱动进度条，且忽略不匹配 plan_id 的事件", async () => {
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
      if (cmd === "review_plan") return Promise.resolve(planOf());
      if (cmd === "review_fetch") return fetchPromise;
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    await click(button(host, S.review.plan.generate));
    await click(button(host, S.review.plan.start));

    expect(callMock).toHaveBeenCalledWith("review_fetch", { plan_id: "plan-1" });
    expect(host.textContent).toContain(S.review.progress.starting);

    // 别的计划的事件必须被忽略
    await act(async () => {
      progressHandler?.({
        plan_id: "other-plan",
        done_series: 99,
        total_series: 100,
        done_requests: 99,
        total_requests: 100,
        current: "别的序列",
      });
    });
    await flush();
    expect(host.textContent).not.toContain("99 / 100");

    // 匹配当前计划的事件推进进度条
    await act(async () => {
      progressHandler?.({
        plan_id: "plan-1",
        done_series: 1,
        total_series: 4,
        done_requests: 3,
        total_requests: 12,
        current: "BTC-USDT-SWAP 1H K 线",
      });
    });
    await flush();

    expect(host.textContent).toContain(S.review.progress.seriesProgress(1, 4));
    expect(host.textContent).toContain("25%");
    expect(host.textContent).toContain(S.review.progress.current("BTC-USDT-SWAP 1H K 线"));
    expect(hasButton(host, S.review.progress.cancel)).toBe(true);
    expect(button(host, S.review.plan.start).disabled).toBe(true);

    await act(async () => {
      resolveFetch?.(reportOf());
    });
    await flush();

    expect(host.textContent).toContain(S.review.report.elapsed(S.review.duration.seconds(13)));
    expect(host.textContent).not.toContain(S.review.progress.title);
  });

  it("采集结果按 status 四态呈现（含 Ok 下的 reason 也显示）", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "review_plan") return Promise.resolve(planOf());
      if (cmd === "review_fetch") {
        return Promise.resolve(
          reportOf({
            series: [
              {
                key: "a",
                label: "BTC K 线",
                status: "Ok",
                rows: 168,
                pages: 3,
                reason: "数据在到达起始时间前已耗尽，该时段可能不完整",
              },
              {
                key: "b",
                label: "多空账户比",
                status: "Unavailable",
                rows: 0,
                pages: 0,
                reason: "该时段没有数据",
              },
              {
                key: "c",
                label: "主动买卖量",
                status: "Skipped",
                rows: 0,
                pages: 0,
                reason: "上次已采集完成，跳过",
              },
              {
                key: "d",
                label: "资金费率",
                status: "Failed",
                rows: 0,
                pages: 0,
                reason: "网络请求失败：超时",
              },
            ],
          }),
        );
      }
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    await click(button(host, S.review.plan.generate));
    await click(button(host, S.review.plan.start));

    expect(host.textContent).toContain(S.review.status.Ok);
    expect(host.textContent).toContain(S.review.status.Unavailable);
    expect(host.textContent).toContain(S.review.status.Skipped);
    expect(host.textContent).toContain(S.review.status.Failed);
    expect(host.textContent).toContain("数据在到达起始时间前已耗尽，该时段可能不完整");
    expect(host.textContent).toContain("该时段没有数据");
    expect(host.textContent).toContain("上次已采集完成，跳过");
    expect(host.textContent).toContain("网络请求失败：超时");
  });

  it("取消返回 false 时提示该计划已结束", async () => {
    let resolveFetch: ((report: ExecutionReport) => void) | undefined;
    const fetchPromise = new Promise<ExecutionReport>((resolve) => {
      resolveFetch = resolve;
    });
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "review_plan") return Promise.resolve(planOf());
      if (cmd === "review_fetch") return fetchPromise;
      if (cmd === "review_cancel") return Promise.resolve(false);
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    await click(button(host, S.review.plan.generate));
    await click(button(host, S.review.plan.start));
    await click(button(host, S.review.progress.cancel));

    expect(callMock).toHaveBeenCalledWith("review_cancel", { plan_id: "plan-1" });
    expect(host.textContent).toContain(S.review.progress.cancelAlreadyDone);

    await act(async () => {
      resolveFetch?.(reportOf({ cancelled: true }));
    });
    await flush();
    expect(host.textContent).toContain(S.review.report.cancelled);
  });

  it("装配复盘：用选中的凭据调用 review_context 并展示结果", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_list") return Promise.resolve([credentialOf()]);
      if (cmd === "review_context") return Promise.resolve(contextOf());
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    expect(hasButton(host, S.review.assemble.run)).toBe(true);

    await click(button(host, S.review.assemble.run));

    expect(callMock).toHaveBeenCalledWith("review_context", {
      request: {
        credential_id: "cred-1",
        from: expect.any(Number),
        to: expect.any(Number),
        bar: "1H",
      },
    });
    // 结果区：来源/精度角标、未归因分组、可信度说明与 warnings 都要出现
    expect(host.textContent).toContain(S.review.result.title);
    expect(host.textContent).toContain(S.review.result.source.merged);
    expect(host.textContent).toContain(S.review.result.precision.approximate);
    expect(host.textContent).toContain(S.review.result.regimeUnavailable);
    expect(host.textContent).toContain(S.review.result.profitFactorNoLosses);
    expect(host.textContent).toContain(S.review.result.trustTitle);
    expect(host.textContent).toContain("本地无留痕记录：无法提供持仓期间的最大浮盈/浮亏。");
    expect(host.textContent).toContain(
      "官方历史仓位在到达起始时间前已耗尽，该时段可能不完整",
    );
  });

  it("装配失败（RangeTooLarge）展示友好提示，而不是原始错误码", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_list") return Promise.resolve([credentialOf()]);
      if (cmd === "review_context") {
        return Promise.reject({
          code: "RangeTooLarge",
          message: "时段超出上限：最多 90 天",
          retryable: false,
        });
      }
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    await click(button(host, S.review.assemble.run));

    expect(host.textContent).toContain(S.review.errors.rangeTooLargeTitle);
    expect(host.textContent).toContain(S.review.errors.assembleRangeTooLarge);
    expect(host.textContent).not.toContain("RangeTooLarge");
  });

  it("没有凭据时装配区提示先添加凭据，且装配按钮不可用", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_list") return Promise.resolve([]);
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    expect(host.textContent).toContain(S.review.assemble.needCredential);
    expect(host.textContent).toContain(S.review.assemble.needCredentialHint);
    expect(button(host, S.review.assemble.run).disabled).toBe(true);
  });

  it("凭据列表读取失败不阻塞采集流程，只在装配区如实说明", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_list") {
        return Promise.reject({ code: "VaultLocked", message: "密钥库未解锁", retryable: true });
      }
      if (cmd === "review_plan") return Promise.resolve(planOf());
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();
    expect(host.textContent).toContain(S.review.assemble.credentialError);
    // 采集流程仍然可用
    await click(button(host, S.review.plan.generate));
    expect(hasButton(host, S.review.plan.start)).toBe(true);
  });

  it("生成区用的是本页的凭据与时段，不另存一份状态", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_list") return Promise.resolve([credentialOf()]);
      if (cmd === "template_list") return Promise.resolve([reviewTemplateOf()]);
      if (cmd === "privacy_get") return Promise.resolve("L1");
      if (cmd === "prompt_build_review") return Promise.resolve(promptOutputOf());
      return Promise.reject(new Error("unexpected"));
    });

    const host = await renderPage();

    // 在页面的时段控件上改成固定值，再从 DOM 读回来当期望值——
    // 期望值不另写一份「默认时段」，否则两边会一起错。
    const fromMs = new Date(2026, 5, 1, 8, 0).getTime();
    const toMs = new Date(2026, 5, 3, 20, 0).getTime();
    const localInputs = [...host.querySelectorAll('input[type="datetime-local"]')];
    const fromInput = localInputs[0];
    const toInput = localInputs[1];
    if (!(fromInput instanceof HTMLInputElement) || !(toInput instanceof HTMLInputElement)) {
      throw new Error("时段输入未渲染");
    }
    await act(async () => {
      setInputValue(fromInput, msToLocalInput(fromMs));
      setInputValue(toInput, msToLocalInput(toMs));
    });
    await flush();

    // 页面上只有 RangePicker 有粒度下拉（装配区只有一条凭据时不是下拉），DOM 顺序即它
    const barSelect = host.querySelector("select");
    if (!(barSelect instanceof HTMLSelectElement)) throw new Error("粒度下拉未渲染");
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
      setter?.call(barSelect, "4H");
      barSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await flush();

    await click(button(host, S.prompt.generate.run));

    expect(localInputToMs(fromInput.value)).toBe(fromMs);
    expect(localInputToMs(toInput.value)).toBe(toMs);
    expect(callMock).toHaveBeenCalledWith("prompt_build_review", {
      request: {
        template_id: "review_performance",
        privacy: "L1",
        credential_id: credentialOf().id,
        from: localInputToMs(fromInput.value),
        to: localInputToMs(toInput.value),
        bar: "4H",
      },
    });
    expect(host.textContent).toContain(promptOutputOf().text);
  });
});
