/**
 * 引导第 2 步：录入 OKX 只读凭据（可跳过）。
 *
 * 保存后自动 `credentials_test`：探测到 `read_only === false` 时必须给出红色警示——
 * 这是设计里明确的安全特性，不能只当作一条普通信息。
 * 明文密钥只向上送（`credentials_save`），界面永远拿不回明文，也不提供任何查看入口。
 */
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";

import { call, type SaveCredentialInput } from "../../lib/ipc";
import { S } from "../../lib/strings";
import type { CredentialProbe } from "../../lib/types";
import { errorPayloadOf } from "../live/errors";
import { posModeLabel } from "../account/format";

const INPUT_CLASS =
  "mt-1 w-full rounded-md border border-neutral-700 bg-neutral-900 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-neutral-500";

function ProbeResult({ probe, onRetry }: { probe: CredentialProbe; onRetry: () => void }) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
      <h3 className="text-sm font-medium text-neutral-200">{S.onboarding.credential.testTitle}</h3>

      {probe.read_only === false ? (
        <div role="alert" className="rounded-lg border border-red-900/60 bg-red-950/40 p-3">
          <p className="text-sm font-medium text-red-300">
            {S.onboarding.credential.readOnlyFalseTitle}
          </p>
          <p className="mt-1 text-sm text-red-200/90">
            {S.onboarding.credential.readOnlyFalse}
          </p>
        </div>
      ) : null}

      {probe.ok ? (
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
          <dt className="text-xs text-neutral-500">{S.onboarding.credential.uid}</dt>
          <dd className="font-mono text-neutral-200">{probe.uid_masked ?? S.account.na}</dd>
          <dt className="text-xs text-neutral-500">{S.onboarding.credential.posMode}</dt>
          <dd className="text-neutral-200">{posModeLabel(probe.pos_mode) ?? S.account.na}</dd>
          <dt className="text-xs text-neutral-500">{S.onboarding.credential.permissions}</dt>
          <dd className="font-mono break-all text-neutral-200">
            {probe.permissions ?? S.account.na}
          </dd>
          <dt className="text-xs text-neutral-500">{S.onboarding.credential.probeOk}</dt>
          <dd className={probe.read_only ? "text-emerald-400" : "text-red-400"}>
            {probe.read_only ? S.onboarding.credential.readOnly : S.onboarding.credential.readOnlyFalseTitle}
          </dd>
        </dl>
      ) : (
        <div className="flex flex-col gap-2">
          <p className="text-sm text-red-300">{S.onboarding.credential.probeFail}</p>
          <p className="font-mono break-all text-sm text-red-200/90">
            {probe.error ?? S.account.na}
          </p>
          <button
            type="button"
            onClick={onRetry}
            className="self-start rounded-md bg-red-900/70 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900"
          >
            {S.onboarding.credential.retryTest}
          </button>
        </div>
      )}
    </div>
  );
}

export function StepCredential({ onNext }: { onNext: () => void }) {
  const [label, setLabel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [demo, setDemo] = useState(false);

  const probe = useMutation({
    mutationFn: (id: string) => call("credentials_test", { id }),
  });

  const save = useMutation({
    mutationFn: (input: SaveCredentialInput) => call("credentials_save", { input }),
    onSuccess: (meta) => probe.mutate(meta.id),
  });

  const complete = [label, apiKey, secretKey, passphrase].every((value) => value.trim().length > 0);
  const savedId = save.data?.id ?? null;

  const submit = () => {
    if (save.isPending || !complete) return;
    save.mutate({
      id: savedId,
      label,
      api_key: apiKey,
      secret_key: secretKey,
      passphrase,
      demo,
    });
  };

  const saveError = save.isError ? errorPayloadOf(save.error) : null;
  const probeError = probe.isError ? errorPayloadOf(probe.error) : null;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold text-neutral-100">
          {S.onboarding.credential.title}
        </h2>
        <p className="mt-1 text-sm text-neutral-400">{S.onboarding.credential.hint}</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="block text-sm text-neutral-300">
          {S.onboarding.credential.label}
          <input
            className={INPUT_CLASS}
            placeholder={S.onboarding.credential.labelPlaceholder}
            value={label}
            onChange={(event) => setLabel(event.target.value)}
          />
        </label>
        <label className="block text-sm text-neutral-300">
          {S.onboarding.credential.apiKey}
          <input
            className={INPUT_CLASS}
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
          />
        </label>
        <label className="block text-sm text-neutral-300">
          {S.onboarding.credential.secretKey}
          <input
            className={INPUT_CLASS}
            value={secretKey}
            onChange={(event) => setSecretKey(event.target.value)}
          />
        </label>
        <label className="block text-sm text-neutral-300">
          {S.onboarding.credential.passphrase}
          <input
            className={INPUT_CLASS}
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
          />
        </label>
      </div>

      <label className="flex items-start gap-2 text-sm text-neutral-300">
        <input
          type="checkbox"
          className="mt-0.5"
          checked={demo}
          onChange={(event) => setDemo(event.target.checked)}
        />
        <span>
          {S.onboarding.credential.demo}
          <span className="ml-2 text-xs text-neutral-500">
            {S.onboarding.credential.demoHint}
          </span>
        </span>
      </label>

      {saveError === null ? null : (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">{saveError.message}</p>
          <p className="mt-1 text-xs text-red-300/70">
            {S.onboarding.errorCode(saveError.code)}
          </p>
        </div>
      )}

      {save.data === undefined ? null : (
        <p className="text-xs text-neutral-500">
          {S.onboarding.credential.saved}
          <span className="ml-2 font-mono text-neutral-300">{save.data.api_key_masked}</span>
          <span className="ml-2">
            {save.data.env === "demo" ? S.account.envDemo : S.account.envLive}
          </span>
        </p>
      )}

      {probeError === null ? null : (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">{probeError.message}</p>
          <p className="mt-1 text-xs text-red-300/70">
            {S.onboarding.errorCode(probeError.code)}
          </p>
        </div>
      )}

      {probe.isPending ? (
        <p className="text-sm text-neutral-400">{S.onboarding.credential.testing}</p>
      ) : probe.data === undefined ? null : (
        <ProbeResult
          probe={probe.data}
          onRetry={() => {
            if (savedId !== null) probe.mutate(savedId);
          }}
        />
      )}

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={submit}
          disabled={save.isPending || !complete}
          className="rounded-md bg-neutral-100 px-4 py-2 text-sm font-medium text-neutral-900 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
        >
          {save.isPending ? S.onboarding.credential.saving : S.onboarding.credential.save}
        </button>
        <button
          type="button"
          onClick={onNext}
          className="rounded-md border border-neutral-700 px-4 py-2 text-sm text-neutral-200 hover:bg-neutral-800"
        >
          {savedId === null ? S.onboarding.credential.skip : S.onboarding.next}
        </button>
      </div>

      <p className="text-xs text-neutral-500">{S.onboarding.credential.skipNote}</p>
    </div>
  );
}
