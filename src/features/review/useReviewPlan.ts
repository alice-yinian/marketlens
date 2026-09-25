/**
 * 采集计划的生成封装（`review_plan`）。
 *
 * 刻意用 mutation 而不是 query：生成计划由「生成计划」按钮触发，
 * 参数来自表单状态而非 URL；而且它是**一次性动作**，不该被缓存或自动重取。
 */
import { useMutation } from "@tanstack/react-query";

import { call, type ReviewPlanRequest } from "../../lib/ipc";

export function useReviewPlan() {
  return useMutation({
    mutationFn: (request: ReviewPlanRequest) => call("review_plan", { request }),
  });
}
