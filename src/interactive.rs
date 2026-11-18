use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;

use crate::catalog::{
    CRYPTO_EXTENDED, CRYPTO_PRESETS, STOCK_EXTENDED, STOCK_PRESETS, completion_script,
    exec_markets, fetch_upbit_krw_markets, is_crypto_like, resolve_crypto_list,
    resolve_crypto_market, resolve_stock_list, resolve_stock_range, resolve_stock_symbol,
    suggest_assets,
};
use crate::cli::{Cli, Commands, WatchAction};
use crate::prefs::{
    UserPrefs, load_history, load_prefs, record_history, save_prefs,
};
use crate::store::{
    Dataset, format_dataset_summary, format_stock_summary, format_summary, load_dataset,
    save_candles, save_stock_bars,
};
use crate::style::{
    Theme, Tone, format_price, format_relative_age, format_volume, render_progress_bar, sparkline,
};
use crate::tui::{
    PromptInput, SelectItem, Spinner, Suggestion, ask_command, is_interactive_tty, select_many,
    select_one,
};
use crate::types::{Candle, Interval, StockBar, StockInterval};
use crate::update::{UpdateOptions, exec_update};
use crate::upbit::fetch_candles;
use crate::yahoo::fetch_stock_bars;

/// 다중 종목 데이터 수집 파이프라인 설정.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub markets: Vec<String>,
    pub symbols: Vec<String>,
    pub crypto_interval: Interval,
    pub count: u32,
    pub stock_interval: StockInterval,
    pub range: String,
    pub out_dir: PathBuf,
    pub throttle_ms: u64,
}

/// Upbit 캔들을 수집해 CSV로 저장하고 최신 5개 행과 요약을 출력한다.
pub async fn exec_fetch<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    market: &str,
    interval: Interval,
    count: u32,
    output: &Path,
    out: &mut W,
) -> anyhow::Result<()> {
    let resolved = resolve_crypto_market(market);
    theme.banner_to(
        out,
        "UPBIT CRYPTO COLLECTOR",
        &format!(
            "{resolved} · {interval:?} · {count} candles -> {}",
            output.display()
        ),
    )?;
    let spinner = Spinner::start(&format!("Upbit에서 {resolved} 캔들을 가져오는 중..."));
    let res = fetch_candles(client, &resolved, interval, count).await;
    spinner.finish().await;
    let candles = res?;

    save_candles(output, &candles)
        .with_context(|| format!("failed to save {}", output.display()))?;
    theme.ok_to(
        out,
        &format!("Saved {} candles -> {}", candles.len(), output.display()),
    )?;

    let dataset = Dataset::Candles(candles.clone());
    if let Some((asset, bars, _, latest, _, change_pct)) = summarize_candles(&candles) {
        let trend = sparkline(&dataset.closes(), 10);
        theme.kpi_strip_to(
            out,
            &[
                ("ASSET", asset, Tone::Info),
                ("BARS", bars.to_string(), Tone::Ok),
                ("LATEST", format_price(latest), Tone::Accent),
                ("CHANGE", format!("{change_pct:+.2}%"), pct_tone(change_pct)),
                ("TREND", trend, pct_tone(change_pct)),
            ],
        )?;
    }

    writeln!(out)?;
    render_dataset_window(theme, &dataset, 5, true, out)?;
    writeln!(out, "  {}", theme.dim(&format_summary(&candles)))?;
    theme.hint_to(
        out,
        &format!("quant show --input {} --limit 10 --tail", output.display()),
        "전체 수집 결과 상세 조회",
    )?;
    Ok(())
}

/// Yahoo Finance 주식 봉을 수집해 CSV로 저장하고 최신 5개 행과 요약을 출력한다.
pub async fn exec_fetch_stock<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    symbol: &str,
    interval: StockInterval,
    range: &str,
    output: &Path,
    out: &mut W,
) -> anyhow::Result<()> {
    let resolved_sym = resolve_stock_symbol(symbol);
    let resolved_range = resolve_stock_range(range);
    theme.banner_to(
        out,
        "YAHOO FINANCE STOCK COLLECTOR",
        &format!(
            "{resolved_sym} · {interval:?} · range {resolved_range} -> {}",
            output.display()
        ),
    )?;
    let spinner = Spinner::start(&format!("Yahoo Finance에서 {resolved_sym} 시세를 가져오는 중..."));
    let res = fetch_stock_bars(client, &resolved_sym, interval, &resolved_range).await;
    spinner.finish().await;
    let bars = res?;

    save_stock_bars(output, &bars)
        .with_context(|| format!("failed to save {}", output.display()))?;
    theme.ok_to(
        out,
        &format!("Saved {} bars -> {}", bars.len(), output.display()),
    )?;

    let dataset = Dataset::Stocks(bars.clone());
    if let Some((asset, count, _, latest, _, change_pct)) = summarize_stocks(&bars) {
        let trend = sparkline(&dataset.closes(), 10);
        theme.kpi_strip_to(
            out,
            &[
                ("ASSET", asset, Tone::Accent),
                ("BARS", count.to_string(), Tone::Ok),
                ("LATEST", format_price(latest), Tone::Info),
                ("CHANGE", format!("{change_pct:+.2}%"), pct_tone(change_pct)),
                ("TREND", trend, pct_tone(change_pct)),
            ],
        )?;
    }

    writeln!(out)?;
    render_dataset_window(theme, &dataset, 5, true, out)?;
    writeln!(out, "  {}", theme.dim(&format_stock_summary(&bars)))?;
    theme.hint_to(
        out,
        &format!("quant show --input {} --limit 10 --tail", output.display()),
        "전체 수집 결과 상세 조회",
    )?;
    Ok(())
}

/// `--all` / `--all-upbit` 플래그를 반영해 파이프라인 설정을 구성하고 실행한다.
#[allow(clippy::too_many_arguments)]
pub async fn exec_pipeline_flags<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    markets: &[String],
    symbols: &[String],
    crypto_interval: Interval,
    count: u32,
    stock_interval: StockInterval,
    range: &str,
    out_dir: &Path,
    all: bool,
    all_upbit: bool,
    out: &mut W,
) -> anyhow::Result<()> {
    let (resolved_markets, resolved_symbols, throttle_ms) = if all_upbit {
        theme.section_to(out, "Fetching Upbit KRW Market List")?;
        let spinner = Spinner::start("Upbit 전체 원화 마켓 목록을 가져오는 중...");
        let res = fetch_upbit_krw_markets(client).await;
        spinner.finish().await;
        let live = res?;
        let m: Vec<String> = live.into_iter().map(|i| i.market).collect();
        theme.ok_to(out, &format!("{}개 Upbit 원화 마켓 조회 완료", m.len()))?;
        (m, Vec::new(), 120)
    } else if all {
        (resolve_crypto_list("all"), resolve_stock_list("all"), 120)
    } else {
        let m: Vec<String> = markets
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "-")
            .map(resolve_crypto_market)
            .collect();
        let s: Vec<String> = symbols
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "-")
            .map(resolve_stock_symbol)
            .collect();
        let throttle = if m.len() + s.len() > 8 { 120 } else { 0 };
        (m, s, throttle)
    };

    let cfg = PipelineConfig {
        markets: resolved_markets,
        symbols: resolved_symbols,
        crypto_interval,
        count,
        stock_interval,
        range: resolve_stock_range(range),
        out_dir: out_dir.to_path_buf(),
        throttle_ms,
    };
    exec_pipeline(client, theme, &cfg, out).await
}

/// 코인 마켓과 주식 심볼 목록을 순회하며 데이터 수집 파이프라인을 일괄 실행한다.
pub async fn exec_pipeline<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    cfg: &PipelineConfig,
    out: &mut W,
) -> anyhow::Result<()> {
    let markets: Vec<String> = cfg
        .markets
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let symbols: Vec<String> = cfg
        .symbols
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let total = markets.len() + symbols.len();
    if total == 0 {
        theme.warn_to(out, "수집할 마켓 또는 심볼이 없습니다.")?;
        return Ok(());
    }

    theme.banner_to(
        out,
        "BATCH DATA COLLECTION PIPELINE",
        &format!(
            "Crypto: {} | Stocks: {} | Output: {}",
            markets.len(),
            symbols.len(),
            cfg.out_dir.display()
        ),
    )?;

    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut step = 0usize;
    let mut summary_rows: Vec<Vec<String>> = Vec::with_capacity(total);

    for market in &markets {
        if step > 0 && cfg.throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(cfg.throttle_ms)).await;
        }
        step += 1;
        let path = market_output_path(&cfg.out_dir, market);
        let file_name = file_label(&path);
        let bar = render_progress_bar(step, total, 12);
        write!(
            out,
            "  {} [{step}/{total}] {} {market:<10} ... ",
            theme.dim(&bar),
            theme.badge("CRYPTO", Tone::Info),
        )?;
        out.flush()?;

        match fetch_candles(client, market, cfg.crypto_interval, cfg.count).await {
            Ok(candles) => match save_candles(&path, &candles) {
                Ok(()) => {
                    succeeded += 1;
                    writeln!(out, "{}", theme.badge("OK", Tone::Ok))?;
                    let (_, count, _, latest, _, pct) = summarize_candles(&candles).unwrap_or_else(
                        || {
                            (
                                market.clone(),
                                candles.len(),
                                String::new(),
                                0.0,
                                String::new(),
                                0.0,
                            )
                        },
                    );
                    let trend = sparkline(&Dataset::Candles(candles).closes(), 8);
                    summary_rows.push(vec![
                        step.to_string(),
                        theme.badge("CRYPTO", Tone::Info),
                        market.clone(),
                        theme.badge("OK", Tone::Ok),
                        count.to_string(),
                        format_price(latest),
                        format_pct(theme, pct),
                        trend,
                        file_name,
                    ]);
                }
                Err(err) => {
                    failed += 1;
                    writeln!(out, "{} (save: {err})", theme.badge("FAIL", Tone::Err))?;
                    summary_rows.push(failed_summary_row(
                        step,
                        theme.badge("CRYPTO", Tone::Info),
                        market,
                        theme,
                        &file_name,
                    ));
                }
            },
            Err(err) => {
                failed += 1;
                writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
                summary_rows.push(failed_summary_row(
                    step,
                    theme.badge("CRYPTO", Tone::Info),
                    market,
                    theme,
                    &file_name,
                ));
            }
        }
    }

    for symbol in &symbols {
        if step > 0 && cfg.throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(cfg.throttle_ms)).await;
        }
        step += 1;
        let path = symbol_output_path(&cfg.out_dir, symbol);
        let file_name = file_label(&path);
        let bar = render_progress_bar(step, total, 12);
        write!(
            out,
            "  {} [{step}/{total}] {} {symbol:<10} ... ",
            theme.dim(&bar),
            theme.badge("STOCK", Tone::Accent),
        )?;
        out.flush()?;

        match fetch_stock_bars(client, symbol, cfg.stock_interval, &cfg.range).await {
            Ok(bars) => match save_stock_bars(&path, &bars) {
                Ok(()) => {
                    succeeded += 1;
                    writeln!(out, "{}", theme.badge("OK", Tone::Ok))?;
                    let (_, count, _, latest, _, pct) = summarize_stocks(&bars).unwrap_or_else(
                        || {
                            (
                                symbol.clone(),
                                bars.len(),
                                String::new(),
                                0.0,
                                String::new(),
                                0.0,
                            )
                        },
                    );
                    let trend = sparkline(&Dataset::Stocks(bars).closes(), 8);
                    summary_rows.push(vec![
                        step.to_string(),
                        theme.badge("STOCK", Tone::Accent),
                        symbol.clone(),
                        theme.badge("OK", Tone::Ok),
                        count.to_string(),
                        format_price(latest),
                        format_pct(theme, pct),
                        trend,
                        file_name,
                    ]);
                }
                Err(err) => {
                    failed += 1;
                    writeln!(out, "{} (save: {err})", theme.badge("FAIL", Tone::Err))?;
                    summary_rows.push(failed_summary_row(
                        step,
                        theme.badge("STOCK", Tone::Accent),
                        symbol,
                        theme,
                        &file_name,
                    ));
                }
            },
            Err(err) => {
                failed += 1;
                writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
                summary_rows.push(failed_summary_row(
                    step,
                    theme.badge("STOCK", Tone::Accent),
                    symbol,
                    theme,
                    &file_name,
                ));
            }
        }
    }

    theme.section_to(out, "Pipeline Summary")?;
    theme.kpi_strip_to(
        out,
        &[
            ("TOTAL", total.to_string(), Tone::Info),
            ("OK", succeeded.to_string(), Tone::Ok),
            (
                "FAIL",
                failed.to_string(),
                if failed > 0 { Tone::Err } else { Tone::Muted },
            ),
        ],
    )?;
    writeln!(out)?;
    theme.table_to(
        out,
        &["#", "TYPE", "ASSET", "STATUS", "BARS", "LATEST", "CHANGE", "TREND", "FILE"],
        &summary_rows,
        &[0, 4, 5, 6],
    )?;
    writeln!(out)?;
    theme.hint_to(
        out,
        &format!("quant status --dir {}", cfg.out_dir.display()),
        "수집된 전체 데이터셋 현황 확인",
    )?;
    Ok(())
}

