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
    kline: "行情",
    account: "账户",
    review: "复盘",
    prompt: "提示词",
    settings: "设置",
    configure: "重新配置",
  },

  // 启动期（读取密钥库状态）的占位与错误
  boot: {
    loading: "正在读取密钥库状态…",
    errorTitle: "启动失败",
  },

  // 首次启动引导（设计文档 §6.9 的四步流程）
  onboarding: {
    title: "首次使用引导",
    subtitle: "四步完成初始化：密钥库 → 网络代理 → OKX 凭据 → 关注标的",
    unlockTitle: "解锁密钥库",
    unlockSubtitle: "输入主密码以继续。",
    reconfigureTitle: "重新配置",
    reconfigureSubtitle: "更新网络代理、OKX 凭据或关注标的。",
    stepIndicator: (n: number, total: number) => `第 ${n} / ${total} 步`,
    finishing: "正在完成引导…",
    steps: {
      vault: "创建 / 解锁密钥库",
      proxy: "网络代理",
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

    // 第 2 步：网络代理。放在凭据之前，是因为受限网络下「测 key」与「拉标的」
    // 都会失败，而失败信息不会指向真正的原因（网络到不了 OKX）。
    proxy: {
      title: "网络代理（可选）",
      hint:
        "如果你的网络无法直接访问 OKX，请在这里填写代理。保存后立即生效——下一步的凭据测试与之后的行情拉取都会走它，不需要重启。",
      url: "代理地址",
      placeholder: "http://127.0.0.1:7890",
      schemes: "支持 http / https / socks5 / socks5h。本地代理通常不需要用户名密码。",
      credentialWarning:
        "地址里若带 `user:pass`，该凭据会随设置明文存在本机——代理必须在密钥库解锁前就生效，因此不能放进密钥库。",
      saved: (url: string) => `当前已保存：${url}`,
      none: "当前未配置代理（沿用系统 / 环境变量代理）",
      test: "保存并测试",
      testing: "正在保存并测试…",
      ok: (ms: number) => `代理连通，往返 ${ms} ms`,
      failTitle: "代理不通",
      failHint:
        "请确认代理进程已启动、端口正确，且地址形如 `http://127.0.0.1:7890`；也检查代理节点本身能否访问 OKX。",
      serverTime: (time: string) => `OKX 服务器时间：${time}`,
      next: "下一步",
      skip: "不使用代理",
      skipHint:
        "不填时会沿用系统 / 环境变量代理（Windows 系统代理、HTTP(S)_PROXY、ALL_PROXY）；系统也没配才是直连。",
      loadErrorTitle: "读取代理配置失败",
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

  // 行情页：K 线 + 逐根指标 → 给 AI 的分析提示词
  kline: {
    title: "行情分析",
    subtitle: "选标的与周期，把原始 K 线与指标交给 AI",

    // 标的选择
    instrument: "标的",
    instrumentEmpty: "关注列表为空",
    instrumentEmptyHint: "请先在设置里添加要观察的标的，然后回到这里。",
    instrumentLoading: "正在读取关注列表…",
    instrumentErrorTitle: "读取关注列表失败",

    // 周期与根数
    bars: "周期",
    barsHint: "可选多个周期；一套模板会按顺序把每个周期都写进提示词。",
    barsLoading: "正在读取可用周期…",
    barsErrorTitle: "读取可用周期失败",
    barsEmpty: "应用没有返回任何可用周期，请重启后再试。",
    count: "每个周期根数",
    totalNote: (total: number, max: number) => `合计 ${total} / ${max} 根`,

    // 指标
    indicators: "指标（可选，逐根附列）",
    indicatorsHint:
      "勾选的指标会给每一根 K 线加一列。周期越长，需要的样本越多——历史不够时那一列会是 `—`。",
    indicatorKind: "指标",
    indicatorPeriod: "周期",
    addIndicator: "添加指标",
    removeIndicator: "删除",
    indicatorDuplicate: "同一个指标与周期只保留一个，重复项已忽略。",
    indicatorLimit: (max: number) => `最多 ${max} 个指标。`,
    indicatorPeriodRange: (max: number) => `周期需在 1 ~ ${max} 之间。`,
    noIndicators: "未选择指标：只给 AI 原始 K 线。",
    indicatorKinds: {
      ema: "指数移动平均（EMA）",
      rsi: "相对强弱（RSI）",
      atr_pct: "真实波幅占价格比（ATR%）",
      realized_vol: "已实现波动率（按周期年化）",
    },

    // 计划
    plan: "生成计划",
    planning: "生成中…",
    planTitle: "本次取数",
    planErrorTitle: "计划生成失败",
    planRequests: (n: number) => `预计 ${n} 个请求`,
    planDuration: (text: string) => `预计耗时 ${text}`,
    planTokens: (n: number) => `提示词约 ${n} token（不含指标列）`,
    planBars: "周期",
    planRows: "根数",
    planRange: "覆盖范围",
    planPages: "页数",
    planWarnings: "提醒",

    // 采集
    fetch: "开始取数",
    fetching: "取数中…",
    fetchHint: "已收盘的 K 线会永久缓存，第二次打开同一份参数不会再联网。",
    cachedNote: "数据已在本地：直接生成提示词即可，不会发起请求。",

    // 生成提示词
    build: "生成提示词",
    building: "生成中…",
    privacyNote: "行情数据是公开数据，不受隐私分级影响，因此这里没有隐私等级选项。",

    errors: {
      planTitle: "计划生成失败",
      emptyBars: "至少选择 1 个周期。",
      emptyInstrument: "请选择标的。",
      countRange: (max: number) => `每个周期的根数需在 1 ~ ${max} 之间。`,
      tooManyBars: (max: number) => `最多选择 ${max} 个周期。`,
      tooManyCandles: (total: number, max: number) =>
        `合计 ${total} 根超出上限 ${max} 根：减少周期数或降低每周期根数。`,
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

  // 复盘采集页（M3）
  review: {
    title: "复盘采集",
    subtitle: "先看预估与可得性，再开始采集",

    // 时段与粒度
    range: {
      title: "时段与粒度",
      from: "开始时间",
      to: "结束时间",
      last24h: "最近 24 小时",
      quick: "快捷时段",
      last7d: "最近 7 天",
      last30d: "最近 30 天",
      rangeDays: (days: number) => `当前约 ${days} 天`,
      maxRangeNote: "时段上限 90 天：超出后无法生成计划（不会自动截断时段）。",
      bar: "K 线粒度",
      tooLarge: "当前时段超过 90 天上限。请缩小范围——本工具不会静默截断时段。",
      orderInvalid: "结束时间必须晚于开始时间。",
      invalid: "请选择有效的开始与结束时间。",
    },

    // 计划预估
    plan: {
      title: "采集计划预估",
      offlineNote: "生成计划不联网，只做校验与估算。",
      generate: "生成计划",
      generating: "生成中…",
      estRequests: "预估请求数",
      estRequestsValue: (n: number) => `${n} 个`,
      estDuration: "预估耗时",
      insts: "标的集",
      instsCount: (n: number) => `${n} 个标的`,
      barLabel: "粒度",
      seriesCount: (n: number) => `共 ${n} 条序列`,
      warningsTitle: "可得性提示：以下数据无法获取",
      warningsHint: "这些指标在本时段内拿不到，界面上会如实标注，不会留空。",
      start: "开始采集",
    },

    // 复盘装配（M4）：采集完成后把原始数据算成结论
    assemble: {
      title: "装配复盘",
      subtitle: "合并官方历史仓位与本地留痕，补上开仓时刻的市场状态，再统计。",
      note: "先采集能拿到更完整的归因数据；未采集也可以装配，缺失项会在下方「装配过程中的问题」里如实说明。",
      rangeNote: "装配使用当前时段的起止时间与粒度。",
      run: "装配复盘",
      running: "装配中…",
      runningHint: "会联网同步官方历史仓位并做归因与统计，可能耗时数秒。",
      needCredential: "尚未添加 OKX 凭据，无法装配复盘。",
      needCredentialHint: "装配需要读取官方历史仓位。请先在引导中添加只读凭据，再回到本页。",
      addCredential: "去添加凭据",
      credentialLoading: "正在读取凭据…",
      credentialError: "凭据列表读取失败，暂时无法装配。",
      retryCredentials: "重试",
      noPositionsYet: "本次装配没有返回任何仓位。",
    },

    // 进度
    progress: {
      title: "采集进行中",
      seriesProgress: (done: number, total: number) => `已完成 ${done} / ${total} 条序列`,
      current: (label: string) => `正在拉取：${label}`,
      requests: (done: number, total: number) => `请求 ${done} / ${total}`,
      starting: "正在启动…",
      cancel: "取消",
      cancelling: "取消中…",
      cancelRequested: "已请求取消，等待当前请求结束…",
      cancelAlreadyDone: "该计划已结束。",
    },

    // 采集结果
    report: {
      title: "采集结果",
      elapsed: (text: string) => `耗时 ${text}`,
      doneRequests: (n: number) => `共完成 ${n} 个请求`,
      cancelled: "已取消：已完成的序列已保存，再次采集会跳过它们。",
      rows: "行数",
      pages: "页数",
    },

    // 序列种类（Rust 变体名 → 中文）
    seriesKind: {
      Candles: "K 线",
      MarkCandles: "标记价 K 线",
      IndexCandles: "指数价 K 线",
      FundingRate: "资金费率",
      LongShortRatio: "多空账户比",
      TakerVolume: "主动买卖量",
    },

    // 序列状态（Rust 变体名 → 中文；四态视觉各异）
    status: {
      Ok: "正常",
      Unavailable: "不可得",
      Failed: "失败",
      Skipped: "已跳过（上次已完成）",
    },

    // 复盘结果展示（M4）
    result: {
      title: "复盘结果",
      range: (text: string) => `时段：${text}`,
      bar: (label: string) => `粒度：${label}`,
      positionsCount: (n: number) => `共 ${n} 笔仓位`,
      empty: "该时段没有已平仓位记录。",
      emptyHint: "时段内可能没有交易，或官方历史与本地留痕都未覆盖到。",

      // a) 统计概览
      overviewTitle: "统计概览",
      winRate: "胜率",
      profitFactor: "盈亏比",
      /** profit_factor 为 null 时按 stats 可区分的原因给出精确文案（绝不显示 0 或 ∞） */
      profitFactorNoTrades: "无交易记录",
      profitFactorNoWins: "无盈利笔",
      profitFactorNoLosses: "无亏损笔",
      profitFactorUnavailable: "无法计算",
      expectancy: "期望值（每笔）",
      totalPnl: "总盈亏",
      avgHold: "平均持仓时长",
      feeDrag: "费用侵蚀",
      feeDragHint: "手续费 + 资金费 ÷ |已实现盈亏|：利润被交易成本吃掉的比例。",
      feeDragUnavailable: "无法计算（无已实现盈亏）",
      feeDragWarning: "交易成本吃掉了相当一部分利润，值得优先改进。",

      // b) 按开仓时市场状态分组
      trendTitle: "按开仓时市场状态",
      trendHint: "同样的方向，在不同市场状态下的胜率可能完全不同——这是复盘的核心。",
      groupUnattributed: "未归因",
      groupUnattributedNote: "开仓时状态不可得，无法归类",
      groupCount: (n: number) => `${n} 笔`,
      colGroup: "分组",
      colCount: "笔数",
      colWinRate: "胜率",
      colPnl: "合计盈亏",

      // c) 逐笔仓位
      positionsTitle: "逐笔仓位",
      direction: { long: "多", short: "空", unknown: "未知" },
      source: { okx: "官方", local: "本地", merged: "官方+本地", unknown: "来源未知" },
      precision: { exact: "时间精确", approximate: "时间近似", unknown: "精度未知" },
      lever: "杠杆",
      openTime: "开仓时间",
      closeTime: "平仓时间",
      openPx: "开仓均价",
      closePx: "平仓均价",
      realizedPnl: "已实现盈亏",
      fee: "手续费",
      fundingFee: "资金费",
      regimeTitle: "开仓时市场状态",
      /** regime 为 null：明确写不可得，绝不留空 */
      regimeUnavailable: "开仓时状态不可得",
      change24h: "开仓前 24h 涨跌",
      extremeTitle: "持仓极值",
      maxFavorable: "最大浮盈",
      maxAdverse: "最大浮亏",
      extremeUnavailable: "需本地留痕",

      // d) 数据可信度
      trustTitle: "数据可信度",
      mergeLabel: "双源合并",
      mergeCounts: (official: number, local: number, enriched: number) =>
        `官方 ${official} 笔 · 仅本地 ${local} 笔 · 被本地补充极值 ${enriched} 笔`,
      traceLabel: "本地留痕覆盖度",
      unattributedNote: (n: number) =>
        `有 ${n} 笔拿不到开仓时的市场状态（多为官方历史窗口之外或缺少采集数据），已归入「未归因」。`,

      // e) warnings
      warningsTitle: "装配过程中的问题",
      warningsHint: "以下问题不致命，但会影响复盘结论的完整性，请留意。",
    },

    // 时长（预估与实测共用）
    duration: {
      underSecond: "不足 1 秒",
      seconds: (n: number) => `约 ${n} 秒`,
      minutes: (n: number) => `约 ${n} 分钟`,
    },

    // 平均持仓时长（天/小时/分钟）
    hold: {
      daysHours: (d: number, h: number) => `${d} 天 ${h} 小时`,
      days: (d: number) => `${d} 天`,
      hoursMinutes: (h: number, m: number) => `${h} 小时 ${m} 分钟`,
      hours: (h: number) => `${h} 小时`,
    },

    // 错误态
    errors: {
      planTitle: "生成计划失败",
      fetchTitle: "采集失败",
      assembleTitle: "装配复盘失败",
      rangeTooLargeTitle: "时段超出上限",
      rangeTooLarge:
        "时段最多 90 天。请缩小范围后重试——本工具不会静默截断时段。",
      assembleRangeTooLarge:
        "装配时段最多 90 天。请缩小范围后重试——本工具不会静默截断时段。",
      retry: "重试",
    },
  },

  // 提示词页（M5）：模板库 + 隐私分级 + 编辑器 + 实时预览 + 导出
  prompt: {
    title: "提示词库",
    subtitle: "管理全部提示词模板（实盘 / 复盘 / 行情）——生成提示词在各自的页面",

    // 各页面里的「生成提示词」区（实盘页 / 复盘页共用）
    generate: {
      title: "生成提示词",
      subtitle: "选一个模板，用当前页面的上下文生成一份可直接粘给 AI 的提示词",
      template: "模板",
      noTemplate:
        "还没有这一类模板：可以到「提示词库」新建一个，或把内置模板「另存为」一份副本。",
      privacyNote: (label: string) => `隐私等级：${label}（在设置页统一调整）`,
      run: "生成提示词",
      running: "生成中…",
      manage: "管理模板",
    },

    // 模板语法校验（编辑模板时；不需要上下文，因此也不联网）
    check: {
      ok: (count: number) => `语法正常，引用了 ${count} 个变量。`,
      variables: (names: string) => `引用变量：${names}`,
      errorTitle: "模板语法错误",
    },

    // 模板库
    templates: {
      title: "模板库",
      loading: "正在读取模板…",
      empty: "没有可用模板。",
      error: "模板列表读取失败。",
      retry: "重试",
      builtinBadge: "内置",
      userBadge: "自定义",
      kindLive: "实盘",
      kindReview: "复盘",
      kindMarket: "行情",
      builtinNote: "内置模板由应用维护，升级时会被更新；改动请用「另存为」。",
    },

    // 模板编辑器
    editor: {
      title: "模板编辑器",
      name: "模板名称",
      namePlaceholder: "例如：我的实盘速览",
      description: "说明（可选）",
      descriptionPlaceholder: "一句话说明这个模板问什么",
      body: "模板正文",
      builtinHint: "内置模板不可直接保存，改动请用「另存为」创建副本。",
      checkTitle: "语法校验",
      checkEmpty: "正文为空时不做校验。",
      checking: "校验中…",
      dirty: "有未保存的改动",
    },

    // 保存 / 另存为 / 删除
    actions: {
      save: "保存",
      saving: "保存中…",
      saveAs: "另存为",
      saveAsNote: "内置模板只能另存为：会以当前正文创建一个新的自定义模板。",
      saveAsCopySuffix: "（副本）",
      delete: "删除",
      deleting: "删除中…",
      deleteConfirm: "确认删除",
      deleteCancel: "取消",
      deleteConfirmNote: "删除后无法恢复。",
      nameRequired: "模板名称不能为空。",
      bodyRequired: "模板正文不能为空。",
      saveFailed: "保存失败",
      deleteFailed: "删除失败",
    },

    // 实盘 / 复盘上下文输入
    context: {
      liveTitle: "实盘上下文",
      reviewTitle: "复盘上下文",
      credential: "凭据",
      noCredential: "不选择凭据（仅行情）",
      credentialNote: "不选凭据时只有市场状态，账户与仓位会明确标注为「数据不可得」。",
      credentialLoading: "正在读取凭据…",
      credentialError: "凭据列表读取失败。",
      force: "强制刷新（跳过行情 30 秒缓存）",
      forceNote: "开启后每次重新生成都会重新联网拉取行情。",
      rangeTooLarge: "当前时段超过 90 天上限。请缩小范围——本工具不会静默截断时段。",
      needCredential: "复盘需要读取官方历史仓位，必须选择凭据。",
    },

    // 实时预览
    preview: {
      title: "预览",
      firstLoading: "正在生成预览…",
      refreshing: "正在按新设置重新生成…",
      empty: "尚无预览。",
      textTitle: "提示词正文",
      tokens: (n: number) => `token 估算：${n}`,
      chars: (n: number) => `字符数：${n}`,
      tokenNote: "token 估算为保守高估（中文约 1 token/字，ASCII 约 4 字符/token）。",
      warningsTitle: "生成过程中的问题",
      warningsHint: "以下问题不致命，但说明提示词里可能缺少部分信息，请留意。",
      errorTitle: "生成失败",
      errorHintTemplate:
        "请按上面的报错修改模板：变量名与语法位置都在消息里，这是唯一的线索。",
      errorHintOther: "这条不是模板问题（看错误代码）：消息里已写明缺什么，照它处理即可。",
      rangeTooLargeTitle: "时段超出上限",
      generatedAt: (time: string) => `生成于 ${time}`,
      templateName: (name: string) => `模板：${name}`,
      unsavedName: "（未保存的模板）",
    },

    // 导出
    export: {
      title: "导出",
      copy: "复制到剪贴板",
      copied: "已复制",
      copyFailed: "复制失败，请手动选择文本复制。",
      saveFile: "保存为文件",
      savedFile: (name: string) => `已保存：${name}`,
      noPreview: "还没有可导出的内容。",
      l0WarningTitle: "将包含完整金额",
      l0Warning:
        "当前为 L0（全量）：导出的提示词会包含完整金额与数量。确认导出？",
      confirm: "确认导出",
      cancel: "取消",
    },

    // 错误态
    errors: {
      listTitle: "模板列表读取失败",
      buildTitle: "生成提示词失败",
      retry: "重试",
    },
  },

  // 设置页（M6）：缓存管理 + 诊断导出 + 版本信息
  settings: {
    title: "设置",
    subtitle: "网络代理 · 缓存管理 · 诊断导出 · 版本信息",

    // 界面主题（全局）：日间 / 夜间 / 跟随系统
    theme: {
      title: "界面主题",
      subtitle: "全局生效：日间 · 夜间 · 跟随系统",
      system: "跟随系统",
      systemDesc: "随操作系统的深色模式自动切换（默认）",
      light: "日间",
      lightDesc: "浅色背景，适合明亮环境",
      dark: "夜间",
      darkDesc: "深色背景，适合暗环境",
      current: (label: string) => `当前：${label}`,
      /** 跟随系统时补一句「实际是日间还是夜间」，否则用户没法知道现在到底哪一档 */
      currentFollowed: (resolved: string) => `当前：跟随系统（实际：${resolved}）`,
      saveFailed: "主题保存失败，请重试。",
    },

    // 隐私等级（全局）：实盘 / 复盘 / 行情三条管线都读它
    privacy: {
      title: "隐私等级",
      subtitle: "全局生效：三条提示词管线用同一个等级，不必在各页面分别维护",
      enforcedNote:
        "隐私分级在后端强制生效：无论模板怎么写，都无法绕过它把金额带进提示词。改完等级，各页面的提示词会按新等级重新生成。",
      current: (label: string) => `当前：${label}`,
      saveFailed: "隐私等级保存失败，请重试。",
      L0: { label: "L0 · 全量", desc: "完整金额与数量都会进入提示词" },
      L1: { label: "L1 · 百分比（默认）", desc: "金额转成占账户权益的百分比，权益只给量级区间" },
      L2: { label: "L2 · 脱敏", desc: "只保留方向、杠杆与市场状态，不含任何金额与数量" },
    },

    // 0) 网络代理（引导里配过一次，这里是随时可改的入口）
    proxy: {
      title: "网络代理",
      subtitle: "保存后立即生效，不需要重启应用",
      url: "代理地址",
      placeholder: "http://127.0.0.1:7890",
      schemes: "支持 http / https / socks5 / socks5h；留空表示不使用显式代理。",
      credentialWarning:
        "地址里若带 `user:pass`，该凭据会随设置明文存在本机（代理不能放进密钥库：它要在解锁前就生效）。",
      save: "保存",
      saving: "保存中…",
      saved: "已保存，后续请求立即走新代理。",
      ok: (ms: number) => `代理连通，往返 ${ms} ms`,
      failTitle: "代理不通",
      failHint: "请确认代理进程已启动、端口正确，并检查代理节点能否访问 OKX。",
      serverTime: (time: string) => `OKX 服务器时间：${time}`,
      loadErrorTitle: "读取代理配置失败",
      saveErrorTitle: "保存失败",
    },

    // 1) 缓存管理
    cache: {
      title: "缓存管理",
      subtitle: "查看本地数据库占用，并按保留策略清理",
      refresh: "刷新",
      refreshing: "刷新中…",
      loading: "正在读取缓存统计…",
      errorTitle: "缓存统计读取失败",
      databaseSize: "数据库总大小",
      columns: { label: "表", rows: "行数", retention: "保留策略" },
      /** retention 为 null 时的展示（不自动清理的表） */
      noRetention: "不自动清理",

      cleanup: "清理缓存",
      cleaning: "清理中…",
      /** 二次确认：必须逐条说清会删什么，且声明不可撤销 */
      confirmTitle: "确认清理缓存？",
      confirmBody:
        "仓位留痕保留最近 90 天；K 线与指标每个序列保留最近 5000 点；提示词生成记录保留最近 200 条。此操作不可撤销。",
      confirm: "确认清理",
      cancel: "取消",

      reportTitle: "清理结果",
      /** deleted 为空时的展示 */
      cleanupNothing: "没有需要清理的数据。",
      cleanupDeleted: (label: string, rows: string) => `${label}：删除 ${rows} 行`,
      cleanupFreed: (text: string) => `回收空间：${text}`,
      /** 删了行但文件没变小：如实说明，不编造释放量 */
      cleanupNoShrink: "删除了行但文件大小未变（SQLite 会复用空闲页）。",
      cleanupErrorTitle: "清理失败",
    },

    // 2) 诊断导出
    diagnostics: {
      title: "诊断导出",
      subtitle: "一键导出脱敏诊断包，并同时落盘",
      export: "导出诊断包",
      exporting: "导出中…",
      errorTitle: "导出失败",

      resultTitle: "导出结果",
      path: "落盘路径",
      copyPath: "复制路径",
      copied: "已复制",
      copyFailed: "复制失败，请手动选择文本复制。",
      size: (text: string) => `文件大小：${text}`,
      generatedAt: (time: string) => `生成时间：${time}`,
      copyJson: "复制诊断包 JSON",
      logFile: (path: string) => `日志文件：${path}`,

      // 脱敏说明（三道结构性防线，逐条具体说明）
      redactionTitle: "脱敏说明",
      redactionIntro: "诊断包已脱敏，由三道结构性防线保证：",
      redactionNoKeys: "不含密钥：凭据只给数量与权限属性，连打码后的 API Key 都没有。",
      redactionNoRows: "不含账户与仓位明细：只统计行数，不导出任何业务数据行。",
      redactionLogs: "日志已逐行脱敏：长随机串 / 家目录路径 / 凭据赋值都会被替换。",

      // redactions 明细（让用户能自己核对）
      redactionsTitle: "本次脱敏明细",
      redactionNone: "本次没有需要脱敏的内容。",
      redactionHit: (pattern: string, hits: number) => `${pattern} × ${hits}`,

      // 凭据概览
      credentialsTitle: "凭据概览",
      credentialsCount: (n: number) => `共 ${n} 条凭据`,
      credentialsNone: "没有已保存的凭据。",
      credentialsNote: "仅数量与权限属性，不含任何密钥信息。",

      // 日志尾部（已脱敏）
      logTitle: "日志尾部（已脱敏）",
      logShow: "展开",
      logHide: "收起",

      // 版本信息
      appTitle: "版本信息",
      appVersion: "应用版本",
      coreVersion: "核心版本",
      schemaVersion: "Schema 版本",
      targetOs: "平台",
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
