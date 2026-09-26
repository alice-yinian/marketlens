/**
 * 提示词页的纯格式化、校验与映射：无副作用、不读全局状态。
 *
 * 这里刻意**不** import `features/review` 的任何东西（那是别人的模块）：
 * 时段选择只借用它的交互思路，实现自成一体的精简版。
 *
 * 单位约定：时间戳一律是 Unix 毫秒。
 */
import { S } from "../../lib/strings";
import type { PrivacyLevel, PromptOutput, TemplateKind } from "../../lib/types";

/** 与 Rust `fetch::plan::MAX_RANGE_MS` 一致的 90 天硬上限 */
export const MAX_RANGE_MS = 90 * 24 * 60 * 60 * 1000;

const DAY_MS = 24 * 60 * 60 * 1000;

/** 可选的 K 线粒度（与后端 `bar_millis` 接受的取值一致） */
export const BAR_OPTIONS: readonly string[] = ["15m", "1H", "4H", "1D"];

/** 隐私等级顺序（界面按此顺序展示，默认 L1） */
export const PRIVACY_LEVELS: readonly PrivacyLevel[] = ["L0", "L1", "L2"];

/** 默认隐私等级：L1（设计文档确认）。 */
export const DEFAULT_PRIVACY: PrivacyLevel = "L1";

/** `bar` → 中文展示 */
export function barLabel(bar: string): string {
  const labels: Record<string, string> = {
    "15m": "15 分钟",
    "1H": "1 小时",
    "4H": "4 小时",
    "1D": "1 天",
  };
  return labels[bar] ?? bar;
}

/**
 * 模板类型 → 名称。
 *
 * 用 `Record` 而不是三元表达式：模板类型每加一种，三元写法都会**默默**把新类型
 * 归到 else 分支上（行情模板会被标成「复盘」），而 `Record` 会让编译期直接报错。
 */
const KIND_LABEL: Record<TemplateKind, string> = {
  live: S.prompt.templates.kindLive,
  review: S.prompt.templates.kindReview,
  market: S.prompt.templates.kindMarket,
};

export function kindLabel(kind: TemplateKind): string {
  return KIND_LABEL[kind];
}

/** 隐私等级 → 名称（含编号） */
export function privacyLabel(level: PrivacyLevel): string {
  return S.prompt.privacy[level].label;
}

/** 隐私等级 → 一句人话说明它到底做了什么 */
export function privacyDescription(level: PrivacyLevel): string {
  return S.prompt.privacy[level].desc;
}

/**
 * 隐私等级的视觉呈现。
 * L0 用琥珀色（「会带出完整金额」需要被看见），L1 中性，L2 绿色（最安全）。
 */
export function privacyBadgeClass(level: PrivacyLevel, active: boolean): string {
  const base = "flex flex-col items-start gap-1 rounded-lg border px-3 py-2 text-left";
  if (!active) {
    return `${base} border-neutral-800 bg-neutral-900/40 text-neutral-400 hover:border-neutral-700`;
  }
  const tones: Record<PrivacyLevel, string> = {
    L0: "border-amber-800 bg-amber-950/40 text-amber-200",
    L1: "border-neutral-500 bg-neutral-800 text-neutral-100",
    L2: "border-emerald-800 bg-emerald-950/40 text-emerald-200",
  };
  return `${base} ${tones[level]}`;
}

// ---------------------------------------------------------------------------
// 时段校验（与后端同口径，但前置在 UI：提前告诉用户为什么不能继续）
// ---------------------------------------------------------------------------

export type RangeProblem = "invalid" | "order" | "tooLarge";

/** 校验时段。返回 null 表示可以生成预览。 */
export function rangeError(from: number, to: number): RangeProblem | null {
  if (!Number.isFinite(from) || !Number.isFinite(to)) return "invalid";
  if (to <= from) return "order";
  if (to - from > MAX_RANGE_MS) return "tooLarge";
  return null;
}

