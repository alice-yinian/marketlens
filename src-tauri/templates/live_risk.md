# OKX 持仓风险体检

生成时间：{{ meta.generated_at | ts }}
账户权益量级：{{ account.equity_magnitude }}

## 我的持仓

{% if positions | length == 0 %}
当前没有持仓，无需体检。
{% else %}
{% for p in positions %}
### {{ p.inst_id }}（{{ p.pos_side | side }} {{ p.leverage }}× {{ p.margin_mode }}）

| 项目 | 数值 |
| --- | --- |
| 数量 | {{ p.contracts | size }} 张 |
| 开仓均价 | {{ p.avg_price }} |
| 标记价 | {{ p.mark_price }} |
| 强平价 | {{ p.liquidation_price | na("不可得") }} |
| 名义价值 | {{ p.notional | money }} |
| 未实现盈亏 | {{ p.unrealized_pnl | money }} |
| 未实现收益率 | {{ p.unrealized_pnl_ratio | pct }} |
| 保证金率 | {{ p.margin_ratio | na("不可得") }} |
| 维持保证金 | {{ account.maintenance_margin | money }} |
| 持仓时长 | {{ (p.updated_at - p.created_at) | dur }} |

{% if p.liquidation_price %}
距离强平：{{ ((p.liquidation_price - p.mark_price) / p.mark_price) | pct }}（相对标记价）
{% else %}
距离强平：不可得（无强平价或为全仓模式）
{% endif %}

{% endfor %}
{% endif %}

## 当前市场状态

{% for m in market %}
- **{{ m.inst_id }}**：趋势 {{ m.trend | na("不可判定") }}｜波动 {{ m.volatility | na("不可判定") }}｜拥挤度 {{ m.crowding }}
  - 24h 涨跌 {{ m.change_24h_pct | pct }}｜资金费率年化 {{ m.funding_annualized | pct }}
  - RSI14 {{ m.rsi14 | na("不可得") }}｜ATR {{ m.atr_pct | na("不可得") | pct }}｜基差 {{ m.basis_pct | na("不可得") | pct }}
{% endfor %}

## 账户整体

{% if account.available %}
- 权益：{{ account.equity | money }}（量级 {{ account.equity_magnitude }}）
- 可用权益：{{ account.available_equity | money }}
- 名义敞口：{{ account.notional | money }}
- 未实现盈亏：{{ account.unrealized_pnl | money }}
- 保证金率：{{ account.margin_ratio | na("不可得") }}
{% else %}
账户数据不可得：{{ account.reason }}。因此**无法评估整体风险**，下面的问题请只回答市场部分。
{% endif %}

## 我想知道

1. 按**爆仓距离**和**保证金率**排序，我的哪一笔持仓最危险？给出排序依据。
2. 我的持仓之间是否**方向重叠**（等于变相加杠杆）？
3. 当前波动与拥挤度下，我的杠杆水平是否偏高？请说明你的阈值假设。
4. 如果市场反向波动 {{ "5" }}%，我的哪笔持仓会先出问题？

**重要**：如果某项数据不可得，请直接说「无法判断」，不要用推测填补。
