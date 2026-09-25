// @vitest-environment jsdom
/**
 * 账户 / 仓位页的行为测试：凭据选择、概览与持仓字段、留痕覆盖度、warnings、
 * 错误态的重试门控、刷新按钮的重新拉取语义。
 *
 * 只 mock IPC 出口（lib/ipc），页面逻辑全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AccountSnapshot, CredentialMeta, Position } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
import { AccountPage } from "./AccountPage";

declare global {
  // React 19 的 act 环境标记（jsdom 下必须显式打开）
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const META: CredentialMeta = {
  id: "cred-1",
  label: "主账户",
  env: "live",
  api_key_masked: "abcd****1234",
  permissions: null,
  uid_masked: null,
  last_ok_at: null,
  last_error: null,
  created_at: 1_700_000_000_000,
};

const SECOND_META: CredentialMeta = { ...META, id: "cred-2", label: "备用账户", env: "demo" };

function positionOf(overrides: Partial<Position> = {}): Position {
  return {
    inst_id: "BTC-USDT-SWAP",
    pos_id: "p1",
    pos_side: "long",
    mgn_mode: "cross",
    lever: 10,
    contracts: 3,
    size_base: 0.03,
    avg_px: 60_000,
    mark_px: 61_000,
    liq_px: null,
    upl: 30,
    upl_ratio: 0.0166,
    mgn_ratio: 0.05,
    notional_usd: 1_830,
    imr: 183,
    mmr: 91.5,
    fee: -0.1,
    funding_fee: 0.02,
    realized_pnl: 0,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_100_000,
    ...overrides,
  };
}

function snapshotOf(overrides: Partial<AccountSnapshot> = {}): AccountSnapshot {
  return {
    overview: {
      total_eq_usd: 12_345.67,
      iso_eq_usd: 0,
      adj_eq_usd: 12_345.67,
      avail_eq_usd: 9_000.5,
      upl: -123.45,
      mgn_ratio: 0.1234,
      imr: 100,
      mmr: 50,
      notional_usd: 50_000,
      pos_mode: "long_short_mode",
      currencies: [
        { ccy: "USDT", eq: 1_000, eq_usd: 1_000, avail_bal: 800, cash_bal: 1_000 },
      ],
      fetched_at: 1_700_000_000_000,
    },
    positions: [positionOf()],
    trace: {
      records: 10,
      has_gaps: true,
      max_gap_ms: 21_600_000,
      last_trace_at: 1_700_000_000_000,
      note: "最近 30 天留痕 10 条，存在超过 6 小时的间隙。",
    },
    warnings: [],
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

async function renderPage(onConfigure: () => void = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(
      <QueryClientProvider client={client}>
        <AccountPage onConfigure={onConfigure} />
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

describe("AccountPage", () => {
  it("没有凭据时提示去引导页添加，且不拉取账户", async () => {
    callMock.mockResolvedValue([]);
    const host = await renderPage();
    expect(host.textContent).toContain(S.account.noCredentials);
    expect(host.textContent).toContain(S.account.noCredentialsHint);
    expect(callMock).not.toHaveBeenCalledWith("account_snapshot", expect.anything());
  });

  it("空态里的「去添加凭据」按钮真的能触发重新配置入口", async () => {
    callMock.mockResolvedValue([]);
    const onConfigure = vi.fn();
    const host = await renderPage(onConfigure);
    await click(button(host, S.account.addCredential));
    expect(onConfigure).toHaveBeenCalledTimes(1);
  });

  it("用选中凭据的 id 拉取快照，并渲染概览（含 pos_mode 中文映射与币种明细）", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list" ? Promise.resolve([META]) : Promise.resolve(snapshotOf()),
    );
    const host = await renderPage();

    expect(callMock).toHaveBeenCalledWith("account_snapshot", {
      query: { credential_id: "cred-1" },
    });
    expect(host.textContent).toContain("$12,345.67");
    expect(host.textContent).toContain("$9,000.50");
    expect(host.textContent).toContain("-$123.45");
    expect(host.textContent).toContain("12.34%");
    expect(host.textContent).toContain("长空双向");
    expect(host.textContent).toContain("USDT");
    expect(host.textContent).toContain("abcd****1234");
  });

  it("持仓展示方向 / 保证金模式 / 杠杆 / 张数 + 币数量，强平价为 null 时显示「无」", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list" ? Promise.resolve([META]) : Promise.resolve(snapshotOf()),
    );
    const host = await renderPage();

    expect(host.textContent).toContain("BTC-USDT-SWAP");
    expect(host.textContent).toContain("多头");
    expect(host.textContent).toContain("全仓");
    expect(host.textContent).toContain("10x");
    expect(host.textContent).toContain(S.account.position.contracts);
    expect(host.textContent).toContain(S.account.position.sizeBase);
    expect(host.textContent).toContain("强平价");
    expect(host.textContent).toContain(S.account.liqNone);
  });

  it("未实现盈亏按正负着色", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list" ? Promise.resolve([META]) : Promise.resolve(snapshotOf()),
    );
    const host = await renderPage();
    const mono = [...host.querySelectorAll("div.font-mono")];
    const profit = mono.find((el) => el.textContent === "$30.00");
    const loss = mono.find((el) => el.textContent === "-$123.45");
    expect(profit?.className).toContain("text-emerald-400");
    expect(loss?.className).toContain("text-red-400");
  });

  it("展示留痕说明，has_gaps 时额外提示持仓时长可能不精确", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list" ? Promise.resolve([META]) : Promise.resolve(snapshotOf()),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("最近 30 天留痕 10 条，存在超过 6 小时的间隙。");
    expect(host.textContent).toContain(S.account.traceGap);
  });

  it("warnings 非空时醒目标示", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list"
        ? Promise.resolve([META])
        : Promise.resolve(snapshotOf({ warnings: ["持仓拉取失败：网络请求失败：超时"] })),
    );
    const host = await renderPage();
    expect(host.textContent).toContain(S.account.warningsTitle);
    expect(host.textContent).toContain("持仓拉取失败：网络请求失败：超时");
  });

  it("IPC 失败且 retryable 时显示错误与重试按钮，点击后重新拉取", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list"
        ? Promise.resolve([META])
        : Promise.reject({ code: "Http", message: "网络请求失败：超时", retryable: true }),
    );
    const host = await renderPage();
    expect(host.textContent).toContain(S.account.errorTitle);
    expect(host.textContent).toContain("网络请求失败：超时");
    expect(host.textContent).toContain(S.account.errorCode("Http"));

    const before = callMock.mock.calls.filter(([cmd]) => cmd === "account_snapshot").length;
    await click(button(host, S.account.retry));
    const after = callMock.mock.calls.filter(([cmd]) => cmd === "account_snapshot").length;
    expect(after).toBe(before + 1);
  });

  it("retryable 为 false 时不显示重试按钮", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list"
        ? Promise.resolve([META])
        : Promise.reject({ code: "Vault", message: "凭据不存在", retryable: false }),
    );
    const host = await renderPage();
    expect(host.textContent).toContain("凭据不存在");
    expect(host.textContent).toContain(S.account.errorNotRetryable);
    expect(hasButton(host, S.account.retry)).toBe(false);
  });

  it("刷新按钮重新拉取同一条凭据（不做自动轮询）", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list" ? Promise.resolve([META]) : Promise.resolve(snapshotOf()),
    );
    const host = await renderPage();
    const before = callMock.mock.calls.filter(([cmd]) => cmd === "account_snapshot").length;
    await click(button(host, S.account.refresh));
    const calls = callMock.mock.calls.filter(([cmd]) => cmd === "account_snapshot");
    expect(calls.length).toBe(before + 1);
    expect(calls[calls.length - 1]).toEqual([
      "account_snapshot",
      { query: { credential_id: "cred-1" } },
    ]);
  });

  it("多条凭据时切换选择器会用新的 id 重新拉取", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "credentials_list"
        ? Promise.resolve([META, SECOND_META])
        : Promise.resolve(snapshotOf()),
    );
    const host = await renderPage();
    const select = host.querySelector("select");
    expect(select).not.toBeNull();

    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
      setter?.call(select, "cred-2");
      select?.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await flush();

    expect(callMock).toHaveBeenLastCalledWith("account_snapshot", {
      query: { credential_id: "cred-2" },
    });
  });
});
