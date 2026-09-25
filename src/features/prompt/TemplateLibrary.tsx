/**
 * 模板库：内置在前、用户在后，按「实盘 / 复盘」分组展示。
 *
 * 内置模板加角标（并解释它为什么不能直接改），用户模板标「自定义」。
 * 选中项高亮；点击只改变选中 id，预览由页面统一触发。
 */
import { S } from "../../lib/strings";
import type { PromptTemplate, TemplateKind } from "../../lib/types";
import { kindLabel } from "./format";

function TemplateRow({
  template,
  selected,
  onSelect,
}: {
  template: PromptTemplate;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect(template.id)}
      aria-pressed={selected}
      className={`flex w-full flex-col items-start gap-1 rounded-lg border px-3 py-2 text-left ${
        selected
          ? "border-indigo-700 bg-indigo-950/30"
          : "border-neutral-800 bg-neutral-900/40 hover:border-neutral-700"
      }`}
    >
      <span className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-medium text-neutral-100">{template.name}</span>
        <span
          className={`rounded border px-1.5 py-0.5 text-[10px] ${
            template.builtin
              ? "border-sky-900 bg-sky-950/50 text-sky-300"
              : "border-violet-900 bg-violet-950/50 text-violet-300"
          }`}
        >
          {template.builtin ? S.prompt.templates.builtinBadge : S.prompt.templates.userBadge}
        </span>
        <span className="rounded border border-neutral-700 bg-neutral-800/60 px-1.5 py-0.5 text-[10px] text-neutral-400">
          {kindLabel(template.kind)}
        </span>
      </span>
      {template.description.length > 0 ? (
        <span className="text-xs text-neutral-400">{template.description}</span>
      ) : null}
    </button>
  );
}

function Group({
  kind,
  templates,
  selectedId,
  onSelect,
}: {
  kind: TemplateKind;
  templates: PromptTemplate[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-xs font-medium tracking-wide text-neutral-500">{kindLabel(kind)}</h3>
      {templates.map((template) => (
        <TemplateRow
          key={template.id}
          template={template}
          selected={template.id === selectedId}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}

export function TemplateLibrary({
  templates,
  loading,
  error,
  onRetry,
  selectedId,
  onSelect,
}: {
  templates: PromptTemplate[];
  loading: boolean;
  error: unknown;
  onRetry: () => void;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const live = templates.filter((template) => template.kind === "live");
  const review = templates.filter((template) => template.kind === "review");

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.prompt.templates.title}</h2>

      {loading ? (
        <p className="text-xs text-neutral-500">{S.prompt.templates.loading}</p>
      ) : error !== null && error !== undefined ? (
        <div className="rounded-lg border border-red-900/60 bg-red-950/30 p-3">
          <p className="text-xs text-red-300">{S.prompt.templates.error}</p>
          <button
            type="button"
            onClick={onRetry}
            className="mt-2 rounded-md bg-red-900/70 px-3 py-1.5 text-xs text-red-100 hover:bg-red-900"
          >
            {S.prompt.templates.retry}
          </button>
        </div>
      ) : templates.length === 0 ? (
        <p className="text-xs text-neutral-500">{S.prompt.templates.empty}</p>
      ) : (
        <>
          {live.length > 0 ? (
            <Group kind="live" templates={live} selectedId={selectedId} onSelect={onSelect} />
          ) : null}
          {review.length > 0 ? (
            <Group kind="review" templates={review} selectedId={selectedId} onSelect={onSelect} />
          ) : null}
          <p className="text-[11px] text-neutral-600">{S.prompt.templates.builtinNote}</p>
        </>
      )}
    </section>
  );
}
