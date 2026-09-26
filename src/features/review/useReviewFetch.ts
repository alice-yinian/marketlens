/**
 * 复盘采集执行（`review_fetch`）的封装。
 *
 * 进度订阅与取消那套状态机在 `lib/usePlanFetch` 里——行情页用的是同一个执行器，
 * 所以共用同一份实现（含「只接收当前 plan_id 事件」这个容易写错的地方）。
 * 这里只负责把命令名接上。
 */
import { usePlanFetch } from "../../lib/usePlanFetch";
import { call } from "../../lib/ipc";

export function useReviewFetch() {
  return usePlanFetch({
    fetch: (planId) => call("review_fetch", { plan_id: planId }),
    cancel: (planId) => call("review_cancel", { plan_id: planId }),
  });
}
