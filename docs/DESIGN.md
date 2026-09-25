# MarketLens 设计文档

> **MarketLens**（`com.marketlens.app`）。一个把「OKX 市场状态 + 你的当前仓位 + 你的历史仓位」装配成结构化上下文、再用提示词模板渲染成 AI 提示词的多端工具。
>
> **核心形态：按需采集，无实时订阅。实盘与复盘是两个分离的工作流。**
>
> 版本：v1.0（设计定稿）｜日期：2026-09-25｜状态：**需求已确认，待实施**

---

## 1. 目标与非目标

### 1.1 目标

| # | 目标 | 可验收的形态 |
|---|---|---|
| G1 | **按需**抓取市场状态 | 用户打开实盘页时，一次性拉取一组标的的「价格 / 涨跌 / 资金费率 / 持仓量 / 多空比 / 主动买卖量 / 波动率 / 趋势状态」并展示 |
| G2 | 读取当前账户仓位 | 列出全部未平仓位（含逐仓/全仓、杠杆、均价、标记价、强平价、未实现盈亏、保证金率）与账户权益 |
| G3 | 读取历史仓位 | 合并「OKX 官方已平仓位记录」+「本地留痕」，可查询、可统计 |
| G4 | **按时段**重建历史市场状态 | 指定时间范围 → 拉取该时段的 K 线/资金费率/持仓量/多空比等 → 计算指标 → 还原「当时市场是什么样」 |
| G5 | 生成 AI 提示词 | 用可编辑模板把上述数据渲染成提示词文本；**实盘模板集与复盘模板集分离**，支持预览、复制、导出 |
| G6 | 双端可用 | Windows 桌面端 + Android 端，功能对等 |

### 1.2 非目标（本期明确不做）

- ❌ **不做实时行情**：不使用 WebSocket，不做常驻轮询，不维护长连接。所有数据在用户需要时按需拉取。
- ❌ **不做交易**：不含下单、撤单、改单、划转。API Key 只申请「读取」权限。
- ❌ **不托管**：无服务端、无账号系统，密钥与数据 100% 留本机。
- ❌ **不内置 LLM 调用**：只产出提示词文本，用户自行粘贴到任意 AI。
- ❌ **不做回测引擎**：历史统计只做描述性统计，不做策略回放。
- ❌ **不做多账户**：单用户、单 OKX 账户（可切换多个 API Key，但不做子账户聚合）。
- ❌ **不做预警通知**：无实时数据即无预警（这是「不要实时」的直接推论）。

### 1.3 两种模式

这是本版本最重要的结构变化——**实盘与复盘彻底分离**：

| | **实盘模式 (Live)** | **复盘模式 (Review)** |
|---|---|---|
| 时间视角 | 此刻 | 用户指定的过去时段 |
| 数据来源 | 当前值端点（ticker / positions / balance / funding-rate 当前值） | 历史端点（history-candles / funding-rate-history / rubik stat with begin-end） |
| 市场状态 | 当前快照 | **时段重建**：整段序列 + 关键时点状态 |
| 仓位 | 未平仓 | 已平仓（官方记录 + 本地留痕） |
| 上下文装配 | `LiveContext` | `ReviewContext`（含时段元数据、序列、逐仓开仓时点状态） |
| 模板集 | `live/*` | `review/*` |
| 典型问题 | 「我现在的持仓有什么风险？」 | 「上周那笔亏损，我入场时机错在哪？」 |
| 采集规模 | 10~30 个请求 | 30~300 个请求（需要进度条与取消） |

**共享的部分**：OKX 客户端、限流器、指标计算、SQLite、密钥库、模板引擎。**分离的部分**：采集计划、上下文装配、模板集、UI 页面。

> ✅ **已确认：单应用双模式**。安装一份、共用配置与密钥、切换成本为零。共享边界已画在 §5 的目录结构中，若将来确实要拆成两个独立应用，重构成本可控。

### 1.4 用户故事

1. 作为交易者，我打开应用进入**实盘**，看到当前仓位与此刻的市场状态，点「生成提示词」→ 得到一段文本，粘给 AI 问「我这些仓位现在有什么风险」。
2. 作为交易者，我要**复盘**上周的一笔亏损：进入复盘模式 → 选时段（上周一到周五）→ 应用拉取该时段的行情与我的平仓记录 → 渲染复盘模板，其中每笔仓位都**自动附带开仓时刻的市场状态** → 让 AI 分析我入场时机的问题。
3. 作为交易者，我不想让 AI 知道我账户的绝对金额：打开「隐私模式 L1」，金额自动百分比化。

---

## 2. 需求确认结果

| 决策项 | 结论 | 影响 |
|---|---|---|
| 技术栈 | **Tauri v2 + 前端框架** | Rust 后端承载全部业务逻辑；前端为薄视图层 |
| 功能边界 | **只读**：行情 + 仓位 + 复盘提示词 | 无交易签名、无下单链路；API Key 权限最小化 |
| 实时性 | **不需要实时行情，按需获取** | **取消 WS 与常驻轮询**；取消预警；取消后台调度器 |
| 模式划分 | **复盘与实盘分开** | 双模式、双上下文、双模板集、双采集路径 |
| 复盘数据范围 | **按时段获取所需数据** | 需要采集计划器、分页续传、进度反馈、时段重建能力 |
| 提示词生成 | **本地模板渲染，产出文本** | 后端集成模板引擎，无外部 API 依赖 |
| 密钥与数据 | **纯本地**：系统密钥库 + 本地 SQLite | 无服务端、无同步、无隐私外泄面 |
| 历史仓位数据源 | **官方接口 + 本地留痕** | 双源合并 + 去重策略（§6.4.3） |
| 应用名 / 包名 | **MarketLens** / `com.marketlens.app` | 已定稿，写入 `tauri.conf.json` 与 Android 签名配置 |
| 默认标的集 | **首次启动由用户自选**（预勾选 BTC/ETH/SOL） | 需新增首次启动引导流程（§6.9） |
| 复盘 K 线粒度 | **默认 1H**，可切换 | 720 根覆盖 30 天，请求量与信息量最均衡 |
| 复盘时段上限 | **90 天**（硬上限） | 时段选择器上限，超出由 `review_plan` 拒绝（§6.2.1） |
| 官方历史数据导入 | **本期不做，预留接口** | `imported_series` 表 + `import/` 模块边界（Phase 3） |
| 实盘快照留档 | **保留，永久** | `live_snapshot` 不设自动清理（§8） |
| 界面语言 | **仅中文，但不硬编码** | 界面文案集中 `strings.ts`；模板正文本身即集中管理（§6.6.4） |
| 模板引擎 | **minijinja（Jinja 风格）** | 嵌套遍历能力为复盘模板所必需 |
| Android 分发 | **自用 APK 为主，同时产出 AAB 备用** | 一条命令同时出两种产物；上架材料暂不准备 |
| 应用结构 | **单应用双模式** | 见 §1.3 |

> 以上为全部需求确认项。§16 风险章节中的原「开放问题」已全部关闭。

## 3. 总体架构

### 3.1 分层

```mermaid
flowchart TB
    subgraph UI["前端（WebView）· React + TS"]
        M0["模式切换：实盘 / 复盘"]
        L1["实盘：市场状态 · 当前仓位"]
        R1["复盘：时段选择 · 历史仓位 · 统计"]
        P1["提示词工作台（双模板集）"]
        S1["设置与密钥"]
    end

    subgraph IPC["Tauri IPC（invoke / event）"]
        CMD["#[tauri::command] 命令层<br/>（唯一入口，无 SQL 暴露）"]
        EVT["事件通道<br/>fetch://progress · fetch://throttled"]
    end

    subgraph CORE["Rust 核心（src-tauri）"]
        OKX["OKX 接入层<br/>REST 签名 · 限流 · 重试 · 分页"]
        PLAN["采集计划器<br/>FetchPlan 展开 · DAG 执行 · 续传"]
        CACHE["缓存层<br/>TTL · 缓存命中判定"]

        subgraph LIVE["实盘管线"]
            LM["当前市场状态<br/>快照式"]
            LP["当前仓位 / 权益"]
        end

        subgraph REV["复盘管线"]
            RM["时段市场状态<br/>序列 + 关键时点重建"]
            RP["历史仓位<br/>双源合并"]
            RS["时段统计"]
        end

        TPL["模板引擎<br/>minijinja · 双模板集"]
        CTX["上下文装配<br/>LiveContext / ReviewContext"]
        VAULT["密钥库<br/>Stronghold"]
        DB["存储层<br/>sqlx + SQLite"]
    end

    UI --> IPC --> CORE
    OKX --> PLAN --> CACHE
    CACHE --> LM & LP & RM & RP & RS
    LM & LP --> CTX
    RM & RP & RS --> CTX
    VAULT -.读取密钥.-> OKX
    CTX --> TPL --> UI
    DB --> CACHE
    PLAN -.推送进度.-> EVT
```

### 3.2 关键架构原则

1. **前端零业务逻辑**：所有 OKX 调用、指标计算、SQL 查询、模板渲染都在 Rust 侧。好处：Win/Android 行为完全一致，模板输出逐字节相同。
2. **前端零数据库权限**：**不使用 `tauri-plugin-sql`**。该插件会把任意 SQL 执行能力暴露给 WebView，而我们的数据含账户与仓位信息——前端一旦被注入等于交出整个数据库。改为 Rust 内 `sqlx` 直连，前端只能调语义化命令。
3. **按需、无状态**：没有后台任务、没有定时器、没有长连接。应用不做任何「用户没要求」的网络请求。启动时**不自动联网**，直到用户触发。
4. **数据新鲜度必须可见**：任何展示的数据都带时间戳与来源（实时拉取 / 缓存）。**绝不静默展示陈旧数据**。缓存命中时 UI 明确标注「缓存于 X 分钟前」并提供刷新按钮。
5. **只读契约**：OKX 客户端不实现任何 `POST` 私有端点——代码层面就不存在下单能力。
6. **一切可配置**：TTL、并发上限、订阅标的、指标参数都落库，不硬编码。

---

## 4. 技术栈

| 层 | 选型 | 理由 |
|---|---|---|
| 应用框架 | **Tauri v2** | 单套 Rust 核心同时产出 Windows 与 Android 产物；官方插件生态覆盖 SQLite/加密库；包体积远小于 Electron |
| 后端语言 | **Rust 2024 edition**（MSRV 1.85；Tauri v2 自身要求 ≥ 1.77.2） | 强类型、无 GC、精确的并发与限流控制。edition 2024 是本工具链的默认，且给了更严格的安全默认值 |
| 异步运行时 | `tokio` + `reqwest`（rustls） | 原生 TLS，免去 Windows/Android 上的 OpenSSL 交叉编译地狱。**不引入 `tokio-tungstenite`**（无 WS） |
| 数据库 | **SQLite**（`sqlx`，`bundled` feature） | 单文件、零运维、跨端一致；`bundled` 免系统库依赖 |
| 密钥库 | **`tauri-plugin-stronghold`**（IOTA Stronghold，argon2 派生） | 官方文档标注 Windows / Android / iOS / Linux / macOS 全平台支持。⚠️ 备选 `keyring` crate 在 Android 上**不可用**：`keyring-rs` 无 Android 后端，`android-native-keyring-store` 需 JNI 手工初始化 app context，复杂度与风险都更高 |
| 模板引擎 | **`minijinja`** | Jinja2 风格（循环/条件/过滤器/继承），表达力足以写真正的复盘模板；纯 Rust，双端一致 |
| 前端 | **React 19 + TypeScript + Vite** | 生态最大；图表库与 Tauri 无关，切换成本低 |
| 样式 | **Tailwind CSS** | 双端响应式布局（桌面宽屏 / 手机窄屏）一套断点搞定 |
| 图表 | **`lightweight-charts`**（TradingView，canvas 渲染） | 体积小、渲染快、Android WebView 上流畅；复盘时需要叠加仓位开平点标记 |
| 状态管理 | **TanStack Query** | 数据来自 IPC 命令，天然是异步资源；缓存/失效语义与「按需拉取 + TTL」模型契合 |
| 打包 | Windows：`tauri build` → NSIS / MSI；Android：`tauri android build` → APK / AAB | 见 §14 |

> 前端框架是本设计里**唯一可低成本替换**的决策：前端只是视图，全部数据来自 Tauri 命令。偏好 Vue 3 或 Svelte 5 的话，替换成本约等于重写视图层，不动核心。

---

## 5. 目录结构

```
datemode/
├─ docs/
│  └─ DESIGN.md                 # 本文档
├─ src/                         # 前端（WebView）
│  ├─ main.tsx
│  ├─ modes/
│  │  ├─ live/                  # 实盘：市场状态 / 当前仓位
│  │  └─ review/                # 复盘：时段选择 / 历史仓位 / 统计
│  ├─ features/onboarding/      # 首次启动引导（建 vault → 录 Key → 选标的）
│  ├─ features/prompt/          # 提示词工作台（双模板集）
│  ├─ features/settings/
│  ├─ lib/
│  │  ├─ ipc.ts                 # 命令的类型化封装（唯一 IPC 出口）
│  │  ├─ strings.ts             # 全部界面文案集中于此（为将来 i18n 留口）
│  │  └─ types.ts               # 由 Rust 生成的类型（见 §7.4）
│  └─ styles/
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json # 权限白名单：仅放行自定义命令
│  ├─ templates/                # 内置模板（编译期 include_str! 进二进制）
│  │  ├─ live/
│  │  │  ├─ daily_review.md.j2
│  │  │  └─ position_risk.md.j2
│  │  └─ review/
│  │     ├─ period_review.md.j2
│  │     └─ trade_postmortem.md.j2
│  ├─ migrations/               # sqlx 迁移（0001_init.sql ...）
│  └─ src/
│     ├─ lib.rs                 # Tauri builder / 插件注册 / 命令注册
│     ├─ error.rs
│     ├─ commands/              # #[tauri::command] 薄封装（参数校验 + 调服务）
│     ├─ okx/
│     │  ├─ client.rs           # REST 客户端（签名/限流/重试）
│     │  ├─ sign.rs             # HMAC-SHA256 + Base64 签名
│     │  ├─ paging.rs           # 游标分页（after/before）通用迭代器
│     │  ├─ models.rs           # 响应反序列化结构体
│     │  ├─ endpoints.rs        # 端点元数据（鉴权/限流/分页方式/历史窗口）
│     │  └─ ratelimit.rs        # 令牌桶
│     ├─ fetch/
│     │  ├─ plan.rs             # FetchPlan 展开
│     │  ├─ executor.rs         # DAG 执行 + 并发控制 + 取消
│     │  └─ cache.rs            # TTL 缓存判定
│     ├─ market/
│     │  ├─ live.rs             # 当前市场状态快照
│     │  ├─ historical.rs       # 时段序列拉取
│     │  ├─ indicators.rs       # EMA/RSI/ATR/已实现波动率/基差
│     │  ├─ regime.rs           # 状态分类
│     │  └─ reconstruct.rs      # 关键时点状态重建（用于复盘逐仓归因）
│     ├─ position/
│     │  ├─ current.rs          # 当前仓位
│     │  ├─ history.rs          # 官方历史仓位同步
│     │  ├─ trace.rs            # 本地留痕（顺带记录）
│     │  └─ merge.rs            # 双源合并/去重/统计
│     ├─ prompt/
│     │  ├─ live_ctx.rs         # LiveContext 装配
│     │  ├─ review_ctx.rs       # ReviewContext 装配
│     │  ├─ privacy.rs          # 隐私分级
│     │  ├─ render.rs           # minijinja 渲染 + 自定义过滤器
│     │  └─ tokens.rs           # token 估算
│     ├─ storage/
│     │  ├─ db.rs               # 连接池、迁移
│     │  └─ repo/
│     └─ vault.rs               # Stronghold 封装
└─ android/                     # tauri android init 生成
```

> 注意：**没有 `scheduler.rs`、没有 `ws.rs`**。这是「不要实时」的直接体现。

---

## 6. 核心模块设计

### 6.1 OKX 接入层

#### 6.1.1 鉴权（已验证的官方规则）

私有端点需要 4 个头：

| Header | 值 |
|---|---|
| `OK-ACCESS-KEY` | API Key |
| `OK-ACCESS-SIGN` | `base64(HMAC-SHA256(secretKey, prehash))` |
| `OK-ACCESS-TIMESTAMP` | Unix 秒（或 ISO-8601 UTC，如 `2026-09-25T00:00:00.000Z`） |
| `OK-ACCESS-PASSPHRASE` | 创建 Key 时设置的 passphrase |

```
prehash = timestamp + method.to_uppercase() + requestPath + body
requestPath 包含 query string，但不含域名
```

**必须处理的坑**：

- 时间戳不同步会报 `50102`（timestamp expired）。每次私有请求前若本地时钟偏移 > 30s，UI 顶部警示。
- 时间戳格式必须与签名时使用的字符串**逐字节一致**——统一用 ISO-8601 毫秒格式，避免「秒 vs 毫秒」混用。
- 模拟盘：请求头 `x-simulated-trading: 1`，做成凭据属性 `env: live | demo`，方便无风险联调。

#### 6.1.2 端点清单

**公开端点**

