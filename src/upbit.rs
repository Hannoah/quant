use crate::error::{QuantError, QuantResult, validate_count, validate_market};
use crate::types::{Candle, Interval};

/// Upbit 공개 API로 봉 데이터를 가져온다.
///
/// `to` 없이 최신 봉부터 `count`개를 반환한다.
pub async fn fetch_candles(
    client: &reqwest::Client,
    market: &str,
    interval: Interval,
    count: u32,
) -> QuantResult<Vec<Candle>> {
    validate_market(market)?;
    validate_count(count)?;

    let response = client
        .get(interval.endpoint())
        .query(&[("market", market), ("count", &count.to_string())])
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

    let candles: Vec<Candle> = response.json().await?;
    if candles.is_empty() {
        return Err(QuantError::EmptyData {
            market: market.to_string(),
        });
    }
    Ok(candles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_covers_all_intervals() {
        for interval in [
            Interval::Days,
            Interval::Weeks,
            Interval::Months,
            Interval::Minutes1,
            Interval::Minutes30,
            Interval::Minutes60,
            Interval::Minutes240,
        ] {
            assert!(interval.endpoint().starts_with("https://api.upbit.com/"));
        }
    }
}
