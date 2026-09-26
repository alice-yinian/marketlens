// @vitest-environment jsdom
/**
 * LivePage 的行为冒烟测试：新鲜度提示、warnings 优先于空态、
 * 「数据不可得」渲染、错误态的重试按钮门控、刷新按钮的 force 语义，
 * 以及页面自带的「生成提示词」区（凭据 / 强制刷新 / 模板下拉）。
 *
 * 只 mock IPC 出口（lib/ipc），页面自身逻辑全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CredentialMeta,
  LiveSnapshot,
  MarketState,
  PromptTemplate,
} from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
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

const LIVE_TEMPLATE: PromptTemplate = {
  id: "live_quick",
  name: "实盘速览",
  description: "市场状态 + 当前持仓",
  kind: "live",
  body: "LIVE BODY {{ meta.generated_at }}",
  builtin: true,
  updated_at: 0,
};

/**
 * 平铺的 IPC 桩。
 *
 * 页面现在除了行情快照，还要读凭据（实盘上下文）与模板 / 隐私等级（生成区），
 * 所以按命令分派，而不是「一律返回同一份快照」——后者会让生成区拿到对象当列表用。
 */
function defaultMock(cmd: string): Promise<unknown> {
  switch (cmd) {
    case "live_refresh":
      return Promise.resolve(snapshotOf());
    case "credentials_list":
      return Promise.resolve([credentialOf()]);
    case "template_list":
      return Promise.resolve([LIVE_TEMPLATE]);
    case "privacy_get":
      return Promise.resolve("L1");
    case "prompt_build_live":
      return Promise.resolve(null);
    default:
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
  }
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

function button(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) => b.textContent === text);
  if (found === undefined) throw new Error(`按钮未渲染：${text}`);
  return found as HTMLButtonElement;
}

function optionTexts(select: HTMLSelectElement): string[] {
  return [...select.options].map((option) => option.textContent ?? "");
}

async function click(target: HTMLElement) {
  await act(async () => {
    target.click();
  });
  await flush();
}

function callsOf(cmd: string) {
  return callMock.mock.calls.filter((callArgs) => callArgs[0] === cmd);
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  callMock.mockReset();
  callMock.mockImplementation((cmd: string) => defaultMock(cmd));
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container?.remove();
  root = undefined;
  container = undefined;
});

describe("LivePage", () => {
  it("进入页面即拉取一次，且不带 force", async () => {
    await renderPage();
    expect(callMock).toHaveBeenCalledWith("live_refresh");
  });

  it("命中缓存时明确写出「缓存于 X 前」，绝不静默展示陈旧数据", async () => {
    // 快照在渲染前就构造好：`fetched_at` 必须早于页面时钟的起点，否则「X 分钟前」会少一分钟
    const cached = snapshotOf({ cache_hit: true, fetched_at: Date.now() - 3 * 60_000 });
    callMock.mockImplementation((cmd: string) =>
      cmd === "live_refresh" ? Promise.resolve(cached) : defaultMock(cmd),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("命中缓存");
    expect(host.textContent).toContain("缓存于 3 分钟前");
    expect(host.textContent).toContain("本次未发起网络请求");
  });

  it("未命中缓存时写「刚刚拉取」", async () => {
    const host = await renderPage();
    expect(host.textContent).toContain("实时拉取");
    expect(host.textContent).toContain("刚刚拉取");
  });

  it("卡片展示 signals、unavailable 与 null 徽章的数据不可得", async () => {
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
    callMock.mockImplementation((cmd: string) =>
      cmd === "live_refresh"
        ? Promise.resolve(
            snapshotOf({
              instruments: [],
              warnings: ["BTC-USDT-SWAP 拉取失败：网络请求失败：超时"],
            }),
          )
        : defaultMock(cmd),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("本次刷新的问题");
    expect(host.textContent).toContain("BTC-USDT-SWAP 拉取失败：网络请求失败：超时");
    expect(host.textContent).toContain("本次刷新没有任何标的成功");
    expect(host.textContent).not.toContain("关注列表为空");
  });

  it("IPC 失败且 retryable 时显示错误信息与重试按钮", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "live_refresh"
        ? Promise.reject({ code: "Http", message: "网络请求失败：超时", retryable: true })
        : defaultMock(cmd),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("拉取失败");
    expect(host.textContent).toContain("网络请求失败：超时");
    expect(host.textContent).toContain("错误代码：Http");
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "重试")).toBe(true);
  });

  it("retryable 为 false 时不显示重试按钮", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "live_refresh"
        ? Promise.reject({ code: "Migration", message: "迁移失败", retryable: false })
        : defaultMock(cmd),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("迁移失败");
    expect(host.textContent).toContain("该错误重试无法自愈");
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "重试")).toBe(false);
  });

  it("刷新按钮传 force: true，请求期间禁用并显示加载态", async () => {
    let release!: (value: LiveSnapshot) => void;
    const pending = new Promise<LiveSnapshot>((resolve) => {
      release = resolve;
    });
    let refreshes = 0;
    callMock.mockImplementation((cmd: string) => {
      if (cmd !== "live_refresh") return defaultMock(cmd);
      refreshes += 1;
      // 首次进入页面照常返回；「刷新」那一次挂起，好观察加载态
      return refreshes === 1 ? Promise.resolve(snapshotOf()) : pending;
    });
    const host = await renderPage();

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

describe("LivePage 生成区", () => {
  it("页面自带生成区：凭据下拉 + 强制刷新勾选 + 当前 kind 的模板下拉", async () => {
    const host = await renderPage();

    expect(host.textContent).toContain(S.prompt.context.liveTitle);
    expect(host.textContent).toContain(S.prompt.generate.title);

    // 两个下拉：实盘上下文的凭据、生成区的模板（行情模板不该出现在这里）
    const selects = [...host.querySelectorAll("select")];
    expect(selects).toHaveLength(2);
    const credentialSelect = selects[0];
    const templateSelect = selects[1];
    if (credentialSelect === undefined || templateSelect === undefined) {
      throw new Error("下拉未渲染");
    }
    expect(optionTexts(credentialSelect)).toEqual([
      S.prompt.context.noCredential,
      `${credentialOf().label} · ${credentialOf().api_key_masked} · ${S.account.envLive}`,
    ]);
    expect(optionTexts(templateSelect)).toEqual([
      `${LIVE_TEMPLATE.name}（${S.prompt.templates.builtinBadge}）`,
    ]);

    // 强制刷新是可点的勾选，且勾上后传给生成请求
    const force = host.querySelector('input[type="checkbox"]');
    if (!(force instanceof HTMLInputElement)) throw new Error("强制刷新勾选未渲染");
    expect(force.checked).toBe(false);

    // 挂载只读模板 / 隐私，不生成
    expect(callsOf("prompt_build_live")).toHaveLength(0);

    await click(force);
    expect(force.checked).toBe(true);

    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
      setter?.call(credentialSelect, credentialOf().id);
      credentialSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await flush();

    await click(button(host, S.prompt.generate.run));

    expect(callsOf("prompt_build_live")).toHaveLength(1);
    expect(callsOf("prompt_build_live")[0]?.[1]).toMatchObject({
      request: { credential_id: credentialOf().id, force: true },
    });
  });
});
