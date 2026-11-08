# quant

A small Rust CLI that collects market candles and shows their status in the terminal.

- **Crypto:** OHLCV candles from the [Upbit](https://upbit.com) public API (no key required)
- **Stocks:** daily/weekly/monthly bars from the Yahoo Finance chart API (no key required)
- **Status view:** one `show` command renders any saved CSV as a table plus a one-line summary
- Built with the [agentic-rust-starter](https://github.com/Hannoah/agentic-rust-starter) conventions
  (`CONVENTIONS.md`, strict `[lints]`, `ai-verify.sh` feedback loop)

## Requirements

- Rust 1.85+ (`edition = "2024"`)
- No API keys. Both data sources are public endpoints.
- Network access for `fetch` / `fetch-stock` (`show` works fully offline)

## Installation

Install once, then `quant` works from any directory (like the Claude Code CLI).

### 1. Install Rust (if needed)

`quant` needs Rust 1.85+ (`edition = "2024"`). If `cargo --version` fails,
install via [rustup](https://rustup.rs):

Windows (PowerShell):

```powershell
winget install Rustlang.Rustup
```

macOS / Linux:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then open a **new** terminal and confirm:

```bash
cargo --version
# cargo 1.85.0 (or newer)
```

### 2. Install `quant`

From the repo root, pick your platform:

Windows (PowerShell):

```powershell
.\install.ps1
```

macOS / Linux:

```bash
./install.sh
```

What the script does:

1. Checks that `cargo` exists
2. Runs `cargo install --locked --path . --force` (release build)
3. Verifies the result with `quant --version`

Manual install is equivalent (needs network access to crates.io on first run):

```bash
git clone https://github.com/Hannoah/quant.git
cd quant
cargo install --locked --path . --force
```

For development without installing, run from the repo root instead:

```bash
cargo run -- show --input data/sample_stock.csv --limit 3
```

### 3. Verify

Open a **new** terminal (so `PATH` refreshes) and run:

```bash
quant --version
# quant 0.1.0
quant show --input data/sample_stock.csv --limit 3
```

### Troubleshooting

- **`quant` is not recognized**: the binary lives in Cargo's bin directory
  (`%USERPROFILE%\.cargo\bin` on Windows, `~/.cargo/bin` on macOS/Linux).
  Open a new terminal first; if it still fails, add that directory to `PATH`.
- **`cargo` is not recognized**: Rust installed but the terminal predates it —
  open a new terminal, or reinstall via rustup (step 1).
- **Install fails on network**: the first `cargo install` downloads
  dependencies from crates.io. Retry with a connection; afterwards builds work
  offline from the local cargo cache.
- **`fetch` / `fetch-stock` fail at runtime**: those need internet to reach
  Upbit / Yahoo Finance. `show` always works offline.

### Update & uninstall

Update to the latest source with the same command you installed with
(`.\install.ps1`, `./install.sh`, or `cargo install --locked --path . --force`).

```bash
cargo uninstall quant
```

## Quick start

```bash
# 1. Collect 30 daily BTC candles (KRW market) into a CSV
quant fetch --market KRW-BTC --interval days --count 30 --output data/candles.csv

# 2. Collect 3 months of Samsung Electronics daily bars
quant fetch-stock --symbol 005930.KS --interval day --range 3mo --output data/stocks.csv

# 3. Show the newest 5 rows plus a status summary
quant show --input data/stocks.csv --limit 5 --tail
```

Example `show` output:

```text
date                 open         high          low        close         volume
2026-11-02       185000.00    189500.00    184000.00    188700.00        9123456
(1 rows shown from data/stocks.csv)
005930.KS | 23 bars | 2026-10-06 ~ 2026-11-06 | latest 188700.00 | high 279500.00 / low 179200.00 | -32.24%
```

No network at hand? The repo ships offline fixtures:

```bash
quant show --input data/sample.csv --limit 3        # Upbit-style candles
quant show --input data/sample_stock.csv --limit 3  # Yahoo-style stock bars
```

## Commands

### `quant fetch` — crypto candles (Upbit)

```bash
quant fetch --market KRW-BTC --interval days --count 30 --output data/candles.csv
```

| Flag | Default | Notes |
|---|---|---|
| `--market` | `KRW-BTC` | Upbit market code (`KRW-ETH`, `KRW-XRP`, …). Also via `QUANT_MARKET` env |
| `--interval` | `days` | `days`, `weeks`, `months`, `minutes1`, `minutes30`, `minutes60`, `minutes240` |
| `--count` | `30` | 1–200, newest bars first |
| `--output` | `data/candles.csv` | Parent directories are created automatically |

After saving, the first 5 rows are previewed in the terminal.

### `quant fetch-stock` — stock bars (Yahoo Finance)

```bash
quant fetch-stock --symbol 005930.KS --interval day --range 3mo --output data/stocks.csv
```

| Flag | Default | Notes |
|---|---|---|
| `--symbol` | `005930.KS` | Yahoo symbol: KOSPI `005930.KS`, KOSDAQ `<code>.KQ`, US `AAPL`. Also via `QUANT_SYMBOL` env |
| `--interval` | `day` | `day` (`1d`), `week` (`1wk`), `month` (`1mo`) |
| `--range` | `3mo` | `1d 5d 1mo 3mo 6mo 1y 2y 5y 10y ytd max` |
| `--output` | `data/stocks.csv` | Parent directories are created automatically |

Bars with missing closes are skipped; missing open/high/low fall back to the close.

### `quant show` — status view (offline)

```bash
quant show --input data/stocks.csv --limit 10   # first 10 rows
quant show --input data/stocks.csv --limit 5 --tail  # newest 5 rows
```

The CSV header decides the format automatically: a `market` column means
Upbit candles, otherwise stock bars. Output is a fixed-width table plus a
summary line (symbol/market, row count, date range, latest price, high/low,
change vs. first open).

## Configuration

| Env var | Purpose |
|---|---|
| `QUANT_MARKET` | Default `--market` for `fetch` |
| `QUANT_SYMBOL` | Default `--symbol` for `fetch-stock` |
| `RUST_LOG` | Log level (e.g. `RUST_LOG=info`), via `tracing` |

## Development

```bash
cargo test        # unit tests (no network needed)
bash ai-verify.sh # cargo check + clippy with -D warnings (Git Bash on Windows)
```

Working rules live in [CONVENTIONS.md](CONVENTIONS.md): type-first design
(types → error types → test skeletons → logic), owned types over lifetimes,
`thiserror` + `anyhow` error handling, no `unwrap`/`expect` outside tests,
no `unsafe`. Crate patterns are collected in
[docs/ai_context/crate_cheatsheet.md](docs/ai_context/crate_cheatsheet.md).

Project layout:

```text
src/
  main.rs    # CLI entry point (tokio runtime, subcommand dispatch)
  cli.rs     # Clap 4 derive definitions (fetch / fetch-stock / show)
  types.rs   # Candle, StockBar, Interval, StockInterval
  error.rs   # QuantError (thiserror) + input validators
  upbit.rs   # Upbit public candles client
  yahoo.rs   # Yahoo Finance chart client + response parser
  store.rs   # CSV save/load, Dataset auto-detect, table + summary formatting
```

## Data sources & disclaimer

- Upbit candles: `GET https://api.upbit.com/v1/candles/{days,weeks,months,minutes/N}`
  — official public endpoint, no authentication.
- Yahoo Finance: `GET https://query1.finance.yahoo.com/v8/finance/chart/{symbol}`
  — unofficial public endpoint, no authentication. It can rate-limit or change
  without notice; if `fetch-stock` fails, retry later.

This is a data-collection and viewing tool, not investment advice.

## License

[MIT](LICENSE) © 2026 Hannoah
