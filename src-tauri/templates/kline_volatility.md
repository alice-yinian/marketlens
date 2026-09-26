# {{ instrument.inst_id }} 波动与风险体检

生成时间：{{ meta.generated_at | ts }}
数据源：{{ meta.data_source }} · 标的：{{ instrument.inst_id }}{% if instrument.base_ccy %}（{{ instrument.base_ccy }}/{{ instrument.quote_ccy }}）{% endif %}

{% if meta.warnings %}
## 数据提醒

{% for w in meta.warnings %}
- {{ w }}
{% endfor %}
{% endif %}

## 你要回答的问题

下面给出 {{ series | length }} 个周期的原始 K 线与指标。请围绕**波动与风险**回答：

1. **当前波动处于什么水平**：与该周期自身的历史相比是偏高还是偏低（用波动率列判断，不要凭价格涨跌幅猜）
2. **波动是在放大还是收敛**：给出具体依据（哪一段开始变化）
3. **单根 K 线的平均振幅**：用 ATR% 说明「一根 K 线通常走多远」，并用它换算关键价位之间的空间
4. **异常波动**：有没有明显放大的单根 K 线，之前之后各发生了什么
5. **不同周期的波动是否一致**：{% if series | length > 1 %}短周期高波动 + 长周期平静这类组合意味着什么{% else %}只有一个周期时，说明这个波动读数覆盖的时间范围{% endif %}

## 数据说明（先读这段）

- 时间列是**本地时间**；`open` / `high` / `low` / `close` 以 {{ instrument.quote_ccy | na("该合约的计价币") }} 计
- `vol` 是**合约张数**，不是成交金额
- 指标列里的 `—` 表示**该根 K 线样本不足**（不是 0）
- `ATR%` 是平均真实波幅占收盘价的比例；`RVol` 是**已实现波动率**，按该周期年化
{% for s in series %}
- {{ s.bar_label }}：最近 {{ s.candle_count }} 根{% if s.from %}，覆盖 {{ s.from | ts }} → {{ s.to | ts }}{% endif %}{% if s.last_candle %}；最后一根（{{ s.last_candle.time }}）{% if s.last_candle.confirmed %}已收盘{% else %}**尚未收盘**，它的振幅还会变{% endif %}{% endif %}
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

- 波动高低的判断必须有**同一周期内**的比较依据，不要拿不同周期直接比数字
- 换算出「一根 K 线通常走多远」这类结论时，把用到的列与数值写出来
- 数据不足以支撑判断时**直接说不确定**，不要编造一个看起来合理的波动区间
