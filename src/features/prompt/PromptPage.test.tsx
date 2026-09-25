// @vitest-environment jsdom
/**
 * 提示词页的行为测试：隐私等级切换触发重新生成、渲染错误原样展示、
 * warnings 展示、内置模板只能另存为、token/字符数展示、复盘 90 天上限友好提示。
 *
 * 只 mock IPC 出口（lib/ipc），页面与 hook 逻辑全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CredentialMeta, PromptOutput, PromptTemplate } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
import { MAX_RANGE_MS, msToLocalInput } from "./format";
import { PromptPage } from "./PromptPage";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const DAY_MS = 24 * 60 * 60 * 1000;

function templateOf(overrides: Partial<PromptTemplate>): PromptTemplate {
  return {
    id: "live_quick",
    name: "实盘速览",
    description: "市场状态 + 当前持仓",
    kind: "live",
    body: "LIVE BODY {{ meta.generated_at }}",
    builtin: true,
    updated_at: 0,
    ...overrides,
  };
}

const TEMPLATES: PromptTemplate[] = [
  templateOf({}),
  templateOf({
    id: "user_1",
    name: "我的实盘模板",
    description: "",
    builtin: false,
    updated_at: 1,
  }),
  templateOf({
    id: "review_performance",
    name: "复盘：绩效与归因",
    description: "统计 + 归因",
    kind: "review",
    body: "REVIEW BODY",
  }),
];

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

function outputOf(overrides: Partial<PromptOutput> = {}): PromptOutput {
  return {
    text: "PROMPT TEXT",
    token_estimate: 123,
    char_count: 456,
    privacy: "L1",
    template_id: "live_quick",
    template_name: "实盘速览",
    warnings: [],
    generated_at: 1_700_000_000_000,
    ...overrides,
  };
}

function defaultMock(cmd: string): Promise<unknown> {
  switch (cmd) {
    case "template_list":
      return Promise.resolve(TEMPLATES);
    case "credentials_list":
      return Promise.resolve([credentialOf()]);
    case "prompt_build_live":
    case "prompt_build_review":
      return Promise.resolve(outputOf());
    default:
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
  }
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
        <PromptPage />
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

function buttonContaining(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) =>
    (b.textContent ?? "").includes(text),
  );
  if (found === undefined) throw new Error(`按钮未渲染（包含）：${text}`);
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

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

function setTextareaValue(textarea: HTMLTextAreaElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
  setter?.call(textarea, value);
  textarea.dispatchEvent(new Event("input", { bubbles: true }));
}

function buildCalls(cmd: string) {
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

describe("PromptPage", () => {
  it("默认选中第一个模板并按 L1 生成预览", async () => {
    const host = await renderPage();
    expect(host.textContent).toContain("实盘速览");
    const calls = buildCalls("prompt_build_live");
    expect(calls.length).toBeGreaterThan(0);
    expect(calls[0]?.[1]).toMatchObject({ request: { privacy: "L1" } });
  });

  it("切换隐私等级会立即触发重新生成（不是只改状态）", async () => {
    const host = await renderPage();
    const before = buildCalls("prompt_build_live").length;

    await click(buttonContaining(host, S.prompt.privacy.L2.label));

    const calls = buildCalls("prompt_build_live");
    expect(calls.length).toBeGreaterThan(before);
    expect(calls.at(-1)?.[1]).toMatchObject({ request: { privacy: "L2" } });
    expect(host.textContent).toContain(S.prompt.privacy.L2.desc);
  });

  it("后端返回的 Template 渲染错误被原样展示，不吞掉", async () => {
    const message = "模板引用了上下文中不存在的变量：alpha、beta";
    callMock.mockImplementation((cmd: string) =>
      cmd === "prompt_build_live"
        ? Promise.reject({ code: "Template", message, retryable: false })
        : defaultMock(cmd),
    );
    const host = await renderPage();

    expect(host.textContent).toContain(S.prompt.preview.errorTitle);
    expect(host.textContent).toContain(message);
    expect(host.textContent).toContain("alpha");
  });

  it("warnings 会被逐条展示", async () => {
    const warning = "账户数据不可得：尚未配置凭据";
    callMock.mockImplementation((cmd: string) =>
      cmd === "prompt_build_live"
        ? Promise.resolve(outputOf({ warnings: [warning] }))
        : defaultMock(cmd),
    );
    const host = await renderPage();

    expect(host.textContent).toContain(S.prompt.preview.warningsTitle);
    expect(host.textContent).toContain(warning);
  });

  it("展示 token 估算与字符数", async () => {
    const host = await renderPage();
    expect(host.textContent).toContain(S.prompt.preview.tokens(123));
    expect(host.textContent).toContain(S.prompt.preview.chars(456));
  });

  it("内置模板不能直接保存，只能另存为（另存为传 null id）", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "template_save"
        ? Promise.resolve(
            templateOf({ id: "user_2", name: "实盘速览（副本）", builtin: false }),
          )
        : defaultMock(cmd),
    );
    const host = await renderPage();

    expect(hasButton(host, S.prompt.actions.save)).toBe(false);
    expect(hasButton(host, S.prompt.actions.saveAs)).toBe(true);

    await click(button(host, S.prompt.actions.saveAs));

    const saveCalls = buildCalls("template_save");
    expect(saveCalls).toHaveLength(1);
    expect(saveCalls[0]?.[1]).toMatchObject({
      request: { id: null, kind: "live", name: "实盘速览（副本）" },
    });
  });

  it("用户模板可以保存（带 id 覆盖）并可删除", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "template_save") {
        return Promise.resolve(templateOf({ id: "user_1", name: "我的实盘模板", builtin: false }));
      }
      if (cmd === "template_delete") return Promise.resolve(undefined);
      return defaultMock(cmd);
    });
    const host = await renderPage();

    await click(buttonContaining(host, "我的实盘模板"));
    expect(hasButton(host, S.prompt.actions.save)).toBe(true);

    await click(button(host, S.prompt.actions.save));
    expect(buildCalls("template_save")[0]?.[1]).toMatchObject({
      request: { id: "user_1" },
    });

    await click(button(host, S.prompt.actions.delete));
    await click(button(host, S.prompt.actions.deleteConfirm));
    expect(buildCalls("template_delete")[0]?.[1]).toEqual({ id: "user_1" });
  });

  it("编辑正文 300ms 防抖后带 body 重新预览", async () => {
    const host = await renderPage();
    const textarea = host.querySelector("textarea");
    if (!(textarea instanceof HTMLTextAreaElement)) throw new Error("未找到正文编辑器");

    await act(async () => {
      setTextareaValue(textarea, "EDITED BODY {{ x }}");
    });
    // 防抖窗口内不应带着新正文请求（此前仍是 template_id 路径）
    expect(
      buildCalls("prompt_build_live").some(
        (callArgs) =>
          (callArgs[1] as { request?: { body?: string | null } }).request?.body ===
          "EDITED BODY {{ x }}",
      ),
    ).toBe(false);

    await act(async () => {
      await new Promise<void>((resolve) => {
        setTimeout(resolve, 350);
      });
    });
    await flush();

    const calls = buildCalls("prompt_build_live");
    expect(calls.at(-1)?.[1]).toMatchObject({ request: { body: "EDITED BODY {{ x }}" } });
  });

  it("复盘模板时段超 90 天时给出友好提示（不是错误码）", async () => {
    const host = await renderPage();

    await click(buttonContaining(host, "复盘：绩效与归因"));

    const inputs = [...host.querySelectorAll('input[type="datetime-local"]')];
    const fromInput = inputs[0];
    if (!(fromInput instanceof HTMLInputElement)) throw new Error("未找到开始时间输入");

    await act(async () => {
      setInputValue(fromInput, msToLocalInput(Date.now() - MAX_RANGE_MS - DAY_MS));
    });
    await flush();

    expect(host.textContent).toContain(S.prompt.context.rangeTooLarge);
    expect(host.textContent).not.toContain("RangeTooLarge");
  });
});
