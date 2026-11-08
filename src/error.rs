use thiserror::Error;

/// 도메인 에러는 `thiserror`, 앱 레벨에서는 `anyhow::Result`로 감싼다.
#[derive(Debug, Error)]
pub enum QuantError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),

    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("upbit api error (status {status}): {message}")]
    Api { status: u16, message: String },

    #[error("invalid input for field `{field}`: {reason}")]
    Validation { field: String, reason: String },

    #[error("no candles returned for market `{market}`")]
    EmptyData { market: String },

    #[error("no bars returned for symbol `{symbol}`")]
    NoBars { symbol: String },
}

pub type QuantResult<T> = Result<T, QuantError>;

/// `market`은 `XXX-YYY` 형태여야 한다 (예: `KRW-BTC`).
pub fn validate_market(market: &str) -> QuantResult<()> {
    let trimmed = market.trim();
    if trimmed.is_empty() {
        return Err(QuantError::Validation {
            field: "market".to_string(),
            reason: "market must not be empty (e.g. KRW-BTC)".to_string(),
        });
    }
    if !trimmed.contains('-') {
        return Err(QuantError::Validation {
            field: "market".to_string(),
            reason: "market must look like KRW-BTC".to_string(),
        });
    }
    Ok(())
}

/// 주식 `symbol`은 비어 있지 않아야 한다 (예: `005930.KS`, `AAPL`).
pub fn validate_symbol(symbol: &str) -> QuantResult<()> {
    if symbol.trim().is_empty() {
        return Err(QuantError::Validation {
            field: "symbol".to_string(),
            reason: "symbol must not be empty (e.g. 005930.KS)".to_string(),
        });
    }
    Ok(())
}

/// Yahoo `range`는 허용된 값 중 하나여야 한다.
pub fn validate_range(range: &str) -> QuantResult<()> {
    const ALLOWED: &[&str] = &[
        "1d", "5d", "1mo", "3mo", "6mo", "1y", "2y", "5y", "10y", "ytd", "max",
    ];
    if ALLOWED.contains(&range) {
        Ok(())
    } else {
        Err(QuantError::Validation {
            field: "range".to_string(),
            reason: "range must be one of 1d 5d 1mo 3mo 6mo 1y 2y 5y 10y ytd max"
                .to_string(),
        })
    }
}

/// Upbit `count`는 1~200 사이여야 한다.
pub fn validate_count(count: u32) -> QuantResult<()> {
    if (1..=200).contains(&count) {
        Ok(())
    } else {
        Err(QuantError::Validation {
            field: "count".to_string(),
            reason: "count must be between 1 and 200".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_validation_rejects_blank_and_flat_codes() {
        assert!(validate_market("").is_err());
        assert!(validate_market("BTC").is_err());
        assert!(validate_market("KRW-BTC").is_ok());
    }

    #[test]
    fn count_validation_enforces_upbit_range() {
        assert!(validate_count(0).is_err());
        assert!(validate_count(201).is_err());
        assert!(validate_count(30).is_ok());
    }

    #[test]
    fn symbol_and_range_validation() {
        assert!(validate_symbol("").is_err());
        assert!(validate_symbol("005930.KS").is_ok());
        assert!(validate_range("1y").is_ok());
        assert!(validate_range("3months").is_err());
    }
}