| 用途 | 端点 | 历史窗口 | 模式 |
|---|---|---|---|
| 全市场行情 | `GET /api/v5/market/tickers?instType=SWAP` | 仅当前 | Live |
| 单标的行情 | `GET /api/v5/market/ticker` | 仅当前 | Live |
| K 线（近期） | `GET /api/v5/market/candles` | 近期，`limit ≤ 300` | Live |
| **K 线（历史）** | `GET /api/v5/market/history-candles` | 完整，`limit ≤ 100`，游标 `after`/`before` | Review |
| **标记价 K 线（历史）** | `GET /api/v5/market/history-mark-price-candles` | 完整 | Review |
| **指数 K 线（历史）** | `GET /api/v5/market/history-index-candles` | 完整 | Review |
| 标记价（当前） | `GET /api/v5/public/mark-price?instType=SWAP` | 仅当前 | Live |
| 资金费率（当前） | `GET /api/v5/public/funding-rate` | 仅当前（含预测值） | Live |
| **资金费率（历史）** | `GET /api/v5/public/funding-rate-history` | **近 3 个月**，`limit ≤ 100`，游标 `after`/`before`（基于 `fundingTime`，新→旧排序） | Review |
| 持仓量（当前） | `GET /api/v5/public/open-interest?instType=SWAP` | 仅当前 | Live |
| 持仓量 5s 序列 | `GET /api/v5/market/open-interest` | **仅近期** | Live |
| **多空账户比（历史）** | `GET /api/v5/rubik/stat/contracts/long-short-account-ratio?ccy=BTC` | **2023-06-09 起**，`begin`/`end`（ms，闭区间），默认 1m 粒度 | Review |
| **持仓量/成交额（历史）** | `GET /api/v5/rubik/stat/contracts/open-interest-volume?ccy=BTC` | **2023-06-09 起**，同上 | Review |
| **主动买卖量（历史）** | `GET /api/v5/rubik/stat/taker-volume?ccy=BTC&instType=CONTRACTS` | **2023-06-09 起**，同上 | Review / Live |
| 合约信息 | `GET /api/v5/public/instruments?instType=SWAP` | 仅当前（**必须拿 `ctVal`**） | 两者 |
| 深度 | `GET /api/v5/market/books` | **无历史** | Live |

**私有端点（只读）**

| 用途 | 端点 | 窗口 |
|---|---|---|
| 账户配置 | `GET /api/v5/account/config` | 当前（含 `posMode`） |
| 余额/权益 | `GET /api/v5/account/balance` | 当前 |
| **当前仓位** | `GET /api/v5/account/positions` | 当前 |
| **历史仓位** | `GET /api/v5/account/positions-history` | 官方保留窗口，`limit ≤ 100`，游标 `after`/`before` |
| 成交明细（近 3 天） | `GET /api/v5/trade/fills` | 3 天 |
| 成交明细（近 3 月） | `GET /api/v5/trade/fills-history` | 3 个月 |
| 账单流水 | `GET /api/v5/account/bills-archive` | 归档 |

> `posMode` 必须读取：净持仓模式下 `posSide` 恒为 `net`，长空模式才有 `long`/`short`。历史展示与统计都要按它分支。

#### 6.1.2.1 真机核对结论（2026-09-25）

以下事实来自对 `www.okx.com` 的**实际调用**，不是文档摘抄——初版设计里有三处摘抄错误，在此修正：

| 项 | 初版（错误） | 实测（正确） |
|---|---|---|
| Rubik 主动买卖量的 `instType` | `SWAP` | **`CONTRACTS`**（合法值只有 `SPOT` / `CONTRACTS`；传 `SWAP` 返回 `51000 Parameter instType error`） |
| 基差数据源 | 以为 `market/ticker` 带 `idxPx` | **ticker 不含 `idxPx`**；基差 = `(markPx − idxPx) / idxPx`，需 `public/mark-price` + `market/index-tickers` 两个端点 |
| `ctVal` 示例值 | BTC-USDT-SWAP = 0.001 BTC | **0.01 BTC**（ETH-USDT-SWAP = 0.1 ETH） |
| 成交量的单位 | 以为 `volCcy24h` 是 USDT 计价 | **`vol24h` 是合约张数，`volCcy24h` 是基础币（如 BTC）**。USD 成交额 = `volCcy24h × last`（实测 90477.2 × 83789.7 ≈ 75.8 亿） |

其它实测确认（影响反序列化实现）：

- **所有数值都是 JSON 字符串**（`"83789.7"`），不是 number；必须逐字段解析。
- **空串是缺失值的常见表示**：`public/funding-rate` 的 `nextFundingRate` 实测就是 `""`。
- **Rubik 端点返回「字符串数组的数组」**（`[["1790322000000","1.24"], ...]`），不是对象数组；需单独解析，不能套用通用 struct。
- `market/candles` 返回 9 元素数组：`[ts, open, high, low, close, vol, volCcy, volQuote, confirm]`。
- `public/open-interest` 提供 **`oi` / `oiCcy` / `oiUsd`** 三个已命名字段，名义价值直接取 `oiUsd`，无需自行换算。
- `www.okx.com` 可达；**`aws.okx.com` 在本开发机不可达**，base URL 固定用 `www.okx.com`。

**私有端点的实测发现（2026-09-25，模拟盘）**：

| 项 | 实测结果 | 影响 |
|---|---|---|
| `posMode` | **`net_mode`** | `posSide` 恒为 `net`——展示与统计都必须按它分支，不能假设一定有 `long`/`short` |
| `account/positions` 的 `notionalUsd` | **OKX 直接提供** | 名义价值直接取它，比自行用 `ctVal` 换算更准；`ctVal` 只用于算「币数量」 |
| `liqPx` | 全仓模式下为 **空串** | 又一次空串陷阱，必须映射为 `None` |
| `perm` | 实测 `read_only` | 判定只读**不能写成等值判断**（`== "read_only"`），否则对方新增权限项时会静默失效 |
| `account/config` 的 `autoLoan` | 是 **JSON 布尔**，不是字符串 | OKX 在同一响应里混用类型，模型只声明需要的字段即可（serde 忽略未知字段） |
| 模拟盘密钥 | 实盘端点返回 `50101 APIKey does not match current environment` | 环境不匹配的错误码明确，可据此判断密钥属于哪个环境 |

#### 6.1.3 指标历史可得性矩阵 ⭐

**这是复盘模式的地基。** 不是所有指标都能还原到过去——必须对用户诚实：

| 指标 | 历史可得性 | 数据源 | 备注 |
|---|---|---|---|
| 价格 OHLCV | ✅ 完整 | `history-candles` | |
| 标记价 | ✅ 完整 | `history-mark-price-candles` | |
| 指数价 | ✅ 完整 | `history-index-candles` | |
| **基差** `(mark-index)/index` | ✅ 可重建 | 上面两者组合 | 复盘判断「当时多头溢价多少」的核心 |
| 资金费率 | ⚠️ **仅近 90 天**（实测 85 天前有、95 天前无） | `funding-rate-history` | 与官方「3 个月」一致；`limit ≤ 100`，游标 `after`/`before` |
| 多空账户比 | ❌ **仅近约 48 小时** | `rubik/stat/...` | **见下方更正**；单次上限 576 点（5 分钟粒度 → 2 天） |
| 持仓量 / 成交额 | ❌ **仅近约 48 小时** | `rubik/stat/...` | 同上 |
| 主动买卖量 | ❌ **仅近约 48 小时** | `rubik/stat/...` | 同上；单次上限仅 **72 点**（6 小时） |
| 持仓量 5s 序列 | ❌ 无历史 | – | 只有近期 |
| 订单簿深度 | ❌ 无历史（API） | – | 官方有 L2 历史文件下载（2023-03 起），体量巨大 |
| 成交量/波动率 | ✅ 可从 K 线算 | `history-candles` | 本地计算，不额外请求 |

**超出 API 窗口的路径（Phase 3 可选）**：OKX 提供历史数据下载门户，含资金费率（2022-03 起）、L2 订单簿（2023-03 起）、借币利率（2021-12 起）等。设计上预留「导入本地数据文件」的入口（`import/` 模块 + `imported_series` 表），本期不实现，但表结构与 `metric_series` 兼容，将来接入不需要重构。

> ⚠️ **重要更正（2026-09-25 实测）**：初版矩阵把 Rubik 系列的「数据起点 2023-06-09」当成了「API 可回溯范围」，
> 这两件事被搜索结果混为一谈。逐段实测（每段 1 天区间、从当前往回推）的结果是：
>
> | 区间起点 | 返回点数 |
> |---|---|
> | 当前往前的 1 天 | 288 ✅ |
> | 1 天前的 1 天 | 288 ✅ |
> | 2 天前的 1 天 | **0** ❌ |
> | 3 / 5 / 10 / 20 天前的 1 天 | **0** ❌ |
>
> 即 **Rubik 全家桶只服务最近约 48 小时**。而且 `end` 分页不工作（只返回边界那 1 个点），
> 无法靠分页把窗口拉长。粒度实测是 **5 分钟**（24 小时 = 288 点），不是初版写的 1 分钟。
>
> **对复盘的实际影响**：任何超过约 2 天的复盘，多空比 / 持仓量 / 主动买卖量**都不可得**，
> 必须走「数据不可得」路径。复盘的历史主干因此是 **K 线（完整）+ 资金费率（90 天）**；
> 拥挤度判定仍可用（它只依赖资金费率），但 `long_short_ratio` 与持仓量在历史窗口里会是 `None`。
> 这正是 `na` 过滤器与「数据不可得」机制存在的意义——它们不是锦上添花，而是这个产品的必要部分。

**UI 上的体现**：复盘时段选择器会**动态提示可得性**。用户选了 2024 年 1 月的区间，若资金费率已超出 API 保留窗口，界面直接标注「资金费率：该时段数据不可得（超出 OKX 保留窗口）」，并且**复盘上下文里对应字段为 `null`，模板渲染时明确显示「数据不可得」而不是留空**——AI 看到空白会自行编造，看到「不可得」才会谨慎。

#### 6.1.4 限流

用**令牌桶 + 按端点分组**，因为 OKX 的限流规则既有按 IP 的也有按 User ID 的。

| 分组 | 作用域 | 初始值 | 来源 |
|---|---|---|---|
| `public.candles` | IP | 20 req / 2s | **保守初值**。官方对 `market/candles` 标注 40 req/2s per IP，但各行情端点数值不一 |
| `public.funding_history` | IP + Instrument | 10 req / 2s | 已确认 |
| `public.rubik` | IP | 5 req / 2s | 已确认 |
| `public.instruments` | IP | 20 req / 2s | 保守初值，低频且有缓存兜底 |
| `private.account` | User ID | 8 req / 2s | **保守初值**（低于公开可查的 10 req/2s 上限） |

> ⚠️ **实现时必须逐端点核对官方文档的 Rate Limit 小节**。搜索到的 `history-candles` 数值互相矛盾（20/2s vs 40/2s），本文档不采信任何未从官方页面直接确认的数字。做法：`endpoints.rs` 里每个端点携带 `rate_limit: RateLimit { scope, quota, window }` 元数据，初值取保守值，核对文档后改常量即可，逻辑不动。

**退避**：`429` 或 `5xx` → 指数退避 + 抖动（`min(2^n × 200ms, 30s) × (0.5~1.0)`），最多 4 次；连续 3 次 429 → 该分组冷却 60s 并推送 `fetch://throttled` 事件。

**因为按需采集，突发流量集中在复盘拉取**——这是限流器最重要的场景（见 §6.2.2）。

---

### 6.2 采集计划器（FetchPlan）

复盘要拉几百个请求，这是新增的核心模块。

#### 6.2.1 计划展开

```rust
pub struct FetchPlan {
    pub id: String,
    pub mode: Mode,                    // Live | Review
    pub from: Option<i64>,             // Review 才有时段
    pub to: Option<i64>,
    pub tasks: Vec<Series>,            // 原子「序列」列表
    pub est_requests: usize,
    pub est_duration_ms: u64,
    pub warnings: Vec<AvailabilityWarning>,  // 时段超出可得窗口的提示
}

/// 一条序列 = 一串分页请求（顺序依赖）
pub struct Series {
    pub endpoint: EndpointId,
    pub params: ParamMap,
    pub inst_id: Option<String>,
    pub pages: PagePlan,               // Cursor{after,before} | Range{begin,end} | Single
    pub priority: u8,                  // 1=关键 2=重要 3=锦上添花
}

/// 执行时展开为 DAG：序列内顺序，序列间并发
pub struct SeriesResult {
    pub series: Series,
    pub points: Vec<SeriesPoint>,
    pub status: SeriesStatus,          // Complete | Partial{cursor} | Failed{err} | Unavailable{reason}
}
```

**展开示例**（具体算例，用于验证设计）：

> 复盘 `BTC-USDT-SWAP` / `ETH-USDT-SWAP` 近 30 天，1 小时 K 线：
> - K 线：720 根 / 100（`history-candles` 上限）= **8 页/标的** → 16 请求
> - 标记价 + 指数价：各 8 页/标的 → 32 请求
> - 资金费率：30 天 × 3 次/天 = 90 条 → 1 页/标的 → 2 请求
> - Rubik 三项统计：1 请求/标的/项 → 6 请求
> - 仓位与成交：约 5~15 请求
> - **合计约 61~71 请求**，在 20 req/2s 的桶下约 **7 秒**完成
>
> 这个量级完全不需要实时订阅——用户点一次、等几秒、拿到完整上下文，比常驻长连接更符合「复盘」这个使用场景。

**时段上限校验**：`review_plan` 强制校验 `to - from ≤ 90 天`（已确认的硬上限），超出直接返回 `AppError::RangeTooLarge` 并在 UI 提示。同时按 bar 校验页数：若估算请求数 > 800，返回警告并建议改用更大 bar（如 1H → 4H），由用户确认后继续——**绝不静默截断时段**（静默截断会让用户以为看全了，是最危险的行为）。

#### 6.2.2 执行器

- **并发模型**：每条 `Series` 一个 `tokio` 任务；`Series` 之间并发，`Series` 内部因游标依赖而顺序执行。全局 `Semaphore`（默认 4）限制在途请求数。
- **优先级调度**：`priority=1`（K 线、仓位）先跑，让用户尽快看到主体内容；`priority=3` 失败不影响整体成功。
- **进度反馈**：每完成一页推送 `fetch://progress {plan_id, done, total, current_series}`，前端显示进度条。
- **取消**：`fetch_cancel(plan_id)` → 通过 `CancellationToken` 中止；已落库的部分数据保留（下次复用）。
- **断点续传**：序列的部分结果连同游标写入 `fetch_state`；同一时段的重复请求直接复用已完成的序列。
- **部分失败**：任一序列失败不使整个计划失败——`SeriesResult.status = Failed`，上下文里该字段标注不可得，并在 `warnings[]` 汇总。**复盘不能因为一个统计接口挂了就整体不可用。**

#### 6.2.3 缓存策略

| 数据 | TTL | 理由 |
|---|---|---|
| 已收盘 K 线（`confirm=1`） | **永久** | 数据已定型，永不再变 |
| 未收盘 K 线（`confirm=0`） | 不缓存 | 会变 |
| 历史资金费率（已结算记录） | 永久 | 已定型 |
| Rubik 统计（过去的时间点） | 永久 | 已定型 |
| 当前仓位 / 权益 | **30s** | 防止连点刷新打爆限流 |
| 当前市场状态（ticker / 当前资金费率） | **30s** | 同上 |
| 历史仓位（`positions-history`） | 6h + 手动强制刷新 | 平仓记录可能补录 |
| 合约信息（`ctVal`） | 24h | 极少变 |

- 所有 `*_fetch` 命令都接受 `force: bool` 绕过 TTL（对应 UI 的「强制刷新」按钮）。
- 缓存命中时，返回结构带 `cache_hit: true` 与 `fetched_at`，**UI 必须展示**。
- 「已定型数据永久缓存」是本设计的红利：**用户复盘过一次的时段，第二次几乎是瞬时的**，且离线可用。

---

### 6.3 市场状态引擎

#### 6.3.1 单标的指标

| 指标 | 定义 |
|---|---|
| `changePct` | 区间（Live=24h，Review=用户时段）涨跌幅 |
| `ema20 / ema60 / ema200` | 收盘价 EMA（`α = 2/(n+1)`） |
| `rsi14` | Wilder 平滑 |
| `atr14Pct` | `ATR14 / close` |
| `realizedVol` | 对数收益率样本标准差 × `√(年化周期数)` |
| `fundingCurrent` / `fundingAvg` | 当前（或时段末）与时段均值；`fundingAnnualized = funding × 3 × 365`（8h 结算 → 日 3 次） |
| `oiChangePct` | 持仓量区间变化率 |
| `basisPct` | `(markPx - indexPx) / indexPx`（**历史可重建**，见 §6.1.3） |
| `longShortRatio` | Rubik 多空账户比 |
| `takerBuySellRatio` | 主动买量 / 主动卖量 |
| `volumeVsAvg` | 区间成交额 / 前 20 期均值 |

#### 6.3.2 状态分类

```rust
pub enum TrendRegime { Uptrend, Downtrend, Range, Transition }
pub enum VolRegime   { Low, Normal, High, Extreme }
pub enum Crowding    { LongCrowded, ShortCrowded, Balanced }
```

判定规则（参数可配置，落 `settings` 表）：

- **趋势**：`ema20 > ema60 > ema200` 且 `close > ema20` → `Uptrend`；反向 → `Downtrend`；`|ema20-ema60|/close < 0.5%` 且价格在 `ema200` 上下 1.5% 内 → `Range`；其余 → `Transition`。
- **波动**：以 `realizedVol` 的历史分位划分——`<25%` Low，`25%~75%` Normal，`75%~95%` High，`>95%` Extreme。

> **M1 的实现偏差（已确认可接受）**：设计目标是「近 90 天分位」，但 90 天的 1H 窗口序列需要约 2160 根 K 线，得靠 M3 的采集计划器分页拉取。M1 的做法是**用现有 300 根 1H K 线做滚动 24 小时窗口**，得到约 276 个样本（回看约 12 天）。
>
> 关键取舍：**当前值与历史样本使用完全相同的窗口长度与周期数**——同一把尺子量出来的分位才有意义。因此宁可回看窗口短，也不混用「日线算历史、小时线算当前」这种口径不一致的做法。M3 只需把样本来源换成更长的序列，判定逻辑不动。
- **拥挤**：`fundingAnnualized > 30%` 或 `longShortRatio > 2.0` → `LongCrowded`；对称反向 → `ShortCrowded`；否则 `Balanced`。

同时输出**规则触发说明**（`signals: Vec<String>`，如 `"资金费率年化 +42%，多头拥挤度偏高"`）。AI 需要的是「为什么」，不只是「是什么」。

