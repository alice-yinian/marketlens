// @vitest-environment jsdom
/**
 * 提示词页（纯管理）的行为测试：
 * - 模板库列**全部三类**（实盘 / 复盘 / 行情）并按类型分组；
 * - 选中即同步草稿，重复点选不会冲掉未保存的改动；
 * - 保存 / 另存为 / 删除语义：内置只能另存为、用户模板可覆盖、删除必须二次确认；
 * - 语法校验的三种态：空正文不校验、成功并列出变量、后端报错原样展示；
 * - 页面上**不再**有生成 / 预览 / 导出 / 隐私选择器（它们按上下文挪到了实盘页、复盘页）。
 *
 * 只 mock IPC 出口（lib/ipc），页面与 hook 逻辑全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PromptTemplate } from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
import { PromptPage } from "./PromptPage";

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
  body: "USER BODY {{ position.size }}",
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

/** 语法校验后端返回的变量名（模板正文里引用了什么） */
const CHECKED_VARIABLES = ["meta.generated_at", "position.size"];

function defaultMock(cmd: string): Promise<unknown> {
  switch (cmd) {
    case "template_list":
      return Promise.resolve(TEMPLATES);
    case "template_check":
      return Promise.resolve(CHECKED_VARIABLES);
    case "template_save":
      return Promise.resolve(
        templateOf({ id: "user_2", name: "实盘速览（副本）", builtin: false }),
      );
    case "template_delete":
      return Promise.resolve(undefined);
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

/** 推进真实计时器：语法校验有 300ms 防抖，必须等它落地 */
async function advance(ms: number) {
  await act(async () => {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, ms);
    });
  });
  await flush();
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

function nameInput(host: HTMLElement): HTMLInputElement {
  const found = host.querySelector(`input[placeholder="${S.prompt.editor.namePlaceholder}"]`);
  if (!(found instanceof HTMLInputElement)) throw new Error("模板名称输入框未渲染");
  return found;
}

function bodyTextarea(host: HTMLElement): HTMLTextAreaElement {
  const found = host.querySelector("textarea");
  if (!(found instanceof HTMLTextAreaElement)) throw new Error("正文编辑器未渲染");
  return found;
}

/** React 19 受控 input：必须走原生 setter + 事件派发 */
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

describe("PromptPage 模板库", () => {
  it("列全部三类模板并按类型分组（管理页不再只列实盘 / 复盘）", async () => {
    const host = await renderPage();

    // 四份模板（实盘内置 + 实盘自定义 + 复盘 + 行情）都在
    for (const template of [LIVE_BUILTIN, LIVE_USER, REVIEW, MARKET]) {
      expect(host.textContent).toContain(template.name);
    }
    // 分组标题顺序 = 实盘 / 复盘 / 行情，且行情模板没有被静默漏掉
    const groups = [...host.querySelectorAll("h3")].map((node) => node.textContent);
    expect(groups).toEqual([
      S.prompt.templates.kindLive,
      S.prompt.templates.kindReview,
      S.prompt.templates.kindMarket,
    ]);
  });

  it("纯管理页：不出现生成 / 预览 / 导出 / 隐私选择器", async () => {
    const host = await renderPage();

    expect(hasButton(host, S.prompt.generate.run)).toBe(false);
    expect(host.textContent).not.toContain(S.prompt.preview.title);
    expect(hasButton(host, S.prompt.export.copy)).toBe(false);
    expect(host.textContent).not.toContain(S.settings.privacy.L0.label);
    // 时段控件属于复盘页，这里也没有
    expect(host.querySelectorAll('input[type="datetime-local"]')).toHaveLength(0);
  });
});

describe("PromptPage 选中与草稿", () => {
  it("选中即同步草稿：默认第一条，换一条就换一套名称 / 说明 / 正文", async () => {
    const host = await renderPage();

    // 默认选中第一条（内置实盘速览）
    expect(nameInput(host).value).toBe(LIVE_BUILTIN.name);
    expect(bodyTextarea(host).value).toBe(LIVE_BUILTIN.body);

    await click(buttonContaining(host, MARKET.name));

    expect(nameInput(host).value).toBe(MARKET.name);
    expect(bodyTextarea(host).value).toBe(MARKET.body);
  });

  it("重复点选同一条不会冲掉未保存的草稿", async () => {
    const host = await renderPage();

    await act(async () => {
      setTextareaValue(bodyTextarea(host), "EDITED BODY");
    });
    await flush();

    await click(buttonContaining(host, LIVE_BUILTIN.name));

    expect(bodyTextarea(host).value).toBe("EDITED BODY");
    expect(host.textContent).toContain(S.prompt.editor.dirty);
  });
});

