# quant

Upbit 공개 API로 봉 데이터를 수집해서 보여주는 Rust CLI.
[`agentic-rust-starter`](C:\Users\kornah\Work\09_Personal_Project\agentic-rust-starter) 규칙(`CONVENTIONS.md`, `[lints]`, `ai-verify.sh`)을 그대로 적용한 프로젝트다.

## 설치 (Claude Code처럼 한 번 설치하면 어디서든 `quant`)

필요한 것: Rust 1.85+ (`edition = "2024"`), API 키 불필요.

```powershell
# 이 폴더에서 실행
.\install.ps1
```

```bash
# macOS / Linux
./install.sh
```

수동 설치도 같다: `cargo install --locked --path . --force`.
설치 후 새 터미널에서 `quant --version`이 나오면 성공이다.
업데이트는 같은 명령을 다시, 삭제는 `cargo uninstall quant`.

## 사용법

```powershell
# 코인 일봉 30개 수집 -> data/candles.csv
quant fetch --market KRW-BTC --interval days --count 30 --output data/candles.csv

# 주식 일봉 3개월치 수집 -> data/stocks.csv (삼성전자 예시)
quant fetch-stock --symbol 005930.KS --interval day --range 3mo --output data/stocks.csv

# 저장된 CSV 앞 10행 + 현황 요약 보기 (코인/주식 자동 구분)
quant show --input data/candles.csv --limit 10

# 뒤(최신) 5행 보기
quant show --input data/candles.csv --limit 5 --tail
```

코인 `--interval` 값: `days`, `weeks`, `months`, `minutes1`, `minutes30`, `minutes60`, `minutes240`.
`--count`는 1~200. 환경변수 `QUANT_MARKET`으로 기본 마켓도 지정할 수 있다.

주식 심볼은 Yahoo 형식: 코스피 `005930.KS`, 코스닥 `종목코드.KQ`, 미국 `AAPL`.
주식 `--interval` 값: `day`, `week`, `month` / `--range` 값: `1d 5d 1mo 3mo 6mo 1y 2y 5y 10y ytd max`.
환경변수 `QUANT_SYMBOL`으로 기본 심볼도 지정할 수 있다.

## 검증

```powershell
cargo test
bash ai-verify.sh   # cargo check + clippy (-D warnings)
```

자세한 작업 규칙은 [CONVENTIONS.md](CONVENTIONS.md),
최신 crate 패턴은 [docs/ai_context/crate_cheatsheet.md](docs/ai_context/crate_cheatsheet.md)를 본다.
