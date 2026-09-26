/**
 * 实盘上下文输入：可选凭据（不选则只有行情）+ 强制刷新开关。
 *
 * 只在实盘页用。复盘页的上下文（凭据 + 时段）由它自己的 `AssemblePanel` 与
 * `RangePicker` 负责——同一个页面上出现两套时段控件只会让两边不一致，
 * 所以这里刻意不提供「通用上下文面板」。
 */
import { S } from "../../lib/strings";
import type { CredentialMeta } from "../../lib/types";

const inputClass =
  "rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-100";

export interface CredentialsState {
  list: CredentialMeta[];
  isPending: boolean;
  isError: boolean;
}

function CredentialSelect({
  credentials,
  credentialId,
  onCredential,
}: {
  credentials: CredentialsState;
  credentialId: string | null;
  onCredential: (id: string | null) => void;
}) {
  if (credentials.isPending) {
    return <p className="text-xs text-neutral-500">{S.prompt.context.credentialLoading}</p>;
  }
  if (credentials.isError) {
    return <p className="text-xs text-red-300">{S.prompt.context.credentialError}</p>;
  }
  return (
    <label className="flex flex-col gap-1 text-xs text-neutral-400">
      {S.prompt.context.credential}
      <select
        className={inputClass}
        value={credentialId ?? ""}
        onChange={(event) => onCredential(event.target.value === "" ? null : event.target.value)}
      >
        <option value="">{S.prompt.context.noCredential}</option>
        {credentials.list.map((meta) => (
          <option key={meta.id} value={meta.id}>
            {`${meta.label} · ${meta.api_key_masked} · ${
              meta.env === "demo" ? S.account.envDemo : S.account.envLive
            }`}
          </option>
        ))}
      </select>
    </label>
  );
}

export function LiveContextPanel({
  credentials,
  credentialId,
  onCredential,
  force,
  onForce,
}: {
  credentials: CredentialsState;
  credentialId: string | null;
  onCredential: (id: string | null) => void;
  force: boolean;
  onForce: (next: boolean) => void;
}) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.prompt.context.liveTitle}</h2>

      <CredentialSelect
        credentials={credentials}
        credentialId={credentialId}
        onCredential={onCredential}
      />
      <p className="text-[11px] text-neutral-500">{S.prompt.context.credentialNote}</p>

      <label className="flex items-center gap-2 text-xs text-neutral-300">
        <input
          type="checkbox"
          checked={force}
          onChange={(event) => onForce(event.target.checked)}
          className="h-4 w-4 rounded border-neutral-700 bg-neutral-900"
        />
        {S.prompt.context.force}
      </label>
      <p className="text-[11px] text-neutral-500">{S.prompt.context.forceNote}</p>
    </section>
  );
}
