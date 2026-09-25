// @vitest-environment jsdom
/**
 * 引导流程的行为测试：三种形态（full / unlock / reconfigure）的闸门、
 * 不可找回警告、只读警示、标的数上下限、完成时写 onboarding_done。
 *
 * 只 mock IPC 出口（lib/ipc），页面逻辑与校验函数全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CredentialMeta, CredentialProbe, WatchlistCandidate } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
import { OnboardingPage, type OnboardingMode } from "./OnboardingPage";
import { WATCHLIST_LIMIT } from "./validation";

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

const PROBE_TRADING: CredentialProbe = {
  ok: true,
  permissions: "read_only,trade",
  uid_masked: "12****34",
  pos_mode: "net_mode",
  read_only: false,
  error: null,
};

const PROBE_FAILED: CredentialProbe = {
  ok: false,
  permissions: null,
  uid_masked: null,
  pos_mode: null,
  read_only: true,
  error: "网络请求失败：超时",
};

function makeCandidates(count: number, preselected: number): WatchlistCandidate[] {
  return Array.from({ length: count }, (_, index) => ({
    inst_id: `C${index}-USDT-SWAP`,
    last: 100 + index,
    volume_24h_usd: 1_000_000 - index * 1_000,
    preselected: index < preselected,
  }));
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

async function renderPage(
  mode: OnboardingMode,
  vaultExists: boolean,
  onDone: () => void = vi.fn(),
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(
      <QueryClientProvider client={client}>
        <OnboardingPage mode={mode} vaultExists={vaultExists} onDone={onDone} />
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

function textInputs(host: HTMLElement): HTMLInputElement[] {
  return [...host.querySelectorAll("input")].filter((el) => el.type !== "checkbox");
}

function checkboxes(host: HTMLElement): HTMLInputElement[] {
  return [...host.querySelectorAll("input")].filter((el) => el.type === "checkbox");
}

async function setValue(input: HTMLInputElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function click(target: HTMLElement) {
  await act(async () => {
    target.click();
  });
  await flush();
}

/** 填完第 2 步的四个必填项 */
async function fillCredentialForm(host: HTMLElement) {
  const inputs = textInputs(host);
  await setValue(inputs[0]!, "主账户");
  await setValue(inputs[1]!, "key");
  await setValue(inputs[2]!, "secret");
  await setValue(inputs[3]!, "pass");
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

describe("引导第 1 步：密钥库", () => {
  it("创建模式明确提示主密码无法找回", async () => {
    callMock.mockResolvedValue({ exists: false, unlocked: false });
    const host = await renderPage("full", false);
    expect(host.textContent).toContain(S.onboarding.vault.irrecoverableTitle);
    expect(host.textContent).toContain(S.onboarding.vault.irrecoverable);
  });

  it("两次密码不一致时禁止提交，且不发起任何 IPC", async () => {
    callMock.mockResolvedValue({ exists: false, unlocked: false });
    const host = await renderPage("full", false);
    const inputs = textInputs(host);
    await setValue(inputs[0]!, "correct-horse");
    await setValue(inputs[1]!, "correct-hors");
    await flush();

    expect(host.textContent).toContain(S.onboarding.vault.mismatch);
    const submit = button(host, S.onboarding.vault.create);
    expect(submit.disabled).toBe(true);
    await click(submit);
    expect(callMock).not.toHaveBeenCalled();
  });

  it("必须勾选「已知晓无法找回」后才能创建；成功后进入第 2 步", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "vault_unlock"
        ? Promise.resolve({ exists: true, unlocked: true })
        : Promise.resolve(null),
    );
    const host = await renderPage("full", false);
    const inputs = textInputs(host);
    await setValue(inputs[0]!, "pw");
    await setValue(inputs[1]!, "pw");
    await flush();

    expect(button(host, S.onboarding.vault.create).disabled).toBe(true);
    await click(checkboxes(host)[0]!);
    expect(button(host, S.onboarding.vault.create).disabled).toBe(false);

    await click(button(host, S.onboarding.vault.create));
    expect(callMock).toHaveBeenCalledWith("vault_unlock", { password: "pw" });
    expect(host.textContent).toContain(S.onboarding.credential.title);
  });

  it("密钥库已存在时进入解锁形态，不再显示创建警告", async () => {
    callMock.mockResolvedValue({ exists: true, unlocked: false });
    const host = await renderPage("full", true);
    expect(host.textContent).toContain(S.onboarding.vault.unlockTitle);
    expect(host.textContent).not.toContain(S.onboarding.vault.irrecoverableTitle);
  });

  it("解锁失败时展示错误载荷的 message 与 code", async () => {
    callMock.mockRejectedValue({ code: "Vault", message: "主密码错误", retryable: false });
    const host = await renderPage("unlock", true);
    await setValue(textInputs(host)[0]!, "wrong");
    await flush();
    await click(button(host, S.onboarding.vault.unlock));
    expect(host.textContent).toContain("主密码错误");
    expect(host.textContent).toContain(S.onboarding.errorCode("Vault"));
  });

  it("unlock 形态解锁成功后直接回调 onDone，不进入第 2 / 3 步", async () => {
    const onDone = vi.fn();
    callMock.mockImplementation((cmd: string) =>
      cmd === "vault_unlock"
        ? Promise.resolve({ exists: true, unlocked: true })
        : Promise.resolve(null),
    );
    const host = await renderPage("unlock", true, onDone);
    expect(host.textContent).toContain(S.onboarding.unlockTitle);
    expect(host.textContent).not.toContain(S.onboarding.steps.credential);

    await setValue(textInputs(host)[0]!, "pw");
    await click(button(host, S.onboarding.vault.unlock));
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(callMock).not.toHaveBeenCalledWith("onboarding_complete");
  });
});

