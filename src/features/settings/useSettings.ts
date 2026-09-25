/**
 * 设置页的数据钩子。
 *
 * 与其余页面一致：查询只在进入页面 / 手动刷新时发起，刻意不配 `refetchInterval`。
 * 清理是破坏性操作，因此只暴露 mutation——调用时机完全由界面的二次确认决定。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { call } from "../../lib/ipc";

export const CACHE_STATS_KEY = ["cache_stats"] as const;

/** 缓存占用统计。刷新用返回的 `refetch()`。 */
export function useCacheStats() {
  return useQuery({
    queryKey: CACHE_STATS_KEY,
    queryFn: () => call("cache_stats"),
  });
}

/**
 * 按保留策略清理缓存。
 *
 * 成功后让统计失效重取：清理会真的改变行数，不刷新的话表格会立刻过期。
 * `CleanupReport` 本身由调用方从 mutation 结果里取，不复用统计。
 */
export function useCacheCleanup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => call("cache_cleanup"),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CACHE_STATS_KEY });
    },
  });
}

/** 一键导出脱敏诊断包（内容 + 落盘路径一起返回）。 */
export function useDiagnosticsExport() {
  return useMutation({
    mutationFn: () => call("diagnostics_export"),
  });
}
