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

  // 主界面顶部标签（已解锁后）
  nav: {
    live: "实盘",
    account: "账户",
    configure: "重新配置",
  },

  // 启动期（读取密钥库状态）的占位与错误
  boot: {
    loading: "正在读取密钥库状态…",
    errorTitle: "启动失败",
  },

  // 首次启动引导（设计文档 §6.9 的三步流程）
  onboarding: {
    title: "首次使用引导",
    subtitle: "三步完成初始化：密钥库 → OKX 凭据 → 关注标的",
    unlockTitle: "解锁密钥库",
    unlockSubtitle: "输入主密码以继续。",
    reconfigureTitle: "重新配置",
    reconfigureSubtitle: "更新 OKX 凭据或关注标的。",
    stepIndicator: (n: number, total: number) => `第 ${n} / ${total} 步`,
    finishing: "正在完成引导…",
    steps: {
      vault: "创建 / 解锁密钥库",
      credential: "录入 OKX 凭据",
      watchlist: "选择标的",
    },
    next: "下一步",
    errorCode: (code: string) => `错误代码：${code}`,

    vault: {
      createTitle: "创建密钥库",
      createHint:
        "密钥库用于在本机加密保存 OKX API 凭据。主密码只在本地派生密钥，不会上传到任何服务器。",
      unlockTitle: "解锁密钥库",
      unlockHint: "输入主密码以解密已保存的凭据。",
      password: "主密码",
      confirm: "确认主密码",
      empty: "主密码不能为空。",
      mismatch: "两次输入的密码不一致。",
      irrecoverableTitle: "主密码无法找回",
      irrecoverable:
        "主密码无法找回，丢失即无法读取已存凭据。请自行妥善备份——本应用没有任何后门或恢复通道。",
      ack: "我已了解：主密码丢失后无法恢复",
      create: "创建并解锁",
      unlock: "解锁",
      working: "处理中…",
    },

    credential: {
      title: "录入 OKX 只读凭据",
      hint:
        "建议使用只读 API Key。凭据仅保存在本机密钥库中，界面只显示掩码，不提供任何查看明文的功能。",
      label: "备注名",
      labelPlaceholder: "例如：主账户只读",
      apiKey: "API Key",
      secretKey: "Secret Key",
      passphrase: "Passphrase",
      demo: "模拟盘",
      demoHint: "开启后请求会带模拟盘标记（x-simulated-trading）。",
      save: "保存并测试",
      saving: "保存中…",
      testing: "测试中…",
      required: "备注名、API Key、Secret Key、Passphrase 都不能为空。",
      skipNote: "可跳过：跳过也能只看行情（不连接账户）。",
      skip: "跳过（仅看行情）",
      testTitle: "凭据测试结果",
      probeOk: "凭据可用",
      probeFail: "凭据不可用",
      readOnly: "该密钥为只读权限。",
      readOnlyFalseTitle: "该密钥带交易权限",
      readOnlyFalse:
        "该密钥带交易权限。本应用在类型层面不存在任何私有 POST 能力，因此无法用它下单；但建议改用只读密钥。",
      uid: "UID",
      posMode: "持仓模式",
      permissions: "权限",
      retryTest: "重试测试",
      saved: "已保存，元数据如下（明文密钥不回传前端）。",
    },

    watchlist: {
      title: "选择关注标的",
      hint: "按 24h 成交额排序的 Top 20 永续合约，已预勾选 BTC / ETH / SOL，可自由增删。",
      loading: "正在拉取候选标的…",
      empty: "没有可用候选标的。",
      count: (n: number, max: number) => `当前 ${n} / ${max}`,
      needOne: "至少选择 1 个标的。",
      tooMany: (max: number) =>
        `最多选择 ${max} 个标的。标的数会直接乘进复盘请求量（3 个标的约 22 个请求，10 个就到 70+），超限无法提交。`,
      volume: "24h 成交额",
      submit: "保存并进入主界面",
      saving: "保存中…",
    },
  },

  // 账户 / 仓位页（M2）
  account: {
    title: "账户与仓位",
    subtitle: "按需采集 · 不做自动轮询",

    refresh: "刷新",
    refreshing: "刷新中…",
    retry: "重试",
    loading: "正在拉取账户快照…",
    loadingHint: "会向 OKX 拉取一次账户概览与全部持仓。",

    credential: "凭据",
    noCredentials: "尚未添加任何凭据。",
    noCredentialsHint:
      "请先在首次启动引导中添加 OKX 只读凭据，然后回到本页刷新。",
    addCredential: "去添加凭据",
    envLive: "实盘",
    envDemo: "模拟盘",

    overviewTitle: "账户概览",
    positionsTitle: "持仓",
    noPositions: "当前没有持仓。",
    currenciesTitle: "币种明细",
    traceTitle: "本地留痕覆盖度",
    traceGap: "数据不连续，持仓时长可能不精确。",
    warningsTitle: "本次刷新的问题",
    fetchedAt: (time: string) => `拉取于 ${time}`,
    positionsCount: (n: number) => `共 ${n} 个持仓`,

    // 概览字段
    overview: {
      totalEq: "总权益",
      availEq: "可用权益",
      upl: "未实现盈亏",
      mgnRatio: "保证金率",
      notional: "名义价值",
      posMode: "持仓模式",
    },

    // 币种明细列
    currency: {
      ccy: "币种",
      eq: "权益",
      eqUsd: "权益（USD）",
      availBal: "可用",
      cashBal: "现金",
    },

    // 持仓字段
    position: {
      side: "方向",
      mgnMode: "保证金模式",
      lever: "杠杆",
      contracts: "张数",
      sizeBase: "币数量",
      avgPx: "均价",
      markPx: "标记价",
      liqPx: "强平价",
      notional: "名义价值",
      upl: "未实现盈亏",
      uplRatio: "收益率",
      mgnRatio: "保证金率",
    },

    // Rust 原始枚举/字符串 → 中文
    posMode: {
      net_mode: "净持仓",
      long_short_mode: "长空双向",
    },
    posSide: {
      long: "多头",
      short: "空头",
      net: "净持仓",
    },
    mgnMode: {
      cross: "全仓",
      isolated: "逐仓",
    },
    unknown: "未知",

    // 数据不可得 / 空值
    na: "数据不可得",
    liqNone: "无",

    // 错误态
    errorTitle: "拉取失败",
    errorCode: (code: string) => `错误代码：${code}`,
    errorNotRetryable: "该错误重试无法自愈，请检查配置或网络环境。",
    refreshFailedTitle: "刷新失败",
    refreshFailedHint: "下方仍展示上一次成功拉取的数据。",
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