describe("引导第 2 步：OKX 凭据（reconfigure 形态从第 2 步开始）", () => {
  it("保存后自动测试；带交易权限时给出红色警示", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_save") return Promise.resolve(META);
      if (cmd === "credentials_test") return Promise.resolve(PROBE_TRADING);
      return Promise.resolve(null);
    });
    const host = await renderPage("reconfigure", true);
    await fillCredentialForm(host);
    await click(button(host, S.onboarding.credential.save));

    expect(callMock).toHaveBeenCalledWith("credentials_save", {
      input: {
        id: null,
        label: "主账户",
        api_key: "key",
        secret_key: "secret",
        passphrase: "pass",
        demo: false,
      },
    });
    expect(callMock).toHaveBeenCalledWith("credentials_test", { id: "cred-1" });
    expect(host.textContent).toContain(S.onboarding.credential.readOnlyFalseTitle);
    expect(host.textContent).toContain("无法用它下单");
    expect(host.textContent).toContain("12****34");
    expect(host.textContent).toContain("净持仓");
    expect(host.textContent).toContain("read_only,trade");
  });

  it("测试失败时展示原因，并允许重试", async () => {
    let testCalls = 0;
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "credentials_save") return Promise.resolve(META);
      if (cmd === "credentials_test") {
        testCalls += 1;
        return Promise.resolve(PROBE_FAILED);
      }
      return Promise.resolve(null);
    });
    const host = await renderPage("reconfigure", true);
    await fillCredentialForm(host);
    await click(button(host, S.onboarding.credential.save));

    expect(host.textContent).toContain("网络请求失败：超时");
    await click(button(host, S.onboarding.credential.retryTest));
    expect(testCalls).toBe(2);
    expect(callMock).toHaveBeenLastCalledWith("credentials_test", { id: "cred-1" });
  });

  it("可跳过：明确写出「跳过也能只看行情」，跳过进入第 3 步", async () => {
    callMock.mockResolvedValue([]);
    const host = await renderPage("reconfigure", true);
    expect(host.textContent).toContain(S.onboarding.credential.skipNote);
    expect(hasButton(host, S.onboarding.credential.skip)).toBe(true);
    await click(button(host, S.onboarding.credential.skip));
    expect(host.textContent).toContain(S.onboarding.watchlist.title);
  });
});

