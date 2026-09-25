/**
 * 采集执行（`review_fetch`）与进度事件（`fetch://progress`）的封装。
 *
 * `review_fetch` 会一直等到采集结束才 resolve，所以进度只能靠事件推进：
 * 这里在 `activePlanId` 非空时订阅一次 `onFetchProgress`，只接收 `plan_id`
 * 匹配当前计划的事件（事件是全局广播，别的计划/迟到的事件必须被丢弃）。
 *
 * 组件不得自己 import 事件 API——订阅与取消订阅都收敛在这里。
 */
import { useMutation } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { call, onFetchProgress } from "../../lib/ipc";
import { S } from "../../lib/strings";
import type { Progress } from "../../lib/types";

export function useReviewFetch() {
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
    mutationFn: (planId: string) => call("review_fetch", { plan_id: planId }),
    onSettled: () => setActivePlanId(null),
  });

  const cancelMutation = useMutation({
    mutationFn: (planId: string) => call("review_cancel", { plan_id: planId }),
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
