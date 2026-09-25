/**
 * IPC 唯一出口（设计文档 §3.2 原则 1/2、§5 的 lib/ipc.ts）。
 *
 * 前端到 Rust 的所有调用都经过这里，组件不直接 import `@tauri-apps/api`。
 * 这样命令面收敛、可审计，也保证前端永远拿不到 SQL 或密钥库的原始能力。
 *
 * 新增命令 = 在 Commands 映射表里加一行，不需要再写包装函数。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AccountSnapshot,
  AppInfo,
  BootstrapState,
  CredentialMeta,
  CredentialProbe,
  ExecutionReport,
  FetchPlan,
  LiveSnapshot,
  Progress,
  VaultStatus,
  WatchlistCandidate,
} from "./types";

/**
 * `credentials_save` 的入参（对应 Rust 的 `SaveCredentialInput`）。
 *
 * 明文密钥只会沿这个方向进入 Rust；没有任何反向命令把它读回来。
 */
export interface SaveCredentialInput {
  /** 为空 / 省略表示新建；有值表示覆盖同一条 */
  id?: string | null;
  label: string;
  api_key: string;
  secret_key: string;
  passphrase: string;
  /** true = 模拟盘（请求需带 `x-simulated-trading: 1`） */
  demo: boolean;
}

/**
 * `review_plan` 的入参（对应 Rust 的 `PlanRequest`）。
 *
 * `from` / `to` 是 Unix 毫秒。`bar` 缺省 `1H`；`inst_ids` 缺省用当前关注列表。
 * 生成计划**不联网**，只做校验与估算。
 */
export interface ReviewPlanRequest {
  from: number;
  to: number;
  bar?: string | null;
  inst_ids?: string[] | null;
}

/** 命令名 → { 入参, 出参 } 的单一事实来源 */
export interface Commands {
  app_info: { args: undefined; result: AppInfo };
  /** 刷新实盘快照；`force: true` 绕过 TTL 缓存（对应界面上的「刷新」按钮） */
  live_refresh: { args: { force?: boolean }; result: LiveSnapshot };
  /** 读取标的集，纯本地操作 */
  watchlist_get: { args: undefined; result: string[] };

  /** 密钥库状态：`exists` 决定进入「创建」还是「解锁」 */
  vault_status: { args: undefined; result: VaultStatus };
  /** 解锁；密钥库不存在时等同于创建 */
  vault_unlock: { args: { password: string }; result: VaultStatus };
  vault_lock: { args: undefined; result: VaultStatus };

  /**
   * 启动状态：一次拿到密钥库是否已创建 / 是否已解锁 / 引导是否完成 / 当前标的集。
   * 顶层据此决定进入「引导（完整 3 步）」「只解锁」还是「主界面」。
   */
  bootstrap_state: { args: undefined; result: BootstrapState };
  /** 引导完成时调用一次，写入 `onboarding_done = true`（幂等） */
  onboarding_complete: { args: undefined; result: void };

  credentials_list: { args: undefined; result: CredentialMeta[] };
  /** 保存凭据：明文进密钥库，元数据进库 */
  credentials_save: { args: { input: SaveCredentialInput }; result: CredentialMeta };
  credentials_delete: { args: { id: string }; result: void };
  /** 探测凭据权限；`read_only === false` 时界面必须红色警示 */
  credentials_test: { args: { id: string }; result: CredentialProbe };

  /** 账户概览 + 全部持仓；每次调用都会发起真实请求 */
  account_snapshot: { args: { query: { credential_id: string } }; result: AccountSnapshot };

  watchlist_set: { args: { watchlist: string[] }; result: string[] };
  /** 引导第 3 步的候选标的（按 24h 成交额降序的 Top 20） */
  watchlist_candidates: { args: undefined; result: WatchlistCandidate[] };

  /** 生成复盘采集计划。**不联网**，只做校验与估算（`RangeTooLarge` 拒绝超 90 天） */
  review_plan: { args: { request: ReviewPlanRequest }; result: FetchPlan };
  /** 执行采集计划；一直等到结束（或被取消）才返回，进度走 `fetch://progress` 事件 */
  review_fetch: { args: { plan_id: string }; result: ExecutionReport };
  /** 取消正在执行的计划；`false` 表示它已经结束了 */
  review_cancel: { args: { plan_id: string }; result: boolean };
}

/**
 * 类型化分发器：命令名决定出参类型，编译期即可发现前后端契约漂移。
 * 这是刻意的 DI 边界（便于测试时替换、也便于将来审计全部命令面）。
 */
export function call<K extends keyof Commands>(
  cmd: K,
  args?: Commands[K]["args"],
): Promise<Commands[K]["result"]> {
  return invoke<Commands[K]["result"]>(cmd, args as Record<string, unknown> | undefined);
}

/**
 * 采集进度事件名。必须与 Rust `commands/review.rs` 的 `PROGRESS_EVENT` 一致。
 */
export const FETCH_PROGRESS_EVENT = "fetch://progress";

/**
 * 订阅采集进度事件。返回取消订阅函数（组件卸载时必须调用）。
 *
 * 这是事件的唯一出口：组件不得自己 import `@tauri-apps/api/event`。
 * 调用方负责按 `Progress.plan_id` 过滤——同一个应用里可能只有一个采集在跑，
 * 但事件是全局广播，过滤放在消费侧才不会漏掉迟到的订阅。
 */
export function onFetchProgress(handler: (progress: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>(FETCH_PROGRESS_EVENT, (event) => handler(event.payload));
}