describe("引导第 3 步：标的集", () => {
  async function renderWatchlist(list: WatchlistCandidate[], onDone?: () => void) {
    callMock.mockImplementation((cmd: string) => Promise.resolve(cmd === "watchlist_candidates" ? list : null));
    const host = await renderPage("reconfigure", true, onDone);
    await click(button(host, S.onboarding.credential.skip));
    return host;
  }

  it("preselected 的候选默认勾选，并显示「当前 X / 10」", async () => {
    const host = await renderWatchlist(makeCandidates(5, 3));
    expect(checkboxes(host).filter((box) => box.checked)).toHaveLength(3);
    expect(host.textContent).toContain(S.onboarding.watchlist.count(3, WATCHLIST_LIMIT));
  });

  it("超过上限时禁止提交，并说明「标的数乘进复盘请求量」的原因", async () => {
    const host = await renderWatchlist(makeCandidates(12, 3));
    for (let index = 0; index < 12; index += 1) {
      const box = checkboxes(host)[index];
      if (box !== undefined && !box.checked) await click(box);
    }
    expect(host.textContent).toContain(S.onboarding.watchlist.tooMany(WATCHLIST_LIMIT));

    const submit = button(host, S.onboarding.watchlist.submit);
    expect(submit.disabled).toBe(true);
    await click(submit);
    expect(callMock).not.toHaveBeenCalledWith("watchlist_set", expect.anything());
  });

  it("全部取消勾选时禁止提交（至少 1 个）", async () => {
    const host = await renderWatchlist(makeCandidates(5, 2));
    for (let index = 0; index < 5; index += 1) {
      const box = checkboxes(host)[index];
      if (box !== undefined && box.checked) await click(box);
    }
    expect(host.textContent).toContain(S.onboarding.watchlist.needOne);
    expect(button(host, S.onboarding.watchlist.submit).disabled).toBe(true);
  });

  it("保存预勾选的标的集后调用 onboarding_complete 并完成引导", async () => {
    const onDone = vi.fn();
    const host = await renderWatchlist(makeCandidates(5, 3), onDone);
    await click(button(host, S.onboarding.watchlist.submit));

    expect(callMock).toHaveBeenCalledWith("watchlist_set", {
      watchlist: ["C0-USDT-SWAP", "C1-USDT-SWAP", "C2-USDT-SWAP"],
    });
    expect(callMock).toHaveBeenCalledWith("onboarding_complete");
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("保存失败时展示后端错误（后端会校验上限）且不完成引导", async () => {
    const onDone = vi.fn();
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "watchlist_candidates") return Promise.resolve(makeCandidates(5, 3));
      if (cmd === "watchlist_set") {
        return Promise.reject({ code: "Config", message: "标的集最多 10 个，当前 11 个", retryable: false });
      }
      return Promise.resolve(null);
    });
    const host = await renderPage("reconfigure", true, onDone);
    await click(button(host, S.onboarding.credential.skip));
    await click(button(host, S.onboarding.watchlist.submit));

    expect(host.textContent).toContain("标的集最多 10 个，当前 11 个");
    expect(onDone).not.toHaveBeenCalled();
  });
});

describe("完整引导（full 形态）", () => {
  it("三步走完才写 onboarding_done 并进入主界面", async () => {
    const onDone = vi.fn();
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "vault_unlock") return Promise.resolve({ exists: true, unlocked: true });
      if (cmd === "watchlist_candidates") return Promise.resolve(makeCandidates(5, 3));
      return Promise.resolve(null);
    });
    const host = await renderPage("full", false, onDone);

    const inputs = textInputs(host);
    await setValue(inputs[0]!, "pw");
    await setValue(inputs[1]!, "pw");
    await flush();
    await click(checkboxes(host)[0]!);
    await click(button(host, S.onboarding.vault.create));
    expect(host.textContent).toContain(S.onboarding.credential.title);
    expect(callMock).not.toHaveBeenCalledWith("onboarding_complete");

    await click(button(host, S.onboarding.credential.skip));
    await click(button(host, S.onboarding.watchlist.submit));

    expect(callMock).toHaveBeenCalledWith("onboarding_complete");
    expect(onDone).toHaveBeenCalledTimes(1);
  });
});
