mod cli;
mod error;
mod store;
mod types;
mod upbit;
mod yahoo;

use anyhow::Context;
use clap::Parser;
use tracing::info;

use cli::{Cli, Commands};
use store::{format_dataset_summary, format_dataset_table, load_dataset, save_candles, save_stock_bars};
use upbit::fetch_candles;
use yahoo::fetch_stock_bars;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Fetch {
            market,
            interval,
            count,
            output,
        } => {
            let client = reqwest::Client::builder()
                .user_agent("quant-cli/0.1")
                .build()
                .context("failed to build http client")?;
            info!("fetching {market} {interval:?} x{count}");
            let candles = fetch_candles(&client, &market, interval, count).await?;
            save_candles(&output, &candles)
                .with_context(|| format!("failed to save {}", output.display()))?;
            println!(
                "saved {} candles to {}",
                candles.len(),
                output.display()
            );
            let preview_len = candles.len().min(5);
            print!("{}", format_dataset_table(&store::Dataset::Candles(
                candles[..preview_len].to_vec()
            )));
        }
        Commands::FetchStock {
            symbol,
            interval,
            range,
            output,
        } => {
            let client = reqwest::Client::builder()
                .user_agent("quant-cli/0.1")
                .build()
                .context("failed to build http client")?;
            info!("fetching {symbol} {interval:?} {range}");
            let bars = fetch_stock_bars(&client, &symbol, interval, &range).await?;
            save_stock_bars(&output, &bars)
                .with_context(|| format!("failed to save {}", output.display()))?;
            println!("saved {} bars to {}", bars.len(), output.display());
            let preview_len = bars.len().min(5);
            print!("{}", format_dataset_table(&store::Dataset::Stocks(
                bars[..preview_len].to_vec()
            )));
        }
        Commands::Show { input, limit, tail } => {
            let dataset =
                load_dataset(&input).with_context(|| format!("failed to load {}", input.display()))?;
            if dataset.is_empty() {
                println!("no rows in {}", input.display());
                return Ok(());
            }
            let rows = if tail {
                dataset.tail(limit)
            } else {
                dataset.head(limit)
            };
            print!("{}", format_dataset_table(&rows));
            println!("({} rows shown from {})", rows.len(), input.display());
            println!("{}", format_dataset_summary(&dataset));
        }
    }

    Ok(())
}
