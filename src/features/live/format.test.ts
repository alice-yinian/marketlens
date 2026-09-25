import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import {
  formatLocalTime,
  formatPct,
  formatPrice,
  formatRatio,
  formatRelativeTime,
  formatSignedPct,
  formatUsd,
  orUnavailable,
} from "./format";

describe("formatUsd", () => {
  it("按量级缩写", () => {
    expect(formatUsd(1_234_567_890)).toBe("$1.23B");
    expect(formatUsd(456_780_000)).toBe("$456.78M");
    expect(formatUsd(12_340)).toBe("$12.34K");
    expect(formatUsd(9.5)).toBe("$9.50");
  });

  it("负数把负号放在 $ 之前", () => {
    expect(formatUsd(-2_500_000)).toBe("-$2.50M");
  });

  it("null / NaN / Infinity 一律视为不可得", () => {
    expect(formatUsd(null)).toBeNull();
    expect(formatUsd(Number.NaN)).toBeNull();
    expect(formatUsd(Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe("formatPrice", () => {
  it("按量级自适应小数位并加千分位", () => {
    expect(formatPrice(65_432.1)).toBe("65,432.10");
    expect(formatPrice(1.5)).toBe("1.50");
    expect(formatPrice(0.1234)).toBe("0.1234");
    expect(formatPrice(0.00001234)).toBe("0.000012");
  });

  it("负数保留符号", () => {
    expect(formatPrice(-1_234.5)).toBe("-1,234.50");
  });

  it("null 不可得", () => {
    expect(formatPrice(null)).toBeNull();
  });
});

describe("formatSignedPct", () => {
  it("小数比例转百分比并带符号", () => {
    expect(formatSignedPct(0.0123)).toBe("+1.23%");
    expect(formatSignedPct(-0.0045)).toBe("-0.45%");
    expect(formatSignedPct(0)).toBe("0.00%");
  });

  it("支持更高精度（资金费率单期）", () => {
    expect(formatSignedPct(0.0001, 4)).toBe("+0.0100%");
    expect(formatSignedPct(-0.0001, 4)).toBe("-0.0100%");
  });

  it("null 不可得", () => {
    expect(formatSignedPct(null)).toBeNull();
  });
});

describe("formatPct / formatRatio", () => {
  it("不带符号的百分比", () => {
    expect(formatPct(0.0123)).toBe("1.23%");
    expect(formatPct(-0.0123)).toBe("-1.23%");
    expect(formatPct(null)).toBeNull();
  });

  it("比值保留小数位", () => {
    expect(formatRatio(1.24)).toBe("1.24");
    expect(formatRatio(54.36, 1)).toBe("54.4");
    expect(formatRatio(null)).toBeNull();
  });
});

describe("formatLocalTime", () => {
  it("输出固定为本地时间 YYYY-MM-DD HH:mm", () => {
    expect(formatLocalTime(1_700_000_000_000)).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });

  it("null 不可得", () => {
    expect(formatLocalTime(null)).toBeNull();
  });
});

describe("formatRelativeTime", () => {
  const now = 1_700_000_000_000;

  it("按最近的时间单位取整", () => {
    expect(formatRelativeTime(now - 200, now)).toBe("刚刚");
    expect(formatRelativeTime(now - 5_000, now)).toBe("5 秒前");
    expect(formatRelativeTime(now - 3 * 60_000, now)).toBe("3 分钟前");
    expect(formatRelativeTime(now - 2 * 3_600_000, now)).toBe("2 小时前");
    expect(formatRelativeTime(now - 26 * 3_600_000, now)).toBe("1 天前");
  });

  it("59 秒 / 59 分钟 / 23 小时的边界不会提前进位", () => {
    expect(formatRelativeTime(now - 59_000, now)).toBe("59 秒前");
    expect(formatRelativeTime(now - 59 * 60_000, now)).toBe("59 分钟前");
    expect(formatRelativeTime(now - 23 * 3_600_000, now)).toBe("23 小时前");
  });
});

describe("orUnavailable", () => {
  it("拿不到的数据显式写成「数据不可得」，绝不留空", () => {
    expect(orUnavailable(null)).toBe(S.live.na);
    expect(orUnavailable("+1.23%")).toBe("+1.23%");
  });
});
