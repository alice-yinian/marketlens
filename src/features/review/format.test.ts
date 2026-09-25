import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import type { SeriesKind, SeriesStatus } from "../../lib/types";
import {
  MAX_RANGE_MS,
  barLabel,
  formatDurationMs,
  localInputToMs,
  msToLocalInput,
  progressPercent,
  rangeDays,
  rangeError,
  seriesKindLabel,
  seriesStatusView,
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