/** 时段天数（向上取整，用于提示「当前约 N 天」） */
export function rangeDays(from: number, to: number): number {
  if (!Number.isFinite(from) || !Number.isFinite(to) || to <= from) return 0;
  return Math.ceil((to - from) / DAY_MS);
}

/** 时段问题的中文说明 */
export function rangeProblemText(problem: RangeProblem): string {
  if (problem === "order") return S.prompt.context.rangeOrder;
  if (problem === "tooLarge") return S.prompt.context.rangeTooLarge;
  return S.prompt.context.rangeInvalid;
}

/** Unix 毫秒 → `<input type="datetime-local">` 的本地时间字符串 */
export function msToLocalInput(ms: number): string {
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

/** `<input type="datetime-local">` 的值 → Unix 毫秒；无法解析返回 null */
export function localInputToMs(value: string): number | null {
  if (value.length === 0) return null;
  const ms = new Date(value).getTime();
  return Number.isFinite(ms) ? ms : null;
}

// ---------------------------------------------------------------------------
// 保存前的本地校验
// ---------------------------------------------------------------------------

/** 保存前本地校验的问题码；返回 null 表示可以提交。 */
export type TemplateProblem = "nameRequired" | "bodyRequired";

/**
 * 校验模板名称与正文。
 * 名称与正文都按**去空白后非空**判断（与后端 `trim().is_empty()` 同口径）。
 */
export function templateProblem(name: string, body: string): TemplateProblem | null {
  if (name.trim().length === 0) return "nameRequired";
  if (body.trim().length === 0) return "bodyRequired";
  return null;
}

/** 校验问题的中文说明 */
export function templateProblemText(problem: TemplateProblem): string {
  return problem === "nameRequired" ? S.prompt.actions.nameRequired : S.prompt.actions.bodyRequired;
}

/**
 * 「另存为」时使用的名称。
 *
 * 从内置模板另存时，若名称未改动就追加「（副本）」，让新模板有个可区分的名字
 * （后端会生成新 id；这里只是让列表里不出现两个同名项）。
 */
export function saveAsName(draftName: string, sourceName: string | null): string {
  const name = draftName.trim();
  if (sourceName !== null && name === sourceName.trim()) {
    return `${name}${S.prompt.actions.saveAsCopySuffix}`;
  }
  return name;
}

// ---------------------------------------------------------------------------
// 时间与导出文件名
// ---------------------------------------------------------------------------

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** 本地时间 YYYY-MM-DD HH:mm:ss */
export function formatTimestamp(ms: number): string {
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return S.live.na;
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(
    d.getHours(),
  )}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

/** 紧凑时间戳 YYYYMMDD-HHmmss（用于文件名） */
export function compactTimestamp(ms: number): string {
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "unknown";
  return `${d.getFullYear()}${pad2(d.getMonth() + 1)}${pad2(d.getDate())}-${pad2(
    d.getHours(),
  )}${pad2(d.getMinutes())}${pad2(d.getSeconds())}`;
}

/**
 * 导出文件名：`{模板名}-{隐私等级}-{时间戳}.md`。
 *
 * 模板名里的路径分隔符等非法字符会被替换成 `_`，避免在 Windows 上写出界。
 * 名称为空时回落到 `template`。
 */
export function exportFileName(name: string, privacy: PrivacyLevel, ms: number): string {
  const safe = name.trim().replace(/[\\/:*?"<>|\s]+/g, "_");
  const base = safe.length > 0 ? safe : "template";
  return `${base}-${privacy}-${compactTimestamp(ms)}.md`;
}

// ---------------------------------------------------------------------------
// 预览统计
// ---------------------------------------------------------------------------

export interface OutputStats {
  tokens: number;
  chars: number;
}

/** 从 `PromptOutput` 取出 token 估算与字符数；非法输入回落为 0。 */
export function outputStats(output: Pick<PromptOutput, "token_estimate" | "char_count">): OutputStats {
  return {
    tokens: Number.isFinite(output.token_estimate) ? output.token_estimate : 0,
    chars: Number.isFinite(output.char_count) ? output.char_count : 0,
  };
}
