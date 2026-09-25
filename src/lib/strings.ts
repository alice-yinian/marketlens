/**
 * 界面文案集中管理（设计文档 §6.6.4 已确认的 i18n 策略）。
 *
 * 本期不上 i18next：中文单语言。但所有界面文案必须集中在这里，
 * 不散落在组件中——将来要国际化时只需替换本文件，不动任何逻辑。
 *
 * 约定：带参数的文案写成箭头函数（`(n) => \`…${n}…\``），
 * 这样组件里连字符串拼接都不需要，也就不会漏掉某条文案。
 */
export const S = {
  appName: "MarketLens",
  tagline: "OKX 市场状态 · 仓位 · AI 复盘提示词",

  live: {
    title: "实盘市场状态",
    subtitle: "按需采集 · 不做自动轮询",

    // 操作与状态
    refresh: "刷新",
    refreshing: "刷新中…",
    retry: "重试",
    loading: "正在拉取市场状态…",
    loadingHint: "首次进入会向 OKX 拉取一次数据，请稍候。",

    // 数据不可得（架构铁律 4：绝不留空）
    na: "数据不可得",
    // 注意：unavailable 列表的条目直接显示 Rust 给的原文（形如「趋势：判定需要至少 200 根 K 线」），
    // 不在这里再拼一次「数据不可得：」——区块标题已经说明这件事，重复拼会变成三层前缀。

    // 新鲜度（硬要求：绝不能静默展示陈旧数据）
    freshness: {
      cached: (ago: string) => `缓存于 ${ago}`,
      cachedNote: "本次未发起网络请求",
      fresh: "刚刚拉取",
      badgeCached: "命中缓存",
      badgeFresh: "实时拉取",
      fetchedAt: (time: string) => `拉取于 ${time}`,
    },

    // 区块标题
    sections: {
      signals: "触发信号",
      unavailable: "数据不可得",
      warnings: "本次刷新的问题",
    },

    // 列表空态
    noSignals: "本次刷新未触发规则信号。",
    empty: "关注列表为空",
    emptyHint: "请先在设置中添加要观察的标的，然后点击刷新。",
    noInstruments: "本次刷新没有任何标的成功。",
    watchlistCount: (n: number) => `关注 ${n} 个标的`,

    // 错误态
    errorTitle: "拉取失败",
    errorCode: (code: string) => `错误代码：${code}`,
    errorNotRetryable: "该错误重试无法自愈，请检查配置或网络环境。",
    refreshFailedTitle: "刷新失败",
    refreshFailedHint: "下方仍展示上一次成功拉取的数据。",

    // 指标标签
    labels: {
      last: "最新价",
      change24h: "24h 涨跌",
      high24h: "24h 最高",
      low24h: "24h 最低",
      volume24h: "24h 成交额",
      fundingRate: "资金费率（当前）",
      fundingAnnualized: "资金费率（年化）",
      nextFundingRate: "资金费率（下期）",
      nextFundingTime: "下期结算时间",
      openInterest: "持仓量",
      basis: "基差",
      longShortRatio: "多空账户比",
      takerBuySellRatio: "主动买卖比",
      ema20: "EMA20",
      ema60: "EMA60",
      ema200: "EMA200",
      rsi14: "RSI14",
      atr: "ATR",
      realizedVol: "已实现波动率",
    },

    // 状态徽章（Rust 原始变体名 → 中文）
    trend: {
      label: "趋势",
      Uptrend: "趋势向上",
      Downtrend: "趋势向下",
      Range: "区间震荡",
      Transition: "趋势转换",
    },
    vol: {
      label: "波动",
      Low: "低波动",
      Normal: "波动正常",
      High: "高波动",
      Extreme: "极端波动",
    },
    crowding: {
      label: "拥挤度",
      LongCrowded: "多头拥挤",
      ShortCrowded: "空头拥挤",
      Balanced: "多空均衡",
    },

    // 相对时间（供 format.ts 的纯函数使用）
    time: {
      justNow: "刚刚",
      secondsAgo: (n: number) => `${n} 秒前`,
      minutesAgo: (n: number) => `${n} 分钟前`,
      hoursAgo: (n: number) => `${n} 小时前`,
      daysAgo: (n: number) => `${n} 天前`,
    },
  },

  // 页脚：紧凑的版本信息（M0 验收路径，保留为常驻诊断信息）
  footer: {
    version: (v: string) => `版本 ${v}`,
    core: (v: string) => `核心 ${v}`,
    schema: (v: number) => `Schema ${v}`,
    targetOs: (os: string) => `平台 ${os}`,
    loading: "正在读取版本信息…",
    fail: "版本信息不可用",
  },
} as const;
