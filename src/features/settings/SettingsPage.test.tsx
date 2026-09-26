// @vitest-environment jsdom
/**
 * 设置页行为测试：
 * - 清理必须二次确认（未确认不调 `cache_cleanup`）；
 * - `retention === null` 渲染为「不自动清理」；
 * - `deleted` 为空显示「没有需要清理的数据」；
 * - `bytes_before === bytes_after` 不编造释放量；
 * - `redactions` 为空显示对应文案；
 * - 字节数格式化为人类可读；
 * - 诊断结果展示路径 / 大小 / 凭据属性 / 版本信息；
 * - 代理「保存」必然伴随一次探测，而「清除」不做无谓的探测；
 * - 隐私等级是全局设置：三档都渲染、点击写回 `privacy_set`、`privacy_get` 非默认值时正确标为选中。
 *
 * 只 mock IPC 出口（lib/ipc），页面与纯函数全部真实执行。
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CacheStats,
  CleanupReport,
  DiagnosticExport,
} from "../../lib/types";

const callMock = vi.fn();
vi.mock("../../lib/ipc", () => ({
  call: (...args: unknown[]) => callMock(...args),
}));

import { S } from "../../lib/strings";
import { SettingsPage } from "./SettingsPage";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const STATS: CacheStats = {
  database_bytes: 12_300_000,
  tables: [
    { name: "candles", label: "K 线", rows: 1_234_567, retention: "每个序列保留最近 5000 根" },
    { name: "position_trace", label: "仓位留痕", rows: 890, retention: "保留最近 90 天" },
    { name: "fills", label: "成交明细", rows: 42, retention: null },
  ],
};

function exportOf(overrides: Partial<DiagnosticExport> = {}): DiagnosticExport {
  return {
    bundle: {
      generated_at: 1_700_000_000_000,
      app: {
        app_version: "0.6.0",
        core_version: "0.6.0",
        schema_version: 7,
        target_os: "linux",
      },
      cache: STATS,
      settings: [],
      credentials: { count: 1, entries: ["demo/read_only"] },
      log_tail: ["line 1", "line 2"],
      log_file: "/var/log/marketlens.log",
      redactions: [],
    },
    path: "/tmp/diag.json",
    bytes: 4096,
    ...overrides,
  };
}

function defaultMock(cmd: string): Promise<unknown> {
  switch (cmd) {
    case "cache_stats":
      return Promise.resolve(STATS);
    case "cache_cleanup":
      return Promise.resolve({
        deleted: [],
        bytes_before: 1000,
        bytes_after: 1000,
      } satisfies CleanupReport);
    case "diagnostics_export":
      return Promise.resolve(exportOf());
    case "proxy_get":
      return Promise.resolve({ url: null });
    case "privacy_get":
      return Promise.resolve("L1");
    case "theme_get":
      return Promise.resolve("system");
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
        <SettingsPage />
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

/** 隐私档位按钮的内部是「名称 + 说明」两个 span，只能按包含匹配 */
function buttonContaining(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((b) =>
    (b.textContent ?? "").includes(text),
  );
  if (found === undefined) throw new Error(`按钮未渲染（包含）：${text}`);
  return found as HTMLButtonElement;
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

describe("SettingsPage 缓存管理", () => {
  it("展示数据库总大小与逐表行数（人类可读 + 千分位）", async () => {
    const host = await renderPage();
    expect(host.textContent).toContain("12.3 MB");
    expect(host.textContent).toContain("1,234,567");
    expect(host.textContent).toContain("K 线");
  });

  it("retention 为 null 的表格显示「不自动清理」", async () => {
    const host = await renderPage();
    expect(host.textContent).toContain(S.settings.cache.noRetention);
    expect(host.textContent).toContain("成交明细");
  });

  it("清理前必须二次确认：未确认时不调用 cache_cleanup", async () => {
    const host = await renderPage();

    await click(button(host, S.settings.cache.cleanup));

    // 确认区出现，且文案逐条列出会删什么、声明不可撤销
    expect(host.textContent).toContain(S.settings.cache.confirmBody);
    expect(host.textContent).toContain("不可撤销");
    expect(callsOf("cache_cleanup")).toHaveLength(0);

    // 取消同样不触发
    await click(button(host, S.settings.cache.cancel));
    expect(callsOf("cache_cleanup")).toHaveLength(0);

    // 确认后才真正调用一次
    await click(button(host, S.settings.cache.cleanup));
    await click(button(host, S.settings.cache.confirm));
    expect(callsOf("cache_cleanup")).toHaveLength(1);
  });

  it("deleted 为空时显示「没有需要清理的数据」", async () => {
    const host = await renderPage();
    await click(button(host, S.settings.cache.cleanup));
    await click(button(host, S.settings.cache.confirm));

    expect(host.textContent).toContain(S.settings.cache.reportTitle);
    expect(host.textContent).toContain(S.settings.cache.cleanupNothing);
  });

  it("bytes_before === bytes_after 时如实说明，不编造释放量", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "cache_cleanup"
        ? Promise.resolve({
            deleted: [{ name: "candles", rows: 500 }],
            bytes_before: 2_000_000,
            bytes_after: 2_000_000,
          } satisfies CleanupReport)
        : defaultMock(cmd),
    );
    const host = await renderPage();
    await click(button(host, S.settings.cache.cleanup));
    await click(button(host, S.settings.cache.confirm));

    expect(host.textContent).toContain(S.settings.cache.cleanupNoShrink);
    expect(host.textContent).not.toContain("回收空间");
    // 删除的行数照常展示（只删了 500 行，用中文表名）
    expect(host.textContent).toContain(S.settings.cache.cleanupDeleted("K 线", "500"));
  });

  it("提供刷新按钮重新拉取统计", async () => {
    const host = await renderPage();
    const before = callsOf("cache_stats").length;
    await click(button(host, S.settings.cache.refresh));
    expect(callsOf("cache_stats").length).toBeGreaterThan(before);
  });
});

