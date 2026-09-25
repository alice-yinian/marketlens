/**
 * 复盘装配（`review_context`）的封装。
 *
 * 与 `useReviewPlan` 一致：用 mutation 而不是 query——装配由按钮触发，
 * 参数来自表单状态，且**会联网**（同步官方历史仓位），绝不该被缓存或自动重取。
 */
import { useMutation } from "@tanstack/react-query";

import { call, type ReviewContextRequest } from "../../lib/ipc";

export function useReviewContext() {
  return useMutation({
    mutationFn: (request: ReviewContextRequest) => call("review_context", { request }),
  });
}
