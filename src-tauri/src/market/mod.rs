//! 市场状态引擎（设计文档 §6.3）。

pub mod indicators;
pub mod live;
pub mod reconstruct;
pub mod regime;

/// 单根 K 线。已从 OKX 的字符串数组解析为数值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candle {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub vol: f64,
    /// `true` = 已收盘。已收盘的 K 线不会再变，可永久缓存（ADR #12）。
    pub confirm: bool,
}

impl Candle {
    /// OKX 的 K 线是 9 元素字符串数组：
    /// `[ts, open, high, low, close, vol, volCcy, volQuote, confirm]`
    ///
    /// 少于 6 元素视为坏行并丢弃：宁可少一根 K 线，也不要一根字段错位的 K 线。
    pub fn parse_row(row: &[String]) -> Option<Self> {
        if row.len() < 6 {
            return None;
        }
        Some(Self {
            ts: row[0].parse().ok()?,
            open: row[1].parse().ok()?,
            high: row[2].parse().ok()?,
            low: row[3].parse().ok()?,
            close: row[4].parse().ok()?,
            vol: row[5].parse().ok()?,
            // 缺 confirm 字段时按「未收盘」处理——保守方向：不会被永久缓存
            confirm: row.get(8).is_some_and(|flag| flag == "1"),
        })
    }

    /// 解析整批 K 线并**按时间升序**排列。
    ///
    /// OKX 返回的是最新在前；指标计算全部假设时间升序，
    /// 在这里统一排序可以避免每个调用点各自记着这件事。
    pub fn parse_rows(rows: &[Vec<String>]) -> Vec<Self> {
        let mut candles: Vec<Self> = rows.iter().filter_map(|row| Self::parse_row(row)).collect();
        candles.sort_unstable_by_key(|candle| candle.ts);
        candles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_candle_row() {
        // 真机响应样本（截取）
        let row: Vec<String> = [
            "1790319600000",
            "84005",
            "84063",
            "83680.3",
            "83823.5",
            "307697.29",
            "3076.9729",
            "258002363.4629",
            "0",
        ]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();

        let candle = Candle::parse_row(&row).expect("解析失败");
        assert_eq!(candle.ts, 1_790_319_600_000);
        assert!((candle.open - 84_005.0).abs() < 1e-9);
        assert!((candle.high - 84_063.0).abs() < 1e-9);
        assert!((candle.low - 83_680.3).abs() < 1e-9);
        assert!((candle.close - 83_823.5).abs() < 1e-9);
        assert!(!candle.confirm, "\"0\" 表示未收盘");
    }

    #[test]
    fn rejects_malformed_rows() {
        assert!(Candle::parse_row(&[]).is_none());
        assert!(Candle::parse_row(&["1".into(), "2".into()]).is_none());
        let bad: Vec<String> = ["x", "1", "1", "1", "1", "1"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert!(Candle::parse_row(&bad).is_none(), "时间戳无法解析应丢弃");
    }

    #[test]
    fn parse_rows_sorts_ascending() {
        let mk = |ts: &str| -> Vec<String> {
            [ts, "1", "2", "0.5", "1.5", "10", "0", "0", "1"]
                .iter()
                .map(std::string::ToString::to_string)
                .collect()
        };
        let rows = vec![mk("3000"), mk("1000"), mk("2000")];
        let candles = Candle::parse_rows(&rows);
        assert_eq!(
            candles.iter().map(|c| c.ts).collect::<Vec<_>>(),
            vec![1000, 2000, 3000]
        );
        assert!(candles[0].confirm);
    }
}
