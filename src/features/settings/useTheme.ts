/**
 * 界面主题的数据钩子。
 *
 * 值存在 Rust 侧的 `settings` 表（`theme_get` / `theme_set`），默认**跟随系统**：
 * 大多数人不会专门去调主题，而系统偏好已经表达了意图。
 *
 * 应用方式：把解析结果写到 `<html data-theme>`。整套配色是 CSS 变量
 * （见 `styles.css` 的日间主题块），所以换主题**不重建任何组件**。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { call } from "../../lib/ipc";
import type { Theme } from "../../lib/types";

export const THEME_KEY = ["theme_get"] as const;

/** 尚未读到设置时用的值——与 Rust `Theme::default()` 必须一致（跟随系统）。 */
export const DEFAULT_THEME: Theme = "system";

/** 实际生效的主题（解析掉 `system` 之后只剩这两档）。 */
export type ResolvedTheme = "light" | "dark";

/**
 * `system` → 看系统的深色偏好；否则就是设置本身。
 *
 * 抽成纯函数是为了能直接测：它不碰 DOM，也不需要渲染。
 */
export function resolveTheme(setting: Theme, systemPrefersDark: boolean): ResolvedTheme {
  if (setting === "light") return "light";
  if (setting === "dark") return "dark";
  return systemPrefersDark ? "dark" : "light";
}

function systemPrefersDarkNow(): boolean {
  // jsdom 没有 matchMedia；拿不到系统偏好时按夜间处理（与本应用原来的观感一致）
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return true;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** 当前主题设置（可能仍是 `system`）。 */
export function useThemeSetting(): Theme {
  const query = useQuery({
    queryKey: THEME_KEY,
    queryFn: () => call("theme_get"),
    staleTime: Infinity,
  });
  return query.data ?? DEFAULT_THEME;
}

/** 写回主题设置。 */
export function useSetTheme() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (theme: Theme) => call("theme_set", { theme }),
    onSuccess: (saved) => {
      queryClient.setQueryData<Theme>(THEME_KEY, saved);
    },
  });
}

/**
 * 实际生效的主题：`system` 时跟随系统偏好，并**监听它的变化**。
 *
 * 设置页用它显示「跟随系统（实际：日间）」，应用层用它写 DOM——两处共用同一个
 * 监听，就不会出现「界面说跟随系统、实际却没跟着变」这种半跟随状态。
 */
export function useResolvedTheme(): ResolvedTheme {
  const setting = useThemeSetting();
  const [prefersDark, setPrefersDark] = useState(systemPrefersDarkNow);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => setPrefersDark(event.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  return resolveTheme(setting, prefersDark);
}

/**
 * 把主题应用到 `<html data-theme>`。
 *
 * **必须在应用顶层调用一次**（`App`）：它是个副作用、不代表任何界面；
 * 只在设置页调用的话，用户切到别的页面就不再跟随系统了。
 */
export function useAppliedTheme(): void {
  const resolved = useResolvedTheme();

  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
  }, [resolved]);
}
