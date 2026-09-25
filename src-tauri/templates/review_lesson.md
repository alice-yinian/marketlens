# 交易复盘：我从这些交易里该学到什么

时段：{{ period.from | ts_date }} → {{ period.to | ts_date }}｜共 {{ stats.total }} 笔

## 事实

- 胜率 {{ stats.win_rate | pct }}｜期望值 {{ stats.expectancy | money }}｜累计 {{ stats.total_realized_pnl | money }}
- 平均持仓 {{ stats.avg_hold_ms | dur }}｜费用侵蚀 {% if stats.fee_drag %}{{ stats.fee_drag | pct }}{% else %}不可得{% endif %}

## 交易清单

{% for p in positions %}
{{ loop.index }}. {{ p.open_time | ts }}｜{{ p.inst_id }}｜{{ p.direction | side }} {{ p.leverage }}×｜
   开 {{ p.open_price }} → 平 {{ p.close_price }}｜{{ p.realized_pnl | money }}（{{ p.pnl_ratio | pct }}）｜
   持仓 {{ p.hold_ms | dur }}｜开仓时状态：{% if p.regime %}{{ p.regime.trend | na("不可得") }}/{{ p.regime.volatility | na("不可得") }}/{{ p.regime.crowding | na("不可得") }}{% else %}不可得{% endif %}
{% endfor %}

## 分组对照

**按开仓时趋势**
{% for group in stats.by_trend %}
- {{ group.key }}{% if group.is_unattributed %}（无法归类：缺开仓时刻状态）{% endif %}：{{ group.count }} 笔，胜率 {{ group.win_rate | pct }}，合计 {{ group.total_pnl | money }}
{% endfor %}

**按方向**
{% for group in stats.by_direction %}
- {{ group.key }}：{{ group.count }} 笔，胜率 {{ group.win_rate | pct }}，合计 {{ group.total_pnl | money }}
{% endfor %}

## 我想知道

请像一位**不客气但讲道理**的交易教练那样回答：

1. 这些交易里，我有没有**重复犯同一个错误**？如果有，具体是哪几笔、错在哪一步？
2. 如果只看**开仓时的市场状态**，我是不是在某种状态下根本不该开仓？
3. 我的持仓时长（平均 {{ stats.avg_hold_ms | dur }}）与结果之间有关系吗？是拿不住还是拿太久？
4. **假设我下一笔交易必须改掉一个习惯**，你建议改哪个？为什么是这个而不是别的？

## 请遵守

- 只用上面列出的数据下结论；数据不可得的地方**直接说不可得**，不要推测。
- 区分「数据能证明的」和「你觉得可能的」。
- {% if stats.total < 10 %}本次样本只有 {{ stats.total }} 笔，请明确说明统计结论不可靠，把重点放在**单笔的决策质量**上。{% else %}样本量尚可，但仍请说明结论的置信程度。{% endif %}