/// 저장된 CSV 파일을 읽어 표와 요약을 출력한다.
pub fn exec_show<W: Write>(
    theme: &Theme,
    input: &Path,
    limit: usize,
    tail: bool,
    filter: Option<&str>,
    out: &mut W,
) -> anyhow::Result<()> {
    let raw_dataset =
        load_dataset(input).with_context(|| format!("failed to load {}", input.display()))?;
    let dataset = match filter.map(str::trim).filter(|s| !s.is_empty()) {
        Some(q) => raw_dataset.filter_date(q),
        None => raw_dataset,
    };

    let title = match filter.map(str::trim).filter(|s| !s.is_empty()) {
        Some(q) => format!("Dataset View :: {} (filter: {q})", input.display()),
        None => format!("Dataset View :: {}", input.display()),
    };
    theme.section_to(out, &title)?;

    if dataset.is_empty() {
        theme.warn_to(out, &format!("no matching rows in {}", input.display()))?;
        return Ok(());
    }

    let shown = render_dataset_window(theme, &dataset, limit, tail, out)?;
    writeln!(out, "({shown} rows shown from {})", input.display())?;

    if let Some((asset, total, _, latest, high_low, pct)) = summarize_dataset(&dataset) {
        let trend = sparkline(&dataset.closes(), 14);
        theme.kpi_strip_to(
            out,
            &[
                ("ASSET", asset, Tone::Info),
                ("TOTAL BARS", total.to_string(), Tone::Accent),
                ("LATEST", format_price(latest), Tone::Ok),
                ("CHANGE", format!("{pct:+.2}%"), pct_tone(pct)),
                ("TREND", trend, pct_tone(pct)),
            ],
        )?;
        theme.kv_to(out, "High / Low", &high_low)?;
    }
    writeln!(out, "{}", format_dataset_summary(&dataset))?;
    Ok(())
}

/// 지정 폴더의 CSV 데이터셋 목록과 현황 대시보드를 출력한다.
pub fn exec_status<W: Write>(theme: &Theme, dir: &Path, out: &mut W) -> anyhow::Result<()> {
    theme.section_to(out, &format!("Pipeline Storage Cockpit ({})", dir.display()))?;
    if !dir.exists() {
        theme.kpi_strip_to(out, &[("DATASETS", "0".into(), Tone::Muted)])?;
        theme.warn_to(
            out,
            &format!("'{}' 폴더가 아직 없습니다. [1]~[3] 또는 [a] 메뉴로 데이터를 수집하세요.", dir.display()),
        )?;
        return Ok(());
    }

    let files = list_csv_files(dir)?;
    if files.is_empty() {
        theme.kpi_strip_to(out, &[("DATASETS", "0".into(), Tone::Muted)])?;
        theme.warn_to(
            out,
            "수집된 CSV 파일이 없습니다. [1]~[3] 또는 [a] 메뉴로 데이터를 수집하세요.",
        )?;
        return Ok(());
    }

    let mut crypto_count = 0usize;
    let mut stock_count = 0usize;
    let mut total_bars = 0usize;
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(files.len());

    for (idx, path) in files.iter().enumerate() {
        let name = file_label(path);
        let age = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(format_relative_age)
            .unwrap_or_else(|_| "-".to_string());

        match load_dataset(path) {
            Ok(dataset) => {
                total_bars += dataset.len();
                let trend = sparkline(&dataset.closes(), 10);
                let type_badge = match dataset {
                    Dataset::Candles(_) => {
                        crypto_count += 1;
                        theme.badge("CRYPTO", Tone::Info)
                    }
                    Dataset::Stocks(_) => {
                        stock_count += 1;
                        theme.badge("STOCK", Tone::Accent)
                    }
                };
                if let Some((asset, bars, period, latest, _, pct)) = summarize_dataset(&dataset) {
                    rows.push(vec![
                        (idx + 1).to_string(),
                        type_badge,
                        name,
                        asset,
                        bars.to_string(),
                        period,
                        format_price(latest),
                        format_pct(theme, pct),
                        trend,
                        age,
                    ]);
                } else {
                    rows.push(vec![
                        (idx + 1).to_string(),
                        type_badge,
                        name,
                        "-".into(),
                        "0".into(),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                        age,
                    ]);
                }
            }
            Err(err) => {
                rows.push(vec![
                    (idx + 1).to_string(),
                    theme.badge("ERROR", Tone::Err),
                    name,
                    "-".into(),
                    "-".into(),
                    truncate_msg(&err.to_string(), 20),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                    age,
                ]);
            }
        }
    }

    theme.kpi_strip_to(
        out,
        &[
            ("DATASETS", files.len().to_string(), Tone::Info),
            ("CRYPTO", crypto_count.to_string(), Tone::Info),
            ("STOCKS", stock_count.to_string(), Tone::Accent),
            ("TOTAL BARS", total_bars.to_string(), Tone::Ok),
        ],
    )?;
    writeln!(out)?;
    theme.table_to(
        out,
        &[
            "#", "TYPE", "FILE", "ASSET", "BARS", "PERIOD", "LATEST", "CHANGE", "TREND", "UPDATED",
        ],
        &rows,
        &[0, 4, 6, 7],
    )?;
    Ok(())
}

/// 관심 종목(Watchlist) 관리 및 일괄 수집 실행.
pub async fn exec_watch<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    action: Option<&WatchAction>,
    out: &mut W,
) -> anyhow::Result<()> {
    let mut prefs = load_prefs();
    match action {
        None | Some(WatchAction::List) => {
            render_watchlist(theme, &prefs, out)?;
        }
        Some(WatchAction::Add { items }) => {
            let mut added = 0usize;
            for raw in items {
                for tok in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if is_crypto_like(tok) {
                        let code = resolve_crypto_market(tok);
                        if prefs.add_crypto(&code) {
                            added += 1;
                            theme.ok_to(out, &format!("관심 코인 추가: {code}"))?;
                        } else {
                            theme.warn_to(out, &format!("이미 등록됨: {code}"))?;
                        }
                    } else {
                        let code = resolve_stock_symbol(tok);
                        if prefs.add_stock(&code) {
                            added += 1;
                            theme.ok_to(out, &format!("관심 주식 추가: {code}"))?;
                        } else {
                            theme.warn_to(out, &format!("이미 등록됨: {code}"))?;
                        }
                    }
                }
            }
            if added > 0 {
                save_prefs(&prefs);
            }
            theme.hint_to(out, "quant watch run", "등록된 관심 종목 전체 일괄 수집")?;
        }
        Some(WatchAction::Remove { items }) => {
            let mut removed = 0usize;
            for raw in items {
                for tok in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let c_code = resolve_crypto_market(tok);
                    let s_code = resolve_stock_symbol(tok);
                    if prefs.remove_target(tok)
                        || prefs.remove_target(&c_code)
                        || prefs.remove_target(&s_code)
                    {
                        removed += 1;
                        theme.ok_to(out, &format!("관심 종목 삭제: {tok}"))?;
                    } else {
                        theme.warn_to(out, &format!("목록에 없음: {tok}"))?;
                    }
                }
            }
            if removed > 0 {
                save_prefs(&prefs);
            }
        }
        Some(WatchAction::Clear) => {
            let count = prefs.total_watchlist();
            prefs.watchlist_crypto.clear();
            prefs.watchlist_stocks.clear();
            save_prefs(&prefs);
            theme.ok_to(out, &format!("관심 종목 목록을 비웠습니다 ({count}개 삭제)"))?;
        }
        Some(WatchAction::Run { out_dir }) => {
            if prefs.total_watchlist() == 0 {
                theme.warn_to(
                    out,
                    "등록된 관심 종목이 없습니다. `quant watch add 솔라나 삼성전자`로 먼저 추가하세요.",
                )?;
                return Ok(());
            }
            let crypto_interval = prefs
                .last_crypto_interval
                .as_deref()
                .and_then(Interval::parse_input)
                .unwrap_or(Interval::Days);
            let count = prefs.last_crypto_count.unwrap_or(30);
            let stock_interval = prefs
                .last_stock_interval
                .as_deref()
                .and_then(StockInterval::parse_input)
                .unwrap_or(StockInterval::Day);
            let range = prefs
                .last_stock_range
                .clone()
                .unwrap_or_else(|| "3mo".to_string());

            exec_pipeline_flags(
                client,
                theme,
                &prefs.watchlist_crypto,
                &prefs.watchlist_stocks,
                crypto_interval,
                count,
                stock_interval,
                &range,
                out_dir,
                false,
                false,
                out,
            )
            .await?;
        }
    }
    Ok(())
}

