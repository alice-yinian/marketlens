/**
 * 顶层装配。
 *
 * 启动时查一次 `bootstrap_state`（密钥库是否存在 / 是否已解锁 / 引导是否完成 / 标的集）：
 * - `!onboarding_done`            → 引导页（完整 3 步）；
 * - `onboarding_done && !unlocked` → 引导页（只显示第 1 步的解锁形态），解锁后直接进主界面；
 * - `onboarding_done && unlocked`  → 主界面（顶部标签切换「实盘」/「账户」/「复盘」/「提示词」）。
 *
 * 引导第 1 步解锁成功后**不能**立刻回写缓存：那会让顶层误判为「已解锁」而跳过第 2、3 步。
 * 只有整段引导走完（或解锁形态完成）才更新缓存。
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { AccountPage } from "./features/account/AccountPage";
import { ErrorPanel } from "./features/account/ErrorPanel";
import { LivePage } from "./features/live/LivePage";
import { OnboardingPage } from "./features/onboarding/OnboardingPage";
import { PromptPage } from "./features/prompt/PromptPage";
import { ReviewPage } from "./features/review/ReviewPage";
import { call } from "./lib/ipc";
import { S } from "./lib/strings";
import type { BootstrapState } from "./lib/types";

export const BOOTSTRAP_KEY = ["bootstrap_state"] as const;

/**
 * 页脚：紧凑的版本信息（M0 验收路径保留为常驻诊断信息）。
 * 它同时是「IPC 通道是否可用」的最轻量探针，因此失败时只做静默降级，不打扰主流程。
 */
function VersionFooter() {
  const info = useQuery({
    queryKey: ["app_info"],
    queryFn: () => call("app_info"),
  });

  return (
    <footer className="border-t border-neutral-900 px-4 py-3 text-center text-xs text-neutral-600 sm:px-6">
      {info.isPending ? (
        <span>{S.footer.loading}</span>
      ) : info.isError ? (
        <span className="text-neutral-500">{S.footer.fail}</span>
      ) : (
        <span className="inline-flex flex-wrap justify-center gap-x-3 gap-y-1">
          <span className="text-neutral-500">{S.appName}</span>
          <span>{S.footer.version(info.data.app_version)}</span>
          <span>{S.footer.core(info.data.core_version)}</span>
          <span>{S.footer.schema(info.data.schema_version)}</span>
          <span>{S.footer.targetOs(info.data.target_os)}</span>
        </span>
      )}
    </footer>
  );
}

type Tab = "live" | "account" | "review" | "prompt";

const TABS: readonly { id: Tab; label: string }[] = [
  { id: "live", label: S.nav.live },
  { id: "account", label: S.nav.account },
  { id: "review", label: S.nav.review },
  { id: "prompt", label: S.nav.prompt },
];

function MainShell({
  tab,
  onSelectTab,
  onConfigure,
}: {
  tab: Tab;
  onSelectTab: (tab: Tab) => void;
  onConfigure: () => void;
}) {
  // 惰性挂载 + 保持挂载：页面首次访问后不再卸载。
  //
  // 之前用的是条件渲染（`tab === "live" ? <LivePage/> : ...`），切走再切回会
  // 让页面重新挂载——实测后果是「切到实盘看一眼再回来，复盘的计划与采集结果
  // 全没了」。保持挂载解决了它，同时**不破坏「按需采集」**：没访问过的标签
  // 仍然不会挂载，所以启动时不会偷偷多发请求。
  const [visited, setVisited] = useState<Record<Tab, boolean>>(() => ({
    live: tab === "live",
    account: tab === "account",
    review: tab === "review",
    prompt: tab === "prompt",
  }));

  const selectTab = (next: Tab) => {
    setVisited((prev) => ({ ...prev, [next]: true }));
    onSelectTab(next);
  };

  const panel = (id: Tab, node: React.ReactNode) =>
    visited[id] ? <div className={tab === id ? "" : "hidden"}>{node}</div> : null;

  return (
    <div className="flex min-h-full flex-col">
      <nav className="flex items-center gap-1 border-b border-neutral-900 px-4 sm:px-6">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => selectTab(item.id)}
            className={`-mb-px border-b-2 px-4 py-2.5 text-sm font-medium ${
              tab === item.id
                ? "border-neutral-100 text-neutral-100"
                : "border-transparent text-neutral-500 hover:text-neutral-300"
            }`}
          >
            {item.label}
          </button>
        ))}
        <button
          type="button"
          onClick={onConfigure}
          className="ml-auto rounded-md border border-neutral-800 px-3 py-1.5 text-xs text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
        >
          {S.nav.configure}
        </button>
      </nav>
      <div className="flex-1">
        {panel("live", <LivePage />)}
        {panel("account", <AccountPage onConfigure={onConfigure} />)}
        {panel("review", <ReviewPage onConfigure={onConfigure} />)}
        {panel("prompt", <PromptPage />)}
      </div>
    </div>
  );
}

export default function App() {
  const queryClient = useQueryClient();
  const bootstrap = useQuery({
    queryKey: BOOTSTRAP_KEY,
    queryFn: () => call("bootstrap_state"),
  });
  const [tab, setTab] = useState<Tab>("live");
  const [reconfiguring, setReconfiguring] = useState(false);

  const state = bootstrap.data;

  /** 解锁完成：只更新解锁位，不改 `onboarding_done` */
  const markUnlocked = () => {
    queryClient.setQueryData<BootstrapState>(BOOTSTRAP_KEY, (prev) =>
      prev === undefined ? prev : { ...prev, vault_unlocked: true },
    );
  };

  /** 引导完成：`onboarding_complete()` 已写入，缓存同步为已完成 + 已解锁 */
  const markOnboarded = () => {
    queryClient.setQueryData<BootstrapState>(BOOTSTRAP_KEY, (prev) =>
      prev === undefined ? prev : { ...prev, onboarding_done: true, vault_unlocked: true },
    );
  };

  const body =
    state === undefined ? (
      <main className="mx-auto w-full max-w-md p-6">
        {bootstrap.isError ? (
          <ErrorPanel
            title={S.boot.errorTitle}
            error={bootstrap.error}
            onRetry={() => void bootstrap.refetch()}
            busy={bootstrap.isFetching}
          />
        ) : (
          <p className="text-sm text-neutral-400">{S.boot.loading}</p>
        )}
      </main>
    ) : reconfiguring ? (
      <OnboardingPage
        mode="reconfigure"
        vaultExists={state.vault_exists}
        onDone={() => setReconfiguring(false)}
      />
    ) : !state.onboarding_done ? (
      <OnboardingPage mode="full" vaultExists={state.vault_exists} onDone={markOnboarded} />
    ) : !state.vault_unlocked ? (
      <OnboardingPage mode="unlock" vaultExists={state.vault_exists} onDone={markUnlocked} />
    ) : (
      <MainShell
        tab={tab}
        onSelectTab={setTab}
        onConfigure={() => setReconfiguring(true)}
      />
    );

  return (
    <div className="flex min-h-full flex-col">
      <div className="flex-1">{body}</div>
      <VersionFooter />
    </div>
  );
}
