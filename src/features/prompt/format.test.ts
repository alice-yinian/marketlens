import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import {
  DEFAULT_PRIVACY,
  MAX_RANGE_MS,
  PRIVACY_LEVELS,
  exportFileName,
  localInputToMs,
  msToLocalInput,
  outputStats,
  privacyDescription,
  privacyLabel,
  rangeDays,
  rangeError,
  rangeProblemText,
  saveAsName,
  templateProblem,
} from "./format";

describe("隐私等级", () => {
  it("三档齐全，默认 L1", () => {
    expect(PRIVACY_LEVELS).toEqual(["L0", "L1", "L2"]);
    expect(DEFAULT_PRIVACY).toBe("L1");
  });

  it("每档都有名称与「做了什么」的人话说明，且互不相同", () => {
    const descs = PRIVACY_LEVELS.map((level) => {
      const desc = privacyDescription(level);
      expect(desc.length).toBeGreaterThan(0);
      expect(privacyLabel(level)).toContain(level);
      return desc;
    });
    const [l0, l1, l2] = descs;
    expect(l0).not.toBe(l1);
    expect(l1).not.toBe(l2);
    expect(l0).not.toBe(l2);
    // L0 必须明确写出会带完整金额
    expect(privacyDescription("L0")).toBe(S.prompt.privacy.L0.desc);
    expect(privacyDescription("L0")).toContain("完整金额");
    // L2 必须明确不含金额与数量
    expect(privacyDescription("L2")).toContain("不含任何金额与数量");
  });
});

describe("rangeError / rangeDays", () => {
  const now = 1_700_000_000_000;

  it("正常时段返回 null", () => {
    expect(rangeError(now - 7 * 24 * 3600_000, now)).toBeNull();
  });

  it("顺序错误、非有限、超 90 天各自归类", () => {
    expect(rangeError(now, now)).toBe("order");
    expect(rangeError(now + 1, now)).toBe("order");
    expect(rangeError(Number.NaN, now)).toBe("invalid");
    expect(rangeError(now - MAX_RANGE_MS - 1, now)).toBe("tooLarge");
  });

  it("90 天整不超限，超过 1 毫秒即超限", () => {
    expect(rangeError(now - MAX_RANGE_MS, now)).toBeNull();
    expect(rangeError(now - MAX_RANGE_MS - 1, now)).toBe("tooLarge");
  });

  it("超限文案友好，且不是错误码", () => {
    expect(rangeProblemText("tooLarge")).toContain("90 天");
    expect(rangeProblemText("tooLarge")).not.toContain("RangeTooLarge");
  });

  it("天数向上取整", () => {
    expect(rangeDays(now, now + 24 * 3600_000)).toBe(1);
    expect(rangeDays(now, now + 24 * 3600_000 + 1)).toBe(2);
    expect(rangeDays(now, now)).toBe(0);
  });
});

describe("datetime-local 往返", () => {
  it("毫秒 → 本地字符串 → 毫秒（分钟精度）", () => {
    const ms = new Date(2026, 0, 2, 3, 4).getTime();
    expect(localInputToMs(msToLocalInput(ms))).toBe(ms);
  });

  it("空值与非法值返回 null", () => {
    expect(localInputToMs("")).toBeNull();
    expect(localInputToMs("not-a-date")).toBeNull();
  });
});

describe("模板保存校验与另存为命名", () => {
  it("名称或正文去空白后为空都算问题", () => {
    expect(templateProblem("  ", "body")).toBe("nameRequired");
    expect(templateProblem("name", " \n ")).toBe("bodyRequired");
    expect(templateProblem("name", "body")).toBeNull();
  });

  it("从同名内置模板另存为时追加副本后缀", () => {
    expect(saveAsName("实盘速览", "实盘速览")).toBe(
      `实盘速览${S.prompt.actions.saveAsCopySuffix}`,
    );
    expect(saveAsName("我的模板", "实盘速览")).toBe("我的模板");
    expect(saveAsName("我的模板", null)).toBe("我的模板");
  });
});

describe("导出文件名", () => {
  const ms = new Date(2026, 8, 25, 10, 20, 30).getTime();

  it("形如 {模板名}-{等级}-{时间戳}.md", () => {
    expect(exportFileName("实盘速览", "L1", ms)).toBe("实盘速览-L1-20260925-102030.md");
  });

  it("非法文件名字符被替换，空名回落", () => {
    expect(exportFileName("a/b:c d", "L0", ms)).toBe("a_b_c_d-L0-20260925-102030.md");
    expect(exportFileName("   ", "L2", ms)).toBe("template-L2-20260925-102030.md");
  });
});

describe("outputStats", () => {
  it("透传 token 与字符数", () => {
    expect(outputStats({ token_estimate: 123, char_count: 456 })).toEqual({
      tokens: 123,
      chars: 456,
    });
  });

  it("非有限值回落为 0（不显示 NaN）", () => {
    expect(outputStats({ token_estimate: Number.NaN, char_count: Number.POSITIVE_INFINITY })).toEqual(
      { tokens: 0, chars: 0 },
    );
  });
});
