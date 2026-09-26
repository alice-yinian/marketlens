/**
 * 全局隐私等级的数据钩子。
 *
 * 值存在 Rust 侧的 `settings` 表（命令 `privacy_get` / `privacy_set`），
 * 所以「全局生效」不是靠各页面自觉传参：它们读的是同一份数据，
 * 而三条提示词管线在后端也会在未指定时回落到它。
 *
 * `staleTime: Infinity`：这个值只在设置页被改，改的时候我们自己失效缓存，
 * 没必要让每个页面各自去轮询一遍 SQLite。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { call } from "../../lib/ipc";
import type { PrivacyLevel } from "../../lib/types";
import { DEFAULT_PRIVACY } from "./format";

export const PRIVACY_KEY = ["privacy_get"] as const;

/**
 * 当前隐私等级。
 *
 * 还没读到之前返回默认值（L1）：**默认值必须与 Rust `PrivacyLevel::default()`
 * 一致**，否则首屏会先用 L1 生成一次、读到真值后再生成一次。
 * 需要「读到之前先别生成」的调用方看 [`usePrivacyState`]。
 */
export function usePrivacy(): PrivacyLevel {
  const query = useQuery({
    queryKey: PRIVACY_KEY,
    queryFn: () => call("privacy_get"),
    staleTime: Infinity,
  });
  return query.data ?? DEFAULT_PRIVACY;
}

/** 与 [`usePrivacy`] 相同，但额外给出「是否还在读」——生成流程用它决定要不要等。 */
export function usePrivacyState(): { level: PrivacyLevel; isPending: boolean } {
  const query = useQuery({
    queryKey: PRIVACY_KEY,
    queryFn: () => call("privacy_get"),
    staleTime: Infinity,
  });
  return { level: query.data ?? DEFAULT_PRIVACY, isPending: query.isPending };
}

/** 写回全局隐私等级。 */
export function useSetPrivacy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (level: PrivacyLevel) => call("privacy_set", { level }),
    onSuccess: (saved) => {
      queryClient.setQueryData<PrivacyLevel>(PRIVACY_KEY, saved);
    },
  });
}
