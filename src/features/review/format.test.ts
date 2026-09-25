import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import type { SeriesKind, SeriesStatus } from "../../lib/types";
import {
  MAX_RANGE_MS,
  barLabel,
  directionLabel,
  directionView,
  extremeText,
  feeDragIsHigh,
  feeDragText,
  formatDurationMs,
  formatHoldDuration,
  localInputToMs,
  msToLocalInput,
  pnlTone,
  precisionView,
  progressPercent,
  profitFactorText,
  rangeDays,
  rangeError,
  regimeView,
  seriesKindLabel,
  seriesStatusView,
  sourceView,
  winRateText,
} from "./format";

describe("progressPercent", () => {
  it("done_series / total_series 取整百分比", () => {
    expect(progressPercent(0, 10)).toBe(0);
    expect(progressPercent(5, 10)).toBe(50);
    expect(progressPercent(1, 3)).toBe(33);
    expect(progressPercent(10, 10)).toBe(100);
  });

  it("total 为 0 或非有限数时返回 0（不显示 NaN）", () => {
    expect(progressPercent(0, 0)).toBe(0);
    expect(progressPercent(3, 0)).toBe(0);
    expect(progressPercent(Number.NaN, 10)).toBe(0);
    expect(progressPercent(1, Number.POSITIVE_INFINITY)).toBe(0);
  });

  it("超出 100% 钳制到 100", () => {
    expect(progressPercent(11, 10)).toBe(100);
  });
});

describe("seriesKindLabel（Rust 变体名 → 中文）", () => {
  it("全部种类都有映射", () => {
    const kinds: SeriesKind[] = [
      "Candles",
      "MarkCandles",
      "IndexCandles",
      "FundingRate",
      "LongShortRatio",
      "TakerVolume",
    ];
    expect(kinds.map(seriesKindLabel)).toEqual([
      "K 线",
      "标记价 K 线",
      "指数价 K 线",
      "资金费率",
      "多空账户比",
      "主动买卖量",
    ]);
  });
});

describe("seriesStatusView（状态 → 视觉/文案）", () => {
  it("四种状态 tone 与文案各异", () => {
    expect(seriesStatusView("Ok").tone).toBe("ok");
    expect(seriesStatusView("Ok").label).toBe(S.review.status.Ok);

    expect(seriesStatusView("Unavailable").tone).toBe("unavailable");
    expect(seriesStatusView("Unavailable").label).toBe(S.review.status.Unavailable);

    expect(seriesStatusView("Failed").tone).toBe("failed");
    expect(seriesStatusView("Failed").label).toBe(S.review.status.Failed);

    expect(seriesStatusView("Skipped").tone).toBe("skipped");
    expect(seriesStatusView("Skipped").label).toBe(S.review.status.Skipped);
  });

  it("Skipped 用中性/蓝色而不是红色（跳过是好事）", () => {
    const skipped = seriesStatusView("Skipped");
    expect(skipped.badge).toContain("sky");
    expect(skipped.badge).not.toContain("red");

    const failed = seriesStatusView("Failed");
    expect(failed.badge).toContain("red");

    const unavailable = seriesStatusView("Unavailable");
    expect(unavailable.badge).toContain("neutral");
  });

  it("四种状态的徽章样式互不相同", () => {
    const statuses: SeriesStatus[] = ["Ok", "Unavailable", "Failed", "Skipped"];
    const badges = new Set(statuses.map((s) => seriesStatusView(s).badge));
    expect(badges.size).toBe(4);
  });
});

describe("rangeError（90 天上限前置校验）", () => {
  const base = 1_700_000_000_000;

  it("合法时段返回 null", () => {
    expect(rangeError(base, base + 24 * 60 * 60 * 1000)).toBeNull();
    expect(rangeError(base, base + MAX_RANGE_MS)).toBeNull();
  });

  it("超过 90 天返回 tooLarge（不静默截断）", () => {
    expect(rangeError(base, base + MAX_RANGE_MS + 1)).toBe("tooLarge");
  });

  it("结束不晚于开始返回 order", () => {
    expect(rangeError(base, base)).toBe("order");
    expect(rangeError(base + 1000, base)).toBe("order");
  });

  it("非有限数返回 invalid", () => {
    expect(rangeError(Number.NaN, base)).toBe("invalid");
    expect(rangeError(base, Number.POSITIVE_INFINITY)).toBe("invalid");
  });
});

