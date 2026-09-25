/**
 * 趋势 / 波动 / 拥挤度徽章。
 *
 * Rust 变体名（`Uptrend`、`LongCrowded`…）在这里映射成中文并按语义配色：
 * 涨 → 绿、跌 → 红、中性 → 灰、风险 → 琥珀、不可得 → 暗灰。
 *
 * `trend` / `vol` 为 null 时徽章**不隐藏**，而是显示「数据不可得」——
 * 隐藏会让用户以为「没有风险」（架构铁律 4）。
 */
import { S } from "../../lib/strings";
import type { Crowding, TrendRegime, VolRegime } from "../../lib/types";

type Tone = "up" | "down" | "neutral" | "risk" | "unavailable";

const TONE_CLASS: Record<Tone, string> = {
  up: "bg-emerald-950 text-emerald-300 ring-emerald-900",
  down: "bg-red-950 text-red-300 ring-red-900",
  neutral: "bg-neutral-800 text-neutral-300 ring-neutral-700",
  risk: "bg-amber-950 text-amber-300 ring-amber-900",
  unavailable: "bg-neutral-900 text-neutral-500 ring-neutral-800",
};

const TREND_TONE: Record<TrendRegime, Tone> = {
  Uptrend: "up",
  Downtrend: "down",
  Range: "neutral",
  Transition: "risk",
};

const VOL_TONE: Record<VolRegime, Tone> = {
  Low: "neutral",
  Normal: "neutral",
  High: "risk",
  Extreme: "down",
};

const CROWDING_TONE: Record<Crowding, Tone> = {
  LongCrowded: "risk",
  ShortCrowded: "risk",
  Balanced: "neutral",
};

function Badge({ label, value, tone }: { label: string; value: string; tone: Tone }) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs ring-1 ring-inset ${TONE_CLASS[tone]}`}
    >
      <span className="opacity-70">{label}</span>
      <span className="font-medium">{value}</span>
    </span>
  );
}

export function RegimeBadges({
  trend,
  vol,
  crowding,
}: {
  trend: TrendRegime | null;
  vol: VolRegime | null;
  crowding: Crowding;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      <Badge
        label={S.live.trend.label}
        value={trend === null ? S.live.na : S.live.trend[trend]}
        tone={trend === null ? "unavailable" : TREND_TONE[trend]}
      />
      <Badge
        label={S.live.vol.label}
        value={vol === null ? S.live.na : S.live.vol[vol]}
        tone={vol === null ? "unavailable" : VOL_TONE[vol]}
      />
      <Badge
        label={S.live.crowding.label}
        value={S.live.crowding[crowding]}
        tone={CROWDING_TONE[crowding]}
      />
    </div>
  );
}
