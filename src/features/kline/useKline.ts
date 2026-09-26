/**
 * 行情页的数据钩子。
 *
 * 三段与 Rust 侧一一对应：`kline_plan`（不联网，只估算）→ `kline_fetch`
 * （执行，带进度与取消）→ `kline_build`（读本地库 + 渲染）。
 * 拆开的意义在于**改指标不用重新取数**，所以 `build` 是个独立 mutation。
 */
import { useMutation, useQuery } from "@tanstack/react-query";

import { call } from "../../lib/ipc";
import { usePlanFetch } from "../../lib/usePlanFetch";
import type { KlineBuildRequest, KlinePlanRequest } from "../../lib/ipc";

export const KLINE_BARS_KEY = ["kline_bars"] as const;
export const WATCHLIST_KEY = ["watchlist"] as const;

/**
 * 受支持的 K 线粒度。
 *
 * `staleTime: Infinity`：这是随代码发布的静态白名单，一次拿到就不再变
 * （真要变了，重启应用即可）。
 */
export function useKlineBars() {
  return useQuery({
    queryKey: KLINE_BARS_KEY,
    queryFn: () => call("kline_bars"),
    staleTime: Infinity,
  });
}

/**
 * 当前关注标的。
 *
 * 行情页只读它、不改它（改标的是引导第 4 步与「重新配置」的事），
 * 所以这里不必有失效逻辑。目前只有行情页用；出现第二个调用方时再挪到共享位置。
 */
export function useWatchlist() {
  return useQuery({
    queryKey: WATCHLIST_KEY,
    queryFn: () => call("watchlist_get"),
  });
}

/** 生成取数计划。**不联网**，所以调参数时可以反复调用。 */
export function useKlinePlan() {
  return useMutation({
    mutationFn: (request: KlinePlanRequest) => call("kline_plan", { request }),
  });
}

/** 执行取数。进度订阅与取消复用复盘那套状态机（Rust 侧是同一个执行器）。 */
export function useKlineFetch() {
  return usePlanFetch({
    fetch: (planId) => call("kline_fetch", { plan_id: planId }),
    cancel: (planId) => call("kline_cancel", { plan_id: planId }),
  });
}

/** 装配并渲染提示词。K 线来自本地库，所以这一步零请求。 */
export function useKlineBuild() {
  return useMutation({
    mutationFn: (request: KlineBuildRequest) => call("kline_build", { request }),
  });
}
