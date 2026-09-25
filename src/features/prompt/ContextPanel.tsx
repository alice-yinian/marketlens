/**
 * 上下文输入：实盘与复盘的差异都在这里。
 *
 * - 实盘：可选凭据（不选则只有行情）+ 强制刷新开关；
 * - 复盘：时段（起止 + 粒度）——自己实现的精简版，不 import review 的组件。
 *   时段上限 90 天前置校验并说明，超限时不请求（后端仍会独立校验）。
 */
import { S } from "../../lib/strings";
import type { CredentialMeta, TemplateKind } from "../../lib/types";
import {
  BAR_OPTIONS,
  barLabel,
  localInputToMs,
  msToLocalInput,
  rangeDays,
  rangeProblemText,
  type RangeProblem,
} from "./format";

const DAY_MS = 24 * 60 * 60 * 1000;

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
  optional,
}: {
  credentials: CredentialsState;
  credentialId: string | null;
  onCredential: (id: string | null) => void;
  optional: boolean;
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
        {optional ? <option value="">{S.prompt.context.noCredential}</option> : null}
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

function LiveContext({
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
        optional
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

function ReviewContext({
  credentials,
  credentialId,
  onCredential,
  from,
  to,
  bar,
  problem,
  onChange,
}: {
  credentials: CredentialsState;
  credentialId: string | null;
  onCredential: (id: string | null) => void;
  from: number;
  to: number;
  bar: string;
  problem: RangeProblem | null;
  onChange: (next: { from?: number; to?: number; bar?: string }) => void;
}) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.prompt.context.reviewTitle}</h2>

      <CredentialSelect
        credentials={credentials}
        credentialId={credentialId}
        onCredential={onCredential}
        optional={false}
      />
      {credentialId === null ? (
        <p className="text-xs text-amber-300">{S.prompt.context.needCredential}</p>
      ) : null}

      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 text-xs text-neutral-400">
          {S.prompt.context.rangeFrom}
          <input
            type="datetime-local"
            className={inputClass}
            value={msToLocalInput(from)}
            onChange={(event) => {
              const ms = localInputToMs(event.target.value);
              if (ms !== null) onChange({ from: ms });
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-neutral-400">
          {S.prompt.context.rangeTo}
          <input
            type="datetime-local"
            className={inputClass}
            value={msToLocalInput(to)}
            onChange={(event) => {
              const ms = localInputToMs(event.target.value);
              if (ms !== null) onChange({ to: ms });
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-neutral-400">
          {S.prompt.context.rangeBar}
          <select
            className={inputClass}
            value={bar}
            onChange={(event) => onChange({ bar: event.target.value })}
          >
            {BAR_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {barLabel(option)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-neutral-500">{S.prompt.context.quick}</span>
        <button
          type="button"
          onClick={() => {
            const now = Date.now();
            onChange({ from: now - 7 * DAY_MS, to: now });
          }}
          className="rounded-md border border-neutral-700 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
        >
          {S.prompt.context.last7d}
        </button>
        <button
          type="button"
          onClick={() => {
            const now = Date.now();
            onChange({ from: now - 30 * DAY_MS, to: now });
          }}
          className="rounded-md border border-neutral-700 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
        >
          {S.prompt.context.last30d}
        </button>
        {from < to ? (
          <span className="text-xs text-neutral-600">
            {S.prompt.context.rangeDays(rangeDays(from, to))}
          </span>
        ) : null}
      </div>

      <p className="text-[11px] text-neutral-500">{S.prompt.context.maxRangeNote}</p>
      {problem === null ? null : (
        <p className="text-xs text-amber-300">{rangeProblemText(problem)}</p>
      )}
    </section>
  );
}

export function ContextPanel({
  kind,
  credentials,
  credentialId,
  onCredential,
  force,
  onForce,
  from,
  to,
  bar,
  problem,
  onRange,
}: {
  kind: TemplateKind;
  credentials: CredentialsState;
  credentialId: string | null;
  onCredential: (id: string | null) => void;
  force: boolean;
  onForce: (next: boolean) => void;
  from: number;
  to: number;
  bar: string;
  problem: RangeProblem | null;
  onRange: (next: { from?: number; to?: number; bar?: string }) => void;
}) {
  return kind === "live" ? (
    <LiveContext
      credentials={credentials}
      credentialId={credentialId}
      onCredential={onCredential}
      force={force}
      onForce={onForce}
    />
  ) : (
    <ReviewContext
      credentials={credentials}
      credentialId={credentialId}
      onCredential={onCredential}
      from={from}
      to={to}
      bar={bar}
      problem={problem}
      onChange={onRange}
    />
  );
}
