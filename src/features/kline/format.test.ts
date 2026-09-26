/**
 * 行情页纯函数的边界测试。
 *
 * 这些上限与 Rust 侧一一对应（前端拦截只为即时反馈，后端才是安全边界），
 * 所以这里刻意**贴着边界值**测：刚好等于上限必须放行、+1 必须拒绝。
 * 只测「中间值能过」的话，把 `>` 写成 `>=` 这种错误照样全绿。
 */
import { describe, expect, it } from "vitest";

import type { IndicatorKind, IndicatorSpec } from "../../lib/types";
import {
  DEFAULT_PERIOD,
  INDICATOR_KINDS,
  INDICATOR_LABEL,
  MAX_BARS_PER_PLAN,
  MAX_CANDLES_PER_BAR,
  MAX_INDICATORS,
  MAX_INDICATOR_PERIOD,
  MAX_TOTAL_CANDLES,
  dedupeIndicators,
  indicatorProblem,
  selectionProblem,
} from "./format";

const KINDS: IndicatorKind[] = ["ema", "rsi", "atr_pct", "realized_vol"];

/** 造 n 个周期名：内容无关紧要，只关心「选了几个周期」 */
function barsOf(n: number): string[] {
  return Array.from({ length: n }, (_, i) => `BAR${i}`);
}

/** 造 n 个互不相同的合法指标（周期从 1 递增，天然不重复也不越界） */
function specsOf(n: number): IndicatorSpec[] {
  const list: IndicatorSpec[] = [];
  for (let i = 0; i < n; i += 1) list.push({ kind: "ema", period: i + 1 });
  return list;
}

