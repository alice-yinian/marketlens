/**
 * 提示词预览：把当前模板正文 / 隐私等级 / 上下文交给后端渲染。
 *
 * 关键性质（对应验收）：
 * - **隐私等级在 queryKey 里**：切等级立即产生新请求，不是只改本地状态；
 * - 正文由页面先做 300ms 防抖再传进来，停手后才请求；
 * - `placeholderData: keepPreviousData`：重新生成时保留上一次结果，
 *   配合 `isFetching` 做细微加载指示，预览不闪烁；
 * - 失败时保留上一次成功结果，错误交给页面原样展示（不吞）。
 */
import { keepPreviousData, useQuery } from "@tanstack/react-query";

import { call } from "../../lib/ipc";
import type { PrivacyLevel, TemplateKind } from "../../lib/types";

export interface PromptPreviewParams {
  kind: TemplateKind;
  /** 已保存模板的 id；纯正文预览时为 null */
  templateId: string | null;
  /** 未保存的正文（与已保存正文相同则为 null，让后端走模板 id 路径） */
  body: string | null;
  privacy: PrivacyLevel;
  credentialId: string | null;
  force: boolean;
  from: number;
  to: number;
  bar: string;
  /** 上下文不满足（如复盘无凭据 / 时段非法）时为 false，不请求 */
  enabled: boolean;
}

export function usePromptPreview(params: PromptPreviewParams) {
  const { kind, templateId, body, privacy, credentialId, force, from, to, bar, enabled } = params;

  return useQuery({
    queryKey: [
      "prompt_preview",
      kind,
      templateId,
      body,
      privacy,
      credentialId,
      force,
      from,
      to,
      bar,
    ],
    queryFn: () => {
      if (kind === "live") {
        return call("prompt_build_live", {
          request: {
            template_id: templateId,
            body,
            privacy,
            credential_id: credentialId,
            force,
          },
        });
      }
      return call("prompt_build_review", {
        request: {
          template_id: templateId,
          body,
          privacy,
          credential_id: credentialId,
          from,
          to,
          bar,
        },
      });
    },
    enabled,
    placeholderData: keepPreviousData,
    // 同一组参数没必要重复请求；缓存命中直接给结果。
    staleTime: Infinity,
  });
}
