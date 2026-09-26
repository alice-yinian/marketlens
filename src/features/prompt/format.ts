/**
 * 模板库与生成面板共用的纯格式化、校验与映射：无副作用、不读全局状态。
 *
 * **时段与粒度的校验不在这里**（那属于复盘页自己的 `RangePicker`）——
 * 同一个概念在两处各有一份实现，迟早会出现「一边放行、一边拒绝」。
 *
 * 这里刻意**不** import `features/review` 的任何东西（那是别人的模块）：
 * 时段选择只借用它的交互思路，实现自成一体的精简版。
 *
 * 单位约定：时间戳一律是 Unix 毫秒。
 */
import { S } from "../../lib/strings";
import type { PrivacyLevel, PromptOutput, TemplateKind } from "../../lib/types";

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