describe("rangeDays", () => {
  it("向上取整", () => {
    expect(rangeDays(0, 24 * 60 * 60 * 1000)).toBe(1);
    expect(rangeDays(0, 25 * 60 * 60 * 1000)).toBe(2);
  });

  it("非法时段返回 0", () => {
    expect(rangeDays(100, 100)).toBe(0);
  });
});

describe("formatDurationMs", () => {
  it("不足 1 秒 / 秒 / 分钟", () => {
    expect(formatDurationMs(500)).toBe(S.review.duration.underSecond);
    expect(formatDurationMs(3000)).toBe(S.review.duration.seconds(3));
    expect(formatDurationMs(60_000)).toBe(S.review.duration.minutes(1));
    expect(formatDurationMs(90_000)).toBe(S.review.duration.minutes(1.5));
  });
});

describe("datetime-local 互转", () => {
  it("本地时间往返一致", () => {
    const ms = new Date(2026, 0, 2, 3, 4, 0, 0).getTime();
    expect(msToLocalInput(ms)).toBe("2026-01-02T03:04");
    expect(localInputToMs("2026-01-02T03:04")).toBe(ms);
  });

  it("空值 / 无法解析返回 null", () => {
    expect(localInputToMs("")).toBeNull();
    expect(localInputToMs("not-a-date")).toBeNull();
  });
});

describe("barLabel", () => {
  it("已知粒度映射为中文，未知原样返回", () => {
    expect(barLabel("15m")).toBe("15 分钟");
    expect(barLabel("1H")).toBe("1 小时");
    expect(barLabel("4H")).toBe("4 小时");
    expect(barLabel("1D")).toBe("1 天");
    expect(barLabel("2H")).toBe("2H");
  });
});

describe("复盘结果：盈亏比文案", () => {
  it("profit_factor 为 null 时按可区分的原因给出精确文案，绝不显示 0 或 ∞", () => {
    expect(profitFactorText({ profit_factor: null, total: 0, win_rate: 0 })).toBe(
      S.review.result.profitFactorNoTrades,
    );
    expect(profitFactorText({ profit_factor: null, total: 5, win_rate: 0 })).toBe(
      S.review.result.profitFactorNoWins,
    );
    expect(profitFactorText({ profit_factor: null, total: 5, win_rate: 1 })).toBe(
      S.review.result.profitFactorNoLosses,
    );
    // 其余情况（理论上只剩「亏损合计为 0」）
    expect(profitFactorText({ profit_factor: null, total: 5, win_rate: 0.5 })).toBe(
      S.review.result.profitFactorUnavailable,
    );

    const texts = [
      S.review.result.profitFactorNoTrades,
      S.review.result.profitFactorNoWins,
      S.review.result.profitFactorNoLosses,
      S.review.result.profitFactorUnavailable,
    ];
    for (const text of texts) {
      expect(text).not.toContain("0");
      expect(text).not.toContain("∞");
      expect(text).not.toContain("Infinity");
    }
  });

  it("有值时保留两位小数", () => {
    expect(profitFactorText({ profit_factor: 1.5, total: 5, win_rate: 0.6 })).toBe("1.50");
    expect(profitFactorText({ profit_factor: 0.333, total: 5, win_rate: 0.4 })).toBe("0.33");
  });

  it("非有限数同样按原因回落到文案", () => {
    expect(
      profitFactorText({
        profit_factor: Number.POSITIVE_INFINITY,
        total: 5,
        win_rate: 0.5,
      }),
    ).toBe(S.review.result.profitFactorUnavailable);
  });
});

describe("复盘结果：费用侵蚀文案", () => {
  it("null 时说明无已实现盈亏，而不是显示 0%", () => {
    const text = feeDragText(null);
    expect(text).toBe(S.review.result.feeDragUnavailable);
    expect(text).not.toBe("0.00%");
  });

  it("有值时是百分比", () => {
    expect(feeDragText(0.1234)).toBe("12.34%");
  });

  it("≥20% 视为偏高（需要提示用户）", () => {
    expect(feeDragIsHigh(0.2)).toBe(true);
    expect(feeDragIsHigh(0.19)).toBe(false);
    expect(feeDragIsHigh(null)).toBe(false);
  });
});

