/**
 * 生成提示词（实盘 / 复盘共用）。
 *
 * 这块能力原来长在提示词页上，而那个页面同时还要管模板——结果是「管理」与
 * 「使用」挤在一起。现在按上下文归位：模板在提示词库集中管理，生成放在
 * 各自的页面（实盘页用实盘上下文，复盘页用复盘上下文，行情页有它自己的计划/取数流程）。
 *
 * 两条刻意的设计：
 *
 * 1. **显式点击才生成**，不在挂载时自动跑。进入实盘页会触发一次行情拉取、
 *    复盘页会先同步官方历史仓位——「打开页面就偷偷发请求」违背按需原则。
 * 2. **隐私等级不在这里选**：它是全局设置（设置页），这里只读。
 *    放在这里会让人以为「这一页单独设」。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import type { PrivacyLevel, PromptOutput } from "../../lib/types";
import { usePrivacyState } from "../settings/usePrivacy";
import { ExportBar } from "./ExportBar";
import { PreviewPanel } from "./PreviewPanel";
import { usePromptBuild } from "./usePromptBuild";
import { useTemplates } from "./useTemplates";

/** 生成所需的上下文：两种类型各有自己的字段，用判别联合而不是「一堆可选参数」。 */
export type PromptPanelContext =
  | { kind: "live"; credentialId: string | null; force: boolean }
  | { kind: "review"; credentialId: string | null; from: number; to: number; bar: string };

const SELECT_CLASS =
  "rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100";

export function PromptPanel({
  context,
  onOpenLibrary,
  blockedReason = null,
}: {
  context: PromptPanelContext;
  onOpenLibrary?: () => void;
  /**
   * 上游已知的「现在不能生成」的原因（如复盘页的时段不合法）。
   *
   * 面板自己不判断时段——那份判断属于持有上下文的页面。但按钮**必须**跟着它禁用：
   * 否则同一个页面上「生成计划」「装配」「生成提示词」三个动作会对同一份时段
   * 给出不一致的反应（前两个拦住、第三个放行到后端才报错）。
   */
  blockedReason?: string | null;
}) {
  const templatesQuery = useTemplates();
  const { level, isPending: levelPending } = usePrivacyState();
  const build = usePromptBuild();

  const [pickedId, setPickedId] = useState<string | null>(null);

  const list = (templatesQuery.data ?? []).filter(
    (template) => template.kind === context.kind,
  );
  // 选中的模板被删 / 列表还没加载时回落到第一条，不做额外 effect 同步。
  const templateId =
    pickedId !== null && list.some((template) => template.id === pickedId)
      ? pickedId
      : (list[0]?.id ?? null);
  const template = list.find((item) => item.id === templateId) ?? null;

  const output: PromptOutput | null = build.data ?? null;

  const generate = () => {
    if (templateId === null) return;
    build.mutate(
      context.kind === "live"
        ? {
            kind: "live",
            templateId,
            privacy: level,
            credentialId: context.credentialId,
            force: context.force,
          }
        : {
            kind: "review",
            templateId,
            privacy: level,
            credentialId: context.credentialId,
            from: context.from,
            to: context.to,
            bar: context.bar,
          },
    );
  };

  const privacy: PrivacyLevel = level;

  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
        <div>
          <h2 className="text-sm font-medium text-neutral-200">{S.prompt.generate.title}</h2>
          <p className="mt-0.5 text-xs text-neutral-500">{S.prompt.generate.subtitle}</p>
        </div>

        {templatesQuery.isPending ? (
          <p className="text-xs text-neutral-500">{S.prompt.templates.loading}</p>
        ) : list.length === 0 ? (
          <p className="text-xs text-amber-300">{S.prompt.generate.noTemplate}</p>
        ) : (
          <label className="flex flex-wrap items-center gap-2 text-sm text-neutral-300">
            {S.prompt.generate.template}
            <select
              className={SELECT_CLASS}
              value={templateId ?? ""}
              disabled={build.isPending}
              onChange={(event) => setPickedId(event.target.value)}
            >
              {list.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.builtin
                    ? `${item.name}（${S.prompt.templates.builtinBadge}）`
                    : item.name}
                </option>
              ))}
            </select>
            <span className="text-xs text-neutral-500">
              {S.prompt.generate.privacyNote(S.settings.privacy[privacy].label)}
            </span>
          </label>
        )}

        {template === null || template.description.length === 0 ? null : (
          <p className="text-xs text-neutral-500">{template.description}</p>
        )}

        {blockedReason === null ? null : (
          <p className="text-xs text-amber-300">{blockedReason}</p>
        )}

        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={generate}
            disabled={
              templateId === null || build.isPending || levelPending || blockedReason !== null
            }
            className="rounded-md bg-indigo-800 px-4 py-2 text-sm font-medium text-indigo-50 hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {build.isPending ? S.prompt.generate.running : S.prompt.generate.run}
          </button>
          <button
            type="button"
            onClick={onOpenLibrary}
            disabled={onOpenLibrary === undefined}
            className="rounded-md border border-neutral-700 px-3 py-2 text-xs text-neutral-300 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {S.prompt.generate.manage}
          </button>
        </div>
      </div>

      <PreviewPanel
        output={output}
        error={build.error}
        isFetching={build.isPending}
        isPending={build.isPending && output === null}
        enabled={templateId !== null}
        templateName={template?.name ?? S.prompt.generate.title}
      />

      <ExportBar
        output={output}
        privacy={privacy}
        templateName={template?.name ?? S.prompt.generate.title}
      />
    </section>
  );
}
