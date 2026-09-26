// @vitest-environment jsdom
/**
 * 生成提示词面板（实盘页 / 复盘页共用）的行为测试：
 * - **显式点击才生成**：挂载不发任何 `prompt_build_*` 请求；
 * - 模板下拉只列当前 kind 的模板，换模板后生成用的是选中的那个；
 * - 请求形状：live 传 credential_id / force，review 传 credential_id / from / to / bar
 *   （「未保存正文」的入参随编辑器预览一起删掉了，这里不该再出现）；
 * - 隐私等级只读全局（`privacy_get`），页面没有隐私选择器；读到之前生成按钮禁用；
 * - 后端 `Template` 错误原样展示；「管理模板」交给 `onOpenLibrary`。
 *
 * 只 mock IPC 出口（lib/ipc），组件与 hook 全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PromptOutput, PromptTemplate } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
import { PromptPanel, type PromptPanelContext } from "./PromptPanel";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

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

const LIVE_BUILTIN = templateOf({});
const LIVE_USER = templateOf({
  id: "user_1",
  name: "我的实盘模板",
  description: "",
  body: "USER BODY",
  builtin: false,
  updated_at: 1,
});
const REVIEW = templateOf({
  id: "review_performance",
  name: "复盘：绩效与归因",
  description: "统计 + 归因",
  kind: "review",
  body: "REVIEW BODY",
});
const MARKET = templateOf({
  id: "market_trend",
  name: "行情：趋势速览",
  description: "K 线与指标",
  kind: "market",
  body: "MARKET BODY",
});

const TEMPLATES: PromptTemplate[] = [LIVE_BUILTIN, LIVE_USER, REVIEW, MARKET];

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
    case "privacy_get":
      return Promise.resolve("L1");
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

async function renderPanel(
  context: PromptPanelContext,
  onOpenLibrary?: () => void,
  blockedReason: string | null = null,
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
        <PromptPanel
          context={context}
          {...(onOpenLibrary === undefined ? {} : { onOpenLibrary })}
          blockedReason={blockedReason}
        />
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

function templateSelect(host: HTMLElement): HTMLSelectElement {
  const found = host.querySelector("select");
  if (!(found instanceof HTMLSelectElement)) throw new Error("模板下拉未渲染");
  return found;
}

function optionTexts(host: HTMLElement): string[] {
  return [...templateSelect(host).options].map((option) => option.textContent ?? "");
}

async function click(target: HTMLElement) {
  await act(async () => {
    target.click();
  });
  await flush();
}

async function pickTemplate(host: HTMLElement, id: string) {
  const select = templateSelect(host);
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
    setter?.call(select, id);
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await flush();
}

function callsOf(cmd: string) {
  return callMock.mock.calls.filter((callArgs) => callArgs[0] === cmd);
}

const LIVE_CONTEXT: PromptPanelContext = { kind: "live", credentialId: "cred-1", force: false };

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

describe("PromptPanel 按需生成", () => {
  it("挂载不发任何生成请求：模板与隐私是「读」，生成要等点击", async () => {
    const host = await renderPanel(LIVE_CONTEXT);

    expect(callsOf("prompt_build_live")).toHaveLength(0);
    expect(callsOf("prompt_build_review")).toHaveLength(0);
    // 列表与隐私等级仍要读，否则连按钮该不该可用都无从判断
    expect(callsOf("template_list")).toHaveLength(1);
    expect(callsOf("privacy_get")).toHaveLength(1);
    expect(host.textContent).toContain(S.prompt.preview.empty);
  });

  it("live 生成：请求带 credential_id 与 force，且不含已删除的 body", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "privacy_get" ? Promise.resolve("L2") : defaultMock(cmd),
    );
    const host = await renderPanel({ kind: "live", credentialId: "cred-1", force: true });

    await click(button(host, S.prompt.generate.run));

    expect(callsOf("prompt_build_live")).toHaveLength(1);
    expect(callsOf("prompt_build_live")[0]?.[1]).toEqual({
      request: {
        template_id: LIVE_BUILTIN.id,
        privacy: "L2",
        credential_id: "cred-1",
        force: true,
      },
    });
    // 生成结果照常展示
    expect(host.textContent).toContain("PROMPT TEXT");
  });

  it("review 生成：请求带本页的 credential_id / from / to / bar", async () => {
    const host = await renderPanel({
      kind: "review",
      credentialId: "cred-9",
      from: 1_700_000_000_000,
      to: 1_700_086_400_000,
      bar: "4H",
    });

    await click(button(host, S.prompt.generate.run));

    expect(callsOf("prompt_build_review")).toHaveLength(1);
    expect(callsOf("prompt_build_review")[0]?.[1]).toEqual({
      request: {
        template_id: REVIEW.id,
        privacy: "L1",
        credential_id: "cred-9",
        from: 1_700_000_000_000,
        to: 1_700_086_400_000,
        bar: "4H",
      },
    });
  });

  it("换一个模板后生成用的是选中的那个", async () => {
    const host = await renderPanel(LIVE_CONTEXT);

    await pickTemplate(host, LIVE_USER.id);
    await click(button(host, S.prompt.generate.run));

    expect(callsOf("prompt_build_live")[0]?.[1]).toMatchObject({
      request: { template_id: LIVE_USER.id },
    });
  });
});

describe("PromptPanel 模板下拉", () => {
  it("live 上下文只列 live 模板（复盘的模板不能在这里被选中）", async () => {
    const host = await renderPanel(LIVE_CONTEXT);

    expect(templateSelect(host).value).toBe(LIVE_BUILTIN.id);
    // 内置模板带角标，自定义模板不带
    expect(optionTexts(host)).toEqual([
      `${LIVE_BUILTIN.name}（${S.prompt.templates.builtinBadge}）`,
      LIVE_USER.name,
    ]);
  });

  it("review 上下文只列 review 模板", async () => {
    const host = await renderPanel({
      kind: "review",
      credentialId: null,
      from: 1,
      to: 2,
      bar: "1H",
    });

    expect(templateSelect(host).value).toBe(REVIEW.id);
    expect(optionTexts(host)).toEqual([
      `${REVIEW.name}（${S.prompt.templates.builtinBadge}）`,
    ]);
  });
});

describe("PromptPanel 隐私等级（只读全局）", () => {
  it("说明文字来自 privacy_get 的值，且页面上没有隐私选择器", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "privacy_get" ? Promise.resolve("L2") : defaultMock(cmd),
    );
    const host = await renderPanel(LIVE_CONTEXT);

    expect(host.textContent).toContain(
      S.prompt.generate.privacyNote(S.settings.privacy.L2.label),
    );
    expect(host.textContent).not.toContain(
      S.prompt.generate.privacyNote(S.settings.privacy.L0.label),
    );
    // 等级是全局设置：这里没有可选档位，也不会写回
    expect(hasButton(host, S.settings.privacy.L0.label)).toBe(false);
    expect(hasButton(host, S.settings.privacy.L2.label)).toBe(false);
    expect(callsOf("privacy_set")).toHaveLength(0);
  });

  it("隐私等级还没读回来时生成按钮禁用，读到后恢复（避免用错等级生成）", async () => {
    let releasePrivacy: ((level: string) => void) | undefined;
    callMock.mockImplementation((cmd: string) => {
      if (cmd !== "privacy_get") return defaultMock(cmd);
      return new Promise<string>((resolve) => {
        releasePrivacy = resolve;
      });
    });
    const host = await renderPanel(LIVE_CONTEXT);

    expect(button(host, S.prompt.generate.run).disabled).toBe(true);

    await act(async () => {
      releasePrivacy?.("L1");
    });
    await flush();

    expect(button(host, S.prompt.generate.run).disabled).toBe(false);
    expect(host.textContent).toContain(
      S.prompt.generate.privacyNote(S.settings.privacy.L1.label),
    );
  });
});

describe("PromptPanel 上游禁用", () => {
  /**
   * 复盘页时段不合法时，「生成计划」与「装配」都被拦住，生成按钮也必须一起拦住——
   * 否则同一页面上三个动作对同一份时段给出不一致的反应（前两个拦住、
   * 第三个放行到后端才报 RangeTooLarge）。
   */
  it("上游给出禁用原因时按钮禁用、原因显示、且不发请求", async () => {
    const host = await renderPanel(
      { kind: "review", credentialId: "cred-1", from: 1, to: 2, bar: "1H" },
      undefined,
      "结束时间必须晚于开始时间。",
    );

    expect(host.textContent).toContain("结束时间必须晚于开始时间。");

    const generate = button(host, S.prompt.generate.run);
    expect(generate.disabled).toBe(true);

    await click(generate);
    expect(callMock).not.toHaveBeenCalledWith("prompt_build_review", expect.anything());
  });
});

describe("PromptPanel 错误与入口", () => {
  it("后端 Template 错误原样展示（变量名是改模板的唯一线索）", async () => {
    const message = "模板引用了上下文中不存在的变量：alpha、beta";
    callMock.mockImplementation((cmd: string) =>
      cmd === "prompt_build_live"
        ? Promise.reject({ code: "Template", message, retryable: false })
        : defaultMock(cmd),
    );
    const host = await renderPanel(LIVE_CONTEXT);

    await click(button(host, S.prompt.generate.run));

    expect(host.textContent).toContain(S.prompt.preview.errorTitle);
    expect(host.textContent).toContain(message);
    expect(host.textContent).toContain("alpha");
  });

  it("「管理模板」调用 onOpenLibrary", async () => {
    const onOpenLibrary = vi.fn();
    const host = await renderPanel(LIVE_CONTEXT, onOpenLibrary);

    await click(button(host, S.prompt.generate.manage));
    expect(onOpenLibrary).toHaveBeenCalledTimes(1);
  });

  it("未传 onOpenLibrary 时「管理模板」禁用（没有可跳转的目标）", async () => {
    const host = await renderPanel(LIVE_CONTEXT);

    expect(button(host, S.prompt.generate.manage).disabled).toBe(true);
  });
});
