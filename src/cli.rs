use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::types::{Interval, StockInterval};

const AFTER_HELP: &str = "\
Examples:
  quant                                                Launch interactive data collection service
  quant markets 삼성 --live                            Search stocks + live Upbit markets
  quant pipeline --all                                 Collect all 10 preset coins + 12 stocks
  quant pipeline --all-upbit                           Collect ALL KRW markets from Upbit
  quant watch add 솔라나 삼성전자 NVDA                 Add favorites (auto-detect crypto/stock)
  quant watch run                                      Collect all saved watchlist items
  quant refresh                                        Refresh all existing CSVs in data/
  quant show --input data/stocks.csv --limit 5 --tail  View newest 5 bars + sparkline
  quant status                                         Show storage cockpit with trend & age
  quant open data/stocks.csv                           Open CSV in default viewer (Excel)
  quant completion powershell                          Print PowerShell tab-completion script
  quant update --check                                 Check for a newer version
";

/// 봉 차트 수집/조회 CLI (코인: Upbit, 주식: Yahoo Finance).
#[derive(Debug, Parser)]
#[command(
    name = "quant",
    version,
    about = "Market data collection pipeline CLI & interactive service (Upbit + Yahoo Finance)",
    styles = crate::style::clap_styles(),
    after_help = AFTER_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
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
    /// 다중 종목(코인+주식) 데이터 수집 파이프라인을 일괄 실행한다.
    Pipeline {
        /// 수집할 Upbit 마켓 목록 (쉼표 구분)
        #[arg(long, value_delimiter = ',', default_value = "KRW-BTC,KRW-ETH")]
        markets: Vec<String>,

        /// 수집할 Yahoo Finance 종목 심볼 목록 (쉼표 구분)
        #[arg(long, value_delimiter = ',', default_value = "005930.KS,AAPL")]
        symbols: Vec<String>,

        /// 코인 봉 간격
        #[arg(long, value_enum, default_value_t = Interval::Days)]
        crypto_interval: Interval,

        /// 코인 봉 개수 (1~200)
        #[arg(long, default_value_t = 30)]
        count: u32,

        /// 주식 봉 간격
        #[arg(long, value_enum, default_value_t = StockInterval::Day)]
        stock_interval: StockInterval,

        /// 주식 기간 (`1d 5d 1mo 3mo 6mo 1y 2y 5y 10y ytd max`)
        #[arg(long, default_value = "3mo")]
        range: String,

        /// CSV 파일을 저장할 폴더
        #[arg(short, long, default_value = "data")]
        out_dir: PathBuf,

        /// 추천 코인 10종 + 추천 주식 12종 전체(22종) 일괄 수집
        #[arg(long, default_value_t = false)]
        all: bool,

        /// Upbit 원화(KRW) 상장 마켓 전체를 실시간 조회하여 일괄 수집
        #[arg(long, default_value_t = false)]
        all_upbit: bool,
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

        /// 날짜/시각 부분 일치 필터 (예: 2026-11)
        #[arg(short, long)]
        filter: Option<String>,
    },
    /// 수집된 CSV 데이터셋 목록과 상태 요약을 확인한다.
    Status {
        /// CSV 파일이 저장된 폴더 경로
        #[arg(short, long, default_value = "data")]
        dir: PathBuf,
    },
    /// 관심 종목(Watchlist)을 관리하거나 일괄 수집한다.
    Watch {
        #[command(subcommand)]
        action: Option<WatchAction>,
    },
    /// data/ 폴더 내 이미 수집된 CSV 파일들의 간격을 자동 감지해 일괄 최신화한다.
    Refresh {
        /// CSV 파일이 저장된 폴더 경로
        #[arg(short, long, default_value = "data")]
        dir: PathBuf,

        /// sample.csv / sample_stock.csv 예제 파일도 함께 덮어쓰기 갱신한다
        #[arg(long, default_value_t = false)]
        include_samples: bool,
    },
    /// 저장된 CSV 파일이나 data/ 폴더를 기본 앱(Excel / 파일 탐색기)으로 연다.
    Open {
        /// 열 파일 또는 폴더 경로 (생략 시 data 폴더)
        target: Option<PathBuf>,
    },
    /// 주요 코인 마켓 및 주식 종목 코드 목록을 조회하거나 검색한다.
    Markets {
        /// 검색 키워드 (예: 비트, 솔라나, 삼성, NVDA)
        query: Option<String>,

        /// Upbit 전체 상장 마켓을 실시간 API로 조회한다
        #[arg(short, long, default_value_t = false)]
        live: bool,
    },
    /// PowerShell 또는 Bash 쉘 탭 자동완성 스크립트를 출력한다.
    Completion {
        /// 대상 쉘 (`powershell` 또는 `bash`)
        shell: String,
    },
    /// 최신 버전을 확인하고 quant CLI 바이너리를 빌드·업데이트한다.
    Update {
        /// 설치하지 않고 최신 버전 여부만 확인한다
        #[arg(short, long, default_value_t = false)]
        check: bool,

        /// 버전/커밋이 같아도 강제로 다시 빌드하여 재설치한다
        #[arg(short, long, default_value_t = false)]
        force: bool,

        /// 빌드 전 `git pull --ff-only` 동기화를 건너뛴다
        #[arg(long, default_value_t = false)]
        no_pull: bool,

        /// 클론/동기화할 Git 원격 저장소 URL
        #[arg(long)]
        from: Option<String>,

        /// 로컬 소스 저장소 경로 (기본: 워크스페이스 또는 QUANT_REPO)
        #[arg(long)]
        repo: Option<PathBuf>,

        /// 원격 버전 확인 및 동기화 브랜치
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// 대화형 데이터 수집 파이프라인 CLI 서비스를 실행한다 (`quant`만 입력해도 기본 실행됨).
    Interactive,
}

#[derive(Debug, Subcommand)]
pub enum WatchAction {
    /// 등록된 관심 종목 목록을 보여준다.
    List,
    /// 관심 종목을 추가한다 (코인/주식 자동 판별, 예: 솔라나 삼성전자 NVDA).
    Add {
        #[arg(required = true)]
        items: Vec<String>,
    },
    /// 관심 종목에서 삭제한다.
    Remove {
        #[arg(required = true)]
        items: Vec<String>,
    },
    /// 등록된 관심 종목 전체 데이터를 일괄 수집한다.
    Run {
        #[arg(short, long, default_value = "data")]
        out_dir: PathBuf,
    },
    /// 등록된 관심 종목을 모두 비운다.
    Clear,
}