#### 6.3.3 实盘 vs 复盘的状态表示

| | 实盘 | 复盘 |
|---|---|---|
| 表示 | **单点快照** `MarketState` | **序列 + 关键时点** |
| 内容 | 此刻全部指标 + 分类 + signals | 时段内每根 K 线的指标序列；时段起点/终点/极值点状态 |
| 用途 | 「现在什么情况」 | 「这段时间发生了什么」+ 逐仓归因 |

#### 6.3.4 关键时点重建 ⭐

复盘最有价值的能力：**把每笔历史仓位与它开仓时刻的市场状态绑定**。

```
对每笔已平仓位 p：
  窗口 = [p.open_time - 24h, p.open_time]
  从本地缓存/API 取该窗口的 K 线 + 可得指标
  计算该窗口末的指标与 regime → p.regime_snapshot
```

结果：可以回答「**我在震荡市做趋势单的胜率是多少**」「我在资金费率极端时开多单的盈亏比是多少」——这是「市场状态」与「历史仓位」真正咬合的地方，也是这个工具区别于普通交易日志的核心价值。

若某指标在该时段不可得（如超出 Rubik 窗口），`regime_snapshot` 对应字段为 `null`，统计时该维度标注「样本不足」。

---

### 6.4 仓位服务

#### 6.4.1 当前仓位

`GET /api/v5/account/positions` → 标准化：

```rust
pub struct Position {
    pub inst_id: String,
    pub pos_side: String,        // long | short | net
    pub mgn_mode: String,        // cross | isolated
    pub lever: f64,
    pub contracts: f64,
    pub size_base: f64,          // contracts × ctVal（用 instruments 表换算）
    pub avg_px: f64,
    pub mark_px: f64,
    pub liq_px: Option<f64>,
    pub upl: f64,
    pub upl_ratio: f64,
    pub mgn_ratio: Option<f64>,
    pub notional_usd: f64,
    pub created_at: i64,
    pub updated_at: i64,
}
```

**关键细节**：张数 → 币数量必须用 `instruments.ct_val`，且 `ctValCcy` 可能不是计价币（币本位合约）。USD 名义价值按 `instType` 分支计算（U 本位用 `markPx`，币本位用 `markPx × 指数价`）。**换算错误会直接给 AI 喂错数据**，必须有单元测试覆盖两种合约。

#### 6.4.2 本地留痕（原「增量快照」的调整）

**变化说明**：原设计是「后台定时快照」，现在没有后台任务了。改为**顺带留痕**：

> 每次用户在实盘模式拉取当前仓位时（这本来就要发一次请求），**顺手把结果写一条 `position_trace`**。零额外网络请求、零额外 API 配额。

| 特性 | 说明 |
|---|---|
| 触发时机 | 用户打开实盘页 / 手动刷新时 |
| 成本 | **零**（复用已拉取的响应） |
| 覆盖度 | 只覆盖「用户打开过应用」的时刻 |
| 间隙 | 明确记录并在统计时标注——`position_trace` 有 `gap_before` 标记 |
| 价值 | 提供官方接口拿不到的信息：持仓期间的**最大浮盈/最大浮亏**（逐次留痕取极值） |

**对用户诚实**：复盘统计页展示「本地留痕覆盖度：本时段内共 12 次记录，存在 3 段间隙」，让用户自己判断样本可信度。

#### 6.4.3 历史仓位：双源合并

**源 A — 官方接口** `positions-history`：已平仓位，含开仓均价、平仓均价、已实现盈亏、盈亏率、手续费、资金费、开平仓时间。精确，但窗口有限。

**源 B — 本地留痕**：通过比对相邻两次留痕推导生命周期：

| 变化 | 判定 |
|---|---|
| `posId` 首次出现 | 仓位开立 |
| 存在且 `sz` 增大 | 加仓 |
| 存在且 `sz` 减小但 > 0 | 减仓 |
| `posId` 消失 | 平仓 |

**合并算法**（`position/merge.rs`）：

```
1. 以 posId 为键（OKX 的 posId 全局唯一）建索引。
2. 源 A 记录 → 写入 position_history，source='okx'。
3. 源 B 推导出、posId 不在源 A 中的 → source='local'。
4. posId 两源都有 → 保留源 A（更精确），本地记录只补充源 A 缺失的字段
   （如持仓期间最大浮盈/浮亏）。
5. 无 posId 的历史（净持仓模式部分场景）→ 退化为
   (instId, posSide, openTime ± 间隙) 模糊匹配；命中则合并，否则并列展示并标注「时间不精确」。
6. 每次同步写 fetch_state 游标，保证可续传。
```

**UI 必须透明**：每条历史仓位带来源角标 `官方` / `本地` / `官方+本地`，以及时间精度 `精确` / `近似`。数据可信度是复盘结论的地基，不能糊。

#### 6.4.4 统计

| 指标 | 定义 |
|---|---|
| 胜率 | `pnl > 0` 笔数 / 总笔数 |
| 盈亏比 | `avg(盈利笔 pnl) / abs(avg(亏损笔 pnl))` |
| 期望值 | `Σpnl / 笔数` |
| 平均持仓时长 | `avg(closeTime - openTime)` |
| 费用侵蚀 | `Σ(fee + fundingFee) / Σ|pnl|` |
| 聚合维度 | 标的 / 方向 / 月份 / 杠杆区间 / **开仓时的市场状态**（最有价值） |

---

### 6.5 密钥库

```rust
pub trait Vault {
    fn unlock(&self, password: &str) -> Result<()>;
    fn lock(&self) -> Result<()>;
    fn is_unlocked(&self) -> bool;
    fn put(&self, key: &str, value: &str) -> Result<()>;
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn delete(&self, key: &str) -> Result<()>;
}
```

- 后端：`tauri-plugin-stronghold` 的 **Rust API**（`Stronghold::new` + `kdf::KeyDerivation::argon2`），
  vault 文件位于 `app_local_data_dir()/vault.hold`，argon2 的 salt 位于同目录 `vault-salt.txt`。
- **比设计文档原本的要求更严**：原措辞是「插件的 JS 命令不在 `capabilities` 里放行」，
  而实现上**根本没有注册这个插件**——它的 JS 命令不存在，WebView 连「尝试读写密钥」
  这个能力都没有。`capabilities/default.json` 仍然只有 `core:default`。
- **锁定即丢弃**：`lock()` 直接 drop 已解锁的 Stronghold，密钥材料不留在内存里等着被读取。
- 存储布局：`cred:{id}` → JSON `{api_key, secret_key, passphrase, demo}`。
  SQLite 的 `api_credentials` 表**只存掩码与探测结果**，永不存明文。
- 凭据类型 `Credentials` 刻意**不实现 `Clone`**，并手写 `Debug` 遮蔽全部密文字段——
  一旦能被 `{:?}` 打印或随手复制，就迟早会出现在日志、错误信息或崩溃转储里（有测试守住）。
- 每次变更后**显式 `save()`**：Stronghold 的快照是显式提交的，忘了它会让
  「看起来保存成功了」的凭据在重启后凭空消失（有真实文件系统的往返测试守住）。
- **client 的三步解析顺序不可调换**：`get_client` → `load_client` → `create_client`。
  `get_client` 只查**内存中的会话 client**（源码原话：*in session client, not being
  persisted in a Snapshot*），`load_client` 才从**快照**加载。少了中间一步，重新解锁时
  第一步必然失败，于是 `create_client` 会**新建一个空 client 顶掉快照里的那个**——
  表现为「凭据保存成功，重启后凭空消失」。这个 bug 由真实文件系统的往返测试抓到，
  纯内存实现根本测不出来（这也是为什么测试刻意用真实临时目录而不是 mock）。
- ⚠️ 构建依赖：Stronghold 经由 `libsodium-sys-stable` 引入 libsodium。它在
  **Linux/macOS** 上从源码编译（需要可用的 C 工具链），但在 **Windows** 上
  源码编译走不通（`configure`/`make` 不可用），`build.rs` 必然回退到下载预编译包
  `libsodium-1.0.22-stable-msvc.zip`（约 26 MB）。

  ⚠️ **这个回退下载在 `build.rs` 里没有重试**：一次 DNS 抖动就让整条 job 变红。
  首次 CI 就撞上了这个——同一提交的两次运行，windows job 一次成功、一次
  死于 `Os { code: 11002 }`（主机名解析失败）。因此 windows job 里加了
  「预取 libsodium 归档」步骤（`curl` 显式重试 5 次）并用 `SODIUM_DIST_DIR`
  指向本地目录，使构建**不再依赖构建期网络**。
  `LATEST.tar.gz` 与预编译 zip 两组文件都要预取——前者是它校验的第一步。

> **未实现**：设计文档原计划的「15 分钟无操作自动锁定」。它需要前端定时器与活动检测，
> 属于后续补充；当前锁定是显式动作（界面提供锁定按钮）。此处如实标注，不假装已有。

---

### 6.6 模板引擎

#### 6.6.1 选型：minijinja

1. 渲染结果与平台无关——同一份模板在 Win 和 Android 上产出**逐字节相同**的文本。若用前端 JS 渲染，两个 WebView 版本的差异会成为不确定性来源。
2. Jinja 语法表达力足够：`{% for %}`、`{% if %}`、`{% macro %}`、`|filter`、模板继承。复盘模板必然需要循环遍历仓位与指标序列。
3. 支持自定义 `Loader` → 用户模板存 SQLite，与内置模板统一加载，支持 `{% include %}`。

#### 6.6.2 自定义过滤器

| 过滤器 | 作用 | 示例 |
|---|---|---|
| `usd` | 金额格式化，自动 K/M 缩写 | `{{ 1234567 \| usd }}` → `$1.23M` |
| `pct` | 百分比，带符号 | `{{ -0.0345 \| pct(2) }}` → `-3.45%` |
| `num` | 数字精度控制 | `{{ 64231.5 \| num(0) }}` → `64232` |
| `ts` | 时间戳 → 本地时区可读时间 | `{{ open_time \| ts }}` → `2026-09-20 14:32` |
| `ago` | 时间戳 → 相对时间 | `{{ open_time \| ago }}` → `5 天前` |
| `dur` | 毫秒时长 → 人类可读 | `{{ 86400000 \| dur }}` → `1 天` |
| `side` | 方向中文化 | `long` → `多` |
| `na` | **不可得数据的统一渲染** | `{{ s.funding_avg \| na("超出 OKX 保留窗口") }}` → `[数据不可得：超出 OKX 保留窗口]` |
| `table` | 数组 → Markdown 表格 | 见下 |
| `json` | 安全序列化为 JSON 代码块 | |

`na` 过滤器是为复盘场景专门加的：**历史数据不可得时，绝不能渲染成空白**——AI 看到空白会自行编造，看到「不可得」才会谨慎。

`table` 过滤器解决手写 Markdown 表格对齐的痛苦：

```jinja
{{ positions | table(cols=["标的","方向","杠杆","名义(USD)","未实现盈亏","保证金率"]) }}
```

#### 6.6.3 变量契约

**实盘 `LiveContext`**：

```
meta       → { generated_at, app_version, template_name, privacy_level, token_estimate, mode: "live" }
market     → { global: {...}, instruments: [MarketState], signals: [String] }
account    → { total_eq_usd, iso_eq_usd, avail_bal_usd, upl, mgn_ratio, pos_mode, currencies: [...] }
positions  → [Position]
trace      → { coverage: {...}, gaps: [...] }        # 本地留痕覆盖度
```

**复盘 `ReviewContext`**：

```
meta       → { ..., mode: "review", range: {from, to, label, days}, availability: [...] }
market     → { instruments: [{
                 inst_id,
                 series: { candles: [...], funding: [...], oi: [...], lsr: [...], basis: [...] },
                 summary: { change_pct, high, low, realized_vol, funding_avg, funding_max, oi_change_pct, ... },
                 regime_at_start, regime_at_end, regime_extremes: [...],
                 unavailable: [ {metric, reason} ]        # 明确列出拿不到的指标
               }] }
history    → [ClosedPosition]                        # 带 source 角标 + regime_snapshot
stats      → { win_rate, profit_factor, expectancy, avg_hold, fee_drag, by_dimension: {...} }
coverage   → { trace_records, gaps, data_quality_notes }   # 数据可信度声明
```

**契约的强制手段**：上下文是 Rust 结构体，`serde` 序列化后注入模板；minijinja 配 `UndefinedBehavior::Strict` → 模板引用不存在的变量**直接报错**，而不是静默渲染成空。否则用户拿到一段缺数据的提示词还浑然不觉，这比报错危险得多。

#### 6.6.4 内置模板（双模板集）

**`live/daily_review.md.j2`（实盘·日常）**

```jinja
# 交易复盘请求 · {{ meta.generated_at | ts }}

你是一位专业的加密货币交易分析师。以下是截至 {{ meta.generated_at | ts }} 的账户状态与市场环境，请给出结构化分析。

## 一、市场环境
{% for s in market.instruments %}
### {{ s.inst_id }}
- 价格 {{ s.last | num(2) }}（24h {{ s.change_pct | pct }}）
- 趋势：{{ s.trend_regime }}｜波动：{{ s.vol_regime }}｜拥挤度：{{ s.crowding }}
- 资金费率年化 {{ s.funding_annualized | pct }}｜持仓量 24h {{ s.oi_change_pct | pct }}
- 多空账户比 {{ s.long_short_ratio | num(2) }}｜主动买卖比 {{ s.taker_buy_sell_ratio | num(2) }}
- 基差 {{ s.basis_pct | pct }}
{% endfor %}
{% if market.signals %}
**规则触发的信号：**
{% for sig in market.signals %}- {{ sig }}
{% endfor %}
{% endif %}

## 二、当前持仓
{% if positions | length == 0 %}
当前无持仓。
{% else %}
{{ positions | table(cols=["标的","方向","杠杆","名义","未实现盈亏","保证金率"]) }}

{% for p in positions %}
- **{{ p.inst_id }} {{ p.pos_side | side }}**：均价 {{ p.avg_px | num(2) }}，标记价 {{ p.mark_px | num(2) }}，强平价 {{ p.liq_px | default("无", true) }}，名义 {{ p.notional_usd | usd }}
{% endfor %}
{% endif %}

## 三、请回答
1. 当前持仓与市场状态是否存在方向性冲突？逐仓说明。
2. 结合资金费率与拥挤度，当前是否存在需要降杠杆的拥挤风险？
3. 给出 3 条具体、可执行、带条件的建议（不要泛泛而谈）。

---
*数据来源：OKX API。隐私等级 L{{ meta.privacy_level }}。*
```

**`review/period_review.md.j2`（复盘·时段）** —— 结构完全不同，围绕「这段时间发生了什么」：

```jinja
# 时段复盘请求 · {{ meta.range.label }}
> 分析区间：{{ meta.range.from | ts }} ~ {{ meta.range.to | ts }}（{{ meta.range.days }} 天）

你是一位专业的加密货币交易分析师。以下是我在**指定历史区间**内的交易记录与当时的市场环境，请做归因分析。

{% if meta.availability | length > 0 %}
> ⚠️ 数据可得性声明：
{% for a in meta.availability %}
> - {{ a.metric }}：{{ a.reason }}
{% endfor %}
{% endif %}

## 一、区间市场概况
{% for s in market.instruments %}
### {{ s.inst_id }}
- 区间涨跌 {{ s.summary.change_pct | pct }}｜最高 {{ s.summary.high | num(2) }}｜最低 {{ s.summary.low | num(2) }}
- 已实现波动率 {{ s.summary.realized_vol | pct }}
- 区间起点状态：{{ s.regime_at_start.trend }} / {{ s.regime_at_start.vol }}
- 区间终点状态：{{ s.regime_at_end.trend }} / {{ s.regime_at_end.vol }}
- 资金费率均值 {{ s.summary.funding_avg | na("超出 OKX 保留窗口") }}
- 持仓量变化 {{ s.summary.oi_change_pct | na("该时段无历史数据") }}
- 基差区间 {{ s.summary.basis_min | pct }} ~ {{ s.summary.basis_max | pct }}
{% endfor %}

## 二、我的平仓记录（{{ history | length }} 笔）
{{ history | table(cols=["标的","方向","开仓均价","平仓均价","盈亏","盈亏率","持仓时长","开仓时市场状态"]) }}

## 三、逐笔归因
{% for p in history %}
### {{ loop.index }}. {{ p.inst_id }} {{ p.pos_side | side }}（{{ p.pnl | usd }}，{{ p.pnl_ratio | pct }}）
- 开仓 {{ p.open_time | ts }} @ {{ p.open_avg_px | num(2) }} → 平仓 {{ p.close_time | ts }} @ {{ p.close_avg_px | num(2) }}
- 持仓时长 {{ p.hold_ms | dur }}｜手续费 {{ p.fee | usd }}｜资金费 {{ p.funding_fee | usd }}
- **开仓时刻的市场状态**：{{ p.regime_snapshot.trend | default("数据不可得", true) }} / {{ p.regime_snapshot.vol | default("数据不可得", true) }}
- 开仓前 24h 涨跌：{{ p.regime_snapshot.change_24h | pct | default("数据不可得", true) }}
- 数据来源：{{ p.source }}（时间精度：{{ p.time_precision }}）
{% endfor %}

## 四、统计
{% if stats %}
- 胜率 {{ stats.win_rate | pct }}｜盈亏比 {{ stats.profit_factor | num(2) }}｜期望值 {{ stats.expectancy | usd }}
- 费用侵蚀占已实现盈亏 {{ stats.fee_drag | pct }}
- 按开仓时市场状态分组：
{% for k, v in stats.by_dimension.regime.items() %}
  - {{ k }}：{{ v.count }} 笔，胜率 {{ v.win_rate | pct }}，合计 {{ v.pnl | usd }}
{% endfor %}
{% endif %}

{% if coverage.gaps | length > 0 %}
## 五、数据可信度声明
本区间本地留痕存在 {{ coverage.gaps | length }} 段间隙，部分持仓时长可能不精确：
{% for g in coverage.gaps %}- {{ g.from | ts }} ~ {{ g.to | ts }}（{{ g.duration_ms | dur }}）
{% endfor %}
{% endif %}

## 六、请回答
1. 从逐笔记录看，我的入场时机存在什么**系统性**问题（不是单笔运气问题）？
2. 按开仓时市场状态分组后，我在哪类环境下表现最差？为什么？
3. 我的止损/止盈执行是否与当时的波动率匹配？
4. 给出 3 条具体、可执行、带条件的改进建议。

---
*数据来源：OKX API + 本地留痕。隐私等级 L{{ meta.privacy_level }}。*
```

