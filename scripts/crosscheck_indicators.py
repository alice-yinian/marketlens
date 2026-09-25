#!/usr/bin/env python3
"""指标公式的独立交叉验证。

思路：Rust 侧的单元测试用的是我自己对公式的预期——如果公式本身理解错了，
测试会一起错。这里用 Python **独立实现**同一套公式，用**同一份输入**重算，
逐位比对。

精确比对的前提是输入相同，所以输入由 Rust 侧落盘：

    cd src-tauri
    cargo test --lib dump_indicators_for_crosscheck -- --ignored
    cd .. && python3 scripts/crosscheck_indicators.py

用法（不带参数）：读取 Rust 落盘的输入并精确比对。
"""
import json
import math
import sys
from pathlib import Path

DUMP = Path(__file__).resolve().parent.parent / "src-tauri/target/indicator-crosscheck.json"

# 与 Rust 侧 `market/live.rs` 的常量保持一致
PERIODS_PER_YEAR_HOURLY = 24 * 365
VOL_WINDOW = 24
# 浮点运算顺序不同会产生末位差异，容差取相对 1e-9
TOLERANCE = 1e-9


def ema(closes: list[float], period: int) -> float:
    """种子 = 前 period 根的简单平均，alpha = 2/(period+1)。"""
    alpha = 2.0 / (period + 1.0)
    value = sum(closes[:period]) / period
    for close in closes[period:]:
        value = alpha * close + (1.0 - alpha) * value
    return value


def rsi_wilder(closes: list[float], period: int) -> float:
    gains = losses = 0.0
    for i in range(1, period + 1):
        delta = closes[i] - closes[i - 1]
        if delta >= 0:
            gains += delta
        else:
            losses -= delta
    avg_gain, avg_loss = gains / period, losses / period
    for i in range(period + 1, len(closes)):
        delta = closes[i] - closes[i - 1]
        gain, loss = (delta, 0.0) if delta >= 0 else (0.0, -delta)
        avg_gain = (avg_gain * (period - 1) + gain) / period
        avg_loss = (avg_loss * (period - 1) + loss) / period
    if avg_loss == 0:
        return 50.0 if avg_gain == 0 else 100.0
    return 100.0 - 100.0 / (1.0 + avg_gain / avg_loss)


def realized_vol(closes: list[float], periods_per_year: float) -> float:
    """对数收益率的样本标准差 × √年化周期数。"""
    rets = [math.log(b / a) for a, b in zip(closes, closes[1:]) if a > 0 and b > 0]
    mean = sum(rets) / len(rets)
    var = sum((r - mean) ** 2 for r in rets) / (len(rets) - 1)
    return math.sqrt(var) * math.sqrt(periods_per_year)


def atr_pct(highs: list[float], lows: list[float], closes: list[float], period: int) -> float:
    """真实波幅的 Wilder 平滑，再除以最新收盘价。"""
    trs = []
    for i in range(1, len(closes)):
        trs.append(
            max(
                highs[i] - lows[i],
                abs(highs[i] - closes[i - 1]),
                abs(lows[i] - closes[i - 1]),
            )
        )
    value = sum(trs[:period]) / period
    for tr in trs[period:]:
        value = (value * (period - 1) + tr) / period
    return value / closes[-1]


def close_enough(a: float, b: float) -> bool:
    scale = max(abs(a), abs(b), 1e-12)
    return abs(a - b) / scale < TOLERANCE


def main() -> int:
    if not DUMP.exists():
        print(f"找不到 {DUMP}", file=sys.stderr)
        print(
            "请先运行：cd src-tauri && cargo test --lib dump_indicators_for_crosscheck -- --ignored",
            file=sys.stderr,
        )
        return 2

    data = json.loads(DUMP.read_text())
    closes = data["closes"]
    highs = data["highs"]
    lows = data["lows"]
    print(f"输入：{data['inst_id']} {data['bar']}，{len(closes)} 根 K 线（由 Rust 落盘）\n")

    # Python 独立重算
    recomputed = {
        "ema20": ema(closes, 20),
        "ema60": ema(closes, 60),
        "ema200": ema(closes, 200),
        "rsi14": rsi_wilder(closes, 14),
        "atr_pct": atr_pct(highs, lows, closes, 14),
        "realized_vol_24h": realized_vol(
            closes[len(closes) - 1 - VOL_WINDOW :], PERIODS_PER_YEAR_HOURLY
        ),
    }

    failures = 0
    print(f"{'指标':<20}{'Rust':>22}{'Python':>22}{'相对差':>12}")
    for key, python_value in recomputed.items():
        rust_value = data[key]
        if rust_value is None:
            print(f"{key:<20}{'Rust 未产出':>22}{python_value:>22.10f}{'-':>12}")
            failures += 1
            continue
        ok = close_enough(python_value, rust_value)
        failures += 0 if ok else 1
        rel = abs(python_value - rust_value) / max(abs(rust_value), 1e-12)
        print(
            f"{'✅ ' if ok else '❌ '}{key:<17}{rust_value:>22.10f}{python_value:>22.10f}{rel:>12.2e}"
        )

    if failures:
        print(f"\n❌ {failures} 项不一致——公式实现存在真实差异")
        return 1

    print("\n✅ 全部一致：Rust 与独立 Python 实现使用同一套公式")
    return 0


if __name__ == "__main__":
    sys.exit(main())
