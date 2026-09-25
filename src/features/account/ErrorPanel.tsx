/**
 * 账户页的错误态：把 Rust `AppError` 的 `{ code, message, retryable }` 完整摊开。
 *
 * 「重试」按钮只在 `retryable` 为 true 时出现——对不可自愈的错误（迁移、配置）
 * 给一个按了也没用的按钮，只会让用户反复空转。
 */
import { S } from "../../lib/strings";
import { errorPayloadOf } from "../live/errors";

export function ErrorPanel({
  title,
  error,
  hint,
  onRetry,
  busy = false,
}: {
  title: string;
  error: unknown;
  hint?: string;
  onRetry?: () => void;
  busy?: boolean;
}) {
  const payload = errorPayloadOf(error);
  return (
    <section className="rounded-xl border border-red-900/60 bg-red-950/30 p-4">
      <h2 className="text-sm font-medium text-red-300">{title}</h2>
      <p className="mt-2 font-mono text-xs break-all text-red-200/90">{payload.message}</p>
      <p className="mt-1 text-xs text-red-300/70">{S.account.errorCode(payload.code)}</p>
      {hint === undefined ? null : <p className="mt-1 text-xs text-neutral-400">{hint}</p>}
      {payload.retryable && onRetry !== undefined ? (
        <button
          type="button"
          onClick={onRetry}
          disabled={busy}
          className="mt-3 rounded-md bg-red-900/70 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {S.account.retry}
        </button>
      ) : null}
      {payload.retryable ? null : (
        <p className="mt-2 text-xs text-red-300/70">{S.account.errorNotRetryable}</p>
      )}
    </section>
  );
}
