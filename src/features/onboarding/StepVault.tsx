/**
 * 引导第 1 步：创建 / 解锁密钥库。
 *
 * 两种模式由 `status.exists` 决定，但都走同一条 `vault_unlock` 命令
 * （Rust 侧注释明确：密钥库不存在时 unlock 等同于创建）。
 *
 * 创建模式的两道闸门是刻意的：两次密码必须一致，且必须勾选「已知晓主密码无法找回」。
 * 主密码丢失后已存凭据就永远读不出来，这不能让用户随手点过去。
 */
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";

import { call } from "../../lib/ipc";
import { S } from "../../lib/strings";
import { errorPayloadOf } from "../live/errors";
import { validateVaultCreate, validateVaultUnlock } from "./validation";

const INPUT_CLASS =
  "mt-1 w-full rounded-md border border-neutral-700 bg-neutral-900 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-neutral-500";

export function StepVault({
  exists,
  onUnlocked,
}: {
  exists: boolean;
  onUnlocked: () => void;
}) {
  const creating = !exists;
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);

  const unlock = useMutation({
    mutationFn: (value: string) => call("vault_unlock", { password: value }),
    onSuccess: () => onUnlocked(),
  });

  const formError = creating
    ? validateVaultCreate(password, confirm)
    : validateVaultUnlock(password);
  const touched = password.length > 0 || confirm.length > 0;
  const blocked = unlock.isPending || formError !== null || (creating && !acknowledged);
  const unlockError = unlock.isError ? errorPayloadOf(unlock.error) : null;

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (!blocked) unlock.mutate(password);
      }}
    >
      <div>
        <h2 className="text-lg font-semibold text-neutral-100">
          {creating ? S.onboarding.vault.createTitle : S.onboarding.vault.unlockTitle}
        </h2>
        <p className="mt-1 text-sm text-neutral-400">
          {creating ? S.onboarding.vault.createHint : S.onboarding.vault.unlockHint}
        </p>
      </div>

      {creating ? (
        <div
          role="alert"
          className="rounded-xl border border-red-900/60 bg-red-950/30 p-4 text-sm"
        >
          <p className="font-medium text-red-300">{S.onboarding.vault.irrecoverableTitle}</p>
          <p className="mt-1 text-red-200/90">{S.onboarding.vault.irrecoverable}</p>
        </div>
      ) : null}

      <label className="block text-sm text-neutral-300">
        {S.onboarding.vault.password}
        <input
          type="password"
          autoComplete="new-password"
          className={INPUT_CLASS}
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
      </label>

      {creating ? (
        <label className="block text-sm text-neutral-300">
          {S.onboarding.vault.confirm}
          <input
            type="password"
            autoComplete="new-password"
            className={INPUT_CLASS}
            value={confirm}
            onChange={(event) => setConfirm(event.target.value)}
          />
        </label>
      ) : null}

      {creating ? (
        <label className="flex items-start gap-2 text-sm text-neutral-300">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={acknowledged}
            onChange={(event) => setAcknowledged(event.target.checked)}
          />
          <span>{S.onboarding.vault.ack}</span>
        </label>
      ) : null}

      {touched && formError !== null ? (
        <p className="text-sm text-red-300">{formError}</p>
      ) : null}

      {unlockError === null ? null : (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">{unlockError.message}</p>
          <p className="mt-1 text-xs text-red-300/70">
            {S.onboarding.errorCode(unlockError.code)}
          </p>
        </div>
      )}

      <button
        type="submit"
        disabled={blocked}
        className="self-start rounded-md bg-neutral-100 px-4 py-2 text-sm font-medium text-neutral-900 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
      >
        {unlock.isPending
          ? S.onboarding.vault.working
          : creating
            ? S.onboarding.vault.create
            : S.onboarding.vault.unlock}
      </button>
    </form>
  );
}