另两个：**`live/position_risk.md.j2`**（聚焦当前单一持仓的强平距离与仓位大小）、**`review/trade_postmortem.md.j2`**（针对单笔已平仓位的深度复盘）。

> **关于「不硬编码」（已确认的 i18n 策略）**：界面文案集中在 `src/lib/strings.ts`；内置模板正文用 `include_str!` 从 `templates/` 目录加载，**不散落在 Rust 代码里**——两者都天然满足「集中管理」。本期不引入 i18next：提示词模板是给 AI 读的，中文不影响效果，且用户随时可「另存为」改成任意语言。将来要国际化时，替换 `strings.ts` 与 `templates/` 目录即可，不需要动任何逻辑。

#### 6.6.5 隐私分级

提示词要粘到外部 AI，必须给用户控制权：

| 等级 | 行为 |
|---|---|
| **L0 全量** | 原始金额、张数、权益全部保留 |
| **L1 百分比化**（默认） | 金额转为占账户权益百分比；绝对权益只给量级区间（如 `$10k–$50k`） |
| **L2 结构脱敏** | 只保留方向、杠杆、相对关系与市场状态，不含任何金额与数量 |

实现位置在 `prompt/privacy.rs` 的**上下文装配阶段（不是模板里）**——这样用户自写模板也**无法绕过**隐私等级，避免「换了个模板结果泄漏了」的事故。

#### 6.6.6 模板编辑器

- CodeMirror 6 + Jinja 语法高亮（`@codemirror/legacy-modes` 有 jinja2 模式）。
- 右侧实时预览：编辑 debounce 300ms → `template_preview` 命令 → 用**真实数据**渲染。
- 预览下方显示 token 估算与字符数。
- 保存前先 `render` 一次，失败则拒绝保存并高亮错误行（minijinja 的 `Error` 带行号）。

---

### 6.7 提示词生成管线

#### 6.7.1 实盘

```mermaid
sequenceDiagram
    participant U as 用户
    participant F as 前端
    participant S as LiveService
    participant DB as SQLite
    participant T as minijinja

    U->>F: 进入实盘页
    F->>S: live_refresh({inst_ids, force})
    S->>S: 缓存命中判定（TTL 30s）
    alt 缓存未命中
        S->>S: 拉取 ticker / funding / oi / lsr / positions / balance
        S->>DB: 写入缓存 + position_trace 留痕
    end
    S-->>F: 市场状态 + 仓位 + cache_hit + fetched_at
    U->>F: 点击「生成提示词」
    F->>S: prompt_build({template_id, privacy})
    S->>S: 装配 LiveContext（应用隐私分级）
    S->>T: render(template, ctx)
    T-->>S: text
    S-->>F: { text, token_estimate, warnings }
```

#### 6.7.2 复盘

```mermaid
sequenceDiagram
    participant U as 用户
    participant F as 前端
    participant R as ReviewService
    participant P as FetchPlanner
    participant O as OKX
    participant DB as SQLite

    U->>F: 选择时段 [from, to] + 标的
    F->>R: review_plan({from, to, inst_ids})
    R->>R: 可得性检查（§6.1.3 矩阵）
    R-->>F: FetchPlan{est_requests, est_ms, warnings}
    U->>F: 确认执行
    F->>R: review_fetch(plan_id)
    R->>P: 执行（并发 + 限流 + 优先级）
    P->>O: 分页请求（含缓存复用）
    P->>DB: 落库（已定型数据永久缓存）
    P-->>F: fetch://progress 事件流
    P-->>R: SeriesResult[]
    R->>R: 重建指标序列 + 关键时点状态 + 统计
    R-->>F: ReviewContext 预览
    U->>F: 生成提示词
    F->>R: prompt_build({template_id, privacy})
    R-->>F: { text, token_estimate, warnings }
```

**Profile（详略档位）**——控制上下文体积：

| Profile | 包含 | 典型 token |
|---|---|---|
| `compact` | 仅核心摘要 | ~600 |
| `standard` | + 逐仓明细 + 统计 | ~1.5k |
| `deep` | + 指标序列采样点 + 逐笔成交 | ~6k+ |

**Token 估算**：不引入完整 tokenizer（体积与维护成本不划算），用 `cjk 字符数 + 非 cjk 字符数 / 4` 的启发式，UI 上**明确标注为「估算」**。宁可标注不确定，也不假装精确。

---

### 6.8 IPC 命令清单

| 命令 | 入参 | 出参 | 说明 |
|---|---|---|---|
| `vault_status` | – | `{exists, unlocked}` | |
| `vault_create` / `vault_unlock` / `vault_lock` | `{password?}` | `()` | |
| `credentials_list` | – | `[CredentialMeta]` | 无明文 |
| `credentials_save` | `{id?, label, api_key, secret_key, passphrase, env}` | `{id}` | 写 vault + 元数据表 |
| `credentials_delete` | `{id}` | `()` | |
| `credentials_test` | `{id}` | `{ok, permissions, pos_mode, uid_masked, error?}` | 调 `account/config`；顺带确认是只读 Key |
| **实盘** | | | |
| `live_refresh` | `{inst_ids, force}` | `LiveSnapshot{cache_hit, fetched_at, ...}` | 按需拉取，带 TTL 缓存 |
| `live_market` | `{inst_ids, force}` | `[MarketState]` | |
| `live_positions` | `{force}` | `{positions, account, trace_coverage}` | 顺带留痕 |
| **复盘** | | | |
| `review_plan` | `{from, to, inst_ids, bar}` | `FetchPlan{est_requests, est_ms, warnings}` | **不联网**，只做可得性检查与估算 |
| `review_fetch` | `{plan_id, force}` | `plan_id` | 异步执行，进度走事件 |
| `review_cancel` | `{plan_id}` | `()` | 取消执行 |
| `review_context` | `{plan_id}` | `ReviewContext` | 拉取完成后取结果 |
| `review_availability` | `{from, to}` | `[AvailabilityInfo]` | 时段可得性预检（供 UI 提示） |
| `positions_history_sync` | `{from?, to?}` | `{synced}` | 官方历史仓位同步 |
| `positions_history_query` | `{filter, page}` | `{items, total}` | |
| `position_stats` | `{from?, to?, group_by?}` | `Stats` | |
| **通用** | | | |
| `templates_list` / `templates_get` / `templates_save` / `templates_delete` | | | 分 `live`/`review` 两集；内置模板不可删，只能另存为 |
| `prompt_build` | `{template_id, profile, privacy, source}` | `{text, token_estimate, warnings}` | `source` 指向实盘快照或复盘 plan |
| `prompt_preview` | `{template_body, source}` | `{text, error?}` | 编辑器实时预览 |
| `prompt_export` | `{text, format, path?}` | `{path}` | |
| `settings_get` / `settings_set` | | | |
| `cache_stats` / `cache_clear` | `{scope?}` | | 缓存管理 |

**事件**：`fetch://progress`、`fetch://throttled`、`fetch://done`、`vault://locked`。

**错误模型**：`AppError` 枚举 → 序列化为 `{code, message, hint?, retryable}`。前端按 `code` 分支（`VaultLocked` → 弹解锁框；`RateLimited` → 显示倒计时；`CredentialInvalid` → 跳设置页；`DataUnavailable` → 标注不可得而非报错；`RangeTooLarge` → 提示时段超限）。

---

### 6.9 首次启动引导与标的集管理

因为已确认「默认标的集由用户自选」，首次启动需要一个引导流程。

**引导步骤**（3 步）：

| 步骤 | 内容 | 约束 |
|---|---|---|
| 1. 创建密钥库 | 设置 vault 主密码（argon2 派生） | 必须完成；明确提示「密码无法找回，丢失即无法读取已存凭据」 |
| 2. 录入 OKX 只读凭据 | API Key / Secret / Passphrase + 环境（实盘 / 模拟盘） | **可跳过**（可先只看行情不连账户）；保存后自动 `credentials_test`，探测到非只读权限时**红色警示** |
| 3. 选择标的 | 展示按 24h 成交额排序的 Top 20 永续合约，**预勾选 BTC / ETH / SOL**，用户自由增删 | 至少 1 个，上限 10 个 |

**为什么预勾选**：完全空白的多选框会让新用户卡住；预勾选推荐值既尊重「自己选」的意愿，又给了合理起点。点「下一步」即完成，想调整随时在设置里改。

**标的集管理**：

- 落 `settings` 表（键 `watchlist`，值为 `inst_id` 数组）。
- 增删时校验标的存在于 `instruments` 表且 `state=live`。
- 移除某标的时**不删除其历史数据**——已定型缓存保留，将来重新加回时仍可用。
- 上限 10 个（控制复盘请求量），UI 明确展示「当前 3 / 10」及原因。

**引导持久化**：`settings.onboarding_done = true`。启动时若未完成引导 → 直接进入引导页，不进入实盘页（避免用户在无标的集状态下看到一个空看板而困惑）。

---

## 7. 数据契约

### 7.1 时间基准

**所有时间戳统一存 Unix 毫秒（i64）**。OKX 部分接口返回字符串型毫秒时间戳，反序列化时统一转换。展示层再转本地时区。存毫秒而非秒，因为 K 线与快照都需要毫秒精度。

### 7.2 序列化

Rust ↔ 前端用 `serde`，字段命名统一 **`snake_case`**（不做 camelCase 转换——少一层映射就少一类 bug）。

**i64 必须声明为 TS 的 `number`（已实现，有测试守卫）**：ts-rs 默认把 `i64` 映射成 TypeScript 的 `bigint`，但我们的 IPC 走 JSON——前端拿到的实际是 `number`。声明成 `bigint` 会让类型系统撒谎：调用方以为要处理 bigint，运行时却是 number。因此**每个 i64 字段都要加 `#[ts(type = "number")]`**，并由 `i64_fields_are_declared_as_number_not_bigint` 测试守住。这在本项目里不是小概率问题：**所有时间戳都是 Unix 毫秒 i64**（§7.1），M1 之后会大量出现。

> 为什么可以安全地当 number：JS number 能精确表示到 2^53，而毫秒时间戳约 1.7×10^12，远在安全区内。

**OKX 的空字符串坑（必须处理）**：OKX 经常用 `""` 表示缺失值（如 `"liqPx": ""`），直接反序列化到 `f64` 会失败：

```rust
fn de_opt_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error> {
    let s: Option<String> = Option::deserialize(d)?;
    Ok(s.filter(|v| !v.is_empty()).and_then(|v| v.parse().ok()))
}
```

### 7.3 版本化

`LiveContext` / `ReviewContext` 均带 `schema_version`。模板可声明 `{% if meta.schema_version >= 2 %}` 做兼容分支。契约破坏性变更升 major，并在迁移脚本里为旧用户模板做标记提示。

### 7.4 类型生成

用 `ts-rs` 从 Rust 结构体生成 `src/lib/types.ts`。**导出目录由仓库根的 `.cargo/config.toml` 固定**：

```toml
[env]
TS_RS_EXPORT_DIR = { value = "src/lib", relative = true }
```

这一步是必需的：ts-rs 的 `#[ts(export_to = "...")]` 是**相对于 `TS_RS_EXPORT_DIR`** 解析的（默认 `./bindings`），不显式固定的话本地与 CI 的导出位置会不一致——而类型契约漂移正是 CI 要拦的东西，导出路径若依赖环境变量就失去了意义。

CI 中校验生成结果与提交版本一致（`cargo test` 触发导出 → `git diff --exit-code -- src/lib/types.ts`），防止前后端类型漂移。**已验证本地可复现：`cargo test` 后 `git diff` 为空。**

> ⚠️ **已知陷阱（实测踩过）**：ts-rs 的 `#[ts(export)]` 会为**每个类型**生成一个独立的导出测试，
> 而它们写同一个文件时是「第一个截断、后续追加」。因此**带过滤地跑测试**
> （例如 `cargo test --lib vault`）会把这个文件**截断成只剩匹配到的那一个类型**——
> 实测中 16 个类型被截成了 1 个，而且丢失的是**尚未提交**的内容，`git checkout` 也救不回来。
>
> 约定：**提交前必须跑完整的 `cargo test`**；CI 的 `git diff --exit-code -- src/lib/types.ts`
> 会拦住被截断的文件进入提交。这不是可以靠小心规避的问题，而是工具链的固有行为——
> 所以把它写在这里，而不是指望每个人都记得。

---

## 8. 数据库 Schema

SQLite，`PRAGMA journal_mode=WAL`，`foreign_keys=ON`。迁移用 `sqlx::migrate!`。

```sql
-- 0001_init.sql

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 凭据元数据（明文密钥在 Stronghold vault 中，此处仅引用）
CREATE TABLE api_credentials (
    id             TEXT PRIMARY KEY,          -- uuid，同时是 vault 中的键名后缀
    label          TEXT NOT NULL,
    env            TEXT NOT NULL DEFAULT 'live',   -- live | demo
    api_key_masked TEXT NOT NULL,             -- 只存掩码用于展示，如 "ab12****cd34"
    permissions    TEXT,
    uid_masked     TEXT,
    last_ok_at     INTEGER,
    last_error     TEXT,
    created_at     INTEGER NOT NULL
);

CREATE TABLE instruments (
    inst_id     TEXT PRIMARY KEY,
    inst_type   TEXT NOT NULL,
    base_ccy    TEXT NOT NULL,
    quote_ccy   TEXT NOT NULL,
    ct_val      REAL NOT NULL DEFAULT 1.0,
    ct_val_ccy  TEXT NOT NULL DEFAULT '',
    lot_sz      REAL, min_sz REAL, tick_sz REAL,
    state       TEXT,
    listed_at   INTEGER,
    updated_at  INTEGER NOT NULL
);

-- K 线缓存（同时是复盘的序列数据源）
CREATE TABLE candles (
    inst_id  TEXT NOT NULL,
    kind     TEXT NOT NULL,       -- last | mark | index
    bar      TEXT NOT NULL,
    ts       INTEGER NOT NULL,
    open REAL NOT NULL, high REAL NOT NULL,
    low  REAL NOT NULL, close REAL NOT NULL,
    vol REAL, vol_ccy REAL, vol_quote REAL,
    confirm  INTEGER NOT NULL DEFAULT 0,   -- 1 = 已收盘（永久缓存）
    PRIMARY KEY (inst_id, kind, bar, ts)
);
CREATE INDEX idx_candles_ts ON candles(ts);

-- 时序指标缓存（资金费率 / 持仓量 / 多空比 / 主动买卖量 / 基差）
CREATE TABLE metric_series (
    inst_id TEXT NOT NULL,
    metric  TEXT NOT NULL,        -- funding | open_interest | lsr | taker_ratio | basis
    ts      INTEGER NOT NULL,
    value   REAL,                 -- 单值指标
    payload TEXT,                 -- 多值指标（如 oiCcy/oiUsd）JSON
    finalized INTEGER NOT NULL DEFAULT 1,  -- 1 = 已定型，永久缓存
    PRIMARY KEY (inst_id, metric, ts)
);
CREATE INDEX idx_metric_range ON metric_series(inst_id, metric, ts);

-- 导入的外部历史数据（Phase 3：OKX 历史数据下载门户文件）
CREATE TABLE imported_series (
    inst_id  TEXT NOT NULL,
    metric   TEXT NOT NULL,
    ts       INTEGER NOT NULL,
    value    REAL,
    payload  TEXT,
    source_file TEXT,
    imported_at INTEGER NOT NULL,
    PRIMARY KEY (inst_id, metric, ts)
);

-- 采集计划与断点续传状态
CREATE TABLE fetch_state (
    series_key  TEXT PRIMARY KEY,   -- endpoint|inst_id|params_hash
    cursor_after  TEXT,
    cursor_before TEXT,
    last_ts     INTEGER,
    status      TEXT,               -- ok | partial | error
    error       TEXT,
    updated_at  INTEGER NOT NULL
);

-- 实盘市场状态快照（用户每次拉取时留档，用于事后对照）
CREATE TABLE live_snapshot (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           INTEGER NOT NULL,
    scope_json   TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_live_snapshot_ts ON live_snapshot(ts DESC);

CREATE TABLE account_snapshot (
    ts            INTEGER PRIMARY KEY,
    total_eq_usd  REAL, iso_eq_usd REAL, avail_bal_usd REAL,
    upl REAL, mgn_ratio REAL, pos_mode TEXT,
    payload_json  TEXT NOT NULL
);

-- 本地留痕：每次实盘拉取仓位时顺带写入
CREATE TABLE position_trace (
    ts          INTEGER NOT NULL,
    pos_id      TEXT NOT NULL,
    inst_id     TEXT NOT NULL,
    pos_side    TEXT NOT NULL,
    mgn_mode    TEXT NOT NULL,
    lever       REAL,
    contracts   REAL NOT NULL,
    avg_px      REAL, mark_px REAL, liq_px REAL,
    upl         REAL, upl_ratio REAL, mgn_ratio REAL,
    created_at  INTEGER, updated_at INTEGER,
    gap_before  INTEGER NOT NULL DEFAULT 0,  -- 1 = 距上次留痕有明显间隙
    PRIMARY KEY (ts, pos_id)
);
CREATE INDEX idx_trace_pos ON position_trace(pos_id, ts);

-- 合并后的历史仓位（源 A 官方 + 源 B 本地留痕）
CREATE TABLE position_history (
    pos_id         TEXT PRIMARY KEY,
    inst_id        TEXT NOT NULL,
    pos_side       TEXT NOT NULL,
    mgn_mode       TEXT NOT NULL,
    lever          REAL,
    open_avg_px    REAL, close_avg_px REAL,
    max_contracts  REAL,
    pnl            REAL, pnl_ratio REAL,
    fee            REAL, funding_fee REAL,
    liq_pnl        REAL, mrg_pnl REAL, realized_pnl REAL,
    open_time      INTEGER, close_time INTEGER,
    close_type     TEXT,
    source         TEXT NOT NULL,        -- okx | local | merged
    time_precision TEXT NOT NULL,        -- exact | approximate
    max_favorable  REAL,                 -- 本地留痕独有：持仓期最大浮盈
    max_adverse    REAL,                 -- 本地留痕独有：持仓期最大浮亏
    regime_snapshot TEXT,                -- 开仓时刻市场状态 JSON（复盘归因核心）
    synced_at      INTEGER NOT NULL
);
CREATE INDEX idx_pos_hist_time ON position_history(close_time DESC);
CREATE INDEX idx_pos_hist_inst ON position_history(inst_id);

CREATE TABLE fills (
    trade_id  TEXT PRIMARY KEY,
    inst_id   TEXT NOT NULL,
    ord_id    TEXT,
    side      TEXT NOT NULL,
    pos_side  TEXT,
    fill_px   REAL NOT NULL,
    fill_sz   REAL NOT NULL,
    fee       REAL, fee_ccy TEXT,
    exec_type TEXT,
    fill_time INTEGER NOT NULL
);
CREATE INDEX idx_fills_time ON fills(fill_time DESC);

CREATE TABLE prompt_templates (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL,        -- live | review
    name        TEXT NOT NULL,
    description TEXT,
    body        TEXT NOT NULL,
    profile     TEXT NOT NULL DEFAULT 'standard',
    builtin     INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE prompt_runs (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    ts             INTEGER NOT NULL,
    scope          TEXT NOT NULL,
    template_id    TEXT NOT NULL,
    profile        TEXT NOT NULL,
    privacy_level  INTEGER NOT NULL,
    range_from     INTEGER,
    range_to       INTEGER,
    context_json   TEXT NOT NULL,
    rendered       TEXT NOT NULL,
    token_estimate INTEGER
);

CREATE TABLE journal (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts      INTEGER NOT NULL,
    inst_id TEXT, pos_id TEXT,
    note    TEXT NOT NULL,
    tags    TEXT
);
```