/// `dir` 내 기존 CSV들의 종목과 간격을 자동 감지해 최신 시세로 일괄 갱신한다.
pub async fn exec_refresh<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    dir: &Path,
    include_samples: bool,
    out: &mut W,
) -> anyhow::Result<()> {
    if !dir.exists() {
        theme.warn_to(out, &format!("'{}' 폴더가 없습니다.", dir.display()))?;
        return Ok(());
    }
    let files = list_csv_files(dir)?;
    let targets: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| {
            include_samples
                || !matches!(
                    file_label(p).as_str(),
                    "sample.csv" | "sample_stock.csv"
                )
        })
        .collect();

    if targets.is_empty() {
        theme.warn_to(
            out,
            "갱신할 수집 CSV 파일이 없습니다. (예제 파일 sample*.csv 제외됨)",
        )?;
        return Ok(());
    }

    theme.banner_to(
        out,
        "SMART REFRESH PIPELINE",
        &format!("{}개 기존 CSV 파일 간격 자동 감지 및 최신화", targets.len()),
    )?;

    let total = targets.len();
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(total);

    for (idx, path) in targets.iter().enumerate() {
        if idx > 0 {
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
        let step = idx + 1;
        let name = file_label(path);
        let bar = render_progress_bar(step, total, 12);
        write!(out, "  {} [{step}/{total}] {name:<18} ... ", theme.dim(&bar))?;
        out.flush()?;

        match load_dataset(path) {
            Ok(Dataset::Candles(existing)) => {
                let Some(market) = existing.last().map(|c| c.market.clone()) else {
                    fail_count += 1;
                    writeln!(out, "{} (빈 파일)", theme.badge("SKIP", Tone::Warn))?;
                    continue;
                };
                let interval = Dataset::Candles(existing.clone()).infer_crypto_interval();
                let count = existing.len().clamp(1, 200) as u32;
                match fetch_candles(client, &market, interval, count).await {
                    Ok(fresh) => match save_candles(path, &fresh) {
                        Ok(()) => {
                            ok_count += 1;
                            writeln!(out, "{} ({interval:?} x{count})", theme.badge("OK", Tone::Ok))?;
                            let (_, bars, _, latest, _, pct) =
                                summarize_candles(&fresh).unwrap_or_else(|| {
                                    (market.clone(), fresh.len(), String::new(), 0.0, String::new(), 0.0)
                                });
                            rows.push(vec![
                                step.to_string(),
                                theme.badge("CRYPTO", Tone::Info),
                                market,
                                format!("{interval:?}"),
                                bars.to_string(),
                                format_price(latest),
                                format_pct(theme, pct),
                                name,
                            ]);
                        }
                        Err(err) => {
                            fail_count += 1;
                            writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
                        }
                    },
                    Err(err) => {
                        fail_count += 1;
                        writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
                    }
                }
            }
            Ok(Dataset::Stocks(existing)) => {
                let Some(symbol) = existing.last().map(|b| b.symbol.clone()) else {
                    fail_count += 1;
                    writeln!(out, "{} (빈 파일)", theme.badge("SKIP", Tone::Warn))?;
                    continue;
                };
                let interval = Dataset::Stocks(existing.clone()).infer_stock_interval();
                let range = infer_stock_range_from_count(existing.len(), interval);
                match fetch_stock_bars(client, &symbol, interval, range).await {
                    Ok(fresh) => match save_stock_bars(path, &fresh) {
                        Ok(()) => {
                            ok_count += 1;
                            writeln!(out, "{} ({interval:?} {range})", theme.badge("OK", Tone::Ok))?;
                            let (_, bars, _, latest, _, pct) =
                                summarize_stocks(&fresh).unwrap_or_else(|| {
                                    (symbol.clone(), fresh.len(), String::new(), 0.0, String::new(), 0.0)
                                });
                            rows.push(vec![
                                step.to_string(),
                                theme.badge("STOCK", Tone::Accent),
                                symbol,
                                format!("{interval:?}/{range}"),
                                bars.to_string(),
                                format_price(latest),
                                format_pct(theme, pct),
                                name,
                            ]);
                        }
                        Err(err) => {
                            fail_count += 1;
                            writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
                        }
                    },
                    Err(err) => {
                        fail_count += 1;
                        writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
                    }
                }
            }
            Err(err) => {
                fail_count += 1;
                writeln!(out, "{} ({err})", theme.badge("FAIL", Tone::Err))?;
            }
        }
    }

    theme.section_to(out, "Refresh Summary")?;
    theme.kpi_strip_to(
        out,
        &[
            ("TOTAL", total.to_string(), Tone::Info),
            ("OK", ok_count.to_string(), Tone::Ok),
            (
                "FAIL",
                fail_count.to_string(),
                if fail_count > 0 { Tone::Err } else { Tone::Muted },
            ),
        ],
    )?;
    if !rows.is_empty() {
        writeln!(out)?;
        theme.table_to(
            out,
            &["#", "TYPE", "ASSET", "INTERVAL", "BARS", "LATEST", "CHANGE", "FILE"],
            &rows,
            &[0, 4, 5, 6],
        )?;
    }
    Ok(())
}

/// CSV 파일이나 데이터 폴더를 OS 기본 앱으로 연다.
pub fn exec_open<W: Write>(theme: &Theme, target: Option<&Path>, out: &mut W) -> anyhow::Result<()> {
    let path = target.unwrap_or_else(|| Path::new("data"));
    if !path.exists() {
        bail!("경로를 찾을 수 없습니다: {}", path.display());
    }
    open_with_os(path)?;
    theme.ok_to(out, &format!("기본 앱으로 열었습니다 -> {}", path.display()))?;
    Ok(())
}

/// 표준 입출력을 사용하는 대화형 CLI 서비스를 실행한다.
pub async fn run_interactive(client: &reqwest::Client, theme: &Theme) -> anyhow::Result<()> {
    let mut reader = BufReader::new(std::io::stdin());
    let mut writer = std::io::stdout();
    run_repl_with_io(client, theme, &mut reader, &mut writer).await
}

/// `/slash` 명령어 접두어를 자동완성한다 (예: `/f` -> `fetch`, `/st` -> `status`).
pub fn resolve_slash_prefix(cmd: &str) -> Result<String, Vec<&'static str>> {
    let lower = cmd.trim().to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "1" | "f" => Some("fetch"),
        "2" | "sto" => Some("stock"),
        "3" | "p" => Some("pipeline"),
        "4" | "sh" => Some("show"),
        "5" | "st" | "ls" => Some("status"),
        "a" => Some("all"),
        "w" => Some("watch"),
        "r" => Some("refresh"),
        "o" => Some("open"),
        "m" => Some("markets"),
        "u" => Some("update"),
        "c" | "cls" => Some("clear"),
        "h" | "?" | "menu" => Some("help"),
        "q" | "e" | "quit" | "0" => Some("exit"),
        _ => None,
    };
    if let Some(c) = canonical {
        return Ok(c.to_string());
    }

    const COMMANDS: &[&str] = &[
        "fetch",
        "fetch-stock",
        "stock",
        "pipeline",
        "show",
        "status",
        "watch",
        "refresh",
        "open",
        "markets",
        "all",
        "update",
        "completion",
        "clear",
        "help",
        "exit",
    ];
    if COMMANDS.contains(&lower.as_str()) {
        return Ok(lower);
    }

    let matches: Vec<&'static str> = COMMANDS
        .iter()
        .copied()
        .filter(|c| c.starts_with(lower.as_str()))
        .collect();
    match matches.len() {
        1 => Ok(matches[0].to_string()),
        n if n > 1 => Err(matches),
        _ => Ok(lower),
    }
}

