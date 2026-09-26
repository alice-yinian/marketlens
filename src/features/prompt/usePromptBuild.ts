/**
 * 生成提示词（实盘 / 复盘）。
 *
 * 与「编辑器实时预览」的区别是**显式触发**：这里是一次普通 mutation，
 * 用户点一下生成一次。原来提示词页用的是带防抖的 query（编辑即重生成），
 * 那是为改模板设计的——现在生成挪到了各自页面，就不该在挂载时自动跑。
 */
import { useMutation } from "@tanstack/react-query";

import { call } from "../../lib/ipc";
import type { PromptOutput, PrivacyLevel } from "../../lib/types";

export type PromptBuildRequest =
  | {
      kind: "live";
      templateId: string;
      privacy: PrivacyLevel;
      credentialId: string | null;
      force: boolean;
    }
  | {
      kind: "review";
      templateId: string;
      privacy: PrivacyLevel;
      credentialId: string | null;
      from: number;
      to: number;
      bar: string;
    };

export function usePromptBuild() {
  return useMutation<PromptOutput, unknown, PromptBuildRequest>({
    mutationFn: (request) => {
      if (request.kind === "live") {
        return call("prompt_build_live", {
          request: {
            template_id: request.templateId,
            privacy: request.privacy,
            credential_id: request.credentialId,
            force: request.force,
          },
        });
      }
      return call("prompt_build_review", {
        request: {
          template_id: request.templateId,
          privacy: request.privacy,
          credential_id: request.credentialId,
          from: request.from,
          to: request.to,
          bar: request.bar,
        },
      });
    },
  });
}
