mod catalog;
mod cli;
mod error;
mod interactive;
mod prefs;
mod store;
mod style;
mod tui;
mod types;
mod update;
mod upbit;
mod yahoo;

use anyhow::Context;
use clap::Parser;
use tracing::info;

use catalog::{completion_script, exec_markets};
use cli::{Cli, Commands};
use interactive::{
    exec_fetch, exec_fetch_stock, exec_open, exec_pipeline_flags, exec_refresh, exec_show,
    exec_status, exec_watch, run_interactive,
};
use style::Theme;
use update::{UpdateOptions, cleanup_old_binaries, exec_update};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    cleanup_old_binaries();

    let cli = Cli::parse();
    let theme = Theme::auto();
    let client = reqwest::Client::builder()
        .user_agent("quant-cli/0.1")
        .build()
        .context("failed to build http client")?;
    let mut stdout = std::io::stdout();

    match cli.command {
        Some(Commands::Fetch {
            market,
            interval,
            count,
            output,
        }) => {
            info!("fetching {market} {interval:?} x{count}");
            exec_fetch(&client, &theme, &market, interval, count, &output, &mut stdout).await?;
        }
        Some(Commands::FetchStock {
            symbol,
            interval,
            range,
            output,
        }) => {
            info!("fetching {symbol} {interval:?} {range}");
            exec_fetch_stock(
                &client,
                &theme,
                &symbol,
                interval,
                &range,
                &output,
                &mut stdout,
            )
            .await?;
        }
        Some(Commands::Pipeline {
            markets,
            symbols,
            crypto_interval,
            count,
            stock_interval,
            range,
            out_dir,
            all,
            all_upbit,
        }) => {
            info!(
                "running pipeline: all={all}, all_upbit={all_upbit}, markets={}, symbols={}",
                markets.len(),
                symbols.len()
            );
            exec_pipeline_flags(
                &client,
                &theme,
                &markets,
                &symbols,
                crypto_interval,
                count,
                stock_interval,
                &range,
                &out_dir,
                all,
                all_upbit,
                &mut stdout,
            )
            .await?;
        }
        Some(Commands::Show {
            input,
            limit,
            tail,
            filter,
        }) => {
            exec_show(&theme, &input, limit, tail, filter.as_deref(), &mut stdout)?;
        }
        Some(Commands::Status { dir }) => {
            exec_status(&theme, &dir, &mut stdout)?;
        }
        Some(Commands::Watch { action }) => {
            exec_watch(&client, &theme, action.as_ref(), &mut stdout).await?;
        }
        Some(Commands::Refresh {
            dir,
            include_samples,
        }) => {
            exec_refresh(&client, &theme, &dir, include_samples, &mut stdout).await?;
        }
        Some(Commands::Open { target }) => {
            exec_open(&theme, target.as_deref(), &mut stdout)?;
        }
        Some(Commands::Markets { query, live }) => {
            exec_markets(&client, &theme, query.as_deref(), live, &mut stdout).await?;
        }
        Some(Commands::Completion { shell }) => match completion_script(&shell) {
            Some(script) => println!("{script}"),
            None => anyhow::bail!("unsupported shell `{shell}`; use `powershell` or `bash`"),
        },
        Some(Commands::Update {
            check,
            force,
            no_pull,
            from,
            repo,
            branch,
        }) => {
            let opts = UpdateOptions {
                check,
                force,
                no_pull,
                from,
                repo,
                branch,
            };
            exec_update(&client, &theme, &opts, &mut stdout).await?;
        }
        Some(Commands::Interactive) | None => {
            run_interactive(&client, &theme).await?;
        }
    }

    Ok(())
}
