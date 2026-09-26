/**
 * 「计划执行」的共享状态机：进度事件订阅 + 取消 + 结果 / 错误。
 *
 * 复盘与行情在 Rust 侧共用同一个执行器（`ExecutablePlan`），所以前端这套状态机
 * 也只该有一份——两份各自订阅进度事件，迟早会出现「一边正确丢弃了迟到事件、
 * 另一边没有」这种只在特定时序下才暴露的差异。
 *
 * `fetch` 会一直等到采集结束才 resolve，所以进度只能靠 `fetch://progress` 事件推进：
 * 这里在 `activePlanId` 非空时订阅一次，**只接收 `plan_id` 匹配当前计划的事件**
 * （事件是全局广播，别的计划/迟到的事件必须被丢弃）。
 *
 * 组件不得自己 import 事件 API——订阅与取消订阅都收敛在这里。
 */
import { useMutation } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { onFetchProgress } from "./ipc";
import { S } from "./strings";
import type { Progress } from "./types";

export function usePlanFetch<Report>(commands: {
  fetch: (planId: string) => Promise<Report>;
  cancel: (planId: string) => Promise<boolean>;
}) {
  const [progress, setProgress] = useState<Progress | null>(null);
  const [activePlanId, setActivePlanId] = useState<string | null>(null);
  const [cancelNote, setCancelNote] = useState<string | null>(null);

  useEffect(() => {
    if (activePlanId === null) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void onFetchProgress((snapshot) => {
      if (!active || snapshot.plan_id !== activePlanId) return;
      setProgress(snapshot);
    }).then((fn) => {
      if (active) unlisten = fn;
      else fn();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [activePlanId]);

  const fetchMutation = useMutation({
    mutationFn: commands.fetch,
    onSettled: () => setActivePlanId(null),
  });

  const cancelMutation = useMutation({
    mutationFn: commands.cancel,
  });

  const start = (planId: string) => {
    fetchMutation.reset();
    setProgress(null);
    setCancelNote(null);
    setActivePlanId(planId);
    fetchMutation.mutate(planId);
  };

  const cancel = (planId: string) => {
    cancelMutation.mutate(planId, {
      onSuccess: (cancelled) => {
        setCancelNote(
          cancelled ? S.review.progress.cancelRequested : S.review.progress.cancelAlreadyDone,
        );
      },
      // 取消请求本身失败（网络等）时，如实说明该计划可能已结束，而不是静默吞掉
      onError: () => setCancelNote(S.review.progress.cancelAlreadyDone),
    });
  };

  const reset = () => {
    fetchMutation.reset();
    setProgress(null);
    setCancelNote(null);
  };

  return {
    progress,
    start,
    reset,
    isFetching: fetchMutation.isPending,
    report: fetchMutation.data ?? null,
    error: fetchMutation.error,
    cancel,
    isCancelling: cancelMutation.isPending,
    cancelNote,
  };
}