/// 실시간 입력 제안 목록 생성 (커서 아래 최대 6개 표시).
pub fn suggest_for_prompt(line: &str) -> Vec<Suggestion> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return Vec::new();
    }

    const MENU_CMDS: &[(&str, &str)] = &[
        ("/fetch", "코인 단일 수집 (검색·선택·자동완성)"),
        ("/stock", "주식 단일 수집 (한국/미국 종목 검색)"),
        ("/pipeline", "다중 종목 체크박스 배치 수집"),
        ("/all", "전체 일괄 수집 (추천 22종 / Upbit 전종목)"),
        ("/watch", "관심 종목(Watchlist) 관리 및 원클릭 수집"),
        ("/refresh", "data/ 내 기존 CSV 간격 자동 감지 최신화"),
        ("/show", "수집된 CSV 표 · 등락률 · 스파크라인 조회"),
        ("/status", "data/ 저장 현황 코크핏 대시보드"),
        ("/markets", "주요 코인·주식 코드표 및 Upbit 실시간 검색"),
        ("/open", "CSV 파일이나 data/ 폴더 기본 앱(Excel) 열기"),
        ("/update", "최신 버전 확인 및 자가 업데이트"),
        ("/clear", "화면 정리 (Ctrl+L)"),
        ("/help", "메뉴 및 명령어 예시 보기"),
        ("/exit", "서비스 종료"),
    ];

    let stripped = trimmed.strip_prefix('/').unwrap_or(trimmed);
    if !stripped.contains(char::is_whitespace) {
        let q = stripped.to_ascii_lowercase();
        return MENU_CMDS
            .iter()
            .filter(|(cmd, _)| {
                let name = cmd.trim_start_matches('/');
                q.is_empty() || name.starts_with(&q) || name.contains(&q)
            })
            .take(6)
            .map(|(cmd, desc)| Suggestion {
                label: (*cmd).to_string(),
                hint: (*desc).to_string(),
            })
            .collect();
    }

    let mut parts = stripped.splitn(2, char::is_whitespace);
    let head = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim();
    let last_tok = rest.split_whitespace().last().unwrap_or("");

    match head.as_str() {
        "show" | "sh" | "open" | "o" => {
            let files = list_csv_files(Path::new("data")).unwrap_or_default();
            let q = last_tok.to_ascii_lowercase();
            files
                .into_iter()
                .filter(|p| {
                    q.is_empty() || p.display().to_string().to_ascii_lowercase().contains(&q)
                })
                .take(6)
                .map(|p| Suggestion {
                    label: p.display().to_string().replace('\\', "/"),
                    hint: "CSV 파일".to_string(),
                })
                .collect()
        }
        "fetch" | "f" | "stock" | "fetch-stock" | "sto" | "markets" | "m" => {
            if last_tok.starts_with('-') {
                return vec![Suggestion {
                    label: format!("/{head} --help"),
                    hint: "옵션 도움말 보기".to_string(),
                }];
            }
            suggest_assets(last_tok, 6)
                .into_iter()
                .map(|(code, ko, kind)| Suggestion {
                    label: code,
                    hint: format!("[{kind}] {ko}"),
                })
                .collect()
        }
        "watch" | "w" => {
            let has_trailing = line.ends_with(char::is_whitespace);
            let typing_first_arg =
                rest.is_empty() || (!rest.contains(char::is_whitespace) && !has_trailing);
            if typing_first_arg {
                const WATCH_CMDS: &[(&str, &str)] = &[
                    ("add", "관심 종목 추가 (예: /watch add btc,삼성전자)"),
                    ("run", "등록된 관심 종목 일괄 수집"),
                    ("list", "관심 종목 목록 보기"),
                    ("remove", "관심 종목 제거"),
                    ("clear", "관심 종목 모두 비우기"),
                ];
                let q = rest.to_ascii_lowercase();
                let matches: Vec<Suggestion> = WATCH_CMDS
                    .iter()
                    .filter(|(c, _)| q.is_empty() || c.starts_with(&q))
                    .map(|(c, desc)| Suggestion {
                        label: (*c).to_string(),
                        hint: (*desc).to_string(),
                    })
                    .collect();
                if !matches.is_empty() {
                    return matches;
                }
            }
            let asset_q = if has_trailing { "" } else { last_tok };
            suggest_assets(asset_q, 6)
                .into_iter()
                .map(|(code, ko, kind)| Suggestion {
                    label: code,
                    hint: format!("[{kind}] {ko}"),
                })
                .collect()
        }
        "pipeline" | "p" | "all" | "a" => vec![
            Suggestion {
                label: "--all".to_string(),
                hint: "추천 코인 10 + 주식 12 전체 수집".to_string(),
            },
            Suggestion {
                label: "--all-upbit".to_string(),
                hint: "Upbit 원화 상장 마켓 전체 수집".to_string(),
            },
            Suggestion {
                label: "--markets".to_string(),
                hint: "수집할 코인 목록 지정 (예: --markets btc,sol)".to_string(),
            },
            Suggestion {
                label: "--symbols".to_string(),
                hint: "수집할 주식 목록 지정 (예: --symbols 삼성전자,NVDA)".to_string(),
            },
        ],
        "refresh" | "r" => vec![Suggestion {
            label: "--include-samples".to_string(),
            hint: "샘플 CSV 파일까지 포함해 최신화".to_string(),
        }],
        "update" | "u" => vec![
            Suggestion {
                label: "--check".to_string(),
                hint: "최신 버전 여부만 확인".to_string(),
            },
            Suggestion {
                label: "--force".to_string(),
                hint: "강제 다시 빌드 및 교체".to_string(),
            },
        ],
        _ => Vec::new(),
    }
}

/// 프롬프트에서 `Tab` 키를 눌렀을 때 입력 줄을 자동완성한다.
pub fn complete_prompt(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let has_trailing_space = line.ends_with(char::is_whitespace);
    let is_slash = trimmed.starts_with('/');
    let stripped = trimmed.strip_prefix('/').unwrap_or(trimmed);

    if !stripped.contains(char::is_whitespace) && !has_trailing_space {
        if let Ok(cmd) = resolve_slash_prefix(stripped) {
            let prefix = if is_slash { "/" } else { "" };
            return Some(format!("{prefix}{cmd} "));
        }
        return None;
    }

    let suggestions = suggest_for_prompt(line);
    let first = suggestions.first()?;
    let tokens: Vec<&str> = stripped.split_whitespace().collect();
    let head = tokens.first().copied().unwrap_or("");
    let prefix = if is_slash { "/" } else { "" };
    if has_trailing_space {
        Some(format!("{} {} ", trimmed.trim_end(), first.label))
    } else if tokens.len() >= 2 {
        let mut rebuilt: Vec<String> = tokens[..tokens.len() - 1]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        rebuilt.push(first.label.clone());
        let joined = rebuilt.join(" ");
        Some(format!("{prefix}{joined} "))
    } else {
        Some(format!("{prefix}{head} {} ", first.label))
    }
}

/// 테스트 가능하도록 `BufRead` / `Write` 스트림을 주입받아 대화형 서비스 루프를 실행한다.
pub async fn run_repl_with_io<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let mut prefs = load_prefs();
    let mut history = load_history();
    print_welcome(theme, &prefs, out)?;

    loop {
        print_prompt_bar(theme, &prefs, out)?;
        let input = ask_command(
            theme,
            "quant ❯",
            &history,
            suggest_for_prompt,
            complete_prompt,
            reader,
            out,
        )?;

        let trimmed = match input {
            PromptInput::Eof | PromptInput::Interrupted => {
                writeln!(out, "quant CLI 서비스를 종료합니다.")?;
                break;
            }
            PromptInput::Line(s) => s,
        };

        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "/" {
            if is_interactive_tty() {
                if let Some(action) = pick_palette_command(theme, reader, out)? {
                    record_history(&mut history, &format!("/{action}"));
                    let quit = dispatch_repl_command(
                        client,
                        theme,
                        &mut prefs,
                        &action,
                        "",
                        true,
                        reader,
                        out,
                    )
                    .await?;
                    if quit {
                        break;
                    }
                }
            } else {
                print_menu(theme, out)?;
                print_command_examples(theme, out)?;
                writeln!(out)?;
            }
            continue;
        }

        record_history(&mut history, &trimmed);
        let is_slash = trimmed.starts_with('/');
        let stripped = trimmed.strip_prefix('/').unwrap_or(&trimmed).trim();
        let mut parts = stripped.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();

        let completed_head = match resolve_slash_prefix(head) {
            Ok(cmd) => cmd,
            Err(candidates) => {
                let list: Vec<String> = candidates.iter().map(|c| format!("/{c}")).collect();
                theme.warn_to(
                    out,
                    &format!("자동완성 후보가 여러 개입니다: {}", list.join(", ")),
                )?;
                writeln!(out)?;
                continue;
            }
        };

        if is_slash && completed_head != head.to_ascii_lowercase() && rest.is_empty() {
            writeln!(
                out,
                "  {} /{}",
                theme.dim("자동완성 ->"),
                theme.bold(&completed_head)
            )?;
        }

        let quit = dispatch_repl_command(
            client,
            theme,
            &mut prefs,
            &completed_head,
            rest,
            false,
            reader,
            out,
        )
        .await?;
        if quit {
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_repl_command<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    prefs: &mut UserPrefs,
    completed_head: &str,
    rest: &str,
    from_palette: bool,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<bool> {
    let started = Instant::now();
    let ok = if rest.is_empty() {
        match completed_head {
            "exit" => {
                writeln!(out, "quant CLI 서비스를 종료합니다.")?;
                return Ok(true);
            }
            "help" => {
                print_menu(theme, out)?;
                print_command_examples(theme, out)?;
                true
            }
            "clear" => {
                if theme.color {
                    write!(out, "\x1b[2J\x1b[H")?;
                }
                print_welcome(theme, prefs, out)?;
                true
            }
            "fetch" => match wizard_fetch(client, theme, prefs, reader, out).await {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "fetch-stock" | "stock" => {
                match wizard_fetch_stock(client, theme, prefs, reader, out).await {
                    Ok(()) => true,
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        false
                    }
                }
            }
            "pipeline" => match wizard_pipeline(client, theme, prefs, reader, out).await {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "all" => match wizard_all(client, theme, reader, out).await {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "watch" => match wizard_watch(client, theme, prefs, reader, out).await {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "refresh" => match exec_refresh(client, theme, Path::new("data"), false, out).await {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "open" => match wizard_open(theme, reader, out) {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "show" => match wizard_show(theme, reader, out) {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "status" => match exec_status(theme, Path::new("data"), out) {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "markets" => match exec_markets(client, theme, None, false, out).await {
                Ok(()) => true,
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    false
                }
            },
            "update" => {
                let opts = default_update_options();
                match exec_update(client, theme, &opts, out).await {
                    Ok(()) => true,
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        false
                    }
                }
            }
            other if !from_palette => run_direct_command(client, theme, other, out).await?,
            _ => true,
        }
    } else {
        let rebuilt = match completed_head {
            "stock" => format!("fetch-stock {rest}"),
            "all" => {
                if rest.eq_ignore_ascii_case("upbit") {
                    "pipeline --all-upbit".to_string()
                } else if rest.starts_with('-') {
                    format!("pipeline {rest}")
                } else {
                    format!("pipeline --all {rest}")
                }
            }
            _ => format!("{completed_head} {rest}"),
        };
        run_direct_command(client, theme, &rebuilt, out).await?
    };

    let elapsed_ms = started.elapsed().as_millis();
    let mark = if ok {
        theme.style("✔", Tone::Ok, true)
    } else {
        theme.style("✖", Tone::Err, true)
    };
    writeln!(
        out,
        "{}",
        theme.dim(&format!("╰ {mark} 완료 ({elapsed_ms}ms)"))
    )?;
    writeln!(out)?;
    Ok(false)
}

fn pick_palette_command<R: BufRead, W: Write>(
    theme: &Theme,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<Option<String>> {
    let actions = [
        ("fetch", "코인 단일 수집 (검색·선택·자동완성)"),
        ("stock", "주식 단일 수집 (한국/미국 종목 검색)"),
        ("pipeline", "다중 종목 체크박스 배치 수집"),
        ("all", "전체 일괄 수집 (추천 22종 / Upbit 전종목)"),
        ("watch", "관심 종목(Watchlist) 관리 및 일괄 수집"),
        ("refresh", "저장된 CSV 간격 자동 감지 최신화"),
        ("show", "수집 데이터 상세 표 & 스파크라인 조회"),
        ("status", "data/ 저장 현황 코크핏 대시보드"),
        ("markets", "주요 코인·주식 코드표 & 실시간 검색"),
        ("open", "CSV 파일 / data 폴더 기본 앱 열기"),
        ("update", "최신 버전 확인 및 자가 업데이트"),
        ("help", "도움말 & 명령어 예시 보기"),
        ("exit", "서비스 종료"),
    ];
    let items: Vec<SelectItem> = actions
        .iter()
        .map(|(cmd, desc)| SelectItem {
            label: format!("/{cmd}"),
            hint: (*desc).to_string(),
        })
        .collect();
    let picked = select_one(theme, "실행할 작업 선택", &items, 0, reader, out)?;
    Ok(picked.map(|idx| actions[idx].0.to_string()))
}

fn print_welcome<W: Write>(theme: &Theme, prefs: &UserPrefs, out: &mut W) -> anyhow::Result<()> {
    print_banner(theme, out)?;
    if let Err(err) = exec_status(theme, Path::new("data"), out) {
        theme.warn_to(out, &format!("데이터 현황 조회 실패: {err}"))?;
    }
    if prefs.total_watchlist() > 0 {
        let mut all = prefs.watchlist_crypto.clone();
        all.extend(prefs.watchlist_stocks.clone());
        writeln!(
            out,
            "  {} {}  {}",
            theme.badge("WATCHLIST", Tone::Warn),
            all.join(", "),
            theme.dim("(/watch 로 바로 수집)")
        )?;
    }
    print_menu(theme, out)?;
    writeln!(out)?;
    Ok(())
}

fn print_prompt_bar<W: Write>(theme: &Theme, prefs: &UserPrefs, out: &mut W) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let count = list_csv_files(Path::new("data")).map(|v| v.len()).unwrap_or(0);
    let watch_n = prefs.total_watchlist();
    let tty_tip = if is_interactive_tty() {
        "Tab 자동완성 · ↑↓ 이력 · / 팔레트"
    } else {
        "/markets · /all · /status · /exit"
    };
    writeln!(
        out,
        "{}",
        theme.dim(&format!(
            "{cwd}  ·  data: {count} CSV  ·  watch: {watch_n}  ·  {tty_tip}"
        ))
    )?;
    Ok(())
}

async fn run_direct_command<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    line: &str,
    out: &mut W,
) -> anyhow::Result<bool> {
    let args = std::iter::once("quant").chain(line.split_whitespace());
    match Cli::try_parse_from(args) {
        Ok(cli) => match cli.command {
            Some(Commands::Fetch {
                market,
                interval,
                count,
                output,
            }) => match exec_fetch(client, theme, &market, interval, count, &output, out).await {
                Ok(()) => Ok(true),
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    Ok(false)
                }
            },
            Some(Commands::FetchStock {
                symbol,
                interval,
                range,
                output,
            }) => {
                match exec_fetch_stock(client, theme, &symbol, interval, &range, &output, out).await
                {
                    Ok(()) => Ok(true),
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        Ok(false)
                    }
                }
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
                match exec_pipeline_flags(
                    client,
                    theme,
                    &markets,
                    &symbols,
                    crypto_interval,
                    count,
                    stock_interval,
                    &range,
                    &out_dir,
                    all,
                    all_upbit,
                    out,
                )
                .await
                {
                    Ok(()) => Ok(true),
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        Ok(false)
                    }
                }
            }
            Some(Commands::Show {
                input,
                limit,
                tail,
                filter,
            }) => match exec_show(theme, &input, limit, tail, filter.as_deref(), out) {
                Ok(()) => Ok(true),
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    Ok(false)
                }
            },
            Some(Commands::Status { dir }) => match exec_status(theme, &dir, out) {
                Ok(()) => Ok(true),
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    Ok(false)
                }
            },
            Some(Commands::Watch { action }) => {
                match exec_watch(client, theme, action.as_ref(), out).await {
                    Ok(()) => Ok(true),
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        Ok(false)
                    }
                }
            }
            Some(Commands::Refresh {
                dir,
                include_samples,
            }) => match exec_refresh(client, theme, &dir, include_samples, out).await {
                Ok(()) => Ok(true),
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    Ok(false)
                }
            },
            Some(Commands::Open { target }) => match exec_open(theme, target.as_deref(), out) {
                Ok(()) => Ok(true),
                Err(err) => {
                    theme.err_to(out, &format!("{err:#}"))?;
                    Ok(false)
                }
            },
            Some(Commands::Markets { query, live }) => {
                match exec_markets(client, theme, query.as_deref(), live, out).await {
                    Ok(()) => Ok(true),
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        Ok(false)
                    }
                }
            }
            Some(Commands::Completion { shell }) => match completion_script(&shell) {
                Some(script) => {
                    writeln!(out, "{script}")?;
                    Ok(true)
                }
                None => {
                    theme.err_to(out, "지원 쉘: powershell, bash")?;
                    Ok(false)
                }
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
                match exec_update(client, theme, &opts, out).await {
                    Ok(()) => Ok(true),
                    Err(err) => {
                        theme.err_to(out, &format!("{err:#}"))?;
                        Ok(false)
                    }
                }
            }
            Some(Commands::Interactive) | None => {
                writeln!(out, "이미 대화형 CLI 서비스 실행 중입니다. (/help, /exit)")?;
                Ok(true)
            }
        },
        Err(err) => {
            writeln!(out, "{err}")?;
            Ok(matches!(
                err.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ))
        }
    }
}

