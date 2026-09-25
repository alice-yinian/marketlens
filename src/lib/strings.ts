/**
 * 界面文案集中管理（设计文档 §6.6.4 已确认的 i18n 策略）。
 *
 * 本期不上 i18next：中文单语言。但所有界面文案必须集中在这里，
 * 不散落在组件中——将来要国际化时只需替换本文件，不动任何逻辑。
 */
export const S = {
  appName: "MarketLens",
  tagline: "OKX 市场状态 · 仓位 · AI 复盘提示词",

  // M0 骨架页
  boot: {
    title: "骨架就绪",
    subtitle: "M0 验收：双端可启动并显示版本号",
    appVersion: "应用版本",
    coreVersion: "核心版本",
    schemaVersion: "数据库 Schema 版本",
    targetOs: "运行平台",
    ipcOk: "IPC 通道正常",
    ipcFail: "IPC 通道异常",
    loading: "正在读取…",
    retry: "重试",
    migrationNote: "数据库迁移已执行",
  },
} as const;