describe("SettingsPage 诊断导出", () => {
  it("导出后展示路径、大小、凭据属性与版本信息", async () => {
    const host = await renderPage();
    await click(button(host, S.settings.diagnostics.export));

    expect(host.textContent).toContain("/tmp/diag.json");
    expect(host.textContent).toContain(S.settings.diagnostics.size("4.1 KB"));
    expect(host.textContent).toContain("demo/read_only");
    expect(host.textContent).toContain("0.6.0");
    expect(host.textContent).toContain(S.settings.diagnostics.copyJson);
    expect(hasButton(host, S.settings.diagnostics.copyPath)).toBe(true);
  });

  it("redactions 为空时显示对应文案；脱敏说明逐条展示", async () => {
    const host = await renderPage();
    await click(button(host, S.settings.diagnostics.export));

    expect(host.textContent).toContain(S.settings.diagnostics.redactionNone);
    expect(host.textContent).toContain(S.settings.diagnostics.redactionNoKeys);
    expect(host.textContent).toContain(S.settings.diagnostics.redactionNoRows);
    expect(host.textContent).toContain(S.settings.diagnostics.redactionLogs);
  });

  it("redactions 非空时逐条展示「模式 × 次数」", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd !== "diagnostics_export") return defaultMock(cmd);
      const base = exportOf();
      return Promise.resolve({
        ...base,
        bundle: {
          ...base.bundle,
          redactions: [{ pattern: "家目录绝对路径（会暴露用户名）", hits: 2 }],
        },
      });
    });
    const host = await renderPage();
    await click(button(host, S.settings.diagnostics.export));

    expect(host.textContent).toContain(
      S.settings.diagnostics.redactionHit("家目录绝对路径（会暴露用户名）", 2),
    );
    expect(host.textContent).not.toContain(S.settings.diagnostics.redactionNone);
  });
});

describe("SettingsPage 网络代理", () => {
  const PROXY = "http://127.0.0.1:7890";
  const OK_PROBE = { ok: true, latency_ms: 37, server_time_ms: 1_700_000_000_000, error: null };

  function input(host: HTMLElement): HTMLInputElement {
    const found = host.querySelector("input");
    if (found === null) throw new Error("代理输入框未渲染");
    return found;
  }

  async function setValue(target: HTMLInputElement, value: string) {
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(target, value);
      target.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await flush();
  }

  it("保存会立即测试，成功时展示往返耗时并说明已生效", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "proxy_set") return Promise.resolve({ url: PROXY });
      if (cmd === "proxy_test") return Promise.resolve(OK_PROBE);
      return defaultMock(cmd);
    });
    const host = await renderPage();
    await setValue(input(host), PROXY);
    await click(button(host, S.settings.proxy.save));

    expect(callMock).toHaveBeenCalledWith("proxy_set", { input: { url: PROXY } });
    expect(callsOf("proxy_test")).toHaveLength(1);
    expect(host.textContent).toContain(S.settings.proxy.ok(37));
    expect(host.textContent).toContain(S.settings.proxy.saved);
  });

  it("代理不通时给出原因与排查提示", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "proxy_set") return Promise.resolve({ url: PROXY });
      if (cmd === "proxy_test") {
        return Promise.resolve({
          ok: false,
          latency_ms: 3,
          server_time_ms: null,
          error: "网络请求失败：代理返回 502",
        });
      }
      return defaultMock(cmd);
    });
    const host = await renderPage();
    await setValue(input(host), PROXY);
    await click(button(host, S.settings.proxy.save));

    expect(host.textContent).toContain(S.settings.proxy.failTitle);
    expect(host.textContent).toContain("代理返回 502");
    expect(host.textContent).toContain(S.settings.proxy.failHint);
  });

  it("已保存配置时提供「不使用代理」，点击后清除并回填为空", async () => {
    callMock.mockImplementation((cmd: string) => {
      if (cmd === "proxy_get") return Promise.resolve({ url: PROXY });
      if (cmd === "proxy_set") return Promise.resolve({ url: null });
      return defaultMock(cmd);
    });
    const host = await renderPage();
    expect(input(host).value).toBe(PROXY);

    await click(button(host, S.onboarding.proxy.skip));

    expect(callMock).toHaveBeenCalledWith("proxy_set", { input: { url: null } });
    expect(input(host).value).toBe("");
    // 清除不该顺带发一次探测请求
    expect(callsOf("proxy_test")).toHaveLength(0);
  });
});

