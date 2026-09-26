/**
 * 隐私等级选择器：L0 / L1 / L2。
 *
 * 它在**设置页**而不是某个生成页：等级是全局的（三条提示词管线共用），
 * 放在某个生成页会让人以为「这一页单独设」——那正是这次重构要消除的状态。
 * 每一档都给出「它到底做了什么」的人话说明，并写明「隐私分级在后端强制，
 * 改模板也绕不过去」。
 *
 * 自己读全局值、自己写回（`usePrivacy` / `useSetPrivacy`），所以放进任何页面即生效，
 * 不需要上层再传一份状态。
 */
import { S } from "../../lib/strings";
import { PRIVACY_LEVELS, privacyBadgeClass, privacyDescription, privacyLabel } from "./format";
import { usePrivacy, useSetPrivacy } from "./usePrivacy";

export function PrivacyPicker() {
  const level = usePrivacy();
  const save = useSetPrivacy();

  return (
    <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.settings.privacy.title}</h2>
      <p className="mt-0.5 text-xs text-neutral-500">{S.settings.privacy.subtitle}</p>
      <p className="mt-2 rounded-lg border border-indigo-900/50 bg-indigo-950/20 p-2 text-xs text-indigo-200/90">
        {S.settings.privacy.enforcedNote}
      </p>

      <div className="mt-3 flex flex-col gap-2">
        {PRIVACY_LEVELS.map((option) => {
          const active = option === level;
          return (
            <button
              key={option}
              type="button"
              aria-pressed={active}
              disabled={save.isPending}
              onClick={() => save.mutate(option)}
              className={`${privacyBadgeClass(option, active)} disabled:cursor-not-allowed disabled:opacity-60`}
            >
              <span className="text-sm font-medium">{privacyLabel(option)}</span>
              <span className="text-xs opacity-90">{privacyDescription(option)}</span>
            </button>
          );
        })}
      </div>

      <p className="mt-2 text-xs text-neutral-500">
        {S.settings.privacy.current(privacyLabel(level))}
      </p>
      {save.isError ? (
        <p className="mt-1 font-mono text-xs break-all text-red-300">
          {S.settings.privacy.saveFailed}
        </p>
      ) : null}
    </section>
  );
}
