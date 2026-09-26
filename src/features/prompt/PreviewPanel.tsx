/**
 * 预览面板：正文 + token/字符数 + warnings + 错误条。
 *
 * 两条硬要求：
 * 1. **错误原样展示**：后端 `Template` 错误消息里带变量名 / 语法位置，
 *    是用户改模板的唯一线索，绝不吞掉、绝不改写成笼统文案。
 * 2. **重新生成不闪烁**：`isFetching` 且已有上一次结果时，保留结果，
 *    只在标题旁给一个细微的加载指示。
 */
import { S } from "../../lib/strings";
import type { PromptOutput } from "../../lib/types";
import { errorPayloadOf } from "../live/errors";
import { formatTimestamp, outputStats } from "./format";

function ErrorBar({ error }: { error: unknown }) {
  const payload = errorPayloadOf(error);
  if (payload.code === "RangeTooLarge") {
    return (
      <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
        <p className="text-xs font-medium text-amber-300">{S.prompt.preview.rangeTooLargeTitle}</p>
        <p className="mt-1 text-xs text-amber-200/90">{S.prompt.context.rangeTooLarge}</p>
      </div>
    );
  }
  // 标题与提示按**错误来源**区分：这个面板现在承接的失败远不只模板问题
  // （缺凭据、时段超限、网络……），把「本机还没配 OKX 凭据」说成「模板渲染失败」
  // 会把人引到完全错误的方向去。
  const templateProblem = payload.code === "Template";

  return (
    <div className="rounded-lg border border-red-900/60 bg-red-950/30 p-3">
      <p className="text-xs font-medium text-red-300">{S.prompt.preview.errorTitle}</p>
      {/* 原样展示后端消息：变量名与语法位置都在里面 */}
      <p className="mt-1 font-mono text-xs break-all text-red-200/90">{payload.message}</p>
      <p className="mt-1 text-[11px] text-red-300/70">
        {templateProblem ? S.prompt.preview.errorHintTemplate : S.prompt.preview.errorHintOther}
      </p>
      <p className="mt-1 text-[11px] text-red-300/60">
        {S.account.errorCode(payload.code)}
      </p>
    </div>
  );
}

export function PreviewPanel({
  output,
  error,
  isFetching,
  isPending,
  enabled,
  templateName,
}: {
  output: PromptOutput | null;
  error: unknown | null;
  isFetching: boolean;
  isPending: boolean;
  enabled: boolean;
  templateName: string;
}) {
  const stats = output === null ? null : outputStats(output);

  return (
    <section className="flex min-w-0 flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium text-neutral-200">{S.prompt.preview.title}</h2>
        <span className="flex flex-wrap items-center gap-x-3 text-xs text-neutral-500">
          <span>{S.prompt.preview.templateName(templateName)}</span>
          {output === null ? null : (
            <span>{S.prompt.preview.generatedAt(formatTimestamp(output.generated_at))}</span>
          )}
          {isFetching ? (
            <span className="text-indigo-300/90">{S.prompt.preview.refreshing}</span>
          ) : null}
        </span>
      </div>

      {error === null || error === undefined ? null : <ErrorBar error={error} />}

      {output !== null && output.warnings.length > 0 ? (
        <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3">
          <p className="text-xs font-medium text-amber-300">{S.prompt.preview.warningsTitle}</p>
          <p className="mt-0.5 text-[11px] text-amber-200/80">{S.prompt.preview.warningsHint}</p>
          <ul className="mt-1 list-inside list-disc text-xs text-amber-200/90">
            {output.warnings.map((warning, index) => (
              <li key={`${index}-${warning}`}>{warning}</li>
            ))}
          </ul>
        </div>
      ) : null}

      {!enabled ? (
        <p className="text-xs text-neutral-500">{S.prompt.preview.empty}</p>
      ) : isPending && output === null ? (
        <p className="text-xs text-neutral-500">{S.prompt.preview.firstLoading}</p>
      ) : output === null ? (
        <p className="text-xs text-neutral-500">{S.prompt.preview.empty}</p>
      ) : (
        <div className="flex min-w-0 flex-col gap-2">
          <pre className="max-h-[28rem] overflow-auto rounded-lg border border-neutral-800 bg-neutral-950 p-3 font-mono text-[12px] leading-relaxed whitespace-pre-wrap text-neutral-200">
            {output.text}
          </pre>
          {stats === null ? null : (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-neutral-400">
              <span>{S.prompt.preview.tokens(stats.tokens)}</span>
              <span>{S.prompt.preview.chars(stats.chars)}</span>
              <span className="text-neutral-600">{S.prompt.preview.tokenNote}</span>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
