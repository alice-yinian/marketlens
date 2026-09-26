/**
 * 提示词库（纯管理）。
 *
 * 这里只有「管理」：列模板（实盘 / 复盘 / 行情三类全在此）、编辑正文、另存为、删除，
 * 以及**不需要上下文**的语法校验。
 *
 * 生成提示词**不在这里**——它需要上下文，所以按上下文归位：实盘页、复盘页、行情页。
 * 这一刀切开的理由：原来同一个页面上既有「改模板」又有「用模板」，
 * 于是「我改了模板但预览没变（在看另一个模板）」这类误会几乎无法避免。
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { call } from "../../lib/ipc";
import { S } from "../../lib/strings";
import { ErrorPanel } from "../account/ErrorPanel";
import { TemplateEditor } from "./TemplateEditor";
import { TemplateLibrary } from "./TemplateLibrary";
import { saveAsName, templateProblem, templateProblemText } from "./format";
import { useDebouncedValue } from "./useDebouncedValue";
import { useDeleteTemplate, useSaveTemplate, useTemplates } from "./useTemplates";

/** 三类模板全在这里管理：库的职责就是把它们放在一起。 */
const ALL_KINDS = ["live", "review", "market"] as const;

const fieldClass =
  "rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100";

interface Draft {
  name: string;
  description: string;
  body: string;
}

/**
 * 语法校验：只需正文，不需要上下文，也不联网。
 *
 * 与生成时的渲染分开：渲染会报「缺哪个变量」（那需要上下文），
 * 这里只回答「这段正文本身写对没有」——正是改模板时最需要的那条反馈。
 * 正文防抖 300ms 后才校验，避免每敲一个字发一次 IPC。
 */
function CheckPanel({ body }: { body: string }) {
  const debounced = useDebouncedValue(body, 300);
  const trimmed = debounced.trim();

  const query = useQuery({
    queryKey: ["template_check", trimmed],
    queryFn: () => call("template_check", { body: trimmed }),
    enabled: trimmed.length > 0,
    staleTime: Infinity,
  });

  return (
    <section className="flex flex-col gap-2 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.prompt.editor.checkTitle}</h2>

      {trimmed.length === 0 ? (
        <p className="text-xs text-neutral-500">{S.prompt.editor.checkEmpty}</p>
      ) : query.isError ? (
        <ErrorPanel
          title={S.prompt.check.errorTitle}
          error={query.error}
          onRetry={() => void query.refetch()}
          busy={query.isFetching}
        />
      ) : query.data === undefined ? (
        <p className="text-xs text-neutral-500">{S.prompt.editor.checking}</p>
      ) : (
        <>
          <p className="text-xs text-emerald-300">{S.prompt.check.ok(query.data.length)}</p>
          {query.data.length === 0 ? null : (
            <p className="font-mono text-xs break-all text-neutral-400">
              {S.prompt.check.variables(query.data.join(" · "))}
            </p>
          )}
        </>
      )}
    </section>
  );
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
  }, [selected]);

  const [confirmDelete, setConfirmDelete] = useState(false);

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
            kinds={ALL_KINDS}
            loading={templatesQuery.isPending}
            error={templatesQuery.error}
            onRetry={() => void templatesQuery.refetch()}
            selectedId={effectiveId}
            onSelect={selectTemplate}
          />
        </div>

        <div className="flex min-w-0 flex-col gap-5">
          {selected === null ? null : (
            <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
              <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h2 className="text-sm font-medium text-neutral-200">{S.prompt.editor.title}</h2>
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
                  onChange={(body) => setDraft((prev) => ({ ...prev, body }))}
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

          <CheckPanel body={draft.body} />
        </div>
      </div>
    </main>
  );
}
