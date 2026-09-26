// @vitest-environment jsdom
/**
 * 顶层装配的闸门测试：bootstrap_state 的三种组合分别进入引导 / 只解锁 / 主界面，
 * 以及主界面「重新配置」入口会以 reconfigure 形态打开引导。
 *
 * 这是最容易回归的一处——引导第 1 步解锁后一旦回写缓存，
 * 就会被顶层判定为「已解锁」而跳过第 2、3、4 步。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BootstrapState } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import App from "../../App";
import { S } from "../../lib/strings";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
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

function mockIpc(state: BootstrapState) {
  callMock.mockImplementation((cmd: string) => {
    if (cmd === "bootstrap_state") return Promise.resolve(state);
    if (cmd === "app_info") {
      return Promise.resolve({
        app_version: "0.1.0",
        core_version: "0.1.0",
        schema_version: 3,
        target_os: "linux",
      });
    }
    if (cmd === "live_refresh") {
      return Promise.resolve({
        ts: Date.now(),
        cache_hit: false,
        fetched_at: Date.now(),
        watchlist: [],
        instruments: [],
        warnings: [],
      });
    }
    if (cmd === "credentials_list") return Promise.resolve([]);
    if (cmd === "theme_get") return Promise.resolve("light");
    return Promise.resolve(null);
  });
}

async function renderApp() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(
      <QueryClientProvider client={client}>
        <App />
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

describe("App 闸门", () => {
  it("引导未完成时进入完整引导页，不显示主界面标签", async () => {
    mockIpc({
      vault_exists: false,
      vault_unlocked: false,
      onboarding_done: false,
      watchlist: [],
    });
    const host = await renderApp();
    expect(host.textContent).toContain(S.onboarding.title);
    expect(host.textContent).toContain(S.onboarding.steps.credential);
    expect(host.textContent).not.toContain(S.nav.account);
    expect(callMock).toHaveBeenCalledWith("bootstrap_state");
  });

  it("引导已完成但密钥库未解锁时只显示解锁形态", async () => {
    mockIpc({
      vault_exists: true,
      vault_unlocked: false,
      onboarding_done: true,
      watchlist: ["BTC-USDT-SWAP"],
    });
    const host = await renderApp();
    expect(host.textContent).toContain(S.onboarding.unlockTitle);
    expect(host.textContent).not.toContain(S.onboarding.steps.credential);
    expect(host.textContent).not.toContain(S.nav.account);
  });

  it("引导完成且已解锁时显示「实盘 / 账户」标签", async () => {
    mockIpc({
      vault_exists: true,
      vault_unlocked: true,
      onboarding_done: true,
      watchlist: ["BTC-USDT-SWAP"],
    });
    const host = await renderApp();
    expect(host.textContent).toContain(S.nav.live);
    expect(host.textContent).toContain(S.nav.account);
    expect(host.textContent).not.toContain(S.onboarding.title);
  });

  it("标签顺序是 实盘 · 行情 · 复盘 · 账户 · 提示词 · 设置（先看当下，配置类靠后）", async () => {
    mockIpc({
      vault_exists: true,
      vault_unlocked: true,
      onboarding_done: true,
      watchlist: [],
    });
    const host = await renderApp();

    const nav = host.querySelector("nav");
    if (!(nav instanceof HTMLElement)) throw new Error("顶部标签未渲染");
    const labels = [...nav.querySelectorAll("button")].map((b) => b.textContent);
    // 末尾那个是「重新配置」入口，不属于标签
    expect(labels.filter((label) => label !== S.nav.configure)).toEqual([
      S.nav.live,
      S.nav.kline,
      S.nav.review,
      S.nav.account,
      S.nav.prompt,
      S.nav.settings,
    ]);
  });

  /// 主题是**全局副作用**，由顶层应用一次：设置页只负责改值。
  /// 这条守住「有人在顶层调用它」——漏了的话设置页点了没反应，而且很难查。
  it("顶层把主题写到 <html data-theme>（设置存的是 light 时）", async () => {
    mockIpc({
      vault_exists: true,
      vault_unlocked: true,
      onboarding_done: true,
      watchlist: [],
    });
    await renderApp();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("切换到「账户」标签渲染账户页", async () => {
    mockIpc({
      vault_exists: true,
      vault_unlocked: true,
      onboarding_done: true,
      watchlist: [],
    });
    const host = await renderApp();
    await click(button(host, S.nav.account));
    expect(host.textContent).toContain(S.account.title);
    expect(callMock).toHaveBeenCalledWith("credentials_list");
  });

  it("「重新配置」入口以 reconfigure 形态打开引导（不含第 1 步，从代理开始）", async () => {
    mockIpc({
      vault_exists: true,
      vault_unlocked: true,
      onboarding_done: true,
      watchlist: [],
    });
    const host = await renderApp();
    await click(button(host, S.nav.configure));
    expect(host.textContent).toContain(S.onboarding.reconfigureTitle);
    expect(host.textContent).toContain(S.onboarding.proxy.title);
    expect(host.textContent).not.toContain(S.onboarding.vault.irrecoverableTitle);
  });
});
