// @vitest-environment jsdom
/**
 * 复盘结果展示的渲染测试。
 *
 * 重点覆盖「数据可信度必须如实展示」的几处硬要求：
 * - 来源角标三态（官方 / 本地 / 官方+本地）都要渲染；
 * - 方向 unknown 显示「未知」而不是留空；
 * - regime 为 null 时显示「开仓时状态不可得」；
 * - profit_factor 为 null 时不能显示 0 或 ∞；
 * - 极值缺失时说明「需本地留痕」。
 *
 * ReviewResult 是纯展示组件，不 mock IPC。
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import type { ClosedPosition, ReviewContext } from "../../lib/types";
import { ReviewResult } from "./ReviewResult";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

function positionOf(overrides: Partial<ClosedPosition> = {}): ClosedPosition {
  return {
    pos_id: "p1",
    inst_id: "BTC-USDT-SWAP",
    direction: "unknown",
    mgn_mode: "cross",
    lever: 5,
    open_avg_px: 60_000,
    close_avg_px: 61_000,
    max_contracts: 1,
    pnl: 100,
    pnl_ratio: 0.01,
    realized_pnl: 12.5,
    fee: -1.2,
    funding_fee: -0.3,
    open_time: 1_700_000_000_000,
    close_time: 1_700_003_600_000,
    source: "okx",
    time_precision: "exact",
    regime: null,
    max_favorable: null,
    max_adverse: null,
    ...overrides,
  };
}

function contextOf(overrides: Partial<ReviewContext> = {}): ReviewContext {
  return {
    from: 1_700_000_000_000,
    to: 1_700_086_400_000,
    bar: "1H",
    positions: [positionOf()],
    stats: {
      total: 1,
      win_rate: 1,
      profit_factor: null,
      expectancy: 12.5,
      total_realized_pnl: 12.5,
      avg_hold_ms: 3_600_000,
      fee_drag: null,
      by_trend: [
        {
          key: "未归因",
          count: 1,
          win_rate: 1,
          total_pnl: 12.5,
          profit_factor: null,
          is_unattributed: true,
        },
      ],
      by_direction: [
        {
          key: "未知",
          count: 1,
          win_rate: 1,
          total_pnl: 12.5,
          profit_factor: null,
          is_unattributed: false,
        },
      ],
      by_instrument: [
        {
          key: "BTC-USDT-SWAP",
          count: 1,
          win_rate: 1,
          total_pnl: 12.5,
          profit_factor: null,
          is_unattributed: false,
        },
      ],
      unattributed: 1,
    },
    merge: {
      from_official: 1,
      from_local: 0,
      enriched: 0,
      coverage_note: "本地无留痕记录：无法提供持仓期间的最大浮盈/浮亏。",
    },
    trace: {
      records: 0,
      has_gaps: false,
      max_gap_ms: 0,
      last_trace_at: null,
      note: "最近 30 天没有本地留痕。",
    },
    warnings: ["官方历史仓位在到达起始时间前已耗尽，该时段可能不完整"],
    ...overrides,
  };
}

let root: Root | undefined;
let container: HTMLDivElement | undefined;

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container?.remove();
  root = undefined;
  container = undefined;
});

async function render(context: ReviewContext): Promise<HTMLDivElement> {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(<ReviewResult context={context} />);
  });
  return container;
}

describe("ReviewResult", () => {
  it("profit_factor 为 null 时按可区分原因给出精确文案，不显示 0 或 ∞", async () => {
    // total=1, win_rate=1 → 无亏损笔
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.profitFactorNoLosses);
    expect(host.textContent).not.toContain("∞");
    expect(host.textContent).not.toContain("Infinity");
    expect(host.textContent).toContain(S.review.result.profitFactor);
  });

  it("profit_factor 为 null 的三种可区分原因都渲染对应文案", async () => {
    // total === 0 时整块概览被空态替代，所以这里只覆盖仍有概览的三种原因
    const cases: Array<[number, number, string]> = [
      [5, 0, S.review.result.profitFactorNoWins],
      [5, 1, S.review.result.profitFactorNoLosses],
      [5, 0.5, S.review.result.profitFactorUnavailable],
    ];
    for (const [total, winRate, expected] of cases) {
      const host = await render(
        contextOf({
          stats: { ...contextOf().stats, total, win_rate: winRate, profit_factor: null },
        }),
      );
      expect(host.textContent).toContain(expected);
    }
  });

  it("direction === unknown 显示「未知」，不显示成多/空", async () => {
    const host = await render(contextOf());
    const card = host.querySelector('[data-testid="position-card"]');
    expect(card?.textContent).toContain(S.review.result.direction.unknown);
  });

  it("regime 为 null 时显示「开仓时状态不可得」，而不是留空", async () => {
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.regimeUnavailable);
  });

  it("来源角标三态（okx / local / merged）都能渲染", async () => {
    const host = await render(
      contextOf({
        positions: [
          positionOf({ pos_id: "a", source: "okx" }),
          positionOf({ pos_id: "b", source: "local", time_precision: "approximate" }),
          positionOf({ pos_id: "c", source: "merged" }),
        ],
        stats: {
          ...contextOf().stats,
          total: 3,
        },
      }),
    );
    const cards = [...host.querySelectorAll('[data-testid="position-card"]')];
    expect(cards).toHaveLength(3);
    expect(cards[0]?.textContent).toContain(S.review.result.source.okx);
    expect(cards[1]?.textContent).toContain(S.review.result.source.local);
    expect(cards[2]?.textContent).toContain(S.review.result.source.merged);
    // 时间精度角标同步展示
    expect(cards[1]?.textContent).toContain(S.review.result.precision.approximate);
  });

  it("极值缺失时说明「需本地留痕」", async () => {
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.extremeUnavailable);
    expect(host.textContent).toContain(S.review.result.maxFavorable);
    expect(host.textContent).toContain(S.review.result.maxAdverse);
  });

  it("逐笔展示开仓/平仓时间与均价、手续费、资金费", async () => {
    const host = await render(contextOf());
    const card = host.querySelector('[data-testid="position-card"]');
    expect(card?.textContent).toContain("60,000.00");
    expect(card?.textContent).toContain("61,000.00");
    expect(card?.textContent).toContain("+$12.50");
    expect(card?.textContent).toContain("-$1.20");
    expect(card?.textContent).toContain("-$0.30");
    expect(card?.textContent).toContain(S.review.result.fee);
    expect(card?.textContent).toContain(S.review.result.fundingFee);
  });

  it("warnings 逐条醒目展示（含非致命的重要提示）", async () => {
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.warningsTitle);
    expect(host.textContent).toContain(
      "官方历史仓位在到达起始时间前已耗尽，该时段可能不完整",
    );
  });

  it("数据可信度区展示双源说明、留痕覆盖度与未归因提示", async () => {
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.trustTitle);
    expect(host.textContent).toContain("本地无留痕记录：无法提供持仓期间的最大浮盈/浮亏。");
    expect(host.textContent).toContain("最近 30 天没有本地留痕。");
    expect(host.textContent).toContain(S.review.result.unattributedNote(1));
  });

  it("按开仓时市场状态分组里「未归因」要能看出是数据缺失", async () => {
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.trendTitle);
    expect(host.textContent).toContain(S.review.result.groupUnattributed);
    expect(host.textContent).toContain(S.review.result.groupUnattributedNote);
  });

  it("数据缺失标注以 is_unattributed 为准，而不是匹配 key 的字面量", async () => {
    // key 不是「未归因」，但显式标记为数据缺失 —— 仍必须标注
    const host = await render(
      contextOf({
        stats: {
          ...contextOf().stats,
          by_trend: [
            {
              key: "趋势未知",
              count: 1,
              win_rate: 1,
              total_pnl: 12.5,
              profit_factor: null,
              is_unattributed: true,
            },
          ],
        },
      }),
    );
    expect(host.textContent).toContain("趋势未知");
    expect(host.textContent).toContain(S.review.result.groupUnattributedNote);
  });

  it("费用侵蚀突出显示并带说明", async () => {
    const host = await render(contextOf());
    expect(host.textContent).toContain(S.review.result.feeDrag);
    expect(host.textContent).toContain(S.review.result.feeDragHint);
    expect(host.textContent).toContain(S.review.result.feeDragUnavailable);
  });

  it("该时段没有仓位时给出空态而不是空白", async () => {
    const host = await render(
      contextOf({
        positions: [],
        stats: {
          ...contextOf().stats,
          total: 0,
          by_trend: [],
          unattributed: 0,
        },
      }),
    );
    expect(host.textContent).toContain(S.review.result.empty);
    expect(host.textContent).toContain(S.review.result.emptyHint);
  });
});
