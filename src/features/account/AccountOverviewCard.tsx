/**
 * 账户概览卡片：总权益 / 可用权益 / 未实现盈亏 / 保证金率 / 名义价值 / 持仓模式，
 * 以及币种明细。
 *
 * 金额用完整两位小数（不做 K/M 缩写）——权益与保证金需要精确值。
 * 拿不到的字段显式写「数据不可得」，不留空白。
 */
import { S } from "../../lib/strings";
import type { AccountOverview } from "../../lib/types";
import { formatPrice, formatPct } from "../live/format";
import { TONE_TEXT, formatAmount, posModeLabel, signedTone, type ValueTone } from "./format";

interface Metric {
  label: string;
  value: string | null;
  tone?: ValueTone;
}

function MetricCell({ metric }: { metric: Metric }) {
  const tone: ValueTone = metric.value === null ? "muted" : (metric.tone ?? "plain");
  return (
    <div className="min-w-0">
      <div className="truncate text-xs text-neutral-500">{metric.label}</div>
      <div className={`font-mono text-sm tabular-nums ${TONE_TEXT[tone]}`}>
        {metric.value ?? S.account.na}
      </div>
    </div>
  );
}

export function AccountOverviewCard({ overview }: { overview: AccountOverview }) {
  const metrics: Metric[] = [
    { label: S.account.overview.totalEq, value: formatAmount(overview.total_eq_usd) },
    { label: S.account.overview.availEq, value: formatAmount(overview.avail_eq_usd) },
    {
      label: S.account.overview.upl,
      value: formatAmount(overview.upl),
      tone: signedTone(overview.upl),
    },
    { label: S.account.overview.mgnRatio, value: formatPct(overview.mgn_ratio) },
    { label: S.account.overview.notional, value: formatAmount(overview.notional_usd) },
    { label: S.account.overview.posMode, value: posModeLabel(overview.pos_mode) },
  ];

  return (
    <section className="flex flex-col gap-4 rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
      <h2 className="text-sm font-medium text-neutral-200">{S.account.overviewTitle}</h2>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
        {metrics.map((metric) => (
          <MetricCell key={metric.label} metric={metric} />
        ))}
      </div>

      <div>
        <h3 className="mb-1.5 text-xs font-medium text-neutral-400">
          {S.account.currenciesTitle}
        </h3>
        {overview.currencies.length === 0 ? (
          <p className="text-xs text-neutral-500">{S.account.na}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="text-neutral-500">
                <tr>
                  <th className="py-1 pr-3 font-normal">{S.account.currency.ccy}</th>
                  <th className="py-1 pr-3 text-right font-normal">{S.account.currency.eq}</th>
                  <th className="py-1 pr-3 text-right font-normal">{S.account.currency.eqUsd}</th>
                  <th className="py-1 pr-3 text-right font-normal">{S.account.currency.availBal}</th>
                  <th className="py-1 text-right font-normal">{S.account.currency.cashBal}</th>
                </tr>
              </thead>
              <tbody className="font-mono tabular-nums text-neutral-200">
                {overview.currencies.map((balance) => (
                  <tr key={balance.ccy} className="border-t border-neutral-800">
                    <td className="py-1.5 pr-3">{balance.ccy}</td>
                    <td className="py-1.5 pr-3 text-right">
                      {formatPrice(balance.eq) ?? S.account.na}
                    </td>
                    <td className="py-1.5 pr-3 text-right">
                      {formatAmount(balance.eq_usd) ?? S.account.na}
                    </td>
                    <td className="py-1.5 pr-3 text-right">
                      {formatPrice(balance.avail_bal) ?? S.account.na}
                    </td>
                    <td className="py-1.5 text-right">
                      {formatPrice(balance.cash_bal) ?? S.account.na}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}