**保留策略**：`position_trace` 保留 90 天；`candles` 每个 `(inst_id, kind, bar)` 保留最近 5000 根；`prompt_runs` 保留最近 200 条。设置里可调。定期 `VACUUM`（每月或数据变更超阈值时）。

**`live_snapshot` 永久保留**（已确认）：不设自动清理，仅在用户手动执行「清空缓存」时按范围删除。单条约 5~20KB，即便每天数十条，年增长也仅数十 MB——可接受，换来的是任何时候都能回溯「我当时看到的市场画面」。

---

## 9. 采集与缓存策略

**没有调度器。** 所有网络请求都由用户操作触发：

| 触发点 | 行为 | 预估请求数 |
|---|---|---|
| 进入实盘页 | 拉取市场状态 + 当前仓位 + 权益（受 TTL 约束） | 5~15 |
| 实盘手动刷新 | 同上，`force=true` | 5~15 |
| 进入复盘页 | 只做**可得性预检**（纯本地计算，**不联网**） | 0 |
| 复盘选定时段并确认 | 执行 FetchPlan | 30~300 |
| 复盘再次查看同一时段 | 缓存命中，几乎瞬时 | 0~5 |
| 生成提示词 | 复用已有上下文；若数据过期则提示用户先刷新 | 0 |

**TTL 表**见 §6.2.3。

**「已定型数据永久缓存」是本设计的核心红利**：
- 用户复盘过一次的时段，第二次打开**几乎瞬时**且**可离线**。
- 反复调整模板、反复生成提示词，**不产生任何网络请求**。
- 数据随时间自然积累，用越久越顺。

**缓存管理**：设置页提供「缓存占用 / 按范围清理 / 全部清空」，以及「清除未定型数据」快捷操作。

---

## 10. 双端差异与适配

### 10.1 Windows

| 项 | 方案 |
|---|---|
| 数据目录 | `%APPDATA%/com.marketlens.app`（`app_data_dir()` 解析） |
| 后台运行 | **不需要**。无实时采集，关闭即结束，无托盘常驻需求 |
| 打包 | `tauri build` → NSIS（`.exe`）+ MSI。WebView2 由 Tauri 处理（Win10 1803+ 需检查运行时） |
| 代码签名 | 可选。未签名 exe 会触发 SmartScreen 警告，正式分发建议购买代码签名证书 |
| 优势场景 | 大屏看图表、模板编辑、批量复盘 |

### 10.2 Android

| 项 | 方案 |
|---|---|
| 最低版本 | **Android 7.0 (API 24)**（Tauri v2 下限） |
| 数据目录 | 应用私有目录，`app_data_dir()` 自动解析 |
| 构建前置 | Android SDK + NDK（`NDK_HOME`）、JDK、Rust 目标 `aarch64-linux-android` + `armv7-linux-androideabi`，然后 `cargo tauri android init` |
| 打包 | `cargo tauri android build --apk --aab --split-per-abi` —— 一条命令同时产出 APK（自用分发）与 AAB（备用，将来上架 Play 时直接可用） |
| 签名 | `keytool -genkey -v -keystore upload-keystore.jks -keyalg RSA -keysize 2048 -validity 10000 -alias upload`，在 `android/app/build.gradle` 的 `signingConfigs.release` 配置。**keystore 与密码必须安全备份——丢失后无法更新已上架应用** |
| 后台限制 | ✅ **本版本基本消失**。没有 WS、没有定时器、没有后台采集 → 不存在「后台被冻结导致数据断流」的问题。这是「不要实时」带来的最大红利 |
| 唯一残留约束 | 复盘拉取 30~300 个请求时，用户需保持应用在前台（约 7~30 秒）。切到后台时系统可能暂停网络请求 → 设计上：**切后台时暂停计划、回前台自动续传**（`fetch_state` 已支持断点续传） |
| UI | 响应式：窄屏底部标签栏 + 单列；宽屏侧边栏 + 多列。图表可横屏全屏 |
| 键盘 | 模板编辑在手机上体验受限 → 移动端主推「选择模板 + 生成 + 复制」，编辑引导到桌面端（窄屏时提示） |
| 网络 | `AndroidManifest.xml` 声明 `INTERNET`（Tauri 模板默认含）；HTTPS 默认放行 |

### 10.3 能力矩阵

| 功能 | Windows | Android |
|---|---|---|
| 实盘市场状态 / 当前仓位 | ✅ | ✅ |
| 复盘时段重建 | ✅ | ✅（切后台会暂停，回前台续传） |
| 历史仓位同步 | ✅ | ✅ |
| 提示词生成（双模板集） | ✅ | ✅ |
| 模板编辑 | ✅ | ⚠️ 只读 + 简单修改 |
| 图表叠加仓位标记 | ✅ 大屏体验佳 | ✅ 可横屏 |
| 离线查看已缓存时段 | ✅ | ✅ |

---

## 11. 安全设计

| 威胁 | 对策 |
|---|---|
| 密钥泄漏 | 明文只存 Stronghold vault（argon2 派生加密）；SQLite 只存掩码；日志脱敏中间件过滤 `OK-ACCESS-*` 头与 secret 字段 |
| 越权交易 | 代码层面不实现任何私有 `POST` 端点；`credentials_test` 主动读取权限，非只读时**红色警示** |
| 前端注入 → 数据泄漏 | 不向前端暴露 SQL 与 vault 命令；`capabilities/default.json` **仅放行 `core:default`**；CSP 收紧为 `default-src 'self'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost`。注意 **`connect-src` 里刻意不含 `okx.com`**——所有交易所请求都由 Rust 侧发起，WebView 永远不需要直连外网，这比原设计更紧 |
| 提示词泄漏隐私 | 隐私分级在上下文装配阶段强制生效；导出时按等级二次确认 |
| 剪贴板残留 | 复制提示词后 60s 自动清空剪贴板（可选，默认开） |
| 无人值守 | 15 分钟无操作自动锁 vault |
| 依赖供应链 | `cargo deny` / `cargo audit` + `npm audit` 接入 CI |

---

## 12. 可观测性

- **结构化日志**：`tracing` + `tracing-subscriber`，滚动文件（保留 7 天）+ 开发模式控制台。
- **采集日志面板**：复盘拉取完成后可展开查看「每个序列：请求数、耗时、命中缓存数、失败原因」。用户能自己判断数据是否完整。
- **数据新鲜度指示**：所有数据展示处标注 `fetched_at` 与 `cache_hit`。
- **诊断导出**：一键导出脱敏诊断包（日志 + 版本 + 配置 + 缓存统计，**不含密钥与仓位明细**）。

---

## 13. 测试策略

| 层 | 工具 | 覆盖 |
|---|---|---|
| 签名 | Rust `#[test]` | 固定时间戳 + 固定密钥 → 断言 Base64 结果（黄金向量） |
| 反序列化 | `serde_json` + 录制样本 | 重点覆盖 `""` 空值、缺失字段、字段类型漂移 |
| 指标计算 | Rust `#[test]` | EMA/RSI/ATR/波动率用已知输入验证；边界：全等价格、单点数据、除零 |
| **分页迭代器** | Rust `#[test]` + `wiremock` | 游标推进、到达时段边界即停、`after`/`before` 语义、重复页去重 |
| **FetchPlan 展开** | Rust `#[test]` | 给定 (时段, 标的, bar) → 断言请求数估算与页数；断言超出可得窗口时产出 `AvailabilityWarning` |
| 限流器 | `tokio::time::pause()` | 断言窗口内放行数量与退避时序，不依赖真实时间 |
| 合并算法 | Rust `#[test]` | 双源 fixture：仅官方 / 仅本地 / 两源同 posId / 无 posId 模糊匹配 / 时间精度标注 |
| **可得性矩阵** | Rust `#[test]` | 对每个指标断言「超出窗口时返回 Unavailable 而非空值」 |
| 模板渲染 | 黄金文件测试 | 固定上下文 → 断言输出与 `tests/golden/*.md` 逐字节一致；**双模板集 × 三隐私等级** |
| 隐私分级 | Rust `#[test]` | **断言 L1/L2 输出中不含原始金额字符串**（安全测试，必须有） |
| 缓存 TTL | Rust `#[test]` + `tokio::time::pause()` | 断言定型数据永久命中、未定型数据不被缓存 |
| OKX 集成 | `wiremock` | 用录制/构造响应驱动完整流程，含 429、5xx、业务错误码 |
| 前端 | Vitest | 格式化函数、进度条状态机、可得性提示渲染 |
| 端到端 | Windows：`tauri-driver`（WebDriver） | 解锁 → 实盘刷新 → 生成提示词 → 断言输出。⚠️ `tauri-driver` **不支持 Android**，Android 用手工冒烟清单 |

### 13.1 已落地的测试

**M0（4 项）**

| 测试 | 守住什么 |
|---|---|
| `migrations_apply_on_empty_database` | `0001_init.sql` 能在空库上干净执行，且建出设计文档 §8 的 15 张表 |
| `cargo_version_matches_tauri_config` | `tauri.conf.json` 与 `Cargo.toml` 的版本号不漂移 |
| `i64_fields_are_declared_as_number_not_bigint` | TS 声明里不出现 `bigint`（§7.2 的契约规则） |
| `export_bindings_appinfo`（ts-rs 自动生成） | 类型导出链路可执行 |

**M1（49 项）**，按关注点分组：

| 关注点 | 代表性测试 |
|---|---|
| **真机响应映射** | `ticker_maps_real_response`、`funding_rate_handles_empty_next_rate`、`open_interest_exposes_named_usd_value` —— 直接使用**真机抓取的 JSON 样本**，字段名或类型判断错就会红 |
| **坏数据不致命** | `malformed_rows_are_skipped_not_fatal`、`rejects_malformed_rows` —— 一行坏数据不该让整段序列消失 |
| **指标数学** | `atr_accounts_for_gaps`（跳空必须走 `|high − prevClose|`）、`rsi_is_100_for_monotonic_rise_and_50_when_flat`（价格不动时取中性 50 而非 100）、`realized_vol_scales_with_period_count` |
| **分类不猜** | `insufficient_candles_reports_unavailable_instead_of_guessing`、`missing_vol_history_is_reported_not_defaulted` —— 样本不足时必须 `None` + 说明原因 |
| **限流时序** | `waits_only_after_quota_exhausted`、`groups_are_counted_independently`（用 `start_paused` 驱动时间，不依赖真实时钟） |
| **缓存语义** | `hit_within_ttl_and_marked_as_cached`、`miss_after_ttl`（注入时间点，无 `sleep`） |
| **退避** | `backoff_stays_within_bounds` |

**联网测试（默认 `#[ignore]`）**：`okx_live_snapshot_is_sane`（端到端采集并打印数值供人工比对）、`dump_indicators_for_crosscheck`（落盘输入与输出）。它们访问真实 API，不进 CI。

### 13.2 指标公式的独立交叉验证 ⭐

**问题**：Rust 侧的指标单元测试用的是我自己对公式的预期——如果公式本身理解错了，测试会跟着一起错。这类错误最危险，因为它会污染每一条 AI 复盘结论。

**做法**：用 Python **独立实现**同一套公式，对**同一份输入**重算并逐位比对。输入由 Rust 落盘，从而消除「行情推进导致两次运行数据不同」的干扰。

```bash
cd src-tauri && cargo test --lib dump_indicators_for_crosscheck -- --ignored
cd .. && python3 scripts/crosscheck_indicators.py
```

**实测结果（2026-09-25）**：

| 指标 | Rust | Python | 相对差 |
|---|---|---|---|
| EMA20 | 84202.4634139598 | 84202.4634139598 | `0.00e+00` |
| EMA60 | 84396.5793598429 | 84396.5793598429 | `0.00e+00` |
| EMA200 | 82426.6968384827 | 82426.6968384827 | `0.00e+00` |
| RSI14 | 44.0026791159 | 44.0026791159 | `0.00e+00` |
| ATR% | 0.0056558330 | 0.0056558330 | `0.00e+00` |
| 已实现波动率 | 0.3859204361 | 0.3859204361 | `0.00e+00` |

> 附带教训：第一版脚本硬编码了「上一次的 Rust 值」，结果因为未收盘 K 线在两次运行间变化，**RSI 报了 0.39% 的假差异**。落盘输入后消除。任何跨运行比对数值的验证都要先固定输入，否则会把数据漂移误判成代码缺陷。

`cargo test`、`cargo fmt --check`、`cargo clippy -D warnings` 均已在开发机通过。

**Android 手工冒烟清单**（每次发版必过）：
1. 冷启动 → 建 vault → 录入只读 Key → `credentials_test` 返回正确 UID 与 `pos_mode`
2. 实盘页刷新 → 数据正确，`fetched_at` 显示，30s 内二次进入命中缓存
3. 复盘选 7 天时段 → 进度条推进 → 完成后图表与统计正确
4. **复盘拉取中切后台再切回** → 计划续传而非从头开始
5. 生成 `review/period_review` 提示词 → 内容非空、无 `undefined`、无模板语法残留、不可得字段显示为「数据不可得」
6. 隐私等级切 L2 → 重新生成 → **肉眼确认无金额**
7. 断网 → 错误提示可读；已缓存时段**仍可离线查看**
8. 旋转屏幕 / 分屏 → 布局不塌

---

## 14. 构建与发布

```yaml
# .github/workflows/build.yml 骨架
jobs:
  check:        # fmt + clippy + test + ts-rs 一致性 + cargo deny
  windows:      # windows-latest → tauri build → NSIS/MSI 产物
  android:      # ubuntu-latest + Android SDK/NDK → tauri android build --apk --aab --split-per-abi
```

- 版本号单一来源：`tauri.conf.json` 的 `version`，构建脚本同步到 `Cargo.toml` 与前端。
- Android keystore 与密码存 CI secrets，**绝不进仓库**。
- **`rustsec/audit-check` 需要 `checks: write`**。它把审计结果发成一个 check run；
  权限不足时会在 `No vulnerabilities were found` **之后**单独抛
  `##[error]Resource not accessible by integration`，整步变红但审计其实是通过的——
  非常容易误判成「发现了漏洞」。check job 的 `permissions` 必须显式给 `checks: write`
  （仓库默认的 `GITHUB_TOKEN` 权限是只读）。
  注：从 fork 的 PR 触发时 GitHub 禁止 Check API，该 action 会降级为「把报告打到日志」，
  这是上游已知限制，不影响 push / 自家 PR。
- `npm audit --audit-level=high` 走**官方源**才能拿到漏洞库；本机 `~/.npmrc` 配的镜像
  （npmmirror）没有实现 audit 端点，会报 `NOT_IMPLEMENTED`。CI 上用的就是官方源，
  故不受影响；本地手测需显式 `--registry=https://registry.npmjs.org`。
- **已知残留（上游未修，非本仓库代码问题）**：`glib 0.18.5` 的
  `RUSTSEC-2024-0429`（unsound：`VariantStrIter` 的 `Iterator` 实现）。
  依赖链 `tauri 2.11.6 → gtk ^0.18 → glib 0.18.5`，而修复版是 `glib 0.20.0`；
  `tauri 2.11.6` 已是当前最新稳定版，且它对 `gtk` 的约束就是 `^0.18`
  （`gtk 0.19` 才要求 `glib ^0.22`）。**必须等上游 Tauri 升级 gtk**。
  所以 Dependabot 会持续报 `security_update_not_possible`
  （`latest-resolvable-version: 0.18.5`），这是符合预期的。
  影响面：**仅 Linux**（`cfg(target_os = "linux")` 才引入），且 `cargo audit`
  将它归入 `warnings.unsound` 而非 `vulnerabilities`（实测 `count: 0`），
  因此**不会让 CI 变红**。当前不抑制、不 ignore，保持可见。
