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
  BarOption,
  BootstrapState,
  CacheStats,
  CleanupReport,
  CredentialMeta,
  CredentialProbe,
  DiagnosticExport,
  ExecutionReport,
  FetchPlan,
  IndicatorSpec,
  KlinePlan,
  LiveSnapshot,
  PrivacyLevel,
  Progress,
  PromptOutput,
  PromptTemplate,
  ProxyProbe,
  ProxySettings,
  ReviewContext,
  TemplateKind,
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

/**
 * `review_context` 的入参（对应 Rust 的 `ContextRequest`）。
 *
 * 与 `ReviewPlanRequest` 不同，它**多一个 `credential_id`**：装配会同步官方
 * 历史仓位（联网），必须知道用哪条凭据；`bar` 缺省 `1H`。
 */
export interface ReviewContextRequest {
  credential_id: string;
  from: number;
  to: number;
  bar?: string | null;
}

/**
 * `template_save` 的入参（对应 Rust 的 `SaveTemplateRequest`）。
 *
 * `id` 为空、或指向内置模板时 → 新建（「另存为」语义，后端生成新 id）；
 * 否则覆盖同一条用户模板。内置模板**永远不可覆盖**。
 */
export interface SaveTemplateRequest {
  id?: string | null;
  name: string;
  description?: string | null;
  kind: TemplateKind;
  body: string;
}

/**
 * `prompt_build_live` 的入参（对应 Rust 的 `BuildLiveRequest`）。
 *
 * `privacy` 缺省时由后端读**全局隐私设置**；`force` 绕过行情 30 秒缓存。
 */
export interface BuildLiveRequest {
  template_id?: string | null;
  privacy?: PrivacyLevel | null;
  credential_id?: string | null;
  force?: boolean | null;
}

/**
 * `prompt_build_review` 的入参（对应 Rust 的 `BuildReviewRequest`）。
 *
 * 会先同步官方历史仓位，首次可能慢几秒；时段上限 90 天（超限 `RangeTooLarge`）。
 * `privacy` 缺省时同样读全局设置。
 */
export interface BuildReviewRequest {
  template_id?: string | null;
  privacy?: PrivacyLevel | null;
  credential_id?: string | null;
  from: number;
  to: number;
  bar?: string | null;
}

/**
 * `proxy_set` 的入参（对应 Rust 的 `SetProxyInput`）。
 *
 * `url` 为 `null` 或空串 = 清除显式配置（回到系统 / 环境变量代理）。
 */
export interface SetProxyInput {
  url?: string | null;
}

/**
 * `kline_plan` 的入参（对应 Rust 的 `KlinePlanRequest`）。
 *
 * **不联网**，只做校验与估算，所以调参数时可以放心地反复调用。
 */
export interface KlinePlanRequest {
  inst_id: string;
  /** K 线粒度，至少 1 个；可用值来自 `kline_bars` */
  bars: string[];
  /** 每个周期取最近多少根 */
  candle_count: number;
}

/**
 * `kline_build` 的入参（对应 Rust 的 `KlineBuildRequest`）。
 *
 * `indicators` 缺省 = 只要原始 K 线。指标是**装配阶段**的参数：
 * 它不影响取数，所以改指标不需要重新联网拉数据。
 */
export interface KlineBuildRequest {
  plan_id: string;
  template_id?: string | null;
  indicators?: IndicatorSpec[];
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

  /**
   * 装配复盘上下文：同步官方历史仓位 → 双源合并 → 开仓时刻归因 → 统计。
   *
   * **会联网**且可能耗时数秒；时段上限同为 90 天（超限 `RangeTooLarge`，不可重试）。
   * 与采集刻意分开：改了统计口径只需重新装配，不必重新拉数据。
   */
  review_context: { args: { request: ReviewContextRequest }; result: ReviewContext };