describe("复盘结果：方向文案（含 unknown）", () => {
  it("long / short / unknown 都有明确文案", () => {
    expect(directionLabel("long")).toBe("多");
    expect(directionLabel("short")).toBe("空");
    expect(directionLabel("unknown")).toBe("未知");
  });

  it("未知取值不静默留空，回落成「未知」", () => {
    expect(directionLabel("sideways")).toBe(S.review.result.direction.unknown);
  });

  it("多空与未知的角标样式互不相同", () => {
    const badges = new Set(
      ["long", "short", "unknown"].map((d) => directionView(d).badge),
    );
    expect(badges.size).toBe(3);
  });
});

describe("复盘结果：来源角标三态", () => {
  it("okx / local / merged 都有文案且样式互不相同", () => {
    expect(sourceView("okx").label).toBe(S.review.result.source.okx);
    expect(sourceView("local").label).toBe(S.review.result.source.local);
    expect(sourceView("merged").label).toBe(S.review.result.source.merged);

    const badges = new Set(
      ["okx", "local", "merged"].map((s) => sourceView(s).badge),
    );
    expect(badges.size).toBe(3);
  });

  it("未知来源不隐藏，回落成「来源未知」", () => {
    expect(sourceView("elsewhere").label).toBe(S.review.result.source.unknown);
  });
});

describe("复盘结果：时间精度角标", () => {
  it("exact / approximate 文案与样式不同", () => {
    expect(precisionView("exact").label).toBe(S.review.result.precision.exact);
    expect(precisionView("approximate").label).toBe(S.review.result.precision.approximate);
    expect(precisionView("exact").badge).not.toBe(precisionView("approximate").badge);
  });

  it("未知精度回落成「精度未知」", () => {
    expect(precisionView("fuzzy").label).toBe(S.review.result.precision.unknown);
  });
});

describe("复盘结果：开仓时市场状态", () => {
  it("regime 为 null 时明确写「不可得」，而不是留空", () => {
    const view = regimeView(null);
    expect(view.available).toBe(false);
    expect(view.unavailableNote).toBe(S.review.result.regimeUnavailable);
    expect(view.trend).toBeNull();
  });

  it("字段缺失时逐个回落，不整块隐藏", () => {
    const view = regimeView({
      trend: "Range",
      vol: null,
      crowding: null,
      change_24h: -0.0123,
      funding_annualized: null,
      basis_pct: null,
    });
    expect(view.available).toBe(true);
    expect(view.trend).toBe(S.live.trend.Range);
    expect(view.vol).toBeNull();
    expect(view.crowding).toBeNull();
    expect(view.change24h).toBe("-1.23%");
  });
});

describe("复盘结果：其它格式化", () => {
  it("平均持仓时长按天/小时/分钟拆分", () => {
    expect(formatHoldDuration(3 * 86_400_000 + 4 * 3_600_000)).toBe("3 天 4 小时");
    expect(formatHoldDuration(5 * 3_600_000 + 30 * 60_000)).toBe("5 小时 30 分钟");
    expect(formatHoldDuration(90_000)).toBe(S.review.duration.minutes(1.5));
    expect(formatHoldDuration(0)).toBe(S.review.duration.underSecond);
  });

  it("持仓极值缺失时说明「需本地留痕」", () => {
    expect(extremeText(null)).toBe(S.review.result.extremeUnavailable);
    expect(extremeText(12)).toBe("+$12.00");
    expect(extremeText(-4)).toBe("-$4.00");
  });

  it("盈亏配色：正绿负红零中性", () => {
    expect(pnlTone(1)).toBe("up");
    expect(pnlTone(-1)).toBe("down");
    expect(pnlTone(0)).toBe("flat");
    expect(pnlTone(Number.NaN)).toBe("flat");
  });

  it("胜率是小数，转成百分比；非法值回落「数据不可得」", () => {
    expect(winRateText(0.5)).toBe("50.00%");
    expect(winRateText(Number.NaN)).toBe(S.live.na);
  });
});
