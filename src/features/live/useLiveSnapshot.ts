/**
 * 实盘快照的查询封装。
 *
 * 数据模型是「按需采集」（ADR #10）：进入页面拉取一次，之后只有用户点「刷新」
 * 才会再发请求。这里刻意不配 `refetchInterval`，也不依赖窗口聚焦重取
 * （全局默认见 src/main.tsx）。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { call } from "../../lib/ipc";
import type { LiveSnapshot } from "../../lib/types";

export const LIVE_SNAPSHOT_KEY = ["live_snapshot"] as const;

/** 进入页面即拉取一次（不带 force，允许命中 Rust 侧 TTL 缓存） */
export function useLiveSnapshot() {
  return useQuery({
    queryKey: LIVE_SNAPSHOT_KEY,
    queryFn: () => call("live_refresh"),
  });
}

/** 「刷新」按钮：force = true 绕过 TTL；成功后把新快照写回查询缓存 */
export function useForceRefresh() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => call("live_refresh", { force: true }),
    onSuccess: (snapshot: LiveSnapshot) => {
      queryClient.setQueryData(LIVE_SNAPSHOT_KEY, snapshot);
    },
  });
}

/**
 * 界面时钟：只驱动「缓存于 X 前」这类相对时间的重渲染，不触发任何数据请求。
 * 没有它，页面挂久了新鲜度提示就会停在拉取那一刻，等于在骗用户。
 */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}
