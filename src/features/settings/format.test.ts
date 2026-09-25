/**
 * 设置页纯函数测试：字节数人类可读、行数千分位、保留策略的空值语义、
 * 清理结果（空 deleted / 文件未变小）与脱敏明细的文案。
 *
 * 这些正是「拿不到的数据必须显式说明」这条铁律最容易出错的地方，
 * 所以每一条都直接断言「不编造数字」。
 */
import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import type { TableStat } from "../../lib/types";
import {
  cleanupDeletedLines,
  cleanupFreedText,
  formatBytes,
  formatCount,
  redactionLines,
  retentionText,
} from "./format";

describe("formatBytes", () => {
  it("按 1000 进制输出人类可读的字节数", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1000)).toBe("1 KB");
    expect(formatBytes(12_300_000)).toBe("12.3 MB");
    expect(formatBytes(3_000_000_000)).toBe("3 GB");
  });

  it("非法值不伪装成数字", () => {
    expect(formatBytes(Number.NaN)).toBe("—");
    expect(formatBytes(-1)).toBe("—");
  });
});

describe("formatCount", () => {
  it("整数带千分位", () => {
    expect(formatCount(0)).toBe("0");
    expect(formatCount(999)).toBe("999");
    expect(formatCount(1234)).toBe("1,234");
    expect(formatCount(5_000_000)).toBe("5,000,000");
  });
});

describe("retentionText", () => {
  it("retention 为 null 时显示「不自动清理」，而不是留空", () => {
    expect(retentionText(null)).toBe(S.settings.cache.noRetention);
    expect(retentionText(null)).not.toBe("");
  });

  it("有保留策略时原样展示后端给的说明", () => {
    expect(retentionText("保留最近 90 天")).toBe("保留最近 90 天");
  });
});

const TABLES: TableStat[] = [
  { name: "candles", label: "K 线", rows: 0, retention: "每个序列保留最近 5000 根" },
  { name: "position_trace", label: "仓位留痕", rows: 0, retention: "保留最近 90 天" },
];

describe("cleanupDeletedLines", () => {
  it("deleted 为空时明确说「没有需要清理的数据」", () => {
    expect(cleanupDeletedLines([], TABLES)).toEqual([S.settings.cache.cleanupNothing]);
  });

  it("有删除时按中文名 + 千分位行数逐行说明", () => {
    const lines = cleanupDeletedLines(
      [
        { name: "candles", rows: 12_345 },
        { name: "unknown_table", rows: 7 },
      ],
      TABLES,
    );
    expect(lines).toEqual([
      S.settings.cache.cleanupDeleted("K 线", "12,345"),
      S.settings.cache.cleanupDeleted("unknown_table", "7"),
    ]);
  });
});

describe("cleanupFreedText", () => {
  it("文件变小了才给出回收空间的具体数字", () => {
    expect(cleanupFreedText(2_000_000, 1_000_000)).toBe(
      S.settings.cache.cleanupFreed(formatBytes(1_000_000)),
    );
  });

  it("bytes_before === bytes_after 时如实说明，不编造释放量", () => {
    const text = cleanupFreedText(2_000_000, 2_000_000);
    expect(text).toBe(S.settings.cache.cleanupNoShrink);
    // 绝不能出现「回收空间：…」这种看起来像真数字的文案
    expect(text).not.toContain("回收空间");
    expect(text).not.toContain("0 B");
  });
});

describe("redactionLines", () => {
  it("redactions 为空时显示对应文案", () => {
    expect(redactionLines([])).toEqual([S.settings.diagnostics.redactionNone]);
  });

  it("逐条展示「模式 × 次数」", () => {
    expect(
      redactionLines([{ pattern: "疑似密钥的长随机串（20 位以上字母数字）", hits: 3 }]),
    ).toEqual([S.settings.diagnostics.redactionHit("疑似密钥的长随机串（20 位以上字母数字）", 3)]);
  });
});
