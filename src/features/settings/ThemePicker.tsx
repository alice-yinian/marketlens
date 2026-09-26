/**
 * 界面主题选择器：跟随系统 / 日间 / 夜间。
 *
 * 只提供设置页这一个入口（导航栏不放快捷按钮）：主题是「设一次就不动」的东西，
 * 而导航栏的每个位置都更该留给高频动作。
 *
 * 自己读全局值、自己写回，所以放进任何页面即生效。
 */
import { S } from "../../lib/strings";
import type { Theme } from "../../lib/types";
import { useResolvedTheme, useSetTheme, useThemeSetting } from "./useTheme";

const THEME_OPTIONS: readonly { value: Theme; label: string; desc: string }[] = [
  { value: "system", label: S.settings.theme.system, desc: S.settings.theme.systemDesc },
  { value: "light", label: S.settings.theme.light, desc: S.settings.theme.lightDesc },
  { value: "dark", label: S.settings.theme.dark, desc: S.settings.theme.darkDesc },
];

/** 选中与未选中的视觉：与隐私等级卡片同一套语言（选中加亮 + 边框）。 */
function optionClass(theme: Theme, active: boolean): string {
  const base = "flex w-full flex-col items-start gap-1 rounded-lg border px-3 py-2 text-left";
  if (!active) {
    return `${base} border-neutral-800 bg-neutral-900/40 text-neutral-400 hover:border-neutral-700`;
  }
  const tones: Record<Theme, string> = {
    system: "border-neutral-500 bg-neutral-800 text-neutral-100",
    light: "border-amber-800 bg-amber-950/40 text-amber-200",
    dark: "border-indigo-800 bg-indigo-950/40 text-indigo-200",
  };
  return `${base} ${tones[theme]}`;
}

export function ThemePicker() {
  const theme = useThemeSetting();
  const save = useSetTheme();

  const labelOf = (value: Theme) =>
    THEME_OPTIONS.find((option) => option.value === value)?.label ?? value;

  const resolved = useResolvedTheme();

  return (
    <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.settings.theme.title}</h2>
      <p className="mt-0.5 text-xs text-neutral-500">{S.settings.theme.subtitle}</p>

      <div className="mt-3 flex flex-col gap-2">
        {THEME_OPTIONS.map((option) => {
          const active = option.value === theme;
          return (
            <button
              key={option.value}
              type="button"
              aria-pressed={active}
              disabled={save.isPending}
              onClick={() => save.mutate(option.value)}
              className={`${optionClass(option.value, active)} disabled:cursor-not-allowed disabled:opacity-60`}
            >
              <span className="text-sm font-medium">{option.label}</span>
              <span className="text-xs opacity-90">{option.desc}</span>
            </button>
          );
        })}
      </div>

      <p className="mt-2 text-xs text-neutral-500">
        {theme === "system"
          ? S.settings.theme.currentFollowed(labelOf(resolved))
          : S.settings.theme.current(labelOf(theme))}
      </p>

      {save.isError ? (
        <p className="mt-1 font-mono text-xs break-all text-red-300">
          {S.settings.theme.saveFailed}
        </p>
      ) : null}
    </section>
  );
}
