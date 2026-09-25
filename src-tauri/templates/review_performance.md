# 交易复盘：绩效与归因

时段：{{ period.from | ts_date }} → {{ period.to | ts_date }}（K 线粒度 {{ period.bar }}）

## 总体表现

- 交易笔数：{{ stats.total }}
- 胜率：{{ stats.win_rate | pct }}
- 盈亏比：{% if stats.profit_factor %}{{ stats.profit_factor | num(2) }}{% else %}不可得（无盈利笔或无亏损笔）{% endif %}
- 期望值（每笔）：{{ stats.expectancy | money }}
- 累计已实现盈亏：{{ stats.total_realized_pnl | money }}
- 平均持仓时长：{{ stats.avg_hold_ms | dur }}
- 费用侵蚀：{% if stats.fee_drag %}{{ stats.fee_drag | pct }}{% else %}不可得（无已实现盈亏）{% endif %}
{% if stats.fee_drag and stats.fee_drag > 0.3 %}

> 费用侵蚀偏高：手续费与资金费吃掉了已实现盈亏的 {{ stats.fee_drag | pct }}。
{% endif %}

## 按开仓时市场状态分组

{% for group in stats.by_trend %}
- **{{ group.key }}**{% if group.is_unattributed %}（数据缺失，无法归类）{% endif %}：{{ group.count }} 笔｜胜率 {{ group.win_rate | pct }}｜合计 {{ group.total_pnl | money }}
{% endfor %}

## 按方向分组

{% for group in stats.by_direction %}
- **{{ group.key }}**：{{ group.count }} 笔｜胜率 {{ group.win_rate | pct }}｜合计 {{ group.total_pnl | money }}
{% endfor %}

## 逐笔明细

| 标的 | 方向 | 杠杆 | 开仓价 | 平仓价 | 已实现盈亏 | 收益率 | 手续费 | 开仓时间 | 持仓时长 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
{% for p in positions %}
| {{ p.inst_id }} | {{ p.direction | side }} | {{ p.leverage | num(0) }}× | {{ p.open_price | price }} | {{ p.close_price | price }} | {{ p.realized_pnl | money }} | {{ p.pnl_ratio | pct }} | {{ p.fee | money }} | {{ p.open_time | ts }} | {{ p.hold_ms | dur }} |
{% endfor %}

## 逐笔的开仓时刻状态

{% for p in positions %}
### {{ p.inst_id }}（{{ p.open_time | ts }}）

{% if p.regime %}
- 趋势：{{ p.regime.trend | na("不可得") }}｜波动：{{ p.regime.volatility | na("不可得") }}｜拥挤度：{{ p.regime.crowding | na("不可得") }}
- 开仓前 24h 涨跌：{{ p.regime.change_24h_pct | na("不可得") | pct }}
- 开仓时资金费率年化：{{ p.regime.funding_annualized | na("不可得") | pct }}
- 开仓时基差：{{ p.regime.basis_pct | na("不可得") | pct }}
{% else %}
开仓时刻的市场状态**不可得**——当时没有本地留痕，且该时刻已超出 K 线可取范围。
{% endif %}
- 持仓期间最大浮盈：{{ p.max_favorable | na("需本地留痕") | money }}
- 持仓期间最大浮亏：{{ p.max_adverse | na("需本地留痕") | money }}
{% endfor %}

## 数据可信度

- 官方历史仓位：{{ data_quality.from_official }} 笔
- 仅本地留痕：{{ data_quality.from_local }} 笔
- 被本地补充极值：{{ data_quality.enriched }} 笔
- {{ data_quality.note }}
- {{ data_quality.trace_note }}

## 我想知道

1. 我的**哪种开仓时状态**表现最好、哪种最差？样本量是否足以支撑这个结论？
2. 我的亏损是**方向判断错了**，还是**进出场时机错了**？请用逐笔数据支持你的判断。
3. 费用侵蚀到什么程度？如果要改进，应该从**减少交易频率**还是**延长持仓**入手？
4. 基于以上数据，最值得改进的**一件事**是什么？请只给一件。

**重要**：请说明哪些结论受样本量限制、哪些受数据缺失限制。样本少于 10 笔时请明确说「不足以得出统计结论」。