- 产物：`MarketLens_x.y.z_x64-setup.exe`（Windows 安装包）、`MarketLens_x.y.z_arm64-v8a.apk`（自用分发）、`MarketLens_x.y.z.aab`（备用，未上架前仅归档）。
- 包名 `com.marketlens.app` 必须与 `tauri.conf.json` 的 `identifier`、Android 工程保持一致。**首次发布后不可更改**——改动会让系统视为全新应用，导致无法覆盖升级。

### 14.1 开发环境与无头验证

开发机是 **AlmaLinux 10 LXC 容器**，而 **RHEL 10 已移除 X.Org 服务器**，这使常规的「跑起来看一眼」路径全部失效：

| 常规方案 | 在本机的结果 |
|---|---|
| Xvfb | ❌ EL10 无此包（X.Org 服务器已被移除） |
| ImageMagick `import` | ❌ EPEL 的构建**未包含 X11 支持**（delegates 里没有 `x`） |
| `xwd` | ❌ 无此包 |
| `weston-screenshooter` | ❌ 服务端拒绝授权（`unauthorized`） |
| weston debug 的 `screenshot` 流 | ❌ weston 14 已无此流 |

**可行组合**（已脚本化）：`weston --backend=headless-backend.so` 造 Wayland 显示 → `--xwayland` 提供真实 X display → 应用以 `GDK_BACKEND=x11` 运行 → `tools/xshot`（x11rb 抓窗口）截图。

```bash
MARKETLENS_DEV=1 scripts/headless-smoke.sh /tmp/shot.png   # debug 构建
MARKETLENS_BIN=src-tauri/target/release/marketlens scripts/headless-smoke.sh /tmp/shot.png
```

两个必须知道的点：

1. **裸 `cargo build`（无论 debug 还是 release）都会连接 `devUrl`，不内嵌前端资源**。
   本文档原先写的是「release 构建才内嵌 `dist`」——**2026-09-25 实测证伪**：
   `cargo build --release` 产出的二进制启动后报
   `Could not connect to localhost: Connection refused`，因为它在连 `devUrl`。

   要产出真正内嵌资源的发布件，必须走官方构建路径：

   ```bash
   npx tauri build --no-bundle    # 只出二进制，不打包（本机无打包工具时用这个）
   npx tauri build                # 出二进制 + 安装包
   ```

   所以用二进制冒烟时：
   - debug 二进制 → 必须先起 `npm run dev`（脚本的 `MARKETLENS_DEV=1` 会代劳）
   - release 二进制 → 必须是 `tauri build` 产出的那个，裸 `cargo build --release` 的不算
2. **无头环境下 WebKit 必须强制软件渲染**：`WEBKIT_DISABLE_COMPOSITING_MODE=1`、`WEBKIT_DISABLE_DMABUF_RENDERER=1`、`LIBGL_ALWAYS_SOFTWARE=1`，否则白屏或崩溃。

**验收边界（已确认）**：本机验证 Linux 桌面端；**Windows 与 Android 的构建验证交给 CI**（`windows-latest` 与 `ubuntu + Android SDK/NDK` 才是这两端的原生环境），运行验证由用户在其设备上完成。
#### 14.1.1 密码派生必须优化编译（实测踩过）

`src-tauri/Cargo.toml` 里有四个 `[profile.dev.package.*]` 的 `opt-level = 3` 覆盖：
`argon2`、`rust-argon2`、`scrypt`、`libsodium-sys-stable`。

没有它们时，**单个 vault 往返测试耗时 9 分钟**；加上后降到 **15 秒**（60 倍差距）。
原因是 argon2 是计算密集型纯 Rust 实现，`opt-level = 0` 让它慢两三个数量级；
树里同时存在两个 argon2 实现（插件用 `argon2`，`iota_stronghold` 内部用 `rust-argon2`），
所以两者都要覆盖。

这属于「功能正常但体验不可用」的问题：用户解锁密钥库要等几分钟，会以为应用卡死。
设计文档原本只提到 `scrypt` 一项（来自插件文档的上游 bug 提示），实际需要覆盖四个包。

---



---

### 14.2 打包前置条件与签名（2026-09-25 核实）

**本机的实际能力边界**（逐项核实过，不是推测）：

| 能力 | 状态 |
|---|---|
| Linux release 构建 | ✅ 系统库齐全（webkit2gtk-4.1 / gtk+-3.0 / libsoup-3.0 均可 `pkg-config` 到） |
| deb / rpm / AppImage 打包 | ❌ 无 `dpkg-deb`、`rpmbuild`、`appimagetool`、`linuxdeploy` |
| Windows 打包 | ❌ 无 `x86_64-pc-windows-msvc` target，无 `makensis`、无 `wine` |
| Android 打包 | ❌ 无 Android SDK、无 NDK（Rust target 倒是装好了） |

所以本机能验证的是「**release 构建 + release 二进制实际跑起来**」，
其余三端交给 CI（§14.1 已确认的验收边界）。

#### Windows

来自 Tauri 官方前置条件，均已确认：

- **Microsoft C++ Build Tools**，勾选「Desktop development with C++」
- **WebView2 Runtime**（安装程序会引导）
- **MSI 需要 VBSCRIPT 可选功能**：若报 `failed to run light.exe`，
  去「设置 → 应用 → 可选功能 → 更多 Windows 功能」勾上 VBSCRIPT。
  Windows 11 24H2 起它**默认可能被移除**，这是构建 MSI 最常见的坑。

产物：`nsis/*.exe` 与 `msi/*.msi`。

#### Android

官方要求五件套：**SDK Platform、Platform-Tools、NDK (Side by Side)、Build-Tools、
Command-line Tools**。两个最容易漏的点：

1. **NDK 必须单独安装**。`android-actions/setup-android` 只装 SDK，
   而 Tauri 编译 Rust 到 Android 需要 NDK 里的 clang 与 sysroot——
   没有它整步直接失败。CI 里已补上，并把 `NDK_HOME` 写进 `$GITHUB_ENV`。
2. **`NDK_HOME` 必须指向版本子目录**（`$ANDROID_HOME/ndk/<version>`），
   指向 `$ANDROID_HOME/ndk` 本身不行。
3. **`run()` 必须标 `#[cfg_attr(mobile, tauri::mobile_entry_point)]`**。
   这个宏生成 JNI 侧的 `start_app` 等入口符号；缺了它 `tauri android build`
   会在「校验动态库」这一步失败：

   ```
   failed to validate library: Library from .../libmarketlens_lib.so does not
   include required runtime symbols. This means you are likely missing the
   tauri::mobile_entry_point macro usage
   ```

   报错文字本身给了线索，但**症状离原因很远**——它看起来像 `.so` 链接问题，
   实际只是少了一行属性宏。桌面端不需要这个宏（`cfg_attr` 保证只在移动端展开），
   所以本地 `cargo check` 完全不会暴露它，只有真去 `tauri android build` 才炸。

   已验证：同一个最小 crate，**加宏后 `.a` 里有 `__start_app` 符号，去掉则为 0 个**。

Rust target：`aarch64-linux-android`、`armv7-linux-androideabi`、
`i686-linux-android`、`x86_64-linux-android`。

#### 签名

**release 构建必须签名才能安装**，未签名的 APK 看起来完全正常，直到有人拿去装才发现装不上。

Android 的签名配置走官方流程（`keystore.properties` + `build.gradle.kts` 的
`signingConfigs`）。但 `src-tauri/gen/android/` **不入库**（每次 CI 重新生成，
保证可重复），所以配置没法「提交一次就完事」——必须在 `tauri android init`
之后自动打补丁：`scripts/patch_android_signing.py`。

这个脚本的关键性质是**锚点找不到就报错退出**：静默跳过会产出一个看似正常的
未签名 APK。它有内建自测（`--self-test`），在 CI 的 check job 里跑——
脚本改的是 CI 里临时生成的工程，本地跑不到，逻辑必须自带验证。

CI 需要的 secrets：

| secret | 说明 |
|---|---|
| `ANDROID_KEY_BASE64` | `base64 -i upload-keystore.jks` 的输出 |
| `ANDROID_KEY_ALIAS` | 通常是 `upload` |
| `ANDROID_KEY_PASSWORD` | 生成 keystore 时设的密码 |

未配置时 CI **不会失败**，但会输出 `::warning::` 明确说明产物未签名——
比静默产出装不上的 APK 好得多。

生成 keystore：

```bash
keytool -genkey -v -keystore ~/upload-keystore.jks \
  -keyalg RSA -keysize 2048 -validity 10000 -alias upload
```

#### 打包签名仍未验证

**签名流程本身未经端到端验证**：本机没有 SDK，无法生成 Android 工程，
也就无法确认补丁后的 Gradle 能编译通过。已验证的只有补丁脚本的逻辑（自测）
与「锚点缺失时明确报错」。**首次跑通 CI 的 android job 才算真正验证。**

---

### 14.3 发布冒烟清单

每个平台首次发布前逐项走一遍。**「能构建」不等于「能用」**——下面每一项都对应
一个真实发生过的失败模式，不是走过场的勾选。

#### 三端通用（必过）

| # | 检查项 | 为什么 |
|---|---|---|
| 1 | 冷启动进引导页，3 步能走完 | 引导是唯一入口，它坏了应用等于不可用 |
| 2 | 密钥库解锁后**冷重启**只要求解锁，不再要求重建 | 解锁状态不该被持久化，但凭据必须能重新解开 |
| 3 | 实盘页拿到真实行情（3 个标的、无警告） | 验证出网、限流器、指标计算三段都通 |
| 4 | 账户页显示真实权益与持仓 | 验证签名算法与私有端点 |
| 5 | 复盘页采集 → 装配 → 统计 → 逐笔归因 | M3 + M4 全链 |
| 6 | 提示词页：4 个内置模板都能渲染；切 L0/L1/L2 **内容随之变化** | M5 全链；切等级不生效是最隐蔽的失败 |
| 7 | 提示词页：改模板正文，预览在 300ms 内更新 | 防抖坏了用户会以为编辑器没保存 |
| 8 | 导出：复制到剪贴板有反馈；另存为产生「自定义」模板 | 导出是唯一的产出出口 |
| 9 | 设置页：缓存统计有数据；清理有二次确认 | 清理不可撤销 |
| 10 | 设置页：诊断导出成功，`redactions` 与实际相符 | 见下条 |

#### 脱敏必须**人工核对一次**

诊断包导出后，**打开 JSON 亲眼确认**：

- 不含任何 `$` 金额、不含持仓明细
- 不含 API Key 的任何片段（连打码后的都不该有）
- 不含用户名（家目录路径已被替换成 `[已脱敏:家目录]`）

自动化测试覆盖了这些（`system::diagnostics::security` 有 5 项），但**首次发布前
人工看一遍**仍然值得：测试断言的是我想到的模式，而泄漏可能来自我没想到的地方。

#### 分平台

**Windows**
- NSIS 安装包能装、能卸、开始菜单有项
- 安装后**不依赖开发机环境**（WebView2 缺失时安装程序应引导安装）
- MSI 若构建失败，先检查 VBSCRIPT 可选功能（§14.2）

**Android**
- APK 能安装（**未签名会装不上**，见 §14.2）
- 冷启动不闪退（Android 上 WebView 初始化比桌面慢，启动期竞态最容易在这里暴露）
- 切到后台再回来，实盘页数据仍正确（长连接已去掉，但缓存时间戳要正确）
- 通知栏/状态栏无异常占用提示

**Linux**
- AppImage / deb 能跑（本机无打包工具，产物来自 CI）

#### 已知的失败模式（清单的由来）

- **裸 `cargo build` 的产物连 `devUrl` 而不是内嵌资源**（debug 与 release 都一样）：
  症状是窗口里一行 `Could not connect to localhost: Connection refused`。
  冒烟必须用 `tauri build` 产出的二进制（§14.1）。这个坑我踩过一次——
  当时以为是前端构建问题，实际是构建方式不对。
- **无头环境必须强制软件渲染**（§14.1）：`WEBKIT_DISABLE_COMPOSITING_MODE=1` 等三项，
  否则白屏或崩溃。
- **release 构建的 WebView 版本与开发机不同**：只在 release 上出现的渲染问题确实存在，
  所以冒烟要用 release 产物。
### 14.4 发布流程

**推一个 tag 就发版**：

```bash
# 1. 改版本号（两处必须一致，CI 会校验）
#    src-tauri/tauri.conf.json 的 version
#    package.json 的 version
# 2. 提交
git commit -am "chore: 版本 0.1.0"
# 3. 打 tag 并推送
git tag v0.1.0
git push origin main --tags
```

CI 会依次：三个平台全部构建成功 → 校验 tag 与版本号一致 → 清点产物 →
创建 GitHub Release 并把安装包附上去。

#### 为什么用 tag 而不是「每次推 main 都发」

推 main 就发版会攒出几十个无意义的 Release，而用户真正想知道的是
「**这一版能装的东西在哪**」。tag 是「这是一个版本」的显式声明，语义准确。

#### 用草稿 Release 中转，而不是 artifact

各平台 job 在构建成功后**直接把自己的安装包传进一个草稿 Release**，
最后由 `release` job 校验资产齐全、写入说明、发布。

两个原因：

1. **私有仓库的 artifact 走存储配额**。配额耗尽时整个 job 会失败
   （实测报 `Artifact storage quota has been hit`），而 **Release 资产不占这个配额**。
2. **草稿天然满足「三端全成才发布」**：在三端都传完之前，用户根本看不到这个 Release。
   这比「先传 artifact 再由 release job 下载合并」更直接，也少一次搬运。

非 tag 构建仍然传 artifact（便于调试），但配额耗尽不会让一次成功的构建变红
（`continue-on-error: true`）。

##### 草稿的创建收敛在单独的 `draft` job

**先说结论：`gh release create` 在 release 已存在时不会报错，也不再复用，
而是再建一条同名记录**（实测确认，且返回 exit 0）。所以「没有就建」这种写法
在并行 job 里是错的。

原先 windows 与 android 各自写了：

```bash
# 注释写着「并发安全」，但事实相反
gh release view "$TAG" >/dev/null 2>&1 || gh release create "$TAG" --draft ...
```

两个 job 同时走到这一步就会各建一条。后果不是干脆的报错，而是隐蔽的错乱：

- `gh release view <tag>` 在多条同名记录里只返回其中一条；
- `gh release upload <tag>` 未必传到同一条。

于是资产被拆散到不同记录上，`release` job 校验时看到「资产不全」而拒绝发布，
或者发布出一个空壳 Release。

还有一条约束排除了「按 id 精准操作」这条退路：**`gh release view/edit/upload`
只接受 tag，不接受 release id**（用 id 报 `release not found`，实测确认）。
所以只能把「该 tag 下恰好一条」这个不变量先立住。

修法是把创建动作收敛到单一的 `draft` job，windows / android 都 `needs: [draft]`：

```yaml
draft:
  if: startsWith(github.ref, 'refs/tags/v')
  permissions: { contents: write }
  steps:
    - run: bash scripts/ensure_draft_release.sh
```

⚠️ **`needs` 有个必须知道的语义**：needs 的 job 被跳过 → 本 job **也会被跳过**。
所以 windows / android 必须显式写条件，否则非 tag 的 main 推送下 `draft` 被跳过，
两个打包 job 会跟着被跳过、main 构建直接废掉：

```yaml
needs: [draft]
if: always() && (needs.draft.result == 'success' || needs.draft.result == 'skipped')
```

这个条件同时覆盖三种情形：非 tag（draft skipped → 放行）、tag 且草稿就绪
（放行）、tag 但草稿检查失败（跳过，不白花 20 分钟构建一个注定传不上去的包）。

`scripts/ensure_draft_release.sh` 负责「有且仅有一条」：不存在则创建，
多于一条则明确报错拒绝继续（例如上次失败的运行留下了残留）。
它还有一处针对 API 行为的处理：**列表接口在 create 之后可能短暂看不到新记录**
（实测返回过 0），所以「存在性」用即时的 `gh release view` 判断，
只有「重复计数」才带重试。

#### 三个刻意的设计

1. **`needs: [draft, check, windows, android]`**——三个平台的产物都成功才发布。
   半成品 Release 比没有 Release 更糟：用户下载了 Windows 包却发现没有 Android 包，
   还得回来翻日志。这里用 `always() &&` 显式列每个 needs 的结果，
   而不是只写 `startsWith(...)`——因为 `draft` 也有 tag 条件，不这么做的话
   它一旦被跳过就会连带跳过 `release`，连「校验失败该报错」的机会都没有。
2. **校验 tag 与版本号一致**。版本号有三个来源（`tauri.conf.json`、`Cargo.toml`、
   `package.json`），前两者由 Rust 测试守住，CI 再守 tag。不校验的话，
   Release 名字会撒谎：标着 `v0.2.0`，装出来的却是 `0.1.0`。
3. **预发布自动识别**：tag 带后缀（`v0.1.0-beta.1`）时发成 prerelease，
   不占「最新版本」的位置。

#### Release 说明里必须写清楚的三件事

发布说明里会明确列出：

- Android 产物**是否已签名**（未签名则给出配置 secret 的指引）
- Windows 产物**未做代码签名**，安装时会有 SmartScreen 警告
- Windows 与 Android 产物**未经真机验证**

这三条都是「用户下载后会立刻遇到的问题」。不写的话，用户会以为是应用坏了，
而不是「这是未签名的开发版本」。

##### 「本次提交」列表：上一个 tag 的两个坑

发布说明里的提交列表靠 `PREV..当前 tag` 这个区间生成。挑 `PREV` 时有两个
实际踩到的坑，都已修：

1. **必须过滤成版本 tag**（`git tag -l 'v*'`）。`--sort=-v:refname` 会把
   **所有** tag 排进来，临时 tag（如演练用的 `zzz-*`）在版本序里可能排在前面，
   于是 `PREV` 指向与发版无关的 tag。实测：`PREV` 选到了 `zzz-not-a-version`。
