use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::types::{Interval, StockInterval};

/// 봉 차트 수집/조회 CLI (코인: Upbit, 주식: Yahoo Finance).
#[derive(Debug, Parser)]
#[command(name = "quant", version, about = "Market data collector (fetch + show)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Upbit에서 봉 데이터를 받아 CSV로 저장한다 (키 불필요, 공개 API).
    Fetch {
        /// 마켓 코드 (예: KRW-BTC, KRW-ETH)
        #[arg(short, long, default_value = "KRW-BTC", env = "QUANT_MARKET")]
        market: String,

        /// 봉 간격
        #[arg(short, long, value_enum, default_value_t = Interval::Days)]
        interval: Interval,

        /// 받아올 봉 개수 (1~200)
        #[arg(short, long, default_value_t = 30)]
        count: u32,

        /// 저장할 CSV 경로
        #[arg(short, long, default_value = "data/candles.csv")]
        output: PathBuf,
    },
    /// Yahoo Finance에서 주식 봉을 받아 CSV로 저장한다 (키 불필요).
    FetchStock {
        /// 종목 심볼 (삼성전자: `005930.KS`, 코스닥: `.KQ`, 미국: `AAPL`)
        #[arg(short, long, default_value = "005930.KS", env = "QUANT_SYMBOL")]
        symbol: String,

        /// 봉 간격
        #[arg(short, long, value_enum, default_value_t = StockInterval::Day)]
        interval: StockInterval,

        /// 가져올 기간 (`1d 5d 1mo 3mo 6mo 1y 2y 5y 10y ytd max`)
        #[arg(short, long, default_value = "3mo")]
        range: String,

        /// 저장할 CSV 경로
        #[arg(short, long, default_value = "data/stocks.csv")]
        output: PathBuf,
    },
    /// 저장된 CSV를 표로 보여준다 (코인/주식 자동 구분).
    Show {
        /// 읽을 CSV 경로
        #[arg(short, long, default_value = "data/candles.csv")]
        input: PathBuf,

        /// 보여줄 행 수
        #[arg(short, long, default_value_t = 10)]
        limit: usize,

        /// 앞이 아니라 뒤(최신) N개를 보여준다
        #[arg(long, default_value_t = false)]
        tail: bool,
    },
}
