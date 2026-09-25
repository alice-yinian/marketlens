/**
 * 设置页的纯格式化与映射：无副作用、不读全局状态。
 *
 * 只做「把 Rust 已经算好的事实翻译成人话」这一件事：
 * 字节数、行数、保留策略、清理结果、脱敏明细。
 * 刻意不在这里做任何推断（比如「应该释放了多少」）——拿不到就不说。
 */
import { S } from "../../lib/strings";
import type { DeletedRows, RedactionNote, TableStat } from "../../lib/types";

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * 字节数 → 人类可读（1000 进制，一位小数）。
 *
 * 用 1000 而非 1024：与 `features/live/format.formatUsd` 的 K/M/B 缩写同口径，
 * 也贴合「如 12.3 MB」这类日常读数。`12_300_000 → "12.3 MB"`、`0 → "0 B"`；
 * 负数 / 非有限值算作不可得，返回 `—`。
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < BYTE_UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  if (unit === 0) return `${Math.round(value)} B`;
  return `${value.toFixed(1).replace(/\.0$/, "")} ${BYTE_UNITS[unit]}`;
}

/** 整数千分位：`12345 → "12,345"`；非法输入返回 `—`。 */
export function formatCount(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return Math.trunc(value)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * 保留策略展示。`null` 表示这张表不自动清理——必须显式说出来，不能留空。
 */
export function retentionText(retention: string | null): string {
  return retention ?? S.settings.cache.noRetention;
}

/** 表名 → 中文名（缓存统计里查得到就用它，查不到就退回表名本身）。 */
function labelOf(name: string, tables: TableStat[]): string {
  return tables.find((table) => table.name === name)?.label ?? name;
}

/**
 * 清理结果里「删了什么」的逐行文案。
 *
 * `deleted` 为空是正常结果（本来就没有超期数据），此时明确说「没有需要清理的数据」，
 * 而不是渲染一个空列表让用户以为界面坏了。
 */
export function cleanupDeletedLines(deleted: DeletedRows[], tables: TableStat[] = []): string[] {
  if (deleted.length === 0) return [S.settings.cache.cleanupNothing];
  return deleted.map((row) =>
    S.settings.cache.cleanupDeleted(labelOf(row.name, tables), formatCount(row.rows)),
  );
}

/**
 * 清理回收的空间。
 *
 * `bytes_before === bytes_after`（或清理后反而变大）时**不编造释放量**：
 * SQLite 删除行后只是把页标记为空闲，`VACUUM` 之前文件大小可以纹丝不动。
 * 这时候如实说明原因，比给一个「已释放 0 B」或凭空估算的数字诚实。
 */
export function cleanupFreedText(bytesBefore: number, bytesAfter: number): string {
  if (!Number.isFinite(bytesBefore) || !Number.isFinite(bytesAfter)) {
    return S.settings.cache.cleanupNoShrink;
  }
  if (bytesAfter >= bytesBefore) return S.settings.cache.cleanupNoShrink;
  return S.settings.cache.cleanupFreed(formatBytes(bytesBefore - bytesAfter));
}

/**
 * 脱敏明细的逐行文案。为空时说明本次没有命中任何敏感模式——
 * 这同样是需要说出来的结论，否则用户无法区分「没脱敏」和「没什么可脱敏」。
 */
export function redactionLines(redactions: RedactionNote[]): string[] {
  if (redactions.length === 0) return [S.settings.diagnostics.redactionNone];
  return redactions.map((note) => S.settings.diagnostics.redactionHit(note.pattern, note.hits));
}
