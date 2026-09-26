/**
 * 网络代理的数据钩子（引导第 2 步与设置页共用）。
 *
 * 「保存」与「测试」刻意合成一个 mutation：分开调用会出现「已保存但没测」的
 * 中间态，而用户此刻真正想知道的只有一件事——**这个地址到底通不通**。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { call } from "../../lib/ipc";
import type { ProxyProbe, ProxySettings } from "../../lib/types";

export const PROXY_SETTINGS_KEY = ["proxy_settings"] as const;

/** 当前代理配置。`url === null` 表示未显式配置（沿用系统 / 环境变量代理）。 */
export function useProxySettings() {
  return useQuery({
    queryKey: PROXY_SETTINGS_KEY,
    queryFn: () => call("proxy_get"),
  });
}

/**
 * 保存代理并**立即探测连通性**。
 *
 * `proxy_set` 会让后端热重建 HTTP 客户端，所以紧接着的 `proxy_test`
 * 测的就是刚保存的这条路径；两者都在同一个 mutation 里，界面不会出现
 * 「保存过了但不知道通不通」的状态。
 */
export function useSaveProxy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      url: string,
    ): Promise<{ settings: ProxySettings; probe: ProxyProbe }> => {
      const settings = await call("proxy_set", { input: { url } });
      const probe = await call("proxy_test");
      return { settings, probe };
    },
    onSuccess: ({ settings }) => {
      queryClient.setQueryData<ProxySettings>(PROXY_SETTINGS_KEY, settings);
    },
  });
}

/** 清除显式代理（回到系统 / 环境变量代理）。不做探测：清空后本来就没有「代理通不通」可言。 */
export function useClearProxy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => call("proxy_set", { input: { url: null } }),
    onSuccess: (settings) => {
      queryClient.setQueryData<ProxySettings>(PROXY_SETTINGS_KEY, settings);
    },
  });
}
