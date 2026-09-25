/**
 * 模板库的数据访问：列表 / 保存 / 删除。
 *
 * 只经 `lib/ipc` 出口，组件不直接碰 Tauri API。
 * 保存与删除成功后失效列表查询，让「内置在前、用户在后」的顺序由后端重新给出。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { call, type SaveTemplateRequest } from "../../lib/ipc";
import type { PromptTemplate } from "../../lib/types";

export const TEMPLATES_KEY = ["template_list"] as const;

/** 全部模板：内置在前，用户模板在后。 */
export function useTemplates() {
  return useQuery({
    queryKey: TEMPLATES_KEY,
    queryFn: () => call("template_list"),
  });
}

/** 保存模板；`id` 为空或指向内置模板时按「另存为」新建。 */
export function useSaveTemplate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: SaveTemplateRequest) => call("template_save", { request }),
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: TEMPLATES_KEY });
      queryClient.setQueryData<PromptTemplate[]>(TEMPLATES_KEY, (prev) => {
        if (prev === undefined) return prev;
        const next = prev.filter((template) => template.id !== saved.id);
        next.push(saved);
        return next;
      });
    },
  });
}

/** 删除用户模板；内置模板不可删除（后端会明确报错）。 */
export function useDeleteTemplate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call("template_delete", { id }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: TEMPLATES_KEY });
    },
  });
}