2. **必须限定为当前 tag 的祖先**（`--merged "$GITHUB_REF_NAME"`）。
   否则重跑一个较旧的 tag 时，`PREV` 会选到比它**更新**的 tag，
   `PREV..当前` 成了反向区间，提交列表变成 0 条（实测踩到）。

```bash
PREV=$(git tag -l 'v*' --merged "$GITHUB_REF_NAME" --sort=-v:refname \
         | grep -v "^${GITHUB_REF_NAME}$" | head -1 || true)
```

再补一道 `git rev-parse --verify --quiet "$PREV^{commit}"` 兜底：
`git tag` 列的是**本地** tag，若该 tag 未 fetch 到就无法解析，此时回退到
「最近 20 条」而不是让 `git log` 报错终止。

---

## 15. 里程碑

| 阶段 | 交付物 | 验收标准 |
|---|---|---|
| ~~**M0 骨架**~~ ✅ **已完成** | Tauri v2 双端空壳、SQLite 迁移、ts-rs 类型生成、CI 骨架 | **实际验收（2026-09-25）**：Linux 桌面端真实启动，界面显示 应用版本 0.1.0 / 核心版本 0.1.0 / 数据库 Schema 版本 1 / 运行平台 linux，IPC 徽章为「通道正常」。Windows 与 Android 的构建验证按 §14.1 交给 CI |
| ~~**M1 实盘**~~ ✅ **已完成** | OKX 公开数据按需拉取、指标计算、状态分类、缓存层、实盘页 | **实际验收（2026-09-25）**：实盘页同时显示 BTC/ETH/SOL 的 17 项指标、状态徽章与触发信号，数据取自真实 OKX API；指标数值经**独立 Python 实现逐位交叉验证**（相对差全部 `0.00e+00`）。详见 §15.2 |
| ~~**M2 账户与仓位**~~ ✅ **已完成** | vault、凭据管理、首次启动引导（3 步，含标的集选择）、当前仓位、权益、本地留痕 | **实际验收（2026-09-25）**：在无头环境**真实走完 3 步引导** → 主界面 → 账户页显示模拟盘真实权益与持仓；非只读密钥触发红色警示；本地留痕写入成功；冷重启只要求解锁。详见 §15.3 |
| ~~**M3 采集计划器**~~ ✅ **已完成** | FetchPlan 展开、分页迭代器、并发执行、进度事件、断点续传、可得性矩阵 | **实际验收（2026-09-25）**：30 天 × 3 标的 = **75 请求 / 5.6 秒**（验收线 10 秒）；二次执行 **0 请求**（续传生效）；可得性提示如实浮现；期间真实遇到 **429 并被退避重试救回**。详见 §15.4 |
| ~~**M4 复盘管线**~~ ✅ **已完成** | 时段市场状态重建、历史仓位双源合并、关键时点归因、统计 | **实际验收（2026-09-25）**：真机跑通「采集 → 同步 → 合并 → 归因 → 统计 → 界面」全链；逐笔仓位绑定到开仓时刻状态（趋势向上/波动正常/多空均衡，开仓前 24h +4.63%）；按状态分组统计与费用侵蚀（89.8%）均正确。详见 §15.5 |
| ~~**M5 提示词**~~ ✅ **已完成** | minijinja 集成、双模板集（4 个内置模板）、双上下文装配、隐私分级、模板编辑器、导出 | **实际验收（2026-09-25）**：4 模板 × 3 隐私等级全部通过黄金测试；L1/L2 输出无原始金额；不可得字段渲染为显式声明；界面无头跑通「选模板 → 切等级立即重生成 → 编辑 300ms 防抖重生成 → 复制/另存为」。详见 §15.6 |
| ~~**M6 发布**~~ ✅ **已完成（带保留）** | 打包签名、诊断导出、缓存管理 UI、文档 | **实际验收（2026-09-25）**：release 二进制（`tauri build --no-bundle`）无头跑通 5 个页签 + 诊断导出 + 缓存管理；诊断包人工核对无密钥/无金额。**Windows 与 Android 产物未实际构建过**（本机无对应工具链，§14.2），签名流程未经端到端验证——**首次跑通 CI 才算真正验证**。详见 §15.7 |

### 15.1 M0 验收记录（2026-09-25）

- **真实启动截图**：应用窗口标题 `MarketLens`，页面显示上表的四项数值，绿色徽章「IPC 通道正常」。
- **Schema 版本 `1` 的来历**：该数字读自数据库 `_sqlx_migrations` 表，不是代码里的常量——所以界面上出现它，本身就证明迁移确实执行过。
- **通过的检查**：`cargo test` 4/4、`cargo fmt --check`、`cargo clippy -D warnings`（零警告）、`npm run build`（tsc 双配置 + vite）、类型契约无漂移（`cargo test` 后 `git diff` 为空）。
- **未在本机验证**：Windows / Android 的构建与运行，原因与替代方案见 §14.1。

### 15.2 M1 验收记录（2026-09-25）

- **真实数据渲染**：实盘页一次性显示 3 个标的（BTC/ETH/SOL）各自的 17 项指标、三个状态徽章与触发信号列表，数据全部来自真实 OKX API。
- **新鲜度提示生效**：顶部显示「实时拉取 / 刚刚拉取 / 拉取于 2026-09-25 16:32」。
- **「数据不可得」链路贯通**：`funding-rate` 的 `nextFundingRate` 实测为空串，界面正确显示「数据不可得」——从 OKX 的空串 → Rust 的 `None` → 界面文案，整条链路验证过。
- **发现并修掉一个真实缺陷**：初版采集全串行，3 个标的耗时 **18.6 秒**。改为两阶段并发（限流器仍负责速率控制，并发只重叠网络往返）后降至 **2.87 秒**。
- **子代理标记的集成风险未发生**：前端 `call("live_refresh")` 不带参数时传 `undefined`，Rust 侧 `Option<bool>` 正常取 `None`，命令成功返回。
- **通过的检查**：Rust 51 项单元测试 + 2 项联网测试（`#[ignore]`）、`clippy -D warnings` 零警告、`fmt --check`、前端 28 项测试、`tsc` 双配置、`vite build`。
- **未直接观察**：M1 原验收标准里的「30 秒内二次进入命中缓存」——界面只在挂载时做一次非强制拉取，无头截图无法模拟「切走再回来」。缓存语义由 `LiveCache` 的单元测试覆盖（TTL 边界、`cache_hit` 标记、覆盖写入），命令侧接线为 3 行。**这一点如实记录，不用间接证据充当直接证据。**

### 15.3 M2 验收记录（2026-09-25）

#### 怎么做到「真实走完引导」的

引导需要键盘输入，而无头截图无法输入，本机也没有 `xdotool` / `xautomation`。因此新增了
`tools/xshot/src/bin/xtype.rs`——用 XTEST 扩展注入键鼠事件（字符 → keycode 的映射
从当前键盘布局实时查询，不硬编码键位表）。配合 `xshot` 截图，就能在无头环境里
驱动完整 UI 流程。**这补齐了 mock 测试覆盖不到的一环**：jsdom 测试把 IPC 全 mock 掉了，
前后端参数形状不匹配这类问题只有在真实 IPC 上才暴露。

#### 逐步验证到的内容

| 步骤 | 证据 |
|---|---|
| 冷启动进引导 | 第 1 步显示「主密码无法找回…没有任何后门或恢复通道」警示 + 双密码 + 「我已了解」勾选三道闸门 |
| **前端校验真的会拦** | 故意留空提交 → 「主密码不能为空。」；两次不一致 → 「两次输入的密码不一致。」，按钮保持禁用 |
| 创建密钥库 | 日志 `INFO 密钥库已解锁 path=…/vault.hold` |
| 凭据保存与探测 | 界面显示「已保存，元数据如下（**明文密钥不回传前端**）`abcd****ef12` 模拟盘」；UID `1234****5678`；持仓模式「净持仓」 |
| **非只读密钥警示** | 后端 `WARN 凭据包含交易权限，非只读 perm=read_only`；界面红色警示原文：「本应用在类型层面不存在任何私有 POST 能力，因此无法用它下单；但建议改用只读密钥。」 |
| 标的集选择 | 真实候选列表按 24h 成交额降序（ETH $6.65B / BTC $6.64B / SOL $1.18B…），**预勾选正好是 BTC/ETH/SOL**，显示「当前 3 / 10」 |
| 完成引导 → 主界面 | 实盘页刷新 `instruments=3 warnings=0`；出现「实盘 / 账户」双标签与「重新配置」入口 |
| 账户页 | 总权益 $104,136.21 / 可用权益 $101,000.02 / 未实现盈亏 $3.25 / 名义价值 $590.29 / **持仓模式 净持仓**；币种明细 4 条；持仓 SOL-USDT-SWAP **净持仓·全仓·3x** +1.66% |
| 本地留痕 | 界面「近 30 天有 1 次留痕，记录连续。」；数据库 `position_trace` 1 条 |
| 冷重启 | 只显示「解锁密钥库」，**不再重走 3 步** |

#### 数据库落盘证据（模拟盘真实数据）

```text
settings:  watchlist = ["ETH-USDT-SWAP","BTC-USDT-SWAP","SOL-USDT-SWAP"]
           onboarding_done = true
凭据元数据: T1 / demo / abcd****ef12 / read_only / 1234****5678   ← 只有掩码，无明文
留痕条数:   1
```

#### 本轮发现并修掉的两个真实缺陷

1. **Stronghold client 解析顺序导致凭据丢失**（§6.5）。`get_client` 只查内存中的会话
   client，`load_client` 才从快照加载。少了中间一步，重新解锁时会**新建空 client 顶掉
   快照里的那个**——「保存成功，重启后消失」。由真实文件系统的往返测试抓到。
2. **argon2 未优化导致解锁慢 60 倍**（§14.1.1）。dev 构建下单个 vault 往返测试耗时
   **9 分钟**，加 `opt-level = 3` 覆盖后 **15 秒**。

另有一个**设计缺口由前端子代理指出**：`settings.onboarding_done` 在设计文档 §6.9 里
写了、但 IPC 清单里没有读写它的命令。后果是每次冷启动都会重走 3 步引导。已补
`bootstrap_state` / `onboarding_complete` 两个命令，并让 `watchlist_candidates` 的
预勾选取自**当前已保存的标的集**（否则用户再次进入引导时，点「下一步」会把选择
静默重置回默认值）。

#### 未验证 / 已知不足

- **未实现**：§6.5 原计划的「15 分钟无操作自动锁定」。当前锁定是显式动作。
- **未验证**：`credentials_delete` 与「重新配置」入口的完整往返（本轮未走）。
- **UX 待改**：账户概览把全仓模式的 `mgnRatio` 直接按百分比显示（实测 3116993.30%），
  数值本身没错（OKX 也这么给），但对全仓仓位没有可读性——后续应改为倍数或仅逐仓显示。
- 本机仍无法验证 Windows / Android 的构建与运行（§14.1）。

### 15.4 M3 验收记录（2026-09-25）

#### 真机实测（30 天 × 3 标的，1 小时粒度）

```text
计划：12 条序列，预估 75 请求，预估耗时 8000 ms
实际：75 请求，耗时 5.6 秒（第二次运行 6.6 秒）

[Ok] ETH/BTC/SOL-USDT-SWAP  K 线          720 行 / 8 页
[Ok] ETH/BTC/SOL-USDT-SWAP  标记价 K 线    720 行 / 8 页
[Ok] ETH/BTC/SOL-USDT-SWAP  指数价 K 线    720 行 / 8 页
[Ok] ETH/BTC/SOL-USDT-SWAP  资金费率         90 行 / 1 页
```

条数与理论值**精确一致**（30 天 × 24 = 720 根；30 天 × 3 次/天 = 90 条）——
多一页或少一根都会被断言抓住，所以这个一致性本身就在证明分页边界判断正确。

> 验收标准原本写「2 标的 ≈ 60 请求」，实际跑的是关注列表里的 **3 标的**，
> 因此请求数按比例升到 75，**时间仍在 10 秒线内**。

#### 退避重试在真实环境救了一次

两次运行都真实撞上 `429 Too Many Requests`：

```text
WARN OKX 返回可重试状态码，退避后重试 path=".../history-candles?...&after=..." status=429 attempt=1
WARN OKX 返回可重试状态码，退避后重试 ... status=429 attempt=2
INFO 采集计划结束 requests=75 elapsed_ms=5612 cancelled=false
```

计划**照常完成**。这是退避+抖动第一次在真实限流下被验证，而不是单元测试里的模拟。

#### 续传

同一计划再跑一次：**首次 12 请求 → 二次 0 请求**，所有序列状态为 `Skipped`。
粒度是**序列级**（已完成的整条跳过，被取消的那条下次重拉），这一点在
`fetch/executor.rs` 的模块注释里如实写明，没有假装支持页级续传。

#### 本轮发现并修掉的三处问题

1. **`plan_id` 参数名大小写会让命令在运行时失败**（前端子代理发现，我核实源码确认）。
   Tauri v2 的宏**默认把命令参数名转成 camelCase**（`tauri-macros` 的
   `ArgumentCase::Camel`），而契约是前后端统一 snake_case。前端按契约发 `plan_id`、
   后端却去找 `planId`，命令会反序列化失败，且错误信息只说「缺少参数」——
   很难联想到是大小写问题。已加 `#[tauri::command(rename_all = "snake_case")]` 并在运行时验证。
2. **契约守卫漏掉了 `u64` → `bigint`**（`FetchPlan.est_duration_ms`）。原守卫用
   **手工维护的类型列表**（只有 7 个，实际已 24 个），新增类型不会自动被覆盖。
   已改为**集中导出 + 就地校验**（见 §7.4）：一个测试负责导出全部类型、校验 bigint、
   校验类型数量，再写文件。顺带消除了「带过滤跑测试会截断 types.ts」的坑。
3. **切换标签会丢失复盘的计划与采集结果**（无头 UI 验证发现，mock 测试测不到）。
   原因是条件渲染会卸载未激活页面。已改为**惰性挂载 + 保持挂载**：首次访问后不再
   卸载，同时没访问过的标签仍不挂载，不破坏「按需采集」。

#### 未验证 / 已知不足

- **进度条的中间态**只截到了完成态；进度事件的驱动逻辑由前端测试覆盖
  （`progressPercent` 与事件订阅的 plan_id 过滤），但没有逐帧确认动画推进。
- **取消**只验证了 Rust 侧的取消令牌与前端 `cancel=false` 分支，未在真实采集中途点过取消。
- **页级续传**未实现（序列级已实现），如实记录在 `executor.rs` 的模块注释里。
- 本机仍无法验证 Windows / Android 的构建与运行（§14.1）。

### 15.5 M4 验收记录（2026-09-25）

#### 真机端到端（模拟盘真实仓位）

完整走通：**M3 采集 → 官方历史仓位同步 → 双源合并 → 开仓时刻归因 → 统计 → 界面渲染**。

```text
采集 4 条序列 / 25 请求

统计概览
  胜率        0.00%（1 笔）
  盈亏比      无盈利笔            ← 精确文案，不是 0 也不是 ∞
  期望值      -$1.96
  平均持仓    约 10 秒
  费用侵蚀    89.80%             ← 亏损 1.96 里手续费占 1.76
  附带结论：「交易成本吃掉了相当一部分利润，值得优先改进。」

按开仓时市场状态
  趋势向上 | 1 笔 | 0.00% | -$1.96

逐笔仓位
  SOL-USDT-SWAP 多 官方 时间精确 杠杆 3× | -$1.96
  开仓 17:14 → 平仓 17:15 | 均价 117.46 → 117.45 | 手续费 -$1.76
  开仓时市场状态：趋势向上 / 波动正常 / 多空均衡 | 开仓前 24h +4.63%

数据可信度
  官方 1 笔 / 仅本地 0 笔 / 被本地补充极值 1 笔
```

**这证明了 M4 的核心价值**：逐笔仓位被绑定到它**开仓时刻**的市场状态，
并按该维度分组统计——「同样的方向，在不同市场状态下的胜率可能完全不同」
从设计设想变成了实测输出。

界面同时验证了：页脚 `Schema 2`（迁移 0002 已应用）、来源角标（官方）、
时间精度角标（精确）、以及「装配过程中的问题」面板如实提示
「官方历史仓位在到达起始时间前已耗尽，该时段可能不完整」。

#### 本轮发现并修掉的问题

1. **`plan_id` 参数名大小写**（M3 遗留，前端子代理发现）：Tauri v2 宏默认把命令参数名
   转 camelCase，与 snake_case 契约冲突。已加 `rename_all = "snake_case"`。
2. **`build_context` 依赖 Tauri 类型导致不可测**：它原先收 `&Vault`，而 Vault 需要
   AppHandle。改为收 `Option<&Credentials>`——编排层不再依赖 Tauri 类型，且
   「无凭据时只用已落库数据离线复盘」成为一个显式分支。
3. **`derive_lifecycles` 输出顺序不确定**：排序键 `first_seen` 会重复，导致顺序依赖
   HashMap 迭代（测试间歇失败、界面跳动）。加 `pos_id` 作次级键。
4. **`profit_factor = null` 语义不可区分**（前端子代理发现）：UI 只能用合并文案。
   改用现有字段派生精确文案（`total`/`win_rate`），无需后端改动。
5. **`StatGroup` 的「未归因」靠字符串匹配**（前端子代理发现）：UI 匹配 key 字面量
   「未归因」是脆弱耦合，后端改文案会静默退化。已加显式字段 `is_unattributed`。
6. **`TraceSnapshot` 一半字段没用上**——但其中 `created_at` 本该有用：它是 OKX 给的
   仓位创建时间，比留痕时间精确得多，本地独有的仓位因此也能还原出接近精确的开仓时间。

#### 未验证 / 已知不足

