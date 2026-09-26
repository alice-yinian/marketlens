# {{ instrument.inst_id }} K 线结构分析

生成时间：{{ meta.generated_at | ts }}
数据源：{{ meta.data_source }} · 标的：{{ instrument.inst_id }}{% if instrument.base_ccy %}（{{ instrument.base_ccy }}/{{ instrument.quote_ccy }}）{% endif %}

{% if meta.warnings %}
## 数据提醒

{% for w in meta.warnings %}
- {{ w }}
{% endfor %}
{% endif %}

## 你要回答的问题

{% if series | length == 1 %}下面是 **1 个周期**的原始 K 线与指标。请判断：

1. **趋势结构**：更高高点 / 更低低点是否成立，当前处于推动段还是调整段
2. **关键价位**：区间边界（前高 / 前低）、成交密集区、整数关口
3. **量价关系**：放量是推动还是滞涨，缩量回调是否健康
4. **指标与价格的配合**：均线与价格的相对位置、强弱指标是否与价格背离
5. **结论的适用范围**：只有一个周期，请说明这个判断在什么条件下会失效
{% else %}下面是 **{{ series | length }} 个周期**的原始 K 线与指标。请判断：

1. **每个周期各自的结构**：更高高点 / 更低低点是否成立，处于推动段还是调整段
2. **多周期是否一致**：{% for s in series %}{{ s.bar_label }}{% if not loop.last %}、{% endif %}{% endfor %} 给出的方向是否冲突
3. **冲突时以哪个为准**：说明理由（哪个是主导周期、哪个更可能是噪声）
4. **关键价位**：区间边界（前高 / 前低）、成交密集区、整数关口
5. **量价关系与指标配合**：放量是推动还是滞涨；均线、强弱指标是否与价格背离
{% endif %}

## 数据说明（先读这段）

- 时间列是**本地时间**；`open` / `high` / `low` / `close` 以 {{ instrument.quote_ccy | na("该合约的计价币") }} 计
- `vol` 是**合约张数**，不是成交金额——比较放量 / 缩量时只能看它的相对大小
- 指标列里的 `—` 表示**该根 K 线样本不足**（不是 0）；表头下方会写明每列从第几根开始才有值
{% for s in series %}
- {{ s.bar_label }}：最近 {{ s.candle_count }} 根{% if s.from %}，覆盖 {{ s.from | ts }} → {{ s.to | ts }}{% endif %}{% if s.last_candle %}；最后一根（{{ s.last_candle.time }}）{% if s.last_candle.confirmed %}已收盘{% else %}**尚未收盘**，其 OHLC 仍会变化，不要把它当成已定型的收盘价{% endif %}{% endif %}
{% endfor %}

{% for s in series %}{% if s.unavailable %}
**{{ s.bar_label }} 的缺失声明**

{% for u in s.unavailable %}
- {{ u }}
{% endfor %}
{% endif %}{% endfor %}

## 各周期数据

{% for s in series %}
### {{ s.inst_id }} · {{ s.bar_label }}（{{ s.bar }}）

{% if s.indicators %}
指标口径：
{% for i in s.indicators %}
- `{{ i.name }}`：{{ i.description }}{% if not i.available %} —— **本列不可得**（{{ i.reason }}）{% elif i.insufficient_bars > 0 %} —— 前 {{ i.insufficient_bars }} 根为 `—`（样本不足）{% endif %}
{% endfor %}
{% endif %}

{{ s.rows | table(s.columns) }}

{% endfor %}

## 回答要求

- 每个结论都要指出**依据哪一段数据**（哪个周期 + 哪段时间），不要只给结论
- 数据不足以支撑判断时**直接说不确定**，不要用「可能」「或许」把空白补上
- 如果不同周期给出的结论冲突，先把冲突原样摆出来，再说明你倾向哪一边以及为什么
