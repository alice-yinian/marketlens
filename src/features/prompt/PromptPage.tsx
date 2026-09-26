/**
 * 提示词页（M5）。
 *
 * 页面本身不含业务逻辑：模板与渲染全部来自 Rust
 * （`template_list` / `prompt_build_live` / `prompt_build_review`），
 * 文案来自 `lib/strings.ts`，组件不直接碰 Tauri API。
 *
 * 数据流：
 *   模板库 → 选中 → 草稿（名称/说明/正文）→ 300ms 防抖 → 预览
 *   隐私等级 → 立即进入预览 queryKey（切等级即重新生成）
 *   上下文（实盘凭据/强制刷新；复盘凭据/时段/粒度）→ 同上
 */
import { useEffect, useRef, useState } from "react";

import { S } from "../../lib/strings";
import type { PrivacyLevel } from "../../lib/types";
import { useCredentials } from "../account/useAccountSnapshot";
import { ContextPanel } from "./ContextPanel";
import { ExportBar } from "./ExportBar";
import { PreviewPanel } from "./PreviewPanel";
import { PrivacyPicker } from "./PrivacyPicker";
import { TemplateEditor } from "./TemplateEditor";
import { TemplateLibrary } from "./TemplateLibrary";
import {
  DEFAULT_PRIVACY,
  rangeError,
  saveAsName,
  templateProblem,
  templateProblemText,
} from "./format";
import { useDebouncedValue } from "./useDebouncedValue";
import { usePromptPreview } from "./usePromptPreview";
import { useDeleteTemplate, useSaveTemplate, useTemplates } from "./useTemplates";

const DAY_MS = 24 * 60 * 60 * 1000;

const fieldClass =
  "rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100";

interface Draft {
  name: string;
  description: string;
  body: string;
}

