import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import {
  formatAmount,
  formatContracts,
  formatLever,
  mgnModeLabel,
  posModeLabel,
  posSideLabel,
  signedTone,
} from "./format";

describe("formatAmount", () => {
  it("完整两位小数 + 千分位（权益需要精确值，不做 K/M 缩写）", () => {
    expect(formatAmount(12_345.6)).toBe("$12,345.60");
    expect(formatAmount(1_234_567.891)).toBe("$1,234,567.89");
    expect(formatAmount(0)).toBe("$0.00");
  });

  it("负数把负号放在 $ 之前", () => {
    expect(formatAmount(-1_234.5)).toBe("-$1,234.50");
  });

  it("null / NaN / Infinity 一律视为不可得", () => {
    expect(formatAmount(null)).toBeNull();
    expect(formatAmount(Number.NaN)).toBeNull();
    expect(formatAmount(Number.NEGATIVE_INFINITY)).toBeNull();
  });
});

describe("formatContracts", () => {
  it("整数带千分位", () => {
    expect(formatContracts(3)).toBe("3");
    expect(formatContracts(1_234)).toBe("1,234");
  });

  it("小数最多 4 位并去掉尾随 0", () => {
    expect(formatContracts(1.5)).toBe("1.5");
    expect(formatContracts(0.25)).toBe("0.25");
  });

  it("null 不可得", () => {
    expect(formatContracts(null)).toBeNull();
  });
});

describe("formatLever", () => {
  it("追加 x 后缀", () => {
    expect(formatLever(10)).toBe("10x");
    expect(formatLever(null)).toBeNull();
  });
});

describe("标签映射（Rust 原始值 → 中文）", () => {
  it("pos_mode", () => {
    expect(posModeLabel("net_mode")).toBe("净持仓");
    expect(posModeLabel("long_short_mode")).toBe("长空双向");
    expect(posModeLabel(null)).toBeNull();
    expect(posModeLabel("")).toBeNull();
  });

  it("未知的 pos_mode 不静默留空", () => {
    expect(posModeLabel("some_future_mode")).toBe(S.account.unknown);
  });

  it("pos_side", () => {
    expect(posSideLabel("long")).toBe("多头");
    expect(posSideLabel("short")).toBe("空头");
    expect(posSideLabel("net")).toBe("净持仓");
    expect(posSideLabel("weird")).toBe(S.account.unknown);
  });

  it("mgn_mode", () => {
    expect(mgnModeLabel("cross")).toBe("全仓");
    expect(mgnModeLabel("isolated")).toBe("逐仓");
    expect(mgnModeLabel("weird")).toBe(S.account.unknown);
    expect(mgnModeLabel(null)).toBeNull();
  });
});

describe("signedTone", () => {
  it("正负与零、不可得分别映射", () => {
    expect(signedTone(1.2)).toBe("up");
    expect(signedTone(-1.2)).toBe("down");
    expect(signedTone(0)).toBe("plain");
    expect(signedTone(null)).toBe("muted");
    expect(signedTone(Number.NaN)).toBe("muted");
  });
});
