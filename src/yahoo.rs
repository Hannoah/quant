use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::{QuantError, QuantResult, validate_range, validate_symbol};
use crate::types::{StockBar, StockInterval};

/// Yahoo Finance 차트 응답 (`/v8/finance/chart/{symbol}`).
#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: Chart,
}

#[derive(Debug, Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
    error: Option<ChartApiError>,
}

#[derive(Debug, Deserialize)]
struct ChartApiError {
    code: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    timestamp: Option<Vec<i64>>,
    indicators: ChartIndicators,
}

#[derive(Debug, Deserialize)]
struct ChartIndicators {
    quote: Vec<ChartQuote>,
}

#[derive(Debug, Deserialize)]
struct ChartQuote {
    open: Option<Vec<Option<f64>>>,
    high: Option<Vec<Option<f64>>>,
    low: Option<Vec<Option<f64>>>,
    close: Option<Vec<Option<f64>>>,
    volume: Option<Vec<Option<f64>>>,
}

/// Yahoo Finance에서 종목 봉을 가져온다 (키 불필요, 공개 차트 API).
///
/// `symbol`은 Yahoo 형식이다 (삼성전자: `005930.KS`, 코스닥: `.KQ`, 미국: `AAPL`).
pub async fn fetch_stock_bars(
    client: &reqwest::Client,
    symbol: &str,
    interval: StockInterval,
    range: &str,
) -> QuantResult<Vec<StockBar>> {
    validate_symbol(symbol)?;
    validate_range(range)?;

    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{symbol}");
    let response = client
        .get(url)
        .query(&[("interval", interval.yahoo_code()), ("range", range)])
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(QuantError::Api {
            status: status.as_u16(),
            message,
        });
    }

    let body: ChartResponse = response.json().await?;
    let Some(result) = body.chart.result.and_then(|mut v| v.pop()) else {
        let message = body
            .chart
            .error
            .map(|e| {
                e.description
                    .or(e.code)
                    .unwrap_or_else(|| "unknown chart error".to_string())
            })
            .unwrap_or_else(|| "empty chart result".to_string());
        return Err(QuantError::Api {
            status: status.as_u16(),
            message,
        });
    };

    let bars = parse_bars(symbol, &result);
    if bars.is_empty() {
        return Err(QuantError::NoBars {
            symbol: symbol.to_string(),
        });
    }
    Ok(bars)
}

/// 차트 응답 한 건을 `StockBar` 리스트로 바꾼다. 결측 봉은 건너뛴다.
fn parse_bars(symbol: &str, result: &ChartResult) -> Vec<StockBar> {
    let timestamps: &[i64] = result.timestamp.as_deref().unwrap_or(&[]);
    let quote = result.indicators.quote.first();
    let opens: &[Option<f64>] = quote.and_then(|q| q.open.as_deref()).unwrap_or(&[]);
    let highs: &[Option<f64>] = quote.and_then(|q| q.high.as_deref()).unwrap_or(&[]);
    let lows: &[Option<f64>] = quote.and_then(|q| q.low.as_deref()).unwrap_or(&[]);
    let closes: &[Option<f64>] = quote.and_then(|q| q.close.as_deref()).unwrap_or(&[]);
    let volumes: &[Option<f64>] = quote
        .and_then(|q| q.volume.as_deref())
        .unwrap_or(&[]);

    let len = [timestamps.len(), opens.len(), highs.len(), lows.len(), closes.len()]
        .into_iter()
        .min()
        .unwrap_or(0);

    let mut bars = Vec::new();
    for i in 0..len {
        let Some(close) = closes[i] else { continue };
        let Some(moment) = DateTime::<Utc>::from_timestamp(timestamps[i], 0) else {
            continue;
        };
        bars.push(StockBar {
            symbol: symbol.to_string(),
            date: moment.format("%Y-%m-%d").to_string(),
            open: opens[i].unwrap_or(close),
            high: highs[i].unwrap_or(close),
            low: lows[i].unwrap_or(close),
            close,
            volume: volumes.get(i).copied().flatten().unwrap_or(0.0),
        });
    }
    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "chart": {
            "result": [{
                "timestamp": [1767225600, 1767312000],
                "indicators": {"quote": [{
                    "open": [70000.0, 71000.0],
                    "high": [72000.0, 71500.0],
                    "low": [69500.0, 70500.0],
                    "close": [71000.0, null],
                    "volume": [1000000.0, 900000.0]
                }]}
            }],
            "error": null
        }
    }"#;

    fn fixture_result() -> ChartResult {
        let body: ChartResponse =
            serde_json::from_str(FIXTURE).expect("fixture must parse");
        body.chart
            .result
            .and_then(|mut v| v.pop())
            .expect("fixture must contain a result")
    }

    #[test]
    fn parses_yahoo_chart_and_skips_null_close() {
        let bars = parse_bars("005930.KS", &fixture_result());
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].symbol, "005930.KS");
        assert_eq!(bars[0].date, "2026-01-01");
        assert_eq!(bars[0].close, 71000.0);
    }

    #[test]
    fn interval_codes_match_yahoo_params() {
        assert_eq!(StockInterval::Day.yahoo_code(), "1d");
        assert_eq!(StockInterval::Week.yahoo_code(), "1wk");
        assert_eq!(StockInterval::Month.yahoo_code(), "1mo");
    }
}
