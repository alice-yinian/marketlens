/**
 * 隐私等级选择器：L0 / L1 / L2，默认 L1。
 *
 * 每一档都给出「它到底做了什么」的人话说明；页面必须写明
 * 「隐私分级在后端强制，改模板也绕不过去」。
 * 点击某一档只负责把新等级交给上层——重新生成由 queryKey 变化自动触发。
 */
import { S } from "../../lib/strings";
import type { PrivacyLevel } from "../../lib/types";
import { PRIVACY_LEVELS, privacyBadgeClass, privacyDescription, privacyLabel } from "./format";

export function PrivacyPicker({
  value,
  onChange,
}: {
  value: PrivacyLevel;
  onChange: (level: PrivacyLevel) => void;
}) {
  return (
    <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.prompt.privacy.title}</h2>
      <p className="mt-1 rounded-lg border border-indigo-900/50 bg-indigo-950/20 p-2 text-xs text-indigo-200/90">
        {S.prompt.privacy.enforcedNote}
      </p>

      <div className="mt-3 flex flex-col gap-2">
        {PRIVACY_LEVELS.map((level) => {
          const active = level === value;
          return (
            <button
              key={level}
              type="button"
              aria-pressed={active}
              onClick={() => onChange(level)}
              className={privacyBadgeClass(level, active)}
            >
              <span className="text-sm font-medium">{privacyLabel(level)}</span>
              <span className="text-xs opacity-90">{privacyDescription(level)}</span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
