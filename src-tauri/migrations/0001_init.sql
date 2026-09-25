-- MarketLens 初始 schema
-- 对应设计文档 §8。约定：
--   * 所有时间戳统一为 Unix 毫秒（i64）
--   * 所有金额字段为 REAL；缺失值一律 NULL（OKX 的空字符串在 Rust 反序列化层已转为 None）
--   * 明文密钥永不落库，只存 api_key_masked

-- 通用键值配置（含 watchlist / onboarding_done / 各类阈值）
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
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
    inst_id    TEXT PRIMARY KEY,
    inst_type  TEXT NOT NULL,
    base_ccy   TEXT NOT NULL,
    quote_ccy  TEXT NOT NULL,
    ct_val     REAL NOT NULL DEFAULT 1.0,
    ct_val_ccy TEXT NOT NULL DEFAULT '',
    lot_sz     REAL,
    min_sz     REAL,
    tick_sz    REAL,
    state      TEXT,
    listed_at  INTEGER,
    updated_at INTEGER NOT NULL
);

-- K 线缓存（同时是复盘的序列数据源）
CREATE TABLE candles (
    inst_id   TEXT NOT NULL,
    kind      TEXT NOT NULL,       -- last | mark | index
    bar       TEXT NOT NULL,
    ts        INTEGER NOT NULL,
    open      REAL NOT NULL,
    high      REAL NOT NULL,
    low       REAL NOT NULL,
    close     REAL NOT NULL,
    vol       REAL,
    vol_ccy   REAL,
    vol_quote REAL,
    confirm   INTEGER NOT NULL DEFAULT 0,   -- 1 = 已收盘（永久缓存）
    PRIMARY KEY (inst_id, kind, bar, ts)
);
CREATE INDEX idx_candles_ts ON candles(ts);

-- 时序指标缓存（资金费率 / 持仓量 / 多空比 / 主动买卖量 / 基差）
CREATE TABLE metric_series (
    inst_id   TEXT NOT NULL,
    metric    TEXT NOT NULL,        -- funding | open_interest | lsr | taker_ratio | basis
    ts        INTEGER NOT NULL,
    value     REAL,                 -- 单值指标
    payload   TEXT,                 -- 多值指标（如 oiCcy/oiUsd）JSON
    finalized INTEGER NOT NULL DEFAULT 1,  -- 1 = 已定型，永久缓存
    PRIMARY KEY (inst_id, metric, ts)
);
CREATE INDEX idx_metric_range ON metric_series(inst_id, metric, ts);

-- 导入的外部历史数据（Phase 3：OKX 历史数据下载门户文件）
CREATE TABLE imported_series (
    inst_id     TEXT NOT NULL,
    metric      TEXT NOT NULL,
    ts          INTEGER NOT NULL,
    value       REAL,
    payload     TEXT,
    source_file TEXT,
    imported_at INTEGER NOT NULL,
    PRIMARY KEY (inst_id, metric, ts)
);

-- 采集计划断点续传状态
CREATE TABLE fetch_state (
    series_key    TEXT PRIMARY KEY,   -- endpoint|inst_id|params_hash
    cursor_after  TEXT,
    cursor_before TEXT,
    last_ts       INTEGER,
    status        TEXT,               -- ok | partial | error
    error         TEXT,
    updated_at    INTEGER NOT NULL
);

-- 实盘市场状态快照（每次用户拉取时留档，永久保留，用于事后回溯「当时看到什么」）
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
    total_eq_usd  REAL,
    iso_eq_usd    REAL,
    avail_bal_usd REAL,
    upl           REAL,
    mgn_ratio     REAL,
    pos_mode      TEXT,
    payload_json  TEXT NOT NULL
);

-- 本地留痕：每次实盘拉取仓位时顺带写入（零额外网络请求）
CREATE TABLE position_trace (
    ts         INTEGER NOT NULL,
    pos_id     TEXT NOT NULL,
    inst_id    TEXT NOT NULL,
    pos_side   TEXT NOT NULL,
    mgn_mode   TEXT NOT NULL,
    lever      REAL,
    contracts  REAL NOT NULL,
    avg_px     REAL,
    mark_px    REAL,
    liq_px     REAL,
    upl        REAL,
    upl_ratio  REAL,
    mgn_ratio  REAL,
    created_at INTEGER,
    updated_at INTEGER,
    gap_before INTEGER NOT NULL DEFAULT 0,  -- 1 = 距上次留痕有明显间隙
    PRIMARY KEY (ts, pos_id)
);
CREATE INDEX idx_trace_pos ON position_trace(pos_id, ts);

-- 合并后的历史仓位（源 A 官方 + 源 B 本地留痕）
CREATE TABLE position_history (
    pos_id          TEXT PRIMARY KEY,
    inst_id         TEXT NOT NULL,
    pos_side        TEXT NOT NULL,
    mgn_mode        TEXT NOT NULL,
    lever           REAL,
    open_avg_px     REAL,
    close_avg_px    REAL,
    max_contracts   REAL,
    pnl             REAL,
    pnl_ratio       REAL,
    fee             REAL,
    funding_fee     REAL,
    liq_pnl         REAL,
    mrg_pnl         REAL,
    realized_pnl    REAL,
    open_time       INTEGER,
    close_time      INTEGER,
    close_type      TEXT,
    source          TEXT NOT NULL,        -- okx | local | merged
    time_precision  TEXT NOT NULL,        -- exact | approximate
    max_favorable   REAL,                 -- 本地留痕独有：持仓期最大浮盈
    max_adverse     REAL,                 -- 本地留痕独有：持仓期最大浮亏
    regime_snapshot TEXT,                 -- 开仓时刻市场状态 JSON（复盘归因核心）
    synced_at       INTEGER NOT NULL
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
    fee       REAL,
    fee_ccy   TEXT,
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
    inst_id TEXT,
    pos_id  TEXT,
    note    TEXT NOT NULL,
    tags    TEXT
);