  // ---- M5 提示词 ----
  /** 列出全部模板：内置在前，用户模板在后 */
  template_list: { args: undefined; result: PromptTemplate[] };
  /** 保存模板；`id` 为空或指向内置模板时按「另存为」新建 */
  template_save: { args: { request: SaveTemplateRequest }; result: PromptTemplate };
  /** 删除用户模板；内置模板不可删除（后端会明确报错） */
  template_delete: { args: { id: string }; result: void };
  /**
   * 校验模板正文的语法，并返回它引用了哪些变量。
   *
   * **不需要上下文、也不联网**：模板库用它给作者即时反馈。真正的渲染
   * （含「缺哪个变量」）在各页面的生成流程里——那里才有上下文。
   */
  template_check: { args: { body: string }; result: string[] };
  /** 读取全局隐私等级（三条提示词管线共用） */
  privacy_get: { args: undefined; result: PrivacyLevel };
  /** 设置全局隐私等级；改完各页面的提示词会按新等级重新生成 */
  privacy_set: { args: { level: PrivacyLevel }; result: PrivacyLevel };
  /** 生成实盘提示词；命中行情 30 秒缓存则不重复请求 */
  prompt_build_live: { args: { request: BuildLiveRequest }; result: PromptOutput };
  /** 生成复盘提示词；会先同步官方历史仓位，首次可能慢几秒 */
  prompt_build_review: { args: { request: BuildReviewRequest }; result: PromptOutput };

  // ---- M6 设置：缓存与诊断 ----
  /** 缓存占用统计：数据库总大小 + 每张表的行数与保留策略 */
  cache_stats: { args: undefined; result: CacheStats };
  /**
   * 按保留策略清理缓存并回收空间。
   *
   * **破坏性操作**：会真的删数据，界面必须先二次确认再调用。
   */
  cache_cleanup: { args: undefined; result: CleanupReport };
  /** 一键导出脱敏诊断包（同时落盘），返回包内容与落盘路径 */
  diagnostics_export: { args: undefined; result: DiagnosticExport };

  // ---- 网络代理 ----
  /** 读取当前代理；`url === null` 表示未显式配置（此时沿用系统/环境变量代理） */
  proxy_get: { args: undefined; result: ProxySettings };
  /**
   * 保存代理并**立即生效**（后端热重建 HTTP 客户端，不需要重启）。
   *
   * `url` 为 `null` 或空串 = 清除显式配置。
   */
  proxy_set: { args: { input: SetProxyInput }; result: ProxySettings };
  /**
   * 用 OKX 最轻量的公开端点验证代理是否真的通。
   *
   * 探测失败**不是命令错误**：返回 `{ok: false, error}` 而不是抛异常，
   * 界面才能把「地址填错」与「代理进程没开」分开提示。
   */
  proxy_test: { args: undefined; result: ProxyProbe };

  // ---- 行情（K 线 + 逐根指标） ----
  /** 受支持的 K 线粒度。由 Rust 白名单生成，前端不另抄一份 */
  kline_bars: { args: undefined; result: BarOption[] };
  /** 生成行情取数计划。**不联网**，只做校验与估算（含提示词长度预估） */
  kline_plan: { args: { request: KlinePlanRequest }; result: KlinePlan };
  /** 执行取数；进度走 `fetch://progress`，可取消 */
  kline_fetch: { args: { plan_id: string }; result: ExecutionReport };
  /** 取消正在执行的取数；`false` 表示它已经结束了 */
  kline_cancel: { args: { plan_id: string }; result: boolean };
  /** 装配并渲染行情提示词（从本地库读 K 线，零请求） */
  kline_build: { args: { request: KlineBuildRequest }; result: PromptOutput };
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
 * 采集进度事件名。必须与 Rust `fetch/executor.rs` 的 `PROGRESS_EVENT` 一致。
 *
 * 复盘与行情**共用这一个事件名**（两边的 `Progress` 是同一个类型），
 * 所以前端按 `plan_id` 过滤自己关心的事件即可。
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