export function PromptPage() {
  const templatesQuery = useTemplates();
  const list = templatesQuery.data ?? [];

  const [selectedId, setSelectedId] = useState<string | null>(null);
  // 选中的模板被删除 / 列表尚未加载时回落到第一条，不做额外 effect 同步。
  const effectiveId =
    selectedId !== null && list.some((template) => template.id === selectedId)
      ? selectedId
      : (list[0]?.id ?? null);
  const selected = list.find((template) => template.id === effectiveId) ?? null;

  const [draft, setDraft] = useState<Draft>({ name: "", description: "", body: "" });
  // 正文是否被用户实际编辑过。选择模板时先复位为 false，
  // 这样「选中模板 → 防抖尚未追上新正文」的空窗期不会带着空/旧正文去请求。
  const [bodyEdited, setBodyEdited] = useState(false);
  const syncedRef = useRef<string | null>(null);
  useEffect(() => {
    if (selected === null) return;
    if (syncedRef.current === selected.id) return;
    syncedRef.current = selected.id;
    setDraft({
      name: selected.name,
      description: selected.description,
      body: selected.body,
    });
    setBodyEdited(false);
  }, [selected]);

  const [privacy, setPrivacy] = useState<PrivacyLevel>(DEFAULT_PRIVACY);
  const [credentialId, setCredentialId] = useState<string | null>(null);
  const [force, setForce] = useState(false);
  const [range, setRange] = useState(() => {
    const now = Date.now();
    return { from: now - 7 * DAY_MS, to: now, bar: "1H" };
  });
  const [confirmDelete, setConfirmDelete] = useState(false);

  const credentialsQuery = useCredentials();
  const credentials = {
    list: credentialsQuery.data ?? [],
    isPending: credentialsQuery.isPending,
    isError: credentialsQuery.isError,
  };

  const kind = selected?.kind ?? "live";
  const rangeProblem = kind === "review" ? rangeError(range.from, range.to) : null;

  // 正文防抖 300ms；隐私等级不进防抖——切等级必须立即重新生成。
  const debouncedBody = useDebouncedValue(draft.body, 300);
  const savedBody = selected?.body ?? "";
  // 只有用户真正编辑过、且防抖后的正文确实不同于已保存正文时，才把 body 交给后端
  // （否则走 template_id 路径，避免选择模板瞬间用旧/空正文发一次请求）。
  const previewBody = bodyEdited && debouncedBody !== savedBody ? debouncedBody : null;

  const previewEnabled =
    selected !== null && (kind === "live" || (credentialId !== null && rangeProblem === null));

  const preview = usePromptPreview({
    kind,
    templateId: selected?.id ?? null,
    body: previewBody,
    privacy,
    credentialId,
    force,
    from: range.from,
    to: range.to,
    bar: range.bar,
    enabled: previewEnabled,
  });

  const saveMutation = useSaveTemplate();
  const deleteMutation = useDeleteTemplate();

  const problem = templateProblem(draft.name, draft.body);
  const dirty =
    selected !== null &&
    (draft.body !== selected.body ||
      draft.name !== selected.name ||
      draft.description !== selected.description);

  const selectTemplate = (id: string) => {
    setSelectedId(id);
    setConfirmDelete(false);
    saveMutation.reset();
    deleteMutation.reset();
  };

  const doSave = (asNew: boolean) => {
    if (selected === null || problem !== null) return;
    const name = asNew ? saveAsName(draft.name, selected.name) : draft.name.trim();
    saveMutation.mutate(
      {
        // 内置模板即使点「保存」也走另存为（后端同样拒绝覆盖内置）；
        // 用户模板的「另存为」传 null id。
        id: asNew || selected.builtin ? null : selected.id,
        name,
        description: draft.description,
        kind: selected.kind,
        body: draft.body,
      },
      {
        onSuccess: (saved) => {
          setSelectedId(saved.id);
          // 让 effect 用后端返回的正文重新同步草稿（另存为的名称也以返回值为准）。
          syncedRef.current = null;
        },
      },
    );
  };

  const doDelete = () => {
    if (selected === null || selected.builtin) return;
    deleteMutation.mutate(selected.id, {
      onSuccess: () => {
        syncedRef.current = null;
        setSelectedId(null);
        setConfirmDelete(false);
      },
    });
  };

  const templateName =
    selected === null ? S.prompt.preview.unsavedName : draft.name || S.prompt.preview.unsavedName;

  return (
    <main className="mx-auto flex w-full max-w-7xl flex-col gap-5 p-4 sm:p-6">
      <header>
        <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{S.prompt.title}</h1>
        <p className="mt-0.5 text-sm text-neutral-400">{S.prompt.subtitle}</p>
      </header>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <div className="flex min-w-0 flex-col gap-5">
          <TemplateLibrary
            templates={list}
            // 提示词页只生成实盘 / 复盘提示词：行情模板引用的是 `series`，
            // 在这页选它会立刻因类型校验被拒。
            kinds={["live", "review"]}
            loading={templatesQuery.isPending}
            error={templatesQuery.error}
            onRetry={() => void templatesQuery.refetch()}
            selectedId={effectiveId}
            onSelect={selectTemplate}
          />

          <PrivacyPicker value={privacy} onChange={setPrivacy} />

          {selected === null ? null : (
            <ContextPanel
              kind={kind}
              credentials={credentials}
              credentialId={credentialId}
              onCredential={setCredentialId}
              force={force}
              onForce={setForce}
              from={range.from}
              to={range.to}
              bar={range.bar}
              problem={rangeProblem}
              onRange={(next) => setRange((prev) => ({ ...prev, ...next }))}
            />
          )}

          {selected === null ? null : (
            <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
              <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h2 className="text-sm font-medium text-neutral-200">
                  {S.prompt.editor.title}
                </h2>
                {dirty ? (
                  <span className="text-xs text-amber-300">{S.prompt.editor.dirty}</span>
                ) : null}
              </div>

              <label className="flex flex-col gap-1 text-xs text-neutral-400">
                {S.prompt.editor.name}
                <input
                  type="text"
                  className={fieldClass}
                  placeholder={S.prompt.editor.namePlaceholder}
                  value={draft.name}
                  onChange={(event) => setDraft((prev) => ({ ...prev, name: event.target.value }))}
                />
              </label>
              <label className="flex flex-col gap-1 text-xs text-neutral-400">
                {S.prompt.editor.description}
                <input
                  type="text"
                  className={fieldClass}
                  placeholder={S.prompt.editor.descriptionPlaceholder}
                  value={draft.description}
                  onChange={(event) =>
                    setDraft((prev) => ({ ...prev, description: event.target.value }))
                  }
                />
              </label>
              <div className="flex flex-col gap-1 text-xs text-neutral-400">
                {S.prompt.editor.body}
                <TemplateEditor
                  value={draft.body}
                  onChange={(body) => {
                    setBodyEdited(true);
                    setDraft((prev) => ({ ...prev, body }));
                  }}
                />
              </div>

              {problem === null ? null : (
                <p className="text-xs text-amber-300">{templateProblemText(problem)}</p>
              )}

              <div className="flex flex-wrap items-center gap-2">
                {selected.builtin ? (
                  <>
                    <button
                      type="button"
                      disabled={problem !== null || saveMutation.isPending}
                      onClick={() => doSave(true)}
                      className="rounded-md bg-indigo-800 px-4 py-2 text-sm font-medium text-indigo-50 hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {saveMutation.isPending ? S.prompt.actions.saving : S.prompt.actions.saveAs}
                    </button>
                    <span className="text-xs text-neutral-500">
                      {S.prompt.actions.saveAsNote}
                    </span>
                  </>
                ) : (
                  <>
                    <button
                      type="button"
                      disabled={problem !== null || saveMutation.isPending}
                      onClick={() => doSave(false)}
                      className="rounded-md bg-indigo-800 px-4 py-2 text-sm font-medium text-indigo-50 hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {saveMutation.isPending ? S.prompt.actions.saving : S.prompt.actions.save}
                    </button>
                    <button
                      type="button"
                      disabled={problem !== null || saveMutation.isPending}
                      onClick={() => doSave(true)}
                      className="rounded-md border border-neutral-700 px-3 py-2 text-sm text-neutral-200 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {S.prompt.actions.saveAs}
                    </button>
                    {confirmDelete ? (
                      <>
                        <span className="text-xs text-amber-300">
                          {S.prompt.actions.deleteConfirmNote}
                        </span>
                        <button
                          type="button"
                          disabled={deleteMutation.isPending}
                          onClick={doDelete}
                          className="rounded-md bg-red-900/70 px-3 py-2 text-sm text-red-100 hover:bg-red-900 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {deleteMutation.isPending
                            ? S.prompt.actions.deleting
                            : S.prompt.actions.deleteConfirm}
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmDelete(false)}
                          className="rounded-md border border-neutral-700 px-3 py-2 text-sm text-neutral-300 hover:bg-neutral-800"
                        >
                          {S.prompt.actions.deleteCancel}
                        </button>
                      </>
                    ) : (
                      <button
                        type="button"
                        onClick={() => setConfirmDelete(true)}
                        className="rounded-md border border-red-900/60 px-3 py-2 text-sm text-red-300 hover:bg-red-950/40"
                      >
                        {S.prompt.actions.delete}
                      </button>
                    )}
                  </>
                )}
              </div>

              {saveMutation.error === null || saveMutation.error === undefined ? null : (
                <p className="font-mono text-xs break-all text-red-300">
                  {S.prompt.actions.saveFailed}：
                  {String((saveMutation.error as { message?: string }).message ?? saveMutation.error)}
                </p>
              )}
              {deleteMutation.error === null || deleteMutation.error === undefined ? null : (
                <p className="font-mono text-xs break-all text-red-300">
                  {S.prompt.actions.deleteFailed}：
                  {String(
                    (deleteMutation.error as { message?: string }).message ?? deleteMutation.error,
                  )}
                </p>
              )}
            </section>
          )}
        </div>

        <div className="flex min-w-0 flex-col gap-5">
          <PreviewPanel
            output={preview.data ?? null}
            error={preview.error}
            isFetching={preview.isFetching}
            isPending={preview.isPending}
            enabled={previewEnabled}
            templateName={templateName}
          />
          <ExportBar
            output={preview.data ?? null}
            privacy={privacy}
            templateName={templateName}
          />
        </div>
      </div>
    </main>
  );
}