describe("SettingsPage 界面主题", () => {
  it("三档都渲染，并说明跟随系统会随系统切换", async () => {
    const host = await renderPage();

    expect(host.textContent).toContain(S.settings.theme.title);
    expect(host.textContent).toContain(S.settings.theme.subtitle);
    expect(host.textContent).toContain(S.settings.theme.system);
    expect(host.textContent).toContain(S.settings.theme.systemDesc);
    expect(host.textContent).toContain(S.settings.theme.light);
    expect(host.textContent).toContain(S.settings.theme.dark);
  });

  it("点击「日间」调用 theme_set（入参就是这一档）并更新「当前」", async () => {
    callMock.mockImplementation((cmd: string, args?: { theme?: string }) => {
      if (cmd === "theme_set") return Promise.resolve(args?.theme ?? null);
      return defaultMock(cmd);
    });
    const host = await renderPage();

    await click(buttonContaining(host, S.settings.theme.light));

    expect(callMock).toHaveBeenCalledWith("theme_set", { theme: "light" });
    expect(host.textContent).toContain(S.settings.theme.current(S.settings.theme.light));
  });

  /// 「跟随系统」下用户无法从设置本身看出现在到底是日间还是夜间，
  /// 所以必须把**实际生效**的那一档写出来。
  it("跟随系统时写出实际生效的档位（jsdom 无 matchMedia → 夜间）", async () => {
    const host = await renderPage();

    expect(host.textContent).toContain(
      S.settings.theme.currentFollowed(S.settings.theme.dark),
    );
  });
});

describe("SettingsPage 隐私等级（全局）", () => {
  const LEVELS = [S.settings.privacy.L0, S.settings.privacy.L1, S.settings.privacy.L2];

  it("三档都渲染，并说清它对提示词做了什么、后端强制生效", async () => {
    const host = await renderPage();

    expect(host.textContent).toContain(S.settings.privacy.title);
    expect(host.textContent).toContain(S.settings.privacy.enforcedNote);
    for (const level of LEVELS) {
      expect(host.textContent).toContain(level.label);
      expect(host.textContent).toContain(level.desc);
    }
  });

  it("点击某一档调用 privacy_set（入参就是那一档）并更新「当前」", async () => {
    callMock.mockImplementation((cmd: string, args?: { level?: string }) => {
      if (cmd === "privacy_set") return Promise.resolve(args?.level ?? null);
      return defaultMock(cmd);
    });
    const host = await renderPage();

    await click(buttonContaining(host, S.settings.privacy.L2.label));

    expect(callMock).toHaveBeenCalledWith("privacy_set", { level: "L2" });
    expect(host.textContent).toContain(
      S.settings.privacy.current(S.settings.privacy.L2.label),
    );
  });

  it("privacy_get 返回非 L1 时把那一档标为选中（不写死默认值）", async () => {
    callMock.mockImplementation((cmd: string) =>
      cmd === "privacy_get" ? Promise.resolve("L0") : defaultMock(cmd),
    );
    const host = await renderPage();

    const pressed = (label: string) =>
      buttonContaining(host, label).getAttribute("aria-pressed");
    expect(pressed(S.settings.privacy.L0.label)).toBe("true");
    expect(pressed(S.settings.privacy.L1.label)).toBe("false");
    expect(pressed(S.settings.privacy.L2.label)).toBe("false");
    expect(host.textContent).toContain(
      S.settings.privacy.current(S.settings.privacy.L0.label),
    );
  });
});
