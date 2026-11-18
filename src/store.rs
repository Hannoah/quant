use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};

use crate::error::QuantResult;
use crate::types::{Candle, Interval, StockBar, StockInterval};

/// `show`가 읽을 수 있는 데이터셋 (헤더로 자동 구분).
#[derive(Debug, Clone, PartialEq)]
pub enum Dataset {
    Candles(Vec<Candle>),
    Stocks(Vec<StockBar>),
}

impl Dataset {
    pub fn len(&self) -> usize {
        match self {
            Dataset::Candles(rows) => rows.len(),
            Dataset::Stocks(rows) => rows.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 앞 N개만 잘라낸다.
    #[cfg(test)]
    pub fn head(&self, limit: usize) -> Dataset {
        match self {
            Dataset::Candles(rows) => {
                Dataset::Candles(rows.iter().take(limit).cloned().collect())
            }
            Dataset::Stocks(rows) => Dataset::Stocks(rows.iter().take(limit).cloned().collect()),
        }
    }

    /// 뒤(최신) N개만 잘라낸다.
    #[cfg(test)]
    pub fn tail(&self, limit: usize) -> Dataset {
        match self {
            Dataset::Candles(rows) => Dataset::Candles(
                rows.iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect(),
            ),
            Dataset::Stocks(rows) => Dataset::Stocks(
                rows.iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect(),
            ),
        }
    }

    /// 날짜/시각 문자열 부분 일치로 행을 필터링한다.
    pub fn filter_date(&self, query: &str) -> Dataset {
        let q = query.trim();
        if q.is_empty() {
            return self.clone();
        }
        match self {
            Dataset::Candles(rows) => Dataset::Candles(
                rows.iter()
                    .filter(|r| r.candle_date_time_kst.contains(q) || r.candle_date_time_utc.contains(q))
                    .cloned()
                    .collect(),
            ),
            Dataset::Stocks(rows) => Dataset::Stocks(
                rows.iter()
                    .filter(|r| r.date.contains(q))
                    .cloned()
                    .collect(),
            ),
        }
    }

    /// 종가 시계열을 반환한다 (스파크라인 렌더링용).
    pub fn closes(&self) -> Vec<f64> {
        match self {
            Dataset::Candles(rows) => rows.iter().map(|c| c.trade_price).collect(),
            Dataset::Stocks(rows) => rows.iter().map(|b| b.close).collect(),
        }
    }

    /// 데이터셋의 대표 종목/마켓 코드를 반환한다.
    #[cfg(test)]
    pub fn asset_code(&self) -> Option<&str> {
        match self {
            Dataset::Candles(rows) => rows.last().map(|c| c.market.as_str()),
            Dataset::Stocks(rows) => rows.last().map(|b| b.symbol.as_str()),
        }
    }

    /// 연속된 캔들 간격을 분석해 Upbit `Interval`을 추론한다.
    pub fn infer_crypto_interval(&self) -> Interval {
        let Dataset::Candles(rows) = self else {
            return Interval::Days;
        };
        let mut min_mins: Option<i64> = None;
        for w in rows.windows(2) {
            if let (Ok(a), Ok(b)) = (
                NaiveDateTime::parse_from_str(&w[0].candle_date_time_kst, "%Y-%m-%dT%H:%M:%S"),
                NaiveDateTime::parse_from_str(&w[1].candle_date_time_kst, "%Y-%m-%dT%H:%M:%S"),
            ) {
                let mins = (b - a).num_minutes().abs();
                if mins > 0 {
                    min_mins = Some(min_mins.map_or(mins, |m| m.min(mins)));
                }
            }
        }
        match min_mins.unwrap_or(1440) {
            0..=5 => Interval::Minutes1,
            6..=45 => Interval::Minutes30,
            46..=120 => Interval::Minutes60,
            121..=600 => Interval::Minutes240,
            601..=4320 => Interval::Days,
            4321..=20000 => Interval::Weeks,
            _ => Interval::Months,
        }
    }

    /// 연속된 봉 날짜 간격을 분석해 Yahoo `StockInterval`을 추론한다.
    pub fn infer_stock_interval(&self) -> StockInterval {
        let Dataset::Stocks(rows) = self else {
            return StockInterval::Day;
        };
        let mut min_days: Option<i64> = None;
        for w in rows.windows(2) {
            if let (Ok(a), Ok(b)) = (
                NaiveDate::parse_from_str(&w[0].date, "%Y-%m-%d"),
                NaiveDate::parse_from_str(&w[1].date, "%Y-%m-%d"),
            ) {
                let days = (b - a).num_days().abs();
                if days > 0 {
                    min_days = Some(min_days.map_or(days, |d| d.min(days)));
                }
            }
        }
        match min_days.unwrap_or(1) {
            0..=4 => StockInterval::Day,
            5..=14 => StockInterval::Week,
            _ => StockInterval::Month,
        }
    }
}

/// CSV 헤더를 보고 코인 봉인지 주식 봉인지 구분해서 읽는다.
pub fn load_dataset(path: &Path) -> QuantResult<Dataset> {
    let mut reader = csv::Reader::from_path(path)?;
    let is_candles = reader
        .headers()?
        .iter()
        .any(|name| name == "market");
    if is_candles {
        let mut rows = Vec::new();
        for record in reader.deserialize() {
            let row: Candle = record?;
            rows.push(row);
        }
        Ok(Dataset::Candles(rows))
    } else {
        let mut rows = Vec::new();
        for record in reader.deserialize() {
            let row: StockBar = record?;
            rows.push(row);
        }
        Ok(Dataset::Stocks(rows))
    }
}

/// 데이터셋 종류에 맞는 현황 요약을 만든다.
pub fn format_dataset_summary(dataset: &Dataset) -> String {
    match dataset {
        Dataset::Candles(rows) => format_summary(rows),
        Dataset::Stocks(rows) => format_stock_summary(rows),
    }
}

/// 봉 리스트를 CSV 파일로 저장한다. 부모 디렉터리는 자동 생성한다.
pub fn save_candles(path: &Path, candles: &[Candle]) -> QuantResult<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(path)?;
    for candle in candles {
        writer.serialize(candle)?;
    }
    writer.flush()?;
    Ok(())
}

/// 주식 봉 리스트를 CSV 파일로 저장한다. 부모 디렉터리는 자동 생성한다.
pub fn save_stock_bars(path: &Path, bars: &[StockBar]) -> QuantResult<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(path)?;
    for bar in bars {
        writer.serialize(bar)?;
    }
    writer.flush()?;
    Ok(())
}

/// 주식 봉 화면 표시용 고정폭 표를 만든다.
#[cfg(test)]
pub fn format_stock_table(bars: &[StockBar]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<12} {:>12} {:>12} {:>12} {:>12} {:>14}\n",
        "date", "open", "high", "low", "close", "volume"
    ));
    for bar in bars {
        out.push_str(&format!(
            "{:<12} {:>12.2} {:>12.2} {:>12.2} {:>12.2} {:>14.0}\n",
            bar.date, bar.open, bar.high, bar.low, bar.close, bar.volume
        ));
    }
    out
}

/// 주식 봉 리스트의 현황 요약을 만든다.
pub fn format_stock_summary(bars: &[StockBar]) -> String {
    let Some(first) = bars.first() else {
        return "no data".to_string();
    };
    let Some(last) = bars.last() else {
        return "no data".to_string();
    };
    let mut high = f64::MIN;
    let mut low = f64::MAX;
    for bar in bars {
        high = high.max(bar.high);
        low = low.min(bar.low);
    }
    let change_pct = if first.open == 0.0 {
        0.0
    } else {
        (last.close - first.open) / first.open * 100.0
    };
    format!(
        "{} | {} bars | {} ~ {} | latest {:.2} | high {:.2} / low {:.2} | {:+.2}%",
        last.symbol,
        bars.len(),
        first.date,
        last.date,
        last.close,
        high,
        low,
        change_pct
    )
}

/// 전체 봉 리스트의 현황 요약을 만든다 (마켓, 기간, 최신가, 등락률).
pub fn format_summary(candles: &[Candle]) -> String {
    let Some(first) = candles.first() else {
        return "no data".to_string();
    };
    let Some(last) = candles.last() else {
        return "no data".to_string();
    };
    let mut high = f64::MIN;
    let mut low = f64::MAX;
    for candle in candles {
        high = high.max(candle.high_price);
        low = low.min(candle.low_price);
    }
    let change_pct = if first.opening_price == 0.0 {
        0.0
    } else {
        (last.trade_price - first.opening_price) / first.opening_price * 100.0
    };
    format!(
        "{} | {} rows | {} ~ {} | latest {:.2} | high {:.2} / low {:.2} | {:+.2}%",
        last.market,
        candles.len(),
        first.candle_date_time_kst,
        last.candle_date_time_kst,
        last.trade_price,
        high,
        low,
        change_pct
    )
}

/// 화면 표시용 고정폭 표를 만든다.
#[cfg(test)]
pub fn format_table(candles: &[Candle]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<20} {:>12} {:>12} {:>12} {:>12} {:>14}\n",
        "kst", "open", "high", "low", "close", "volume"
    ));
    for candle in candles {
        out.push_str(&format!(
            "{:<20} {:>12.2} {:>12.2} {:>12.2} {:>12.2} {:>14.4}\n",
            candle.candle_date_time_kst,
            candle.opening_price,
            candle.high_price,
            candle.low_price,
            candle.trade_price,
            candle.candle_acc_trade_volume
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Candle> {
        vec![Candle {
            market: "KRW-BTC".to_string(),
            candle_date_time_utc: "2026-01-01T00:00:00".to_string(),
            candle_date_time_kst: "2026-01-01T09:00:00".to_string(),
            opening_price: 100.0,
            high_price: 110.0,
            low_price: 90.0,
            trade_price: 105.0,
            timestamp: 1767225600000,
            candle_acc_trade_price: 12345.0,
            candle_acc_trade_volume: 120.5,
        }]
    }

    #[test]
    fn csv_roundtrip_preserves_candles() {
        let dir = tempfile::tempdir().expect("tempdir must exist");
        let path = dir.path().join("candles.csv");
        save_candles(&path, &sample()).expect("save must succeed");
        let loaded = load_dataset(&path).expect("load must succeed");
        assert_eq!(loaded, Dataset::Candles(sample()));
    }

    #[test]
    fn table_contains_header_and_row() {
        let table = format_table(&sample());
        assert!(table.contains("kst"));
        assert!(table.contains("2026-01-01T09:00:00"));
    }

    #[test]
    fn summary_reports_latest_and_change() {
        let summary = format_summary(&sample());
        assert!(summary.contains("KRW-BTC"));
        assert!(summary.contains("105.00"));
    }

    #[test]
    fn summary_handles_empty() {
        assert_eq!(format_summary(&[]), "no data");
    }

    fn stock_sample() -> Vec<StockBar> {
        vec![
            StockBar {
                symbol: "005930.KS".to_string(),
                date: "2026-01-01".to_string(),
                open: 70000.0,
                high: 72000.0,
                low: 69500.0,
                close: 71000.0,
                volume: 1000000.0,
            },
            StockBar {
                symbol: "005930.KS".to_string(),
                date: "2026-01-02".to_string(),
                open: 71000.0,
                high: 73000.0,
                low: 70000.0,
                close: 72500.0,
                volume: 900000.0,
            },
        ]
    }

    #[test]
    fn stock_csv_roundtrip_and_detect() {
        let dir = tempfile::tempdir().expect("tempdir must exist");
        let path = dir.path().join("stocks.csv");
        save_stock_bars(&path, &stock_sample()).expect("save must succeed");
        let dataset = load_dataset(&path).expect("load must succeed");
        assert_eq!(dataset, Dataset::Stocks(stock_sample()));
        assert_eq!(dataset.len(), 2);
    }

    #[test]
    fn stock_table_and_summary() {
        let table = format_stock_table(&stock_sample());
        assert!(table.contains("2026-01-01"));
        let summary = format_stock_summary(&stock_sample());
        assert!(summary.contains("005930.KS"));
        assert!(summary.contains("72500.00"));
        assert_eq!(format_stock_summary(&[]), "no data");
    }

    #[test]
    fn dataset_head_and_tail_keep_order() {
        let dataset = Dataset::Stocks(stock_sample());
        let head = dataset.head(1);
        let tail = dataset.tail(1);
        assert_eq!(head.len(), 1);
        assert_eq!(tail.len(), 1);
        if let Dataset::Stocks(rows) = head {
            assert_eq!(rows[0].date, "2026-01-01");
        } else {
            panic!("expected stocks");
        }
        if let Dataset::Stocks(rows) = tail {
            assert_eq!(rows[0].date, "2026-01-02");
        } else {
            panic!("expected stocks");
        }
    }

    #[test]
    fn dataset_filter_closes_and_interval_inference() {
        let dataset = Dataset::Stocks(stock_sample());
        assert_eq!(dataset.asset_code(), Some("005930.KS"));
        assert_eq!(dataset.closes(), vec![71000.0, 72500.0]);
        assert_eq!(dataset.infer_stock_interval(), StockInterval::Day);

        let filtered = dataset.filter_date("2026-01-02");
        assert_eq!(filtered.len(), 1);
    }
}
