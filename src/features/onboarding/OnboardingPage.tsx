/**
 * 首次启动引导（设计文档 §6.9）与「重新配置」入口。
 *
 * 三种形态共用一个组件：
 * - `full`：完整 3 步（密钥库 → OKX 凭据 → 关注标的），首次运行走这条；
 * - `unlock`：只显示第 1 步的解锁形态，解锁后直接回主界面（引导早已完成）；
 * - `reconfigure`：只显示第 2、3 步，用于从主界面回来更新凭据 / 标的。
 *
 * 闸门规则：第 1 步（解锁）必须完成才能进入第 2 步；第 2 步可跳过（只看行情）；
 * 第 3 步至少选 1 个标的。`full` / `reconfigure` 走完后调用一次
 * `onboarding_complete()`（幂等）再回调 `onDone`，任何一步未完成都不会进入主界面。
 */
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";

import { call } from "../../lib/ipc";
import { S } from "../../lib/strings";
import { errorPayloadOf } from "../live/errors";
import { StepCredential } from "./StepCredential";
import { StepVault } from "./StepVault";
import { StepWatchlist } from "./StepWatchlist";

export type OnboardingMode = "full" | "unlock" | "reconfigure";

const HEADINGS: Record<OnboardingMode, { title: string; subtitle: string }> = {
  full: { title: S.onboarding.title, subtitle: S.onboarding.subtitle },
  unlock: { title: S.onboarding.unlockTitle, subtitle: S.onboarding.unlockSubtitle },
  reconfigure: {
    title: S.onboarding.reconfigureTitle,
    subtitle: S.onboarding.reconfigureSubtitle,
  },
};

const STEP_LABELS: Record<number, string> = {
  1: S.onboarding.steps.vault,
  2: S.onboarding.steps.credential,
  3: S.onboarding.steps.watchlist,
};

/** 各形态包含的步骤号 */
function stepsOf(mode: OnboardingMode): number[] {
  if (mode === "unlock") return [1];
  if (mode === "reconfigure") return [2, 3];
  return [1, 2, 3];
}

export function OnboardingPage({
  mode,
  vaultExists,
  onDone,
}: {
  mode: OnboardingMode;
  vaultExists: boolean;
  onDone: () => void;
}) {
  const steps = stepsOf(mode);
  const [step, setStep] = useState<number>(steps[0] ?? 1);

  const complete = useMutation({
    mutationFn: () => call("onboarding_complete"),
    onSuccess: () => onDone(),
  });

  const heading = HEADINGS[mode];
  const finish = () => complete.mutate();
  const completeError = complete.isError ? errorPayloadOf(complete.error) : null;

  return (
    <main className="mx-auto flex w-full max-w-2xl flex-col gap-5 p-4 sm:p-6">
      <header>
        <h1 className="text-xl font-semibold tracking-tight sm:text-2xl">{heading.title}</h1>
        <p className="mt-0.5 text-sm text-neutral-400">{heading.subtitle}</p>
      </header>

      {mode === "unlock" ? null : (
        <ol className="flex flex-wrap gap-2 text-xs">
          {steps.map((number) => {
            const active = number === step;
            const done = number < step;
            const label = STEP_LABELS[number] ?? String(number);
            return (
              <li
                key={number}
                aria-current={active ? "step" : undefined}
                className={`rounded-full px-3 py-1 ring-1 ring-inset ${
                  active
                    ? "bg-neutral-100 text-neutral-900 ring-neutral-100"
                    : done
                      ? "bg-emerald-950 text-emerald-300 ring-emerald-900"
                      : "bg-neutral-900 text-neutral-500 ring-neutral-800"
                }`}
              >
                {`${number}. ${label}`}
              </li>
            );
          })}
        </ol>
      )}

      <section className="rounded-xl border border-neutral-800 bg-neutral-900/40 p-4 sm:p-6">
        {mode === "unlock" ? (
          <StepVault exists={vaultExists} onUnlocked={onDone} />
        ) : step === 1 ? (
          <StepVault exists={vaultExists} onUnlocked={() => setStep(2)} />
        ) : step === 2 ? (
          <StepCredential onNext={() => setStep(3)} />
        ) : (
          <StepWatchlist onDone={finish} />
        )}
      </section>

      {complete.isPending ? (
        <p className="text-xs text-neutral-400">{S.onboarding.finishing}</p>
      ) : null}

      {completeError === null ? null : (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">{completeError.message}</p>
          <p className="mt-1 text-xs text-red-300/70">
            {S.onboarding.errorCode(completeError.code)}
          </p>
          <button
            type="button"
            onClick={finish}
            className="mt-2 rounded-md bg-red-900/70 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900"
          >
            {S.account.retry}
          </button>
        </div>
      )}

      {mode === "unlock" ? null : (
        <p className="text-xs text-neutral-500">
          {S.onboarding.stepIndicator(steps.indexOf(step) + 1, steps.length)}
        </p>
      )}
    </main>
  );
}