describe("selectionProblem", () => {
  it("没选标的、或标的是空白串 → inst", () => {
    expect(selectionProblem({ instId: null, bars: ["1H"], candleCount: 200 })).toBe("inst");
    expect(selectionProblem({ instId: "", bars: ["1H"], candleCount: 200 })).toBe("inst");
    expect(selectionProblem({ instId: "   ", bars: ["1H"], candleCount: 200 })).toBe("inst");
  });

  it("一个周期都没选 → bars", () => {
    expect(selectionProblem({ instId: "BTC-USDT-SWAP", bars: [], candleCount: 200 })).toBe("bars");
  });

  it("周期数刚好等于 MAX_BARS_PER_PLAN 放行，多一个 → too_many_bars", () => {
    expect(selectionProblem({ instId: "BTC", bars: barsOf(MAX_BARS_PER_PLAN), candleCount: 1 })).toBeNull();
    expect(
      selectionProblem({ instId: "BTC", bars: barsOf(MAX_BARS_PER_PLAN + 1), candleCount: 1 }),
    ).toBe("too_many_bars");
  });

  it("根数为 0 / 负数 / 非整数 / 超上限 → count；1 与 MAX_CANDLES_PER_BAR 放行", () => {
    for (const candleCount of [0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(selectionProblem({ instId: "BTC", bars: ["1H"], candleCount })).toBe("count");
    }
    expect(selectionProblem({ instId: "BTC", bars: ["1H"], candleCount: MAX_CANDLES_PER_BAR + 1 })).toBe(
      "count",
    );
    expect(selectionProblem({ instId: "BTC", bars: ["1H"], candleCount: 1 })).toBeNull();
    expect(selectionProblem({ instId: "BTC", bars: ["1H"], candleCount: MAX_CANDLES_PER_BAR })).toBeNull();
  });

  it("根数越界时只报 count（不因为合计也越界而报 total）", () => {
    expect(selectionProblem({ instId: "BTC", bars: barsOf(4), candleCount: MAX_CANDLES_PER_BAR + 1 })).toBe(
      "count",
    );
  });

  it("根数 × 周期数 刚好等于 MAX_TOTAL_CANDLES 放行，多一根 → total", () => {
    const bars = barsOf(4);
    // 4 × 375 = 1500 = 上限
    expect(selectionProblem({ instId: "BTC", bars, candleCount: 375 })).toBeNull();
    expect(selectionProblem({ instId: "BTC", bars, candleCount: 376 })).toBe("total");
    // 单周期的边界另一侧：500 根已到单周期上限，3 个周期刚好压线
    expect(selectionProblem({ instId: "BTC", bars: barsOf(3), candleCount: 500 })).toBeNull();
  });

  it("全部合规 → null", () => {
    expect(
      selectionProblem({ instId: "BTC-USDT-SWAP", bars: ["1H", "4H"], candleCount: 200 }),
    ).toBeNull();
  });
});

describe("indicatorProblem", () => {
  it("空清单 → null（指标是可选的，不选就只给原始 K 线）", () => {
    expect(indicatorProblem([])).toBeNull();
  });

  it("数量刚好等于 MAX_INDICATORS 放行，多一个 → too_many", () => {
    expect(specsOf(MAX_INDICATORS)).toHaveLength(MAX_INDICATORS);
    expect(indicatorProblem(specsOf(MAX_INDICATORS))).toBeNull();
    expect(indicatorProblem(specsOf(MAX_INDICATORS + 1))).toBe("too_many");
  });

  it("周期必须是不小于 1 的整数：1 与 MAX_INDICATOR_PERIOD 放行，越界 → period_range", () => {
    for (const period of [0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, MAX_INDICATOR_PERIOD + 1]) {
      expect(indicatorProblem([{ kind: "ema", period }])).toBe("period_range");
    }
    expect(indicatorProblem([{ kind: "ema", period: 1 }])).toBeNull();
    expect(indicatorProblem([{ kind: "ema", period: MAX_INDICATOR_PERIOD }])).toBeNull();
  });

  it("只报第一个问题：数量超限优先于周期越界", () => {
    const list = [...specsOf(MAX_INDICATORS), { kind: "ema", period: 0 } satisfies IndicatorSpec];
    expect(list).toHaveLength(MAX_INDICATORS + 1);
    expect(indicatorProblem(list)).toBe("too_many");
  });

  it("清单里混着越界项与非法项时，越界项优先于合法项之后的判断（顺序扫描）", () => {
    const list: IndicatorSpec[] = [
      { kind: "ema", period: 20 },
      { kind: "rsi", period: MAX_INDICATOR_PERIOD + 1 },
    ];
    expect(indicatorProblem(list)).toBe("period_range");
  });
});

describe("dedupeIndicators", () => {
  it("同「种类 + 周期」只留首次出现的那条，顺序不变", () => {
    const list: IndicatorSpec[] = [
      { kind: "ema", period: 20 },
      { kind: "rsi", period: 14 },
      { kind: "ema", period: 20 },
      { kind: "rsi", period: 14 },
      { kind: "ema", period: 20 },
    ];
    expect(dedupeIndicators(list)).toEqual([
      { kind: "ema", period: 20 },
      { kind: "rsi", period: 14 },
    ]);
  });

  it("周期不同不算重复（EMA20 与 EMA50 是两列）", () => {
    const list: IndicatorSpec[] = [
      { kind: "ema", period: 20 },
      { kind: "ema", period: 50 },
      { kind: "rsi", period: 20 },
    ];
    expect(dedupeIndicators(list)).toEqual(list);
  });

  it("空清单 → 空清单", () => {
    expect(dedupeIndicators([])).toEqual([]);
  });

  it("清单本身无重复时原样返回（顺序与内容都不变）", () => {
    const list: IndicatorSpec[] = [
      { kind: "realized_vol", period: 24 },
      { kind: "atr_pct", period: 14 },
    ];
    expect(dedupeIndicators(list)).toEqual([
      { kind: "realized_vol", period: 24 },
      { kind: "atr_pct", period: 14 },
    ]);
  });
});

describe("INDICATOR_KINDS / INDICATOR_LABEL / DEFAULT_PERIOD 的一致性", () => {
  it("可选项就是这 4 种，顺序即下拉框顺序", () => {
    expect([...INDICATOR_KINDS]).toEqual(["ema", "rsi", "atr_pct", "realized_vol"]);
  });

  it("INDICATOR_LABEL 的键与 INDICATOR_KINDS 完全一致（漏一个键只会静默少一段界面文案）", () => {
    const kindKeys = [...INDICATOR_KINDS].sort();
    expect(Object.keys(INDICATOR_LABEL).sort()).toEqual(kindKeys);
  });

  it("DEFAULT_PERIOD 的键与 INDICATOR_KINDS 完全一致", () => {
    const kindKeys = [...INDICATOR_KINDS].sort();
    expect(Object.keys(DEFAULT_PERIOD).sort()).toEqual(kindKeys);
  });

  it("每个种类的标签非空、默认周期本身合法（否则「添加指标」一按就报错）", () => {
    for (const kind of KINDS) {
      expect(INDICATOR_LABEL[kind].length).toBeGreaterThan(0);
      expect(indicatorProblem([{ kind, period: DEFAULT_PERIOD[kind] }])).toBeNull();
    }
  });
});

describe("上限常量与 Rust 侧同口径", () => {
  it("五个上限就是 Rust 里那几个常量（前端拦截只是即时反馈，后端才是边界）", () => {
    expect(MAX_BARS_PER_PLAN).toBe(6);
    expect(MAX_CANDLES_PER_BAR).toBe(500);
    expect(MAX_TOTAL_CANDLES).toBe(1500);
    expect(MAX_INDICATORS).toBe(8);
    expect(MAX_INDICATOR_PERIOD).toBe(500);
  });
});