describe("PromptPage 保存 / 另存为 / 删除", () => {
  it("内置模板只能另存为：没有「保存」，另存为传 null id 且同名时追加副本后缀", async () => {
    const host = await renderPage();

    expect(hasButton(host, S.prompt.actions.save)).toBe(false);
    expect(hasButton(host, S.prompt.actions.saveAs)).toBe(true);
    expect(host.textContent).toContain(S.prompt.actions.saveAsNote);
    // 内置模板不可删除
    expect(hasButton(host, S.prompt.actions.delete)).toBe(false);

    await click(button(host, S.prompt.actions.saveAs));

    const saveCalls = callsOf("template_save");
    expect(saveCalls).toHaveLength(1);
    expect(saveCalls[0]?.[1]).toMatchObject({
      request: {
        // id 为 null = 新建：后端生成新 id，内置模板永不被覆盖
        id: null,
        kind: "live",
        name: `${LIVE_BUILTIN.name}${S.prompt.actions.saveAsCopySuffix}`,
        body: LIVE_BUILTIN.body,
      },
    });
  });

  it("内置模板改名后另存为：用改后的名字，不再追加副本后缀", async () => {
    const host = await renderPage();

    await act(async () => {
      setInputValue(nameInput(host), "我的实盘速览");
    });
    await flush();

    await click(button(host, S.prompt.actions.saveAs));

    expect(callsOf("template_save")[0]?.[1]).toMatchObject({
      request: { id: null, name: "我的实盘速览" },
    });
  });

  it("用户模板可以保存（带 id 覆盖，不新建）", async () => {
    const host = await renderPage();
    await click(buttonContaining(host, LIVE_USER.name));

    expect(hasButton(host, S.prompt.actions.save)).toBe(true);

    await click(button(host, S.prompt.actions.save));
    expect(callsOf("template_save")).toHaveLength(1);
    expect(callsOf("template_save")[0]?.[1]).toMatchObject({
      request: {
        id: LIVE_USER.id,
        name: LIVE_USER.name,
        kind: "live",
        body: LIVE_USER.body,
      },
    });
  });

  it("正文为空时本地就拦住提交：按钮禁用 + 人话说明，不白跑一次 IPC", async () => {
    const host = await renderPage();
    await click(buttonContaining(host, LIVE_USER.name));

    await act(async () => {
      setTextareaValue(bodyTextarea(host), "   ");
    });
    await flush();

    expect(button(host, S.prompt.actions.save).disabled).toBe(true);
    expect(host.textContent).toContain(S.prompt.actions.bodyRequired);
    expect(callsOf("template_save")).toHaveLength(0);
  });

  it("删除必须二次确认：先展开确认区，确认后才调用 template_delete", async () => {
    const host = await renderPage();
    await click(buttonContaining(host, LIVE_USER.name));

    await click(button(host, S.prompt.actions.delete));
    expect(host.textContent).toContain(S.prompt.actions.deleteConfirmNote);
    expect(callsOf("template_delete")).toHaveLength(0);

    // 取消不触发删除
    await click(button(host, S.prompt.actions.deleteCancel));
    expect(callsOf("template_delete")).toHaveLength(0);
    expect(host.textContent).not.toContain(S.prompt.actions.deleteConfirmNote);

    // 再次展开并确认才真的删
    await click(button(host, S.prompt.actions.delete));
    await click(button(host, S.prompt.actions.deleteConfirm));
    expect(callsOf("template_delete")).toHaveLength(1);
    expect(callsOf("template_delete")[0]?.[1]).toEqual({ id: LIVE_USER.id });
  });
});

describe("PromptPage 语法校验", () => {
  it("正文防抖 300ms 后才校验：停手前不重复发请求", async () => {
    const host = await renderPage();

    await advance(350);
    expect(callsOf("template_check")).toHaveLength(1);
    expect(callsOf("template_check")[0]?.[1]).toEqual({ body: LIVE_BUILTIN.body });

    await act(async () => {
      setTextareaValue(bodyTextarea(host), "EDITED {{ alpha }}");
    });
    // 防抖窗口内不发请求
    expect(callsOf("template_check")).toHaveLength(1);

    await advance(350);
    const calls = callsOf("template_check");
    expect(calls).toHaveLength(2);
    expect(calls.at(-1)?.[1]).toEqual({ body: "EDITED {{ alpha }}" });
  });

  it("成功时显示变量个数与逐个变量名", async () => {
    const host = await renderPage();
    await advance(350);

    expect(host.textContent).toContain(S.prompt.editor.checkTitle);
    expect(host.textContent).toContain(S.prompt.check.ok(CHECKED_VARIABLES.length));
    expect(host.textContent).toContain(
      S.prompt.check.variables(CHECKED_VARIABLES.join(" · ")),
    );
  });

  it("正文为空时不做校验（也不发请求），只说明原因", async () => {
    const host = await renderPage();

    await act(async () => {
      setTextareaValue(bodyTextarea(host), "   ");
    });
    await advance(350);

    expect(host.textContent).toContain(S.prompt.editor.checkEmpty);
    expect(callsOf("template_check")).toHaveLength(0);
  });

  it("后端报错时原样展示错误标题与消息（变量名 / 语法位置是唯一线索）", async () => {
    const message = "模板语法错误：意外的 '}}'（第 3 行）";
    callMock.mockImplementation((cmd: string) =>
      cmd === "template_check"
        ? Promise.reject({ code: "Template", message, retryable: false })
        : defaultMock(cmd),
    );

    const host = await renderPage();
    await advance(350);

    expect(host.textContent).toContain(S.prompt.check.errorTitle);
    expect(host.textContent).toContain(message);
  });
});
