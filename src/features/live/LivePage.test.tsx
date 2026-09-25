// @vitest-environment jsdom
/**
 * LivePage 的行为冒烟测试：新鲜度提示、warnings 优先于空态、
 * 「数据不可得」渲染、错误态的重试按钮门控、刷新按钮的 force 语义。
 *
 * 只 mock IPC 出口（lib/ipc），页面自身逻辑全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { LiveSnapshot, MarketState } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { LivePage } from "./LivePage";

declare global {
  // React 19 的 act 环境标记（jsdom 下必须显式打开）
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const state: MarketState = {
  inst_id: "BTC-USDT-SWAP",
  ts: 1_700_000_000_000,
  last: 65_432.1,
  change_pct: 0.0123,
  high_24h: 66_000,
  low_24h: 64_000,
  volume_24h_usd: 1_234_567_890,
  funding_rate: 0.0001,
  funding_annualized: 0.1095,
  next_funding_rate: 0.00012,
  next_funding_time: 1_700_028_800_000,
  open_interest_usd: null,
  basis_pct: -0.0012,
  long_short_ratio: 1.24,
  taker_buy_sell_ratio: 1.05,
  ema20: null,
  ema60: null,
  ema200: null,
  rsi14: 54.3,
  atr_pct: 0.0087,
  realized_vol: 0.42,
  trend: null,
  vol: "High",
  crowding: "LongCrowded",
  signals: ["资金费率年化 +42.0%，多头拥挤度偏高"],
  unavailable: ["趋势：判定需要至少 200 根 K 线"],
};

function snapshotOf(overrides: Partial<LiveSnapshot> = {}): LiveSnapshot {
  return {
    ts: Date.now(),
    cache_hit: false,
    fetched_at: Date.now(),
    watchlist: ["BTC-USDT-SWAP"],
    instruments: [state],
    warnings: [],
    ...overrides,
  };
}

let root: Root | undefined;
let container: HTMLDivElement | undefined;

/**
 * React Query 的 notifyManager 用 setTimeout 批量通知，必须推进一次宏任务才能看到结果。
 *
 * 注：`Promise.withResolvers()` 需要 lib ES2024，而本项目 tsconfig 固定 lib ES2022
 * （不在本任务可改范围内），因此这里保留 executor 形式。
 */
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
        <LivePage />
      </QueryClientProvider>,
    );
  });
  await flush();
  return container;
}

function refreshButton(host: HTMLElement): HTMLButtonElement {
  const button = [...host.querySelectorAll("button")].find(
    (b) => b.textContent === "刷新" || b.textContent === "刷新中…",
  );
  if (button === undefined) throw new Error("刷新按钮未渲染");
  return button as HTMLButtonElement;
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  callMock.mockReset();
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container?.remove();
  root = undefined;
  container = undefined;
});

describe("LivePage", () => {
  it("进入页面即拉取一次，且不带 force", async () => {
    callMock.mockResolvedValue(snapshotOf());
    await renderPage();
    expect(callMock).toHaveBeenCalledWith("live_refresh");
  });

  it("命中缓存时明确写出「缓存于 X 前」，绝不静默展示陈旧数据", async () => {
    callMock.mockResolvedValue(
      snapshotOf({ cache_hit: true, fetched_at: Date.now() - 3 * 60_000 }),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("命中缓存");
    expect(host.textContent).toContain("缓存于 3 分钟前");
    expect(host.textContent).toContain("本次未发起网络请求");
  });

  it("未命中缓存时写「刚刚拉取」", async () => {
    callMock.mockResolvedValue(snapshotOf({ cache_hit: false }));
    const host = await renderPage();
    expect(host.textContent).toContain("实时拉取");
    expect(host.textContent).toContain("刚刚拉取");
  });

  it("卡片展示 signals、unavailable 与 null 徽章的数据不可得", async () => {
    callMock.mockResolvedValue(snapshotOf());
    const host = await renderPage();
    expect(host.textContent).toContain("资金费率年化 +42.0%，多头拥挤度偏高");
    expect(host.textContent).toContain("趋势：判定需要至少 200 根 K 线");
    // trend 为 null → 徽章不隐藏，显示「数据不可得」；vol / crowding 正常映射
    expect(host.textContent).toContain("趋势数据不可得");
    expect(host.textContent).toContain("高波动");
    expect(host.textContent).toContain("多头拥挤");
    // open_interest_usd 为 null → 该指标也必须显式写不可得
    expect(host.textContent).toContain("持仓量数据不可得");
  });

  it("instruments 为空但 warnings 非空时，显示警告而不是只显示空态", async () => {
    callMock.mockResolvedValue(
      snapshotOf({ instruments: [], warnings: ["BTC-USDT-SWAP 拉取失败：网络请求失败：超时"] }),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("本次刷新的问题");
    expect(host.textContent).toContain("BTC-USDT-SWAP 拉取失败：网络请求失败：超时");
    expect(host.textContent).toContain("本次刷新没有任何标的成功");
    expect(host.textContent).not.toContain("关注列表为空");
  });

  it("IPC 失败且 retryable 时显示错误信息与重试按钮", async () => {
    callMock.mockRejectedValue({ code: "Http", message: "网络请求失败：超时", retryable: true });
    const host = await renderPage();
    expect(host.textContent).toContain("拉取失败");
    expect(host.textContent).toContain("网络请求失败：超时");
    expect(host.textContent).toContain("错误代码：Http");
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "重试")).toBe(true);
  });

  it("retryable 为 false 时不显示重试按钮", async () => {
    callMock.mockRejectedValue({ code: "Migration", message: "迁移失败", retryable: false });
    const host = await renderPage();
    expect(host.textContent).toContain("迁移失败");
    expect(host.textContent).toContain("该错误重试无法自愈");
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "重试")).toBe(false);
  });

  it("刷新按钮传 force: true，请求期间禁用并显示加载态", async () => {
    callMock.mockResolvedValueOnce(snapshotOf());
    const host = await renderPage();

    let release!: (value: LiveSnapshot) => void;
    const pending = new Promise<LiveSnapshot>((resolve) => {
      release = resolve;
    });
    callMock.mockImplementationOnce(() => pending);

    const button = refreshButton(host);
    await act(async () => {
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    await flush();

    expect(callMock).toHaveBeenLastCalledWith("live_refresh", { force: true });
    expect(button.disabled).toBe(true);
    expect(button.textContent).toBe("刷新中…");

    await act(async () => {
      release(snapshotOf({ cache_hit: false }));
    });
    await flush();
    expect(button.disabled).toBe(false);
    expect(button.textContent).toBe("刷新");
  });
});