async fn wizard_fetch<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    prefs: &mut UserPrefs,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let mut items: Vec<SelectItem> = CRYPTO_PRESETS
        .iter()
        .chain(CRYPTO_EXTENDED.iter())
        .map(|e| SelectItem {
            label: e.code.to_string(),
            hint: format!("{} ({})", e.name_ko, e.name_en),
        })
        .collect();
    let presets_total = items.len();
    let idx_all_10 = presets_total;
    items.push(SelectItem {
        label: "[추천 10종 전체]".to_string(),
        hint: "메이저 코인 10종 일괄 수집".to_string(),
    });
    let idx_all_upbit = presets_total + 1;
    items.push(SelectItem {
        label: "[Upbit 원화 전종목]".to_string(),
        hint: "Upbit 상장 KRW-* 전체 마켓 실시간 수집".to_string(),
    });
    let idx_live_search = presets_total + 2;
    items.push(SelectItem {
        label: "[Upbit 실시간 검색]".to_string(),
        hint: "내장 목록 외 코인 실시간 조회".to_string(),
    });

    let Some(picked) = select_one(theme, "코인 선택 (Upbit)", &items, 0, reader, out)? else {
        writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
        return Ok(());
    };

    if picked == idx_all_10 || picked == idx_all_upbit {
        let (interval, count) = pick_crypto_interval_and_count(theme, prefs, reader, out)?;
        let out_dir = prefs.last_out_dir.as_deref().unwrap_or("data");
        return exec_pipeline_flags(
            client,
            theme,
            &resolve_crypto_list("all"),
            &[],
            interval,
            count,
            StockInterval::Day,
            "3mo",
            Path::new(out_dir),
            false,
            picked == idx_all_upbit,
            out,
        )
        .await;
    }

    let market = if picked == idx_live_search {
        let spinner = Spinner::start("Upbit 전체 원화 마켓을 불러오는 중...");
        let res = fetch_upbit_krw_markets(client).await;
        spinner.finish().await;
        let live = res?;
        let live_items: Vec<SelectItem> = live
            .iter()
            .map(|m| SelectItem {
                label: m.market.clone(),
                hint: format!("{} ({})", m.korean_name, m.english_name),
            })
            .collect();
        let Some(li) = select_one(
            theme,
            "Upbit 원화 마켓 실시간 선택",
            &live_items,
            0,
            reader,
            out,
        )?
        else {
            writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
            return Ok(());
        };
        live[li].market.clone()
    } else {
        items[picked].label.clone()
    };

    theme.ok_to(out, &format!("선택된 코인: {market}"))?;
    let (interval, count) = pick_crypto_interval_and_count(theme, prefs, reader, out)?;
    let out_dir = prefs.last_out_dir.as_deref().unwrap_or("data");
    let output_path = market_output_path(Path::new(out_dir), &market);

    prefs.last_crypto_interval = Some(format!("{interval:?}").to_ascii_lowercase());
    prefs.last_crypto_count = Some(count);
    save_prefs(prefs);

    exec_fetch(client, theme, &market, interval, count, &output_path, out).await
}

async fn wizard_fetch_stock<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    prefs: &mut UserPrefs,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let mut items: Vec<SelectItem> = STOCK_PRESETS
        .iter()
        .chain(STOCK_EXTENDED.iter())
        .map(|e| {
            let tag = if e.code.ends_with(".KS") || e.code.ends_with(".KQ") {
                "KR"
            } else {
                "US"
            };
            SelectItem {
                label: e.code.to_string(),
                hint: format!("[{tag}] {} ({})", e.name_ko, e.name_en),
            }
        })
        .collect();
    let presets_total = items.len();
    let idx_all = presets_total;
    items.push(SelectItem {
        label: "[추천 12종 전체]".to_string(),
        hint: "한국 6 + 미국 6 일괄 수집".to_string(),
    });
    let idx_kr = presets_total + 1;
    items.push(SelectItem {
        label: "[한국 6종 전체]".to_string(),
        hint: "국내 주요 대형주 일괄 수집".to_string(),
    });
    let idx_us = presets_total + 2;
    items.push(SelectItem {
        label: "[미국 6종 전체]".to_string(),
        hint: "미국 주요 빅테크 일괄 수집".to_string(),
    });
    let idx_custom = presets_total + 3;
    items.push(SelectItem {
        label: "[직접 종목 입력]".to_string(),
        hint: "종목코드(005930.KQ) / 미국 티커 직접 입력".to_string(),
    });

    let Some(picked) = select_one(theme, "주식 종목 선택 (Yahoo Finance)", &items, 0, reader, out)?
    else {
        writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
        return Ok(());
    };

    if picked == idx_all || picked == idx_kr || picked == idx_us {
        let group = if picked == idx_all {
            "all"
        } else if picked == idx_kr {
            "kr"
        } else {
            "us"
        };
        let symbols = resolve_stock_list(group);
        let (interval, range) = pick_stock_interval_and_range(theme, prefs, reader, out)?;
        let out_dir = prefs.last_out_dir.as_deref().unwrap_or("data");
        let cfg = PipelineConfig {
            markets: Vec::new(),
            symbols,
            crypto_interval: Interval::Days,
            count: 30,
            stock_interval: interval,
            range,
            out_dir: PathBuf::from(out_dir),
            throttle_ms: 120,
        };
        return exec_pipeline(client, theme, &cfg, out).await;
    }

    let symbol = if picked == idx_custom {
        let Some(raw) = prompt_with_default(reader, out, "  종목 심볼 또는 한글명", "005930.KS")?
        else {
            return Ok(());
        };
        resolve_stock_symbol(&raw)
    } else {
        items[picked].label.clone()
    };

    theme.ok_to(out, &format!("선택된 종목: {symbol}"))?;
    let (interval, range) = pick_stock_interval_and_range(theme, prefs, reader, out)?;
    let out_dir = prefs.last_out_dir.as_deref().unwrap_or("data");
    let output_path = symbol_output_path(Path::new(out_dir), &symbol);

    prefs.last_stock_interval = Some(format!("{interval:?}").to_ascii_lowercase());
    prefs.last_stock_range = Some(range.clone());
    save_prefs(prefs);

    exec_fetch_stock(client, theme, &symbol, interval, &range, &output_path, out).await
}

async fn wizard_pipeline<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    prefs: &mut UserPrefs,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let crypto_items: Vec<SelectItem> = CRYPTO_PRESETS
        .iter()
        .chain(CRYPTO_EXTENDED.iter())
        .map(|e| SelectItem {
            label: e.code.to_string(),
            hint: format!("{} ({})", e.name_ko, e.name_en),
        })
        .collect();
    let crypto_defaults: Vec<bool> = (0..crypto_items.len()).map(|i| i < 2).collect();
    let Some(picked_c) = select_many(
        theme,
        "배치 수집할 코인 선택 (Space 체크 · Enter 확정)",
        &crypto_items,
        &crypto_defaults,
        reader,
        out,
    )?
    else {
        writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
        return Ok(());
    };
    let markets: Vec<String> = picked_c
        .into_iter()
        .map(|i| crypto_items[i].label.clone())
        .collect();

    let stock_items: Vec<SelectItem> = STOCK_PRESETS
        .iter()
        .chain(STOCK_EXTENDED.iter())
        .map(|e| SelectItem {
            label: e.code.to_string(),
            hint: format!("{} ({})", e.name_ko, e.name_en),
        })
        .collect();
    let stock_defaults: Vec<bool> = (0..stock_items.len()).map(|i| i == 0 || i == 6).collect();
    let Some(picked_s) = select_many(
        theme,
        "배치 수집할 주식 선택 (Space 체크 · Enter 확정)",
        &stock_items,
        &stock_defaults,
        reader,
        out,
    )?
    else {
        writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
        return Ok(());
    };
    let symbols: Vec<String> = picked_s
        .into_iter()
        .map(|i| stock_items[i].label.clone())
        .collect();

    if markets.is_empty() && symbols.is_empty() {
        theme.warn_to(out, "선택된 코인 및 주식이 없습니다.")?;
        return Ok(());
    }

    let (crypto_interval, count) = if markets.is_empty() {
        (Interval::Days, 30)
    } else {
        pick_crypto_interval_and_count(theme, prefs, reader, out)?
    };

    let (stock_interval, range) = if symbols.is_empty() {
        (StockInterval::Day, "3mo".to_string())
    } else {
        pick_stock_interval_and_range(theme, prefs, reader, out)?
    };

    let out_dir = prefs.last_out_dir.as_deref().unwrap_or("data");
    let throttle_ms = if markets.len() + symbols.len() > 8 {
        120
    } else {
        0
    };
    let cfg = PipelineConfig {
        markets,
        symbols,
        crypto_interval,
        count,
        stock_interval,
        range,
        out_dir: PathBuf::from(out_dir),
        throttle_ms,
    };
    exec_pipeline(client, theme, &cfg, out).await
}

async fn wizard_all<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let items = vec![
        SelectItem {
            label: "추천 코인 10종 + 추천 주식 12종 전체".into(),
            hint: "22종 한 번에 수집".into(),
        },
        SelectItem {
            label: "추천 메이저 코인 10종 전체".into(),
            hint: "KRW-BTC ~ KRW-DOT".into(),
        },
        SelectItem {
            label: "추천 주식 12종 전체".into(),
            hint: "한국 6종 + 미국 6종".into(),
        },
        SelectItem {
            label: "한국 주요 주식 6종 전체".into(),
            hint: "삼성전자, SK하이닉스, NAVER, 현대차, 카카오, LG엔솔".into(),
        },
        SelectItem {
            label: "미국 주요 주식 6종 전체".into(),
            hint: "AAPL, NVDA, MSFT, TSLA, GOOGL, AMZN".into(),
        },
        SelectItem {
            label: "Upbit 원화(KRW) 상장 마켓 전체".into(),
            hint: "실시간 상장 목록 조회 후 안전 수집".into(),
        },
    ];
    let Some(choice) = select_one(theme, "전체 일괄 수집 범위 선택", &items, 0, reader, out)?
    else {
        writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
        return Ok(());
    };

    let (markets, symbols, all_upbit) = match choice {
        0 => (resolve_crypto_list("all"), resolve_stock_list("all"), false),
        1 => (resolve_crypto_list("all"), Vec::new(), false),
        2 => (Vec::new(), resolve_stock_list("all"), false),
        3 => (Vec::new(), resolve_stock_list("kr"), false),
        4 => (Vec::new(), resolve_stock_list("us"), false),
        _ => (Vec::new(), Vec::new(), true),
    };

    let mut prefs = load_prefs();
    let (crypto_interval, count) = if !markets.is_empty() || all_upbit {
        pick_crypto_interval_and_count(theme, &prefs, reader, out)?
    } else {
        (Interval::Days, 30)
    };
    let (stock_interval, range) = if !symbols.is_empty() {
        pick_stock_interval_and_range(theme, &prefs, reader, out)?
    } else {
        (StockInterval::Day, "3mo".to_string())
    };
    let out_dir = prefs.last_out_dir.take().unwrap_or_else(|| "data".into());

    exec_pipeline_flags(
        client,
        theme,
        &markets,
        &symbols,
        crypto_interval,
        count,
        stock_interval,
        &range,
        Path::new(&out_dir),
        false,
        all_upbit,
        out,
    )
    .await
}

async fn wizard_watch<R: BufRead, W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    prefs: &mut UserPrefs,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    render_watchlist(theme, prefs, out)?;
    let actions = vec![
        SelectItem {
            label: "관심 종목 전체 수집 실행".into(),
            hint: format!("등록된 {}종목 일괄 수집", prefs.total_watchlist()),
        },
        SelectItem {
            label: "관심 종목 추가 (체크박스)".into(),
            hint: "추천 코인·주식에서 여러 개 선택해 등록".into(),
        },
        SelectItem {
            label: "관심 종목 삭제 (체크박스)".into(),
            hint: "등록된 관심 종목에서 제거".into(),
        },
    ];
    let Some(pick) = select_one(theme, "관심 종목 (Watchlist) 관리", &actions, 0, reader, out)?
    else {
        return Ok(());
    };

    match pick {
        0 => exec_watch(client, theme, Some(&WatchAction::Run { out_dir: PathBuf::from("data") }), out).await,
        1 => {
            let mut pool_items = Vec::new();
            let mut pool_entries: Vec<(bool, String)> = Vec::new();
            for e in CRYPTO_PRESETS.iter().chain(CRYPTO_EXTENDED.iter()) {
                pool_items.push(SelectItem {
                    label: e.code.to_string(),
                    hint: format!("[CRYPTO] {}", e.name_ko),
                });
                pool_entries.push((true, e.code.to_string()));
            }
            for e in STOCK_PRESETS.iter().chain(STOCK_EXTENDED.iter()) {
                pool_items.push(SelectItem {
                    label: e.code.to_string(),
                    hint: format!("[STOCK] {}", e.name_ko),
                });
                pool_entries.push((false, e.code.to_string()));
            }
            let initial: Vec<bool> = pool_entries
                .iter()
                .map(|(is_c, code)| {
                    if *is_c {
                        prefs.watchlist_crypto.iter().any(|c| c == code)
                    } else {
                        prefs.watchlist_stocks.iter().any(|s| s == code)
                    }
                })
                .collect();
            let Some(selected) = select_many(
                theme,
                "관심 종목 체크박스 선택 (Space 토글 · Enter 저장)",
                &pool_items,
                &initial,
                reader,
                out,
            )?
            else {
                return Ok(());
            };

            let mut added = 0usize;
            for idx in selected {
                let (is_c, code) = &pool_entries[idx];
                let inserted = if *is_c {
                    prefs.add_crypto(code)
                } else {
                    prefs.add_stock(code)
                };
                if inserted {
                    added += 1;
                }
            }
            save_prefs(prefs);
            theme.ok_to(
                out,
                &format!("관심 종목 저장 완료 (신규 {added}개 추가, 총 {}개)", prefs.total_watchlist()),
            )?;
            Ok(())
        }
        _ => {
            if prefs.total_watchlist() == 0 {
                theme.warn_to(out, "삭제할 관심 종목이 없습니다.")?;
                return Ok(());
            }
            let mut current_items = Vec::new();
            let mut current_codes = Vec::new();
            for c in &prefs.watchlist_crypto {
                current_items.push(SelectItem {
                    label: c.clone(),
                    hint: "[CRYPTO]".into(),
                });
                current_codes.push(c.clone());
            }
            for s in &prefs.watchlist_stocks {
                current_items.push(SelectItem {
                    label: s.clone(),
                    hint: "[STOCK]".into(),
                });
                current_codes.push(s.clone());
            }
            let initial = vec![false; current_items.len()];
            let Some(picked) = select_many(
                theme,
                "삭제할 관심 종목 선택 (Space 체크 · Enter 삭제)",
                &current_items,
                &initial,
                reader,
                out,
            )?
            else {
                return Ok(());
            };
            let mut removed = 0usize;
            for idx in picked {
                if prefs.remove_target(&current_codes[idx]) {
                    removed += 1;
                }
            }
            save_prefs(prefs);
            theme.ok_to(out, &format!("{removed}개 관심 종목을 삭제했습니다."))?;
            Ok(())
        }
    }
}

fn wizard_show<R: BufRead, W: Write>(
    theme: &Theme,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let files = list_csv_files(Path::new("data")).unwrap_or_default();
    let input_path = if files.is_empty() {
        let Some(p) = prompt_with_default(reader, out, "  조회할 CSV 경로", "data/sample_stock.csv")?
        else {
            return Ok(());
        };
        PathBuf::from(p)
    } else if is_interactive_tty() {
        let items: Vec<SelectItem> = files
            .iter()
            .map(|p| {
                let hint = match load_dataset(p) {
                    Ok(ds) => format_dataset_summary(&ds),
                    Err(e) => format!("읽기 오류: {e}"),
                };
                SelectItem {
                    label: p.display().to_string().replace('\\', "/"),
                    hint,
                }
            })
            .collect();
        let Some(idx) = select_one(theme, "조회할 CSV 파일 선택", &items, 0, reader, out)? else {
            writeln!(out, "  {}", theme.dim("취소되었습니다."))?;
            return Ok(());
        };
        files[idx].clone()
    } else {
        theme.section_to(out, "Step Wizard · Inspect CSV Dataset")?;
        writeln!(out, "  사용 가능한 파일:")?;
        for (idx, path) in files.iter().enumerate() {
            writeln!(out, "    [{}] {}", idx + 1, path.display())?;
        }
        let Some(target) = prompt_with_default(reader, out, "  조회할 파일 번호 또는 CSV 경로", "1")?
        else {
            return Ok(());
        };
        if let Ok(num) = target.parse::<usize>()
            && num >= 1
            && num <= files.len()
        {
            files[num - 1].clone()
        } else {
            PathBuf::from(target)
        }
    };

    let Some(limit_str) = prompt_with_default(reader, out, "  표시할 행 수", "10")? else {
        return Ok(());
    };
    let limit: usize = limit_str
        .parse()
        .with_context(|| format!("잘못된 행 수: {limit_str}"))?;

    let Some(tail_str) = prompt_with_default(reader, out, "  최신 행부터 보기 (Y/n)", "Y")?
    else {
        return Ok(());
    };
    let tail = !matches!(tail_str.to_ascii_lowercase().as_str(), "n" | "no" | "false");

    exec_show(theme, &input_path, limit, tail, None, out)
}

fn wizard_open<R: BufRead, W: Write>(
    theme: &Theme,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<()> {
    let files = list_csv_files(Path::new("data")).unwrap_or_default();
    let mut items = vec![SelectItem {
        label: "data/ 폴더 열기".to_string(),
        hint: "파일 탐색기에서 데이터 폴더 열기".to_string(),
    }];
    let mut paths = vec![PathBuf::from("data")];
    for f in files {
        items.push(SelectItem {
            label: f.display().to_string().replace('\\', "/"),
            hint: "기본 앱(Excel)으로 CSV 열기".to_string(),
        });
        paths.push(f);
    }
    let Some(idx) = select_one(theme, "열 파일 또는 폴더 선택", &items, 0, reader, out)? else {
        return Ok(());
    };
    exec_open(theme, Some(&paths[idx]), out)
}

fn pick_crypto_interval_and_count<R: BufRead, W: Write>(
    theme: &Theme,
    prefs: &UserPrefs,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<(Interval, u32)> {
    let options = [
        (Interval::Days, "일봉 (days)", "1일 단위"),
        (Interval::Weeks, "주봉 (weeks)", "1주 단위"),
        (Interval::Months, "월봉 (months)", "1개월 단위"),
        (Interval::Minutes1, "1분봉 (1m)", "초단기 분봉"),
        (Interval::Minutes30, "30분봉 (30m)", "단타 분봉"),
        (Interval::Minutes60, "60분봉 (1h)", "1시간 봉"),
        (Interval::Minutes240, "240분봉 (4h)", "4시간 봉"),
    ];
    let default_idx = prefs
        .last_crypto_interval
        .as_deref()
        .and_then(Interval::parse_input)
        .and_then(|iv| options.iter().position(|(v, _, _)| *v == iv))
        .unwrap_or(0);
    let items: Vec<SelectItem> = options
        .iter()
        .map(|(_, lbl, hint)| SelectItem {
            label: (*lbl).to_string(),
            hint: (*hint).to_string(),
        })
        .collect();
    let Some(idx) = select_one(theme, "코인 봉 간격 선택", &items, default_idx, reader, out)?
    else {
        anyhow::bail!("입력이 취소되었습니다.");
    };
    let interval = options[idx].0;

    let def_count = prefs.last_crypto_count.unwrap_or(30).to_string();
    let Some(count_str) = prompt_with_default(reader, out, "  코인 봉 개수 (1~200)", &def_count)?
    else {
        anyhow::bail!("입력이 취소되었습니다.");
    };
    let count: u32 = count_str
        .parse()
        .with_context(|| format!("잘못된 봉 개수: {count_str}"))?;
    Ok((interval, count))
}

fn pick_stock_interval_and_range<R: BufRead, W: Write>(
    theme: &Theme,
    prefs: &UserPrefs,
    reader: &mut R,
    out: &mut W,
) -> anyhow::Result<(StockInterval, String)> {
    let intervals = [
        (StockInterval::Day, "일봉 (day)", "거래일 기준 일간 봉"),
        (StockInterval::Week, "주봉 (week)", "주간 봉"),
        (StockInterval::Month, "월봉 (month)", "월간 봉"),
    ];
    let def_iv = prefs
        .last_stock_interval
        .as_deref()
        .and_then(StockInterval::parse_input)
        .and_then(|iv| intervals.iter().position(|(v, _, _)| *v == iv))
        .unwrap_or(0);
    let iv_items: Vec<SelectItem> = intervals
        .iter()
        .map(|(_, lbl, hint)| SelectItem {
            label: (*lbl).to_string(),
            hint: (*hint).to_string(),
        })
        .collect();
    let Some(iv_idx) = select_one(theme, "주식 봉 간격 선택", &iv_items, def_iv, reader, out)?
    else {
        anyhow::bail!("입력이 취소되었습니다.");
    };

    let ranges = [
        ("1mo", "최근 1개월"),
        ("3mo", "최근 3개월"),
        ("6mo", "최근 6개월"),
        ("1y", "최근 1년"),
        ("2y", "최근 2년"),
        ("5y", "최근 5년"),
        ("ytd", "올해 초 ~ 현재"),
        ("max", "상장 전체 기간"),
    ];
    let def_rng = prefs
        .last_stock_range
        .as_deref()
        .and_then(|r| ranges.iter().position(|(code, _)| *code == r))
        .unwrap_or(1);
    let rng_items: Vec<SelectItem> = ranges
        .iter()
        .map(|(code, hint)| SelectItem {
            label: (*code).to_string(),
            hint: (*hint).to_string(),
        })
        .collect();
    let Some(r_idx) = select_one(theme, "조회 기간 선택", &rng_items, def_rng, reader, out)?
    else {
        anyhow::bail!("입력이 취소되었습니다.");
    };

    Ok((intervals[iv_idx].0, ranges[r_idx].0.to_string()))
}

fn render_watchlist<W: Write>(theme: &Theme, prefs: &UserPrefs, out: &mut W) -> anyhow::Result<()> {
    theme.section_to(out, "Saved Watchlist (관심 종목)")?;
    if prefs.total_watchlist() == 0 {
        theme.warn_to(out, "등록된 관심 종목이 없습니다.")?;
        return Ok(());
    }
    let mut rows = Vec::new();
    for (i, c) in prefs.watchlist_crypto.iter().enumerate() {
        let csv_path = market_output_path(Path::new("data"), c);
        let status = if csv_path.is_file() {
            load_dataset(&csv_path)
                .ok()
                .and_then(|ds| summarize_dataset(&ds))
                .map(|(_, _, _, latest, _, pct)| format!("{} ({pct:+.2}%)", format_price(latest)))
                .unwrap_or_else(|| "저장됨".into())
        } else {
            "미수집".into()
        };
        rows.push(vec![
            (i + 1).to_string(),
            theme.badge("CRYPTO", Tone::Info),
            c.clone(),
            status,
        ]);
    }
    let offset = prefs.watchlist_crypto.len();
    for (i, s) in prefs.watchlist_stocks.iter().enumerate() {
        let csv_path = symbol_output_path(Path::new("data"), s);
        let status = if csv_path.is_file() {
            load_dataset(&csv_path)
                .ok()
                .and_then(|ds| summarize_dataset(&ds))
                .map(|(_, _, _, latest, _, pct)| format!("{} ({pct:+.2}%)", format_price(latest)))
                .unwrap_or_else(|| "저장됨".into())
        } else {
            "미수집".into()
        };
        rows.push(vec![
            (offset + i + 1).to_string(),
            theme.badge("STOCK", Tone::Accent),
            s.clone(),
            status,
        ]);
    }
    theme.table_to(out, &["#", "TYPE", "ASSET", "LATEST / STATUS"], &rows, &[0])?;
    Ok(())
}

fn render_dataset_window<W: Write>(
    theme: &Theme,
    dataset: &Dataset,
    limit: usize,
    tail: bool,
    out: &mut W,
) -> anyhow::Result<usize> {
    match dataset {
        Dataset::Candles(candles) => {
            let mut rows_with_chg: Vec<Vec<String>> = Vec::with_capacity(candles.len());
            for (i, c) in candles.iter().enumerate() {
                let base = if i == 0 {
                    c.opening_price
                } else {
                    candles[i - 1].trade_price
                };
                let chg = if base == 0.0 {
                    0.0
                } else {
                    (c.trade_price - base) / base * 100.0
                };
                rows_with_chg.push(vec![
                    c.candle_date_time_kst.clone(),
                    format_price(c.opening_price),
                    format_price(c.high_price),
                    format_price(c.low_price),
                    format_price(c.trade_price),
                    format_pct(theme, chg),
                    format_volume(c.candle_acc_trade_volume),
                ]);
            }
            let slice = slice_rows(&rows_with_chg, limit, tail);
            let count = slice.len();
            theme.table_to(
                out,
                &["kst", "open", "high", "low", "close", "chg%", "volume"],
                slice,
                &[1, 2, 3, 4, 5, 6],
            )?;
            Ok(count)
        }
        Dataset::Stocks(bars) => {
            let mut rows_with_chg: Vec<Vec<String>> = Vec::with_capacity(bars.len());
            for (i, b) in bars.iter().enumerate() {
                let base = if i == 0 { b.open } else { bars[i - 1].close };
                let chg = if base == 0.0 {
                    0.0
                } else {
                    (b.close - base) / base * 100.0
                };
                rows_with_chg.push(vec![
                    b.date.clone(),
                    format_price(b.open),
                    format_price(b.high),
                    format_price(b.low),
                    format_price(b.close),
                    format_pct(theme, chg),
                    format_volume(b.volume),
                ]);
            }
            let slice = slice_rows(&rows_with_chg, limit, tail);
            let count = slice.len();
            theme.table_to(
                out,
                &["date", "open", "high", "low", "close", "chg%", "volume"],
                slice,
                &[1, 2, 3, 4, 5, 6],
            )?;
            Ok(count)
        }
    }
}

fn slice_rows<T>(rows: &[T], limit: usize, tail: bool) -> &[T] {
    if limit == 0 || rows.is_empty() {
        return &[];
    }
    if tail {
        let start = rows.len().saturating_sub(limit);
        &rows[start..]
    } else {
        &rows[..rows.len().min(limit)]
    }
}

fn failed_summary_row(
    step: usize,
    kind_badge: String,
    asset: &str,
    theme: &Theme,
    file_name: &str,
) -> Vec<String> {
    vec![
        step.to_string(),
        kind_badge,
        asset.to_string(),
        theme.badge("FAIL", Tone::Err),
        "-".into(),
        "-".into(),
        "-".into(),
        "-".into(),
        file_name.to_string(),
    ]
}

fn summarize_dataset(dataset: &Dataset) -> Option<(String, usize, String, f64, String, f64)> {
    match dataset {
        Dataset::Candles(rows) => summarize_candles(rows),
        Dataset::Stocks(rows) => summarize_stocks(rows),
    }
}

fn summarize_candles(candles: &[Candle]) -> Option<(String, usize, String, f64, String, f64)> {
    let first = candles.first()?;
    let last = candles.last()?;
    let mut high = f64::MIN;
    let mut low = f64::MAX;
    for c in candles {
        high = high.max(c.high_price);
        low = low.min(c.low_price);
    }
    let pct = if first.opening_price == 0.0 {
        0.0
    } else {
        (last.trade_price - first.opening_price) / first.opening_price * 100.0
    };
    let period = format!(
        "{} ~ {}",
        short_date(&first.candle_date_time_kst),
        short_date(&last.candle_date_time_kst)
    );
    Some((
        last.market.clone(),
        candles.len(),
        period,
        last.trade_price,
        format!("{} / {}", format_price(high), format_price(low)),
        pct,
    ))
}

fn summarize_stocks(bars: &[StockBar]) -> Option<(String, usize, String, f64, String, f64)> {
    let first = bars.first()?;
    let last = bars.last()?;
    let mut high = f64::MIN;
    let mut low = f64::MAX;
    for b in bars {
        high = high.max(b.high);
        low = low.min(b.low);
    }
    let pct = if first.open == 0.0 {
        0.0
    } else {
        (last.close - first.open) / first.open * 100.0
    };
    let period = format!("{} ~ {}", first.date, last.date);
    Some((
        last.symbol.clone(),
        bars.len(),
        period,
        last.close,
        format!("{} / {}", format_price(high), format_price(low)),
        pct,
    ))
}

fn infer_stock_range_from_count(len: usize, interval: StockInterval) -> &'static str {
    match interval {
        StockInterval::Day => match len {
            0..=6 => "5d",
            7..=25 => "1mo",
            26..=70 => "3mo",
            71..=140 => "6mo",
            141..=260 => "1y",
            _ => "2y",
        },
        StockInterval::Week => match len {
            0..=15 => "3mo",
            16..=30 => "6mo",
            31..=60 => "1y",
            _ => "2y",
        },
        StockInterval::Month => match len {
            0..=15 => "1y",
            16..=30 => "2y",
            _ => "5y",
        },
    }
}

fn open_with_os(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .spawn()
            .with_context(|| format!("failed to open {}", path.display()))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .with_context(|| format!("failed to open {}", path.display()))?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| format!("failed to open {}", path.display()))?;
    }
    Ok(())
}