- **持仓极值的样本数未暴露**：模拟盘那笔仓位只存在 10 秒、只留痕 1 次，于是
  界面显示「最大浮盈 +$3.25 / 最大浮亏 +$3.25」。数值在数学上正确（单元素集合的
  极值就是它自己），但**会误导**——看起来像「这笔仓位从没亏过」。
  真实使用中仓位持续数小时到数天、留痕有几十上百次，这个问题自然消失；
  但要彻底诚实，应当在 `ClosedPosition` 上暴露「极值基于几次留痕」，
  让界面在样本过少时降级为「仅 1 次留痕，极值参考价值有限」。
- **`OkxFill` / 成交明细同步未实现**：模型已按真机字段写好，但本期没有调用方
  （M5 若需要「逐笔成交」进入提示词再启用）。
- 本机仍无法验证 Windows / Android 的构建与运行（§14.1）。

### 15.6 M5 验收记录（2026-09-25）

#### 无头端到端（真实 OKX 行情，无凭据）

完整走通：**引导 → 解锁 → 提示词页 → 选模板 → 切隐私等级 → 编辑 → 实时预览 → 复制/另存为**。

```text
模板库（4 个内置，按实盘/复盘分组，内置角标）
  实盘速览 / 持仓风险体检
  复盘：绩效与归因 / 复盘：教训

隐私等级
  L0 · 全量       完整金额与数量都会进入提示词
  L1 · 百分比（默认） 金额转成占账户权益的百分比，权益只给量级区间
  L2 · 脱敏       只保留方向、杠杆与市场状态，不含任何金额与数量
  「隐私分级在后端强制生效：无论模板怎么写，都无法绕过它把金额带进提示词。
    切等级会立即重新生成预览。」

预览（真实数据）
  # OKX 实盘速览 / 生成时间 20:22:39 / 数据源 OKX
  生成过程中的问题：• 尚未配置凭据
  市场状态 × 3（真实价格、RSI、ATR、资金费率、基差、多空比、规则信号）
  ## 我的持仓
  账户数据不可得：尚未配置凭据
  当前没有持仓。
  token 估算：715   字符数：1201

导出：复制到剪贴板（「已复制」反馈）/ 保存为文件
另存为 → 模板已保存 template_id=user_1790339117391（库中出现「实盘速览（副本）」+ 自定义角标）
```

**验证到的关键行为**：
- **切隐私等级立即重新生成**（不是只改状态）：L1→L2 后预览内容与 token 估算都变了
- **编辑 300ms 防抖后重新生成**：插入 `AAA` 后预览出现 `「数据支持的判断」AAA和「你的推测」`，
  字符数 1198→1201
- **账户不可得时渲染成显式声明**，而不是留空，也没有让模板渲染失败
- **内置模板只能另存为**：不会覆盖内置模板，产生的是带「自定义」角标的副本

#### 界面跑起来才发现的问题（单元测试和夹具测试都发现不了）

1. **裸 f64 泄漏进提示词**：`RSI14：65.54630065390165`、`主动买卖比：0.3855541718069005`。
   夹具里的数字都是我挑的整数，永远不会暴露这个。已加 `price` 过滤器
   （按量级自适应小数位：BTC 2 位、DOGE 4 位、小币 8 位），并**系统性排查**
   了所有未格式化的模板变量（现在模板里剩余的裸变量全是字符串枚举）。
2. **`{{ a - b | dur }}` 实际是 `a - (b | dur)`**：Jinja 的 `|` 比 `-` 结合更紧。
   黄金测试抓到的——它直接报 `tried to use - operator on unsupported types
   number and string`。必须写 `{{ (a - b) | dur }}`。
3. **提示词里混英文枚举**：`cross` / `isolated` → 加 `margin` 过滤器输出「全仓 / 逐仓」。
4. **`leverage` 渲染成 `3.0×`** → `| num(0)`。

#### 子代理发现的后端 bug（已修 + 已补回归测试）

`resolve_template` 的 body 分支硬编码 `kind = TemplateKind::Live`，而
`prompt_build_review` 拿到模板后会校验类型——于是「**复盘模板 + 编辑器实时预览**」
必然报「模板「（未保存的模板）」是实盘模板，不能用于复盘」。
原代码注释还写着「命令层已在上游校验过类型，不会用到这个值」，与实现自相矛盾。

**根因**：后端自测只覆盖了 `template_id` 那条路径，而**前端编辑后总是传 body**。
已补 4 项回归测试（预览路径 / 已保存路径 / 不存在的 id / 数据库往返）。

**教训**：「预览未保存的正文」和「用已保存的模板」是两条路径，两条都要测。

#### 测试工具改进（无头驱动必需）

- `xtype geom`：打印窗口在 root 上的偏移。**必需**——`xshot` 抓的是子窗口，
  而 XTEST 注入用的是 root 坐标，写死坐标会在某次运行后突然失效，
  且症状是「点了没反应」，极难联想到坐标问题。
- `xtype scroll` 支持向上（负数）与指定坐标。双栏布局里左右栏是**各自独立**的
  滚动容器，固定滚窗口中心只能滚到左栏。
- 已知坑：vite 会监视 `tools/` 并**整页重载**，所以验收期间不能重建 xtype
  （会重置页签与隐私等级，看起来像应用 bug）。

#### 未验证 / 已知不足

- **真实账户数据未走通 M5 装配**：开发库在修「重复建表」时被重置，凭据元数据丢失
  （密钥仍在 vault 里，但 vault 没有枚举 id 的接口）。因此本次只验证了
  「无凭据 + 真实行情」这条路径。账户/仓位相关的装配由夹具 + 黄金测试覆盖，
  真实账户金额的泄漏断言写在 `prompt::templates::real_data`（`#[ignore]`，
  需 `MARKETLENS_TEST_*` 环境变量）。
- **复盘模板的实时预览未跑通**：它需要凭据才能同步历史仓位。修复后的类型校验
  已由回归测试覆盖，但未在界面上实际点过。
- CodeMirror 使主 chunk 增至 772KB（gzip 240KB）。若在意移动端体积，
  可把 `TemplateEditor` 做成 `React.lazy` 动态导入。
- 导出用的是浏览器 Blob 下载，不是 `@tauri-apps/plugin-fs`——避免为 M5 引入
  新的原生依赖，也回避 Windows/Android 的路径差异。

### 15.7 M6 验收记录（2026-09-25）

#### release 构建与二进制冒烟

```text
npx tauri build --no-bundle
  Finished `release` profile [optimized] target(s) in 1m 18s
  Built application at: src-tauri/target/release/marketlens   （30.8 MB）
```

用这个二进制在无头环境跑通：**解锁 → 5 个页签 → 实盘真实行情 → 设置页 → 诊断导出**。
（无头脚本 `scripts/headless-smoke.sh` 的 release 路径。）

#### 界面跑起来才发现的两个问题

1. **裸 `cargo build --release` 的产物连 `devUrl`，不内嵌前端资源**。
   启动后窗口里只有一行 `Could not connect to localhost: Connection refused`。
   本文档原先写的是「release 构建才内嵌 `dist`」——**被实测证伪**。
   正确做法是走官方构建路径 `npx tauri build`（§14.1 已修正）。
   这个坑很费时间：症状看起来像前端构建问题，实际是构建方式不对。

2. **家目录脱敏漏了 `/root/`**。这是**人工核对真实诊断包**时发现的：
   自动化测试断言的是我想到的模式（`/home/`、`/Users/`），
   而开发机以 root 运行，真实日志里的路径是 `/root/.local/share/...`，
   于是用户名 `root` 原样进了诊断包。
   已补 `/root/` 并加了对应的回归测试。

   **这正是 §14.3 冒烟清单里「脱敏必须人工核对一次」那一条的价值**：
   测试通过不代表没问题，只代表测试想到的地方没问题。

#### 设置页验收

```text
缓存管理
  数据库总大小  159.7 KB
  表 / 行数 / 保留策略
    K 线            0   每个序列保留最近 5000 根
    指标序列        0   每个序列保留最近 5000 点
    仓位留痕        0   保留最近 90 天
    历史仓位        0   不自动清理          ← retention=null 显示为「不自动清理」
    ...
  [刷新] [清理缓存]

  点「清理缓存」→ 展开确认区（不是直接执行）：
    确认清理缓存？
    仓位留痕保留最近 90 天；K 线与指标每个序列保留最近 5000 点；
    提示词生成记录保留最近 200 条。此操作不可撤销。
    [确认清理] [取消]

诊断导出
  落盘路径  /root/.local/share/com.marketlens.app/diagnostics/diagnostic-*.json  [复制路径]
  文件大小  2.3 KB
  生成时间  2026-09-25 20:46
  日志文件  .../logs/marketlens.2026-09-25.log

  脱敏说明
    诊断包已脱敏，由三道结构性防线保证：
    • 不含密钥：凭据只给数量与权限属性，连打码后的 API Key 都没有。
    • 不含账户与仓位明细：只统计行数，不导出任何业务数据行。
    • 日志已逐行脱敏：长随机串 / 家目录路径 / 凭据赋值都会被替换。
  本次脱敏明细：本次没有需要脱敏的内容。
  凭据概览：共 0 条凭据 / 仅数量与权限属性，不含任何密钥信息。
  日志尾部（已脱敏）[展开]
  [复制诊断包 JSON]
  版本信息：应用版本 0.1.0 / 核心版本 0.1.0 / Schema 版本 2 / 平台 linux
```

#### 人工核对真实诊断包（冒烟清单要求）

导出后逐项看过落盘的 JSON：

- ✅ 无金额、无持仓明细——`cache.tables` 只有行数，`credentials` 只有数量
- ✅ 无 API Key 的任何片段（本机未配凭据，`count: 0`）
- ✅ 设置走白名单，只有 `watchlist` 与 `onboarding_done`
- ✅ `redactions: []` 与实际相符（本次日志里确实没有需要脱敏的内容）
- ❌ **日志里的 `/root/...` 未被脱敏** → 已修（见上）

#### CI 修复（首次运行就会红的两处）

1. **Android 缺 NDK**：`android-actions/setup-android` 只装 SDK，
   而 Tauri 编译 Rust 到 Android 需要 NDK 的 clang 与 sysroot。
   已补安装步骤 + `NDK_HOME`（必须指向版本子目录）。
2. **Linux 依赖缺两项**：官方列表里还有 `libxdo-dev` 与 `libssl-dev`。

#### 未验证 / 已知不足

- **签名流程未经端到端验证**：本机没有 Android SDK，无法生成 Android 工程，
  也就无法确认补丁后的 Gradle 能编译通过。已验证的只有补丁脚本的逻辑
  （`--self-test` 通过）与「锚点缺失时明确报错」。
  **首次跑通 CI 的 android job 才算真正验证。**
- **Windows / Android 产物均未实际构建过**：本机无 Windows target、无打包工具、
  无 Android SDK/NDK（§14.2 已逐项核实）。验收边界见 §14.1。
- **未做代码签名（Windows）**：NSIS/MSI 能构建但未签名，用户安装时会看到
  SmartScreen 警告。签名需要证书，属于发布决策而非代码问题。
- **`live_snapshot` / `account_snapshot` 表始终是 0 行**：界面缓存走的是内存
  （`fetch::cache::LiveCache`），这两张表定义了但没被写入。不影响功能，
  但要么用起来要么删掉——留着会让人以为有持久化缓存。

**关键路径**：无。M0–M6 全部完成。

---

## 16. 风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| OKX 限流数值不确定（搜索到的 `history-candles` 数值互相矛盾） | 复盘批量拉取时被 429 | 保守初值 + 端点元数据可配置 + 实现时逐项核对官方文档 + 退避与冷却 |
| **Rubik 统计只服务最近约 48 小时**（实测更正，见 §6.1.3） | 任何超过约 2 天的复盘都拿不到多空比 / 持仓量 / 主动买卖量 | 已设计为「明确标注不可得」而非静默空白；复盘主干是 K 线（完整）+ 资金费率（90 天）；预留 `imported_series` 表接入官方历史数据文件（Phase 3） |
| OKX 历史仓位窗口有限 | 早期仓位永久丢失 | 本地留痕是唯一解；**越早开始用越好**（留痕零成本，只要打开应用就记录） |
| `ctVal` 换算错误导致名义价值错 | 给 AI 喂错数据 | 单元测试覆盖币本位/U 本位两种合约；UI 标注换算依据 |
| 复盘拉取 300 个请求耗时较长 | 用户等待焦虑 | 进度条 + 可取消 + 优先级调度（先出主体内容）+ 完成后永久缓存 |
| 模板里不可得字段被 AI 当成「无数据即无风险」 | 错误结论 | `na` 过滤器强制输出「数据不可得：原因」；提示词开头有可得性声明段落 |
| Stronghold argon2 在低端 Android 解锁慢 | 体验差 | 实测解锁耗时；必要时降低 argon2 参数（安全 vs 体验权衡需记录） |
| minijinja `Strict` 模式下用户模板易报错 | 挫败感 | 编辑器实时校验 + 友好错误（行号 + 变量名 + 建议） |

**全部开放问题已于 2026-09-25 确认关闭**，结论汇总见 §2 需求确认结果表。

---

## 17. 决策记录（ADR 摘要）

| # | 决策 | 理由 | 被否方案 |
|---|---|---|---|
| 1 | 业务逻辑全在 Rust 侧 | 双端行为一致，模板输出逐字节相同 | 前端承担逻辑（两端 WebView 差异成为不确定性来源） |
| 2 | 不用 `tauri-plugin-sql` | 避免把任意 SQL 能力暴露给 WebView | 插件直连（开发快，但扩大攻击面） |
| 3 | 密钥库用 Stronghold | 官方支持 Windows/Android 全平台 | `keyring` crate（Android 无原生后端）；`android-native-keyring-store`（需 JNI 手工初始化，风险高） |
| 4 | 模板引擎用 minijinja（Rust 侧） | 跨端一致 + 表达力足够 + 支持自定义 Loader | 前端 Handlebars（双端差异）、Mustache（无逻辑，复盘模板写不动） |
| 5 | 不内置 LLM 调用 | 零成本、零网络依赖、用户自选模型 | 内置 API 调用（多一份密钥管理与网络故障面） |
| 6 | 历史仓位双源合并 | 官方窗口有限，本地留痕是长期唯一解 | 只用官方（数据永久丢失）、只用本地（只能记录首次运行之后） |
| 7 | 时间戳统一 Unix 毫秒 | K 线与快照都需毫秒精度 | 秒级（K 线场景不够用） |
| 8 | 字段命名 `snake_case` 贯穿前后端 | 少一层映射就少一类 bug | 自动 camelCase 转换 |
| 9 | 隐私分级在上下文装配阶段强制 | 用户自写模板无法绕过 | 在模板里做（换个模板就泄漏） |
| **10** | **不使用 WebSocket，全部按需拉取** | 使用场景是「我想看/我想复盘」，不是盯盘；去掉长连接后 Android 后台冻结问题基本消失，无预警需求，无重连/心跳/漏事件的复杂度 | WS 实时订阅（对复盘场景零价值，却带来整套连接管理负担） |
| **11** | **实盘与复盘双模式分离** | 两者时间视角、数据源、上下文、模板、采集规模都不同；混在一起会让提示词逻辑纠缠 | 单模式（用参数区分，会导致上下文装配分支爆炸） |
| 12 | 敏感数值**自描述**（`{kind, value}`），不做「敏感字段清单」 | 清单漏一个字段就是一次泄漏，而且漏了没有任何症状；自描述让「凡是钱天然在射程内」 | 维护字段路径清单（新增金额字段时静默失效） |
| 13 | 模板契约**与隐私等级无关**（账户不可得时也输出全部键，值为「不可得」） | 切隐私等级只是换个脱敏程度，不该让模板突然渲染失败 | 按等级裁剪字段（用户一切等级模板就坏，且报错难懂） |
| 14 | 数值过滤器**透传「数据不可得」占位符** | `{{ x \| na("原因") \| pct }}` 是模板作者最自然的写法，两个过滤器不能组合的话模板会长到没法维护 | 要求模板处处写 `{% if %}` |
| 15 | 内置模板**不入库**，随代码发布 | 避免「升级后旧副本覆盖新版本」；用户「另存为」的副本才落库 | 首次启动把内置模板写进库（升级时无法区分用户改动与旧版内置） |
| 16 | 渲染前用静态分析**列出全部缺失变量** | minijinja 严格模式的报错只说 `undefined value`，不说哪个变量，模板一长就没法查 | 只依赖严格模式（报错无法定位） |
| **12** | **已定型历史数据永久缓存** | K 线 `confirm=1`、已结算资金费率、过去的统计点都不会再变；永久缓存让二次复盘瞬时且可离线 | 统一 TTL（浪费重复请求，复盘体验差） |
| **13** | **本地留痕改为「顺带记录」而非后台定时** | 无后台任务的前提下，复用实盘拉取结果零成本留痕 | 放弃本地源（丢失官方拿不到的最大浮盈/浮亏信息） |
| **14** | **不可得数据显式渲染为「数据不可得：原因」** | AI 看到空白会编造，看到明确声明才会谨慎；这是复盘结论可信度的前提 | 渲染为空/省略（会导致 AI 幻觉） |
| **15** | **可得性预检不联网** | `review_plan` 纯本地计算，避免「打开复盘页就偷偷发请求」 | 打开页面即拉取（违背「按需」原则） |
| **16** | **复盘时段硬上限 90 天** | 90 天 ≈ 8 页/标的 K 线，单次拉取 10 秒内完成，覆盖月度/季度复盘需求；配合永久缓存，反复查看零成本 | 不设上限（单次数千请求，等待时间不可控） |
| **17** | **首次启动由用户自选标的（预勾选 BTC/ETH/SOL）** | 尊重用户对标的的自主权，同时用预勾选避免新用户面对空白多选框卡住；上限 10 个控制复盘请求量 | 硬编码默认标的（用户第一眼看到不关心的币） |
| **18** | **界面文案集中 `strings.ts`，本期不上 i18next** | 满足「不硬编码」的可维护性诉求，又不引入本期无收益的双语工作量；模板正文本身已是集中管理 | 现在就双语（无收益）、直接写死（将来难抽） |