use serde::{Deserialize, Serialize};

/// Upbit candle (days/weeks/months/minutes 공통 필드).
///
/// Upbit가 내려주는 JSON 키 이름을 그대로 유지한다.
/// 가격은 `f64`, 시각은 소유 `String`으로 받는다 (CONVENTIONS 2: 소유 타입 우선).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candle {
    pub market: String,
    pub candle_date_time_utc: String,
    pub candle_date_time_kst: String,
    pub opening_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub trade_price: f64,
    pub timestamp: i64,
    pub candle_acc_trade_price: f64,
    pub candle_acc_trade_volume: f64,
}

/// Candle 봉 간격. CLI에서 값으로 직접 받는다 (Clap 4 derive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Interval {
    /// 일봉 (`/v1/candles/days`)
    Days,
    /// 주봉 (`/v1/candles/weeks`)
    Weeks,
    /// 월봉 (`/v1/candles/months`)
    Months,
    /// 1분봉 (`/v1/candles/minutes/1`)
    Minutes1,
    /// 30분봉 (`/v1/candles/minutes/30`)
    Minutes30,
    /// 60분봉 (`/v1/candles/minutes/60`)
    Minutes60,
    /// 240분봉 (`/v1/candles/minutes/240`)
    Minutes240,
}

impl Interval {
    /// Upbit API 경로를 반환한다.
    pub fn endpoint(&self) -> &'static str {
        match self {
            Interval::Days => "https://api.upbit.com/v1/candles/days",
            Interval::Weeks => "https://api.upbit.com/v1/candles/weeks",
            Interval::Months => "https://api.upbit.com/v1/candles/months",
            Interval::Minutes1 => "https://api.upbit.com/v1/candles/minutes/1",
            Interval::Minutes30 => "https://api.upbit.com/v1/candles/minutes/30",
            Interval::Minutes60 => "https://api.upbit.com/v1/candles/minutes/60",
            Interval::Minutes240 => "https://api.upbit.com/v1/candles/minutes/240",
        }
    }
}

/// 주식 봉 (Yahoo Finance 차트 API 기준).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockBar {
    pub symbol: String,
    /// 일자 (`YYYY-MM-DD`, Yahoo 타임스탬프를 UTC 날짜로 변환)
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// 주식 봉 간격 (Yahoo `interval` 파라미터에 대응).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum StockInterval {
    /// 일봉 (`1d`)
    Day,
    /// 주봉 (`1wk`)
    Week,
    /// 월봉 (`1mo`)
    Month,
}

impl StockInterval {
    /// Yahoo Finance `interval` 파라미터 값을 반환한다.
    pub fn yahoo_code(&self) -> &'static str {
        match self {
            StockInterval::Day => "1d",
            StockInterval::Week => "1wk",
            StockInterval::Month => "1mo",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_endpoints_match_upbit_paths() {
        assert_eq!(
            Interval::Days.endpoint(),
            "https://api.upbit.com/v1/candles/days"
        );
        assert_eq!(
            Interval::Minutes60.endpoint(),
            "https://api.upbit.com/v1/candles/minutes/60"
        );
    }

    #[test]
    fn candle_deserializes_upbit_day_payload() {
        let payload = r#"[{
            "market": "KRW-BTC",
            "candle_date_time_utc": "2026-01-01T00:00:00",
            "candle_date_time_kst": "2026-01-01T09:00:00",
            "opening_price": 100.0,
            "high_price": 110.0,
            "low_price": 90.0,
            "trade_price": 105.0,
            "timestamp": 1767225600000,
            "candle_acc_trade_price": 12345.0,
            "candle_acc_trade_volume": 120.5
        }]"#;
        let candles: Vec<Candle> =
            serde_json::from_str(payload).expect("fixture must parse");
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].market, "KRW-BTC");
        assert_eq!(candles[0].trade_price, 105.0);
    }
}