fn short_date(ts: &str) -> &str {
    ts.split('T').next().unwrap_or(ts)
}

fn pct_tone(pct: f64) -> Tone {
    if pct > 0.0 {
        Tone::Ok
    } else if pct < 0.0 {
        Tone::Err
    } else {
        Tone::Muted
    }
}

fn format_pct(theme: &Theme, pct: f64) -> String {
    theme.style(&format!("{pct:+.2}%"), pct_tone(pct), pct.abs() >= 10.0)
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.csv")
        .to_string()
}

fn truncate_msg(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

fn default_update_options() -> UpdateOptions {
    UpdateOptions {
        check: false,
        force: false,
        no_pull: false,
        from: None,
        repo: None,
        branch: "main".to_string(),
    }
}

fn prompt_with_default<R: BufRead, W: Write>(
    reader: &mut R,
    out: &mut W,
    label: &str,
    default: &str,
) -> anyhow::Result<Option<String>> {
    write!(out, "{label} [{default}]: ")?;
    out.flush()?;
    let mut buf = String::new();
    let bytes = reader.read_line(&mut buf)?;
    if bytes == 0 {
        return Ok(None);
    }
    let trimmed = buf.trim().trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        Ok(Some(default.to_string()))
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn market_output_path(out_dir: &Path, market: &str) -> PathBuf {
    let stem: String = market
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    out_dir.join(format!("{stem}.csv"))
}

fn symbol_output_path(out_dir: &Path, symbol: &str) -> PathBuf {
    let stem: String = symbol
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    out_dir.join(format!("{stem}.csv"))
}

fn list_csv_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn print_banner<W: Write>(theme: &Theme, out: &mut W) -> anyhow::Result<()> {
    theme.banner_to(
        out,
        &format!(
            "QUANT CLI v{} · Market Data Collection Pipeline",
            env!("CARGO_PKG_VERSION")
        ),
        "Upbit (Crypto) & Yahoo Finance (Stocks) · Interactive Terminal Workbench",
    )?;
    Ok(())
}

fn print_menu<W: Write>(theme: &Theme, out: &mut W) -> anyhow::Result<()> {
    theme.section_to(out, "Pipeline Actions & Slash Commands")?;
    writeln!(
        out,
        "  {} {}  코인 수집 (검색·선택)     {} {}  관심 종목 관리·수집",
        theme.style("[1]", Tone::Info, true),
        theme.bold("/fetch   "),
        theme.style("[w]", Tone::Warn, true),
        theme.bold("/watch  ")
    )?;
    writeln!(
        out,
        "  {} {}  주식 수집 (검색·선택)     {} {}  저장 데이터 일괄 최신화",
        theme.style("[2]", Tone::Info, true),
        theme.bold("/stock   "),
        theme.style("[r]", Tone::Ok, true),
        theme.bold("/refresh")
    )?;
    writeln!(
        out,
        "  {} {}  다중 종목 체크박스 수집   {} {}  전체 마켓·종목 검색",
        theme.style("[3]", Tone::Info, true),
        theme.bold("/pipeline"),
        theme.style("[m]", Tone::Accent, true),
        theme.bold("/markets")
    )?;
    writeln!(
        out,
        "  {} {}  전체 일괄 수집 (22종/업빗) {} {}  CSV/폴더 기본 앱 열기",
        theme.style("[a]", Tone::Ok, true),
        theme.bold("/all     "),
        theme.style("[o]", Tone::Accent, true),
        theme.bold("/open   ")
    )?;
    writeln!(
        out,
        "  {} {}  데이터 차트·표 상세 조회  {} {}  버전 확인 및 업데이트",
        theme.style("[4]", Tone::Info, true),
        theme.bold("/show    "),
        theme.style("[u]", Tone::Warn, true),
        theme.bold("/update ")
    )?;
    writeln!(
        out,
        "  {} {}  저장 현황 대시보드        {} {}  도움말 · {} {} 종료",
        theme.style("[5]", Tone::Info, true),
        theme.bold("/status  "),
        theme.style("[h]", Tone::Muted, true),
        theme.bold("/help"),
        theme.style("[q]", Tone::Muted, true),
        theme.bold("/exit")
    )?;
    Ok(())
}

fn print_command_examples<W: Write>(theme: &Theme, out: &mut W) -> anyhow::Result<()> {
    theme.section_to(out, "Direct Command Examples")?;
    writeln!(
        out,
        "  {}  {}",
        theme.bold("/"),
        theme.dim("# 화살표 명령 팔레트 열기 (Tab 자동완성, ↑↓ 이력 지원)")
    )?;
    writeln!(
        out,
        "  {}  {}",
        theme.bold("/watch add 솔라나 삼성전자 NVDA"),
        theme.dim("# 관심 종목 등록 (코인/주식 자동 판별)")
    )?;
    writeln!(
        out,
        "  {}  {}",
        theme.bold("/refresh"),
        theme.dim("# data/ 폴더 내 수집된 CSV 모두 자동 최신화")
    )?;
    writeln!(
        out,
        "  {}  {}",
        theme.bold("/pipeline --all"),
        theme.dim("# 추천 코인 10 + 주식 12 전체 수집")
    )?;
    writeln!(
        out,
        "  {}  {}",
        theme.bold("/pipeline --all-upbit"),
        theme.dim("# Upbit 원화 마켓 전체 수집")
    )?;
    writeln!(
        out,
        "  {}  {}",
        theme.bold("/show --input data/sample_stock.csv --limit 5 --tail"),
        theme.dim("# CSV 조회")
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn pipeline_paths_normalize_names() {
        let dir = Path::new("data");
        assert_eq!(
            market_output_path(dir, "KRW-BTC"),
            PathBuf::from("data").join("krw-btc.csv")
        );
        assert_eq!(
            symbol_output_path(dir, "005930.KS"),
            PathBuf::from("data").join("005930_ks.csv")
        );
    }

    #[test]
    fn slash_prefix_autocomplete_and_tab_completion_work() {
        assert_eq!(resolve_slash_prefix("f").as_deref(), Ok("fetch"));
        assert_eq!(resolve_slash_prefix("w").as_deref(), Ok("watch"));
        assert_eq!(resolve_slash_prefix("r").as_deref(), Ok("refresh"));
        assert_eq!(resolve_slash_prefix("sh").as_deref(), Ok("show"));
        assert_eq!(resolve_slash_prefix("st").as_deref(), Ok("status"));
        assert!(resolve_slash_prefix("s").is_err());

        assert_eq!(complete_prompt("/wa").as_deref(), Some("/watch "));
        assert_eq!(complete_prompt("/op").as_deref(), Some("/open "));
        assert_eq!(complete_prompt("/fetch 솔").as_deref(), Some("/fetch KRW-SOL "));
        assert_eq!(complete_prompt("/watch a").as_deref(), Some("/watch add "));
        assert_eq!(complete_prompt("/watch r").as_deref(), Some("/watch run "));
        assert_eq!(complete_prompt("/watch add 삼").as_deref(), Some("/watch add 005930.KS "));
        assert!(!suggest_for_prompt("/").is_empty());
        assert!(!suggest_for_prompt("/s").is_empty());
        assert!(!suggest_for_prompt("/stock 삼").is_empty());
    }

    #[tokio::test]
    async fn repl_handles_status_markets_autocomplete_show_errors_and_quit() {
        let client = reqwest::Client::new();
        let theme = Theme::plain();
        let input = "\
/st
/markets
/s
4
data/sample_stock.csv
2
y
/show --input data/sample.csv --limit 2 --filter 2026-01
unknown-cmd
/exit
";
        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();

        run_repl_with_io(&client, &theme, &mut reader, &mut output)
            .await
            .expect("repl must succeed");

        let text = String::from_utf8(output).expect("valid utf8");
        assert!(text.contains("QUANT CLI v"));
        assert!(text.contains("Pipeline Storage Cockpit"));
        assert!(text.contains("TREND"));
        assert!(text.contains("QUANT MARKET & SYMBOL CATALOG"));
        assert!(text.contains("자동완성 후보가 여러 개입니다"));
        assert!(text.contains("71,000"));
        assert!(text.contains("(2 rows shown from data/sample.csv)"));
        assert!(text.contains("unrecognized subcommand"));
        assert!(text.contains("quant CLI 서비스를 종료합니다."));
    }
}
