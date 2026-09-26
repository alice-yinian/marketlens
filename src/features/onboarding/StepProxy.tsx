/**
 * 引导第 2 步：网络代理（可选）。
 *
 * **为什么排在凭据之前**：受限网络下，下一步的 `credentials_test` 与再下一步的
 * `watchlist_candidates` 都会失败，而这两个失败都不指向真正的原因（网络到不了 OKX）。
 * 代理放在这里，用户才有机会在遇到那两个失败**之前**把路打通。
 *
 * 「保存并测试」是一个动作：只保存不测试，用户不知道填对没有；只测试不保存，
 * 下一步用的还是旧配置。成功后原地出现「下一步」，改了输入则结果自动作废——
 * 界面不会出现「测过但改过」的含糊状态。
 */
import { useState } from "react";

import { S } from "../../lib/strings";
import { ErrorPanel } from "../account/ErrorPanel";
import { formatLocalTime } from "../live/format";
import { errorPayloadOf } from "../live/errors";
import {
  useClearProxy,
  useProxySettings,
  useSaveProxy,
} from "../settings/useProxySettings";

const INPUT_CLASS =
  "mt-1 w-full rounded-md border border-neutral-700 bg-neutral-900 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-neutral-500";

const PRIMARY_BUTTON =
  "rounded-md bg-neutral-100 px-3 py-1.5 text-sm font-medium text-neutral-900 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50";

const SECONDARY_BUTTON =
  "rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50";

export function StepProxy({ onNext }: { onNext: () => void }) {
  const current = useProxySettings();
  const save = useSaveProxy();
  const clear = useClearProxy();

  // `null` = 用户还没改过输入，此时跟随已保存的值（reconfigure 形态下预填）。
  const [input, setInput] = useState<string | null>(null);

  const value = input ?? current.data?.url ?? "";
  const trimmed = value.trim();

  // 探测结果只对**当前输入**有效：改过输入后旧结果立刻作废。
  const probe = save.isSuccess && save.variables.trim() === trimmed ? save.data.probe : null;

  const saveError = save.isError ? errorPayloadOf(save.error) : null;

  const clearThenNext = () => clear.mutate(undefined, { onSuccess: () => onNext() });

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold text-neutral-100">{S.onboarding.proxy.title}</h2>
        <p className="mt-1 text-sm text-neutral-400">{S.onboarding.proxy.hint}</p>
      </div>

      {current.isPending ? (
        <p className="text-xs text-neutral-500">{S.boot.loading}</p>
      ) : current.isError && current.data === undefined ? (
        <ErrorPanel
          title={S.onboarding.proxy.loadErrorTitle}
          error={current.error}
          onRetry={() => void current.refetch()}
          busy={current.isFetching}
        />
      ) : (
        <p className="text-xs text-neutral-500">
          {current.data?.url == null
            ? S.onboarding.proxy.none
            : S.onboarding.proxy.saved(current.data.url)}
        </p>
      )}

      <label className="block text-sm text-neutral-300">
        {S.onboarding.proxy.url}
        <input
          className={INPUT_CLASS}
          placeholder={S.onboarding.proxy.placeholder}
          value={value}
          onChange={(event) => setInput(event.target.value)}
        />
      </label>

      <div className="flex flex-col gap-1">
        <p className="text-xs text-neutral-500">
          {trimmed === "" ? S.onboarding.proxy.skipHint : S.onboarding.proxy.schemes}
        </p>
        <p className="text-xs text-neutral-600">{S.onboarding.proxy.credentialWarning}</p>
      </div>

      {saveError === null ? null : (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">{saveError.message}</p>
          <p className="mt-1 text-xs text-red-300/70">
            {S.onboarding.errorCode(saveError.code)}
          </p>
        </div>
      )}

      {probe === null ? null : probe.ok ? (
        <div className="rounded-xl border border-emerald-900/60 bg-emerald-950/30 p-3 text-sm">
          <p className="text-emerald-300">{S.onboarding.proxy.ok(probe.latency_ms)}</p>
          <p className="mt-1 text-xs text-emerald-200/70">
            {S.onboarding.proxy.serverTime(
              formatLocalTime(probe.server_time_ms) ?? S.account.na,
            )}
          </p>
        </div>
      ) : (
        <div role="alert" className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-medium text-red-300">{S.onboarding.proxy.failTitle}</p>
          <p className="mt-1 font-mono break-all text-red-200/90">
            {probe.error ?? S.account.na}
          </p>
          <p className="mt-1 text-xs text-red-300/70">{S.onboarding.proxy.failHint}</p>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3">
        {trimmed === "" ? (
          <button type="button" className={PRIMARY_BUTTON} disabled={clear.isPending} onClick={clearThenNext}>
            {clear.isPending ? S.onboarding.proxy.testing : S.onboarding.proxy.skip}
          </button>
        ) : probe?.ok === true ? (
          <button type="button" className={PRIMARY_BUTTON} onClick={onNext}>
            {S.onboarding.proxy.next}
          </button>
        ) : (
          <button
            type="button"
            className={PRIMARY_BUTTON}
            disabled={save.isPending}
            onClick={() => save.mutate(trimmed)}
          >
            {save.isPending ? S.onboarding.proxy.testing : S.onboarding.proxy.test}
          </button>
        )}

        {trimmed !== "" && current.data?.url != null ? (
          <button
            type="button"
            className={SECONDARY_BUTTON}
            disabled={clear.isPending}
            onClick={clearThenNext}
          >
            {S.onboarding.proxy.skip}
          </button>
        ) : null}
      </div>
    </div>
  );
}
