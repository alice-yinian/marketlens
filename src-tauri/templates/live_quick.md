# OKX 实盘速览

生成时间：{{ meta.generated_at | ts }}
数据源：{{ meta.data_source }}

{% if meta.warnings %}
## 数据提醒
{% for w in meta.warnings %}
- {{ w }}
{% endfor %}
{% endif %}

## 市场状态

{% for m in market %}
### {{ m.inst_id }}

- 最新价：{{ m.last }}
- 24h 涨跌：{{ m.change_24h_pct | pct }}
- 24h 区间：{{ m.low_24h }} – {{ m.high_24h }}
- 24h 成交额：{{ m.volume_24h_usd | usd }}
- 趋势：{{ m.trend | na("K 线不足，无法判定") }}
- 波动：{{ m.volatility | na("K 线不足，无法判定") }}
- 拥挤度：{{ m.crowding }}
- RSI14：{{ m.rsi14 | na("K 线不足") }}
- ATR：{{ m.atr_pct | na("K 线不足") | pct }}
- 已实现波动：{{ m.realized_vol | na("K 线不足") | pct }}
- 资金费率年化：{{ m.funding_annualized | pct }}
- 基差：{{ m.basis_pct | na("无基差数据") | pct }}
- 多空比：{{ m.long_short_ratio | na("Rubik 指标不可得") }}
- 主动买卖比：{{ m.taker_buy_sell_ratio | na("Rubik 指标不可得") }}
- 持仓量：{{ m.open_interest_usd | na("接口未返回") | usd }}
{% if m.signals %}

规则信号：
{% for s in m.signals %}
- {{ s }}
{% endfor %}
{% endif %}
{% if m.unavailable %}

本标的以下数据不可得：{{ m.unavailable | join("；") }}
{% endif %}

{% endfor %}

## 我的持仓

{% if account.available %}
账户权益：{{ account.equity | money }}（量级 {{ account.equity_magnitude }}）
未实现盈亏：{{ account.unrealized_pnl | money }}
保证金率：{{ account.margin_ratio | na("接口未返回") }}
持仓模式：{{ account.position_mode | na("未知") }}
{% else %}
账户数据不可得：{{ account.reason }}
{% endif %}

{% if positions | length == 0 %}
当前没有持仓。
{% else %}
{% for p in positions %}
### {{ p.inst_id }}

- 方向：{{ p.pos_side | side }}
- 杠杆：{{ p.leverage }}×（{{ p.margin_mode }}）
- 数量：{{ p.contracts | size }} 张
- 开仓均价：{{ p.avg_price }}
- 标记价：{{ p.mark_price }}
- 强平价：{{ p.liquidation_price | na("全仓或无强平价") }}
- 名义价值：{{ p.notional | money }}
- 未实现盈亏：{{ p.unrealized_pnl | money }}（{{ p.unrealized_pnl_ratio | pct }}）
- 保证金率：{{ p.margin_ratio | na("接口未返回") }}
- 持仓时长：{{ (p.updated_at - p.created_at) | dur }}
{% endfor %}
{% endif %}

## 我想知道

请基于以上数据回答：

1. 当前市场状态对我的持仓是**顺风还是逆风**？依据是哪些具体指标？
2. 我的持仓在当前状态下**最需要警惕什么**？请指出最脆弱的一环。
3. 有哪些数据是不可得的？这些缺失会让你的判断**在哪些方面不可靠**？

请明确区分「数据支持的判断」和「你的推测」。
