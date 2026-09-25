/**
 * 账户快照的查询封装。
 *
 * 与 M1 一致：进入页面拉取一次，之后只有用户点「刷新」才会再发请求——
 * 刻意不配 `refetchInterval`（ADR #10 按需采集）。
 */
import { useQuery } from "@tanstack/react-query";

import { call } from "../../lib/ipc";

export const CREDENTIALS_KEY = ["credentials_list"] as const;

/** 凭据列表（只含元数据与掩码，不含明文） */
export function useCredentials() {
  return useQuery({
    queryKey: CREDENTIALS_KEY,
    queryFn: () => call("credentials_list"),
  });
}

/**
 * 指定凭据的账户快照。`credentialId` 为 null 时不发请求（尚无可用凭据）。
 * 刷新用返回的 `refetch()`，它会绕过 `staleTime: Infinity` 强制重新拉取。
 */
export function useAccountSnapshot(credentialId: string | null) {
  return useQuery({
    queryKey: ["account_snapshot", credentialId],
    queryFn: () => {
      if (credentialId === null) throw new Error("没有可用的凭据");
      return call("account_snapshot", { query: { credential_id: credentialId } });
    },
    enabled: credentialId !== null,
  });
}
