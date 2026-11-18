use std::collections::HashSet;
use std::io::Write;
use std::time::Duration;

use serde::Deserialize;

use crate::style::Theme;
use crate::tui::Spinner;

/// 내장 추천 종목/마켓 엔트리.
#[derive(Debug, Clone, Copy)]
pub struct AssetEntry {
    pub code: &'static str,
    pub name_ko: &'static str,
    pub name_en: &'static str,
    pub aliases: &'static [&'static str],
}

/// Upbit `/v1/market/all` 실시간 응답 항목.
#[derive(Debug, Clone, Deserialize)]
pub struct UpbitMarketInfo {
    pub market: String,
    pub korean_name: String,
    pub english_name: String,
}

/// 자동완성 매칭 결과: 단일 확정 또는 복수 후보.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOutcome {
    Exact(String, String),
    Candidates(Vec<(String, String)>),
}

pub const CRYPTO_PRESETS: &[AssetEntry] = &[
    AssetEntry { code: "KRW-BTC", name_ko: "비트코인", name_en: "Bitcoin", aliases: &["btc", "비트"] },
    AssetEntry { code: "KRW-ETH", name_ko: "이더리움", name_en: "Ethereum", aliases: &["eth", "이더"] },
    AssetEntry { code: "KRW-XRP", name_ko: "리플", name_en: "XRP", aliases: &["xrp", "ripple", "리플"] },
    AssetEntry { code: "KRW-SOL", name_ko: "솔라나", name_en: "Solana", aliases: &["sol", "솔라나"] },
    AssetEntry { code: "KRW-DOGE", name_ko: "도지코인", name_en: "Dogecoin", aliases: &["doge", "도지"] },
    AssetEntry { code: "KRW-SUI", name_ko: "수이", name_en: "Sui", aliases: &["sui", "수이"] },
    AssetEntry { code: "KRW-ADA", name_ko: "에이다", name_en: "Cardano", aliases: &["ada", "cardano", "카르다노"] },
    AssetEntry { code: "KRW-AVAX", name_ko: "아발란체", name_en: "Avalanche", aliases: &["avax", "아발란체"] },
    AssetEntry { code: "KRW-LINK", name_ko: "체인링크", name_en: "Chainlink", aliases: &["link", "체인링크"] },
    AssetEntry { code: "KRW-DOT", name_ko: "폴카닷", name_en: "Polkadot", aliases: &["dot", "폴카닷"] },
];

pub const CRYPTO_EXTENDED: &[AssetEntry] = &[
    AssetEntry { code: "KRW-SHIB", name_ko: "시바이누", name_en: "Shiba Inu", aliases: &["shib", "시바"] },
    AssetEntry { code: "KRW-PEPE", name_ko: "페페", name_en: "Pepe", aliases: &["pepe", "페페"] },
    AssetEntry { code: "KRW-TRX", name_ko: "트론", name_en: "TRON", aliases: &["trx", "트론"] },
    AssetEntry { code: "KRW-BCH", name_ko: "비트코인캐시", name_en: "Bitcoin Cash", aliases: &["bch", "비캐"] },
    AssetEntry { code: "KRW-ETC", name_ko: "이더리움클래식", name_en: "Ethereum Classic", aliases: &["etc", "이클"] },
    AssetEntry { code: "KRW-NEAR", name_ko: "니어프로토콜", name_en: "NEAR Protocol", aliases: &["near", "니어"] },
    AssetEntry { code: "KRW-APT", name_ko: "앱토스", name_en: "Aptos", aliases: &["apt", "앱토스"] },
    AssetEntry { code: "KRW-ARB", name_ko: "아비트럼", name_en: "Arbitrum", aliases: &["arb", "아비트럼"] },
    AssetEntry { code: "KRW-OP", name_ko: "옵티미즘", name_en: "Optimism", aliases: &["op", "옵티미즘"] },
    AssetEntry { code: "KRW-ATOM", name_ko: "코스모스", name_en: "Cosmos", aliases: &["atom", "코스모스"] },
    AssetEntry { code: "KRW-STX", name_ko: "스택스", name_en: "Stacks", aliases: &["stx", "스택스"] },
    AssetEntry { code: "KRW-HBAR", name_ko: "헤데라", name_en: "Hedera", aliases: &["hbar", "헤데라"] },
    AssetEntry { code: "KRW-SAND", name_ko: "샌드박스", name_en: "The Sandbox", aliases: &["sand", "샌드박스"] },
    AssetEntry { code: "KRW-MANA", name_ko: "디센트럴랜드", name_en: "Decentraland", aliases: &["mana", "디센트럴"] },
];

pub const STOCK_PRESETS: &[AssetEntry] = &[
    AssetEntry { code: "005930.KS", name_ko: "삼성전자", name_en: "Samsung Electronics", aliases: &["삼전", "삼성전자"] },
    AssetEntry { code: "000660.KS", name_ko: "SK하이닉스", name_en: "SK Hynix", aliases: &["하이닉스", "sk하이닉스", "hynix"] },
    AssetEntry { code: "035420.KS", name_ko: "NAVER", name_en: "Naver", aliases: &["네이버", "naver"] },
    AssetEntry { code: "005380.KS", name_ko: "현대차", name_en: "Hyundai Motor", aliases: &["현대자동차", "현대차"] },
    AssetEntry { code: "035720.KS", name_ko: "카카오", name_en: "Kakao", aliases: &["카카오", "kakao"] },
    AssetEntry { code: "373220.KS", name_ko: "LG에너지솔루션", name_en: "LG Energy Solution", aliases: &["엘지엔솔", "lg엔솔", "에너지솔루션"] },
    AssetEntry { code: "AAPL", name_ko: "애플", name_en: "Apple", aliases: &["애플", "apple"] },
    AssetEntry { code: "NVDA", name_ko: "엔비디아", name_en: "NVIDIA", aliases: &["엔비디아", "nvidia"] },
    AssetEntry { code: "MSFT", name_ko: "마이크로소프트", name_en: "Microsoft", aliases: &["마소", "마이크로소프트", "microsoft"] },
    AssetEntry { code: "TSLA", name_ko: "테슬라", name_en: "Tesla", aliases: &["테슬라", "tesla"] },
    AssetEntry { code: "GOOGL", name_ko: "알파벳(구글)", name_en: "Alphabet Google", aliases: &["구글", "알파벳", "google"] },
    AssetEntry { code: "AMZN", name_ko: "아마존", name_en: "Amazon", aliases: &["아마존", "amazon"] },
];

pub const STOCK_EXTENDED: &[AssetEntry] = &[
    AssetEntry { code: "207940.KS", name_ko: "삼성바이오로직스", name_en: "Samsung Biologics", aliases: &["삼바", "삼성바이오"] },
    AssetEntry { code: "006400.KS", name_ko: "삼성SDI", name_en: "Samsung SDI", aliases: &["삼성sdi"] },
    AssetEntry { code: "028260.KS", name_ko: "삼성물산", name_en: "Samsung C&T", aliases: &["삼성물산"] },
    AssetEntry { code: "000270.KS", name_ko: "기아", name_en: "Kia", aliases: &["기아", "기아차", "kia"] },
    AssetEntry { code: "068270.KS", name_ko: "셀트리온", name_en: "Celltrion", aliases: &["셀트리온"] },
    AssetEntry { code: "005490.KS", name_ko: "POSCO홀딩스", name_en: "POSCO Holdings", aliases: &["포스코", "포스코홀딩스", "posco"] },
    AssetEntry { code: "105560.KS", name_ko: "KB금융", name_en: "KB Financial", aliases: &["kb금융", "국민은행"] },
    AssetEntry { code: "055550.KS", name_ko: "신한지주", name_en: "Shinhan Financial", aliases: &["신한지주", "신한금융"] },
    AssetEntry { code: "086790.KS", name_ko: "하나금융지주", name_en: "Hana Financial", aliases: &["하나금융"] },
    AssetEntry { code: "012450.KS", name_ko: "한화에어로스페이스", name_en: "Hanwha Aerospace", aliases: &["한화에어로", "에어로스페이스"] },
    AssetEntry { code: "042660.KS", name_ko: "한화오션", name_en: "Hanwha Ocean", aliases: &["한화오션"] },
    AssetEntry { code: "034020.KS", name_ko: "두산에너빌리티", name_en: "Doosan Enerbility", aliases: &["두산에너빌리티", "두빌"] },
    AssetEntry { code: "247540.KQ", name_ko: "에코프로비엠", name_en: "EcoPro BM", aliases: &["에코프로비엠", "에코비엠"] },
    AssetEntry { code: "086520.KQ", name_ko: "에코프로", name_en: "EcoPro", aliases: &["에코프로"] },
    AssetEntry { code: "196170.KQ", name_ko: "알테오젠", name_en: "Alteogen", aliases: &["알테오젠"] },
    AssetEntry { code: "META", name_ko: "메타", name_en: "Meta Platforms", aliases: &["메타", "페이스북", "meta"] },
    AssetEntry { code: "AVGO", name_ko: "브로드컴", name_en: "Broadcom", aliases: &["브로드컴", "broadcom"] },
    AssetEntry { code: "AMD", name_ko: "AMD", name_en: "AMD", aliases: &["amd"] },
    AssetEntry { code: "NFLX", name_ko: "넷플릭스", name_en: "Netflix", aliases: &["넷플릭스", "netflix"] },
    AssetEntry { code: "PLTR", name_ko: "팔란티어", name_en: "Palantir", aliases: &["팔란티어", "palantir"] },
    AssetEntry { code: "COIN", name_ko: "코인베이스", name_en: "Coinbase", aliases: &["코인베이스", "coinbase"] },
    AssetEntry { code: "QQQ", name_ko: "나스닥100 ETF", name_en: "Invesco QQQ", aliases: &["나스닥", "qqq"] },
    AssetEntry { code: "SPY", name_ko: "S&P500 ETF", name_en: "SPDR S&P 500", aliases: &["sp500", "spy"] },
    AssetEntry { code: "SOXL", name_ko: "반도체 3배 ETF", name_en: "Direxion Semi Bull 3X", aliases: &["속슬", "soxl"] },
];

/// 코인 입력값(번호, 티커, 한글/영문 부분 검색)을 자동완성/매칭한다.
pub fn match_crypto_query(input: &str, live_markets: &[UpbitMarketInfo]) -> MatchOutcome {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return MatchOutcome::Exact("KRW-BTC".into(), "비트코인".into());
    }
    if let Ok(idx) = trimmed.parse::<usize>()
        && idx >= 1
        && idx <= CRYPTO_PRESETS.len()
    {
        let e = CRYPTO_PRESETS[idx - 1];
        return MatchOutcome::Exact(e.code.to_string(), e.name_ko.to_string());
    }

    let q = trimmed.to_ascii_lowercase();
    let pool = build_crypto_pool(live_markets);

    for (code, ko, en, aliases) in &pool {
        let ticker = code.strip_prefix("KRW-").unwrap_or(code).to_ascii_lowercase();
        if code.eq_ignore_ascii_case(trimmed)
            || ticker == q
            || ko.eq_ignore_ascii_case(trimmed)
            || en.eq_ignore_ascii_case(trimmed)
            || aliases.iter().any(|a| a.eq_ignore_ascii_case(trimmed))
        {
            return MatchOutcome::Exact(code.clone(), ko.clone());
        }
    }

    let mut prefix_hits = Vec::new();
    let mut sub_hits = Vec::new();
    for (code, ko, en, aliases) in &pool {
        let ticker = code.strip_prefix("KRW-").unwrap_or(code).to_ascii_lowercase();
        let ko_l = ko.to_ascii_lowercase();
        let en_l = en.to_ascii_lowercase();
        let code_l = code.to_ascii_lowercase();
        let is_prefix = ticker.starts_with(&q)
            || code_l.starts_with(&q)
            || ko_l.starts_with(&q)
            || en_l.starts_with(&q)
            || aliases.iter().any(|a| a.to_ascii_lowercase().starts_with(&q));
        let is_sub = ticker.contains(&q)
            || code_l.contains(&q)
            || ko_l.contains(&q)
            || en_l.contains(&q)
            || aliases.iter().any(|a| a.to_ascii_lowercase().contains(&q));
        if is_prefix {
            prefix_hits.push((code.clone(), ko.clone()));
        } else if is_sub {
            sub_hits.push((code.clone(), ko.clone()));
        }
    }

    prefix_hits.extend(sub_hits);
    if prefix_hits.len() == 1 {
        let (code, label) = prefix_hits.remove(0);
        return MatchOutcome::Exact(code, label);
    }
    if prefix_hits.len() > 1 {
        prefix_hits.truncate(15);
        return MatchOutcome::Candidates(prefix_hits);
    }

    let fallback = if !trimmed.contains('-') && trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
        format!("KRW-{}", trimmed.to_ascii_uppercase())
    } else {
        trimmed.to_ascii_uppercase()
    };
    MatchOutcome::Exact(fallback, "직접 입력".to_string())
}

/// 주식 입력값(번호, 6자리 코드, 티커, 한글/영문 부분 검색)을 자동완성/매칭한다.
pub fn match_stock_query(input: &str) -> MatchOutcome {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return MatchOutcome::Exact("005930.KS".into(), "삼성전자".into());
    }
    if let Ok(idx) = trimmed.parse::<usize>()
        && idx >= 1
        && idx <= STOCK_PRESETS.len()
    {
        let e = STOCK_PRESETS[idx - 1];
        return MatchOutcome::Exact(e.code.to_string(), e.name_ko.to_string());
    }

    let all_entries = STOCK_PRESETS.iter().chain(STOCK_EXTENDED.iter());
    if trimmed.len() == 6 && trimmed.chars().all(|c| c.is_ascii_digit()) {
        for e in all_entries {
            if e.code.starts_with(trimmed) {
                return MatchOutcome::Exact(e.code.to_string(), e.name_ko.to_string());
            }
        }
        return MatchOutcome::Exact(format!("{trimmed}.KS"), "국내종목".to_string());
    }

    let q = trimmed.to_ascii_lowercase();
    for e in STOCK_PRESETS.iter().chain(STOCK_EXTENDED.iter()) {
        let stem = e.code.split('.').next().unwrap_or(e.code);
        if e.code.eq_ignore_ascii_case(trimmed)
            || stem.eq_ignore_ascii_case(trimmed)
            || e.name_ko.eq_ignore_ascii_case(trimmed)
            || e.name_en.eq_ignore_ascii_case(trimmed)
            || e.aliases.iter().any(|a| a.eq_ignore_ascii_case(trimmed))
        {
            return MatchOutcome::Exact(e.code.to_string(), e.name_ko.to_string());
        }
    }

    let mut prefix_hits = Vec::new();
    let mut sub_hits = Vec::new();
    for e in STOCK_PRESETS.iter().chain(STOCK_EXTENDED.iter()) {
        let code_l = e.code.to_ascii_lowercase();
        let ko_l = e.name_ko.to_ascii_lowercase();
        let en_l = e.name_en.to_ascii_lowercase();
        let is_prefix = code_l.starts_with(&q)
            || ko_l.starts_with(&q)
            || en_l.starts_with(&q)
            || e.aliases.iter().any(|a| a.to_ascii_lowercase().starts_with(&q));
        let is_sub = code_l.contains(&q)
            || ko_l.contains(&q)
            || en_l.contains(&q)
            || e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(&q));
        if is_prefix {
            prefix_hits.push((e.code.to_string(), e.name_ko.to_string()));
        } else if is_sub {
            sub_hits.push((e.code.to_string(), e.name_ko.to_string()));
        }
    }

    prefix_hits.extend(sub_hits);
    if prefix_hits.len() == 1 {
        let (code, label) = prefix_hits.remove(0);
        return MatchOutcome::Exact(code, label);
    }
    if prefix_hits.len() > 1 {
        prefix_hits.truncate(15);
        return MatchOutcome::Candidates(prefix_hits);
    }

    MatchOutcome::Exact(trimmed.to_ascii_uppercase(), "직접 입력".to_string())
}

/// 단일 CLI 플래그용 코인 마켓 코드 정규화기.
pub fn resolve_crypto_market(input: &str) -> String {
    match match_crypto_query(input, &[]) {
        MatchOutcome::Exact(code, _) => code,
        MatchOutcome::Candidates(list) => list
            .into_iter()
            .next()
            .map(|(code, _)| code)
            .unwrap_or_else(|| input.trim().to_ascii_uppercase()),
    }
}

/// 단일 CLI 플래그용 주식 심볼 정규화기.
pub fn resolve_stock_symbol(input: &str) -> String {
    match match_stock_query(input) {
        MatchOutcome::Exact(code, _) => code,
        MatchOutcome::Candidates(list) => list
            .into_iter()
            .next()
            .map(|(code, _)| code)
            .unwrap_or_else(|| input.trim().to_ascii_uppercase()),
    }
}

/// 코인 다중 입력(`1,2,4`, `1-5`, `all`, `btc,솔라나`, `-`)을 마켓 리스트로 변환한다.
pub fn resolve_crypto_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    if trimmed.eq_ignore_ascii_case("all")
        || trimmed == "*"
        || trimmed.eq_ignore_ascii_case("a")
    {
        return CRYPTO_PRESETS.iter().map(|e| e.code.to_string()).collect();
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tok in trimmed.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(indices) = expand_range_token(tok, CRYPTO_PRESETS.len()) {
            for idx in indices {
                let code = CRYPTO_PRESETS[idx - 1].code.to_string();
                if seen.insert(code.clone()) {
                    out.push(code);
                }
            }
        } else {
            let code = resolve_crypto_market(tok);
            if seen.insert(code.clone()) {
                out.push(code);
            }
        }
    }
    out
}

/// 주식 다중 입력(`1,7`, `1-6`, `all`, `kr`, `us`, `삼성전자,엔비디아`, `-`)을 심볼 리스트로 변환한다.
pub fn resolve_stock_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    if trimmed.eq_ignore_ascii_case("all")
        || trimmed == "*"
        || trimmed.eq_ignore_ascii_case("a")
    {
        return STOCK_PRESETS.iter().map(|e| e.code.to_string()).collect();
    }
    if trimmed.eq_ignore_ascii_case("kr") {
        return STOCK_PRESETS
            .iter()
            .filter(|e| e.code.ends_with(".KS") || e.code.ends_with(".KQ"))
            .map(|e| e.code.to_string())
            .collect();
    }
    if trimmed.eq_ignore_ascii_case("us") {
        return STOCK_PRESETS
            .iter()
            .filter(|e| !e.code.contains('.'))
            .map(|e| e.code.to_string())
            .collect();
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tok in trimmed.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(indices) = expand_range_token(tok, STOCK_PRESETS.len()) {
            for idx in indices {
                let code = STOCK_PRESETS[idx - 1].code.to_string();
                if seen.insert(code.clone()) {
                    out.push(code);
                }
            }
        } else {
            let code = resolve_stock_symbol(tok);
            if seen.insert(code.clone()) {
                out.push(code);
            }
        }
    }
    out
}

/// 번호(`1`~`8`) 또는 Yahoo 기간 문자열을 정규화한다.
pub fn resolve_stock_range(input: &str) -> String {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" => "1mo".to_string(),
        "2" => "3mo".to_string(),
        "3" => "6mo".to_string(),
        "4" => "1y".to_string(),
        "5" => "2y".to_string(),
        "6" => "5y".to_string(),
        "7" => "ytd".to_string(),
        "8" => "max".to_string(),
        other => other.to_string(),
    }
}

/// Upbit `/v1/market/all`에서 원화(`KRW-`) 상장 마켓 전체를 가져온다.
pub async fn fetch_upbit_krw_markets(
    client: &reqwest::Client,
) -> anyhow::Result<Vec<UpbitMarketInfo>> {
    let resp = client
        .get("https://api.upbit.com/v1/market/all")
        .query(&[("isDetails", "false")])
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("upbit market/all HTTP {}", resp.status().as_u16());
    }
    let all: Vec<UpbitMarketInfo> = resp.json().await?;
    Ok(all
        .into_iter()
        .filter(|m| m.market.starts_with("KRW-"))
        .collect())
}

/// 주요 코인·주식 프리셋 및 실시간 Upbit 마켓을 검색/표시한다.
pub async fn exec_markets<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    query: Option<&str>,
    live: bool,
    out: &mut W,
) -> anyhow::Result<()> {
    let q_opt = query.map(str::trim).filter(|s| !s.is_empty());

    if q_opt.is_none() && !live {
        theme.banner_to(
            out,
            "QUANT MARKET & SYMBOL CATALOG",
            "번호 · 한글명 · 단축 티커(btc, 005930) 자동완성 지원",
        )?;
        render_crypto_presets(theme, out)?;
        render_stock_presets(theme, out)?;
        writeln!(out)?;
        theme.hint_to(
            out,
            "quant markets <키워드> --live",
            "Upbit 원화 상장 마켓 전체 실시간 검색",
        )?;
        return Ok(());
    }

    let spinner = Spinner::start("Upbit 실시간 마켓 목록을 조회하는 중...");
    let live_res = fetch_upbit_krw_markets(client).await;
    spinner.finish().await;

    let live_markets = match live_res {
        Ok(list) => list,
        Err(err) => {
            theme.warn_to(
                out,
                &format!("Upbit 실시간 마켓 조회 실패 ({err}) — 내장 카탈로그로 검색합니다."),
            )?;
            Vec::new()
        }
    };

    if let Some(q) = q_opt {
        theme.banner_to(out, "MARKET & SYMBOL SEARCH", &format!("Keyword: {q}"))?;
        let crypto_hits = filter_crypto(&live_markets, q);
        let stock_hits = filter_stocks(q);

        theme.section_to(out, &format!("Crypto Matches ({})", crypto_hits.len()))?;
        if crypto_hits.is_empty() {
            writeln!(out, "  (일치하는 코인 없음)")?;
        } else {
            let rows: Vec<Vec<String>> = crypto_hits
                .iter()
                .take(25)
                .enumerate()
                .map(|(i, (code, ko, en))| {
                    vec![(i + 1).to_string(), code.clone(), ko.clone(), en.clone()]
                })
                .collect();
            theme.table_to(out, &["#", "MARKET", "NAME (KO)", "NAME (EN)"], &rows, &[0])?;
        }

        theme.section_to(out, &format!("Stock Matches ({})", stock_hits.len()))?;
        if stock_hits.is_empty() {
            writeln!(out, "  (일치하는 주식 없음)")?;
        } else {
            let rows: Vec<Vec<String>> = stock_hits
                .iter()
                .take(25)
                .enumerate()
                .map(|(i, e)| {
                    vec![
                        (i + 1).to_string(),
                        e.code.to_string(),
                        e.name_ko.to_string(),
                        e.name_en.to_string(),
                    ]
                })
                .collect();
            theme.table_to(out, &["#", "SYMBOL", "NAME (KO)", "NAME (EN)"], &rows, &[0])?;
        }
    } else {
        theme.banner_to(
            out,
            "UPBIT LIVE KRW MARKETS",
            &format!("{} markets available", live_markets.len()),
        )?;
        let rows: Vec<Vec<String>> = live_markets
            .iter()
            .enumerate()
            .map(|(i, m)| {
                vec![
                    (i + 1).to_string(),
                    m.market.clone(),
                    m.korean_name.clone(),
                    m.english_name.clone(),
                ]
            })
            .collect();
        theme.table_to(out, &["#", "MARKET", "NAME (KO)", "NAME (EN)"], &rows, &[0])?;
        writeln!(out)?;
        theme.hint_to(
            out,
            "quant pipeline --all-upbit",
            "조회된 Upbit 원화 마켓 전체 일괄 수집",
        )?;
    }

    Ok(())
}

/// 대화형 위저드용 코인 프리셋 표 출력.
pub fn render_crypto_presets<W: Write>(theme: &Theme, out: &mut W) -> anyhow::Result<()> {
    theme.section_to(
        out,
        &format!(
            "Crypto Presets (1~{} + extended {})",
            CRYPTO_PRESETS.len(),
            CRYPTO_EXTENDED.len()
        ),
    )?;
    let rows: Vec<Vec<String>> = CRYPTO_PRESETS
        .iter()
        .enumerate()
        .map(|(i, e)| {
            vec![
                format!("[{}]", i + 1),
                e.code.to_string(),
                e.name_ko.to_string(),
                e.name_en.to_string(),
            ]
        })
        .collect();
    theme.table_to(out, &["NO", "MARKET", "NAME (KO)", "NAME (EN)"], &rows, &[])?;
    Ok(())
}

/// 대화형 위저드용 주식 프리셋 표 출력.
pub fn render_stock_presets<W: Write>(theme: &Theme, out: &mut W) -> anyhow::Result<()> {
    theme.section_to(
        out,
        &format!(
            "Stock Presets (1~{} + extended {})",
            STOCK_PRESETS.len(),
            STOCK_EXTENDED.len()
        ),
    )?;
    let rows: Vec<Vec<String>> = STOCK_PRESETS
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let market_tag = if e.code.ends_with(".KS") || e.code.ends_with(".KQ") {
                "KR"
            } else {
                "US"
            };
            vec![
                format!("[{}]", i + 1),
                market_tag.to_string(),
                e.code.to_string(),
                e.name_ko.to_string(),
                e.name_en.to_string(),
            ]
        })
        .collect();
    theme.table_to(
        out,
        &["NO", "MKT", "SYMBOL", "NAME (KO)", "NAME (EN)"],
        &rows,
        &[],
    )?;
    Ok(())
}

/// PowerShell / Bash 쉘 탭 자동완성 스크립트를 생성한다.
pub fn completion_script(shell: &str) -> Option<String> {
    let cmds = [
        "fetch",
        "fetch-stock",
        "pipeline",
        "show",
        "status",
        "watch",
        "refresh",
        "open",
        "markets",
        "update",
        "completion",
        "interactive",
        "help",
    ];
    let mut markets_and_symbols: Vec<&str> = CRYPTO_PRESETS.iter().map(|e| e.code).collect();
    markets_and_symbols.extend(STOCK_PRESETS.iter().map(|e| e.code));

    match shell.trim().to_ascii_lowercase().as_str() {
        "powershell" | "ps1" | "pwsh" => {
            let cmd_list = cmds.join("','");
            let sym_list = markets_and_symbols.join("','");
            Some(format!(
                "Register-ArgumentCompleter -Native -CommandName quant -ScriptBlock {{\n\
                 \x20 param($word, $ast, $pos)\n\
                 \x20 $text = $ast.ToString()\n\
                 \x20 $pool = if ($text -match '--market|--symbol|--markets|--symbols') {{ @('{sym_list}') }} else {{ @('{cmd_list}','--all','--all-upbit','--check','--force','--live','--tail','--help') }}\n\
                 \x20 $pool | Where-Object {{ $_ -like \"$word*\" }} | ForEach-Object {{\n\
                 \x20   [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)\n\
                 \x20 }}\n\
                 }}"
            ))
        }
        "bash" => Some(format!(
            "complete -W \"{} {}\" quant",
            cmds.join(" "),
            markets_and_symbols.join(" ")
        )),
        _ => None,
    }
}

fn build_crypto_pool(live_markets: &[UpbitMarketInfo]) -> Vec<(String, String, String, &'static [&'static str])> {
    let mut pool = Vec::new();
    let mut seen = HashSet::new();
    for e in CRYPTO_PRESETS.iter().chain(CRYPTO_EXTENDED.iter()) {
        if seen.insert(e.code.to_string()) {
            pool.push((
                e.code.to_string(),
                e.name_ko.to_string(),
                e.name_en.to_string(),
                e.aliases,
            ));
        }
    }
    for m in live_markets {
        if seen.insert(m.market.clone()) {
            pool.push((
                m.market.clone(),
                m.korean_name.clone(),
                m.english_name.clone(),
                &[],
            ));
        }
    }
    pool
}

fn filter_crypto(live_markets: &[UpbitMarketInfo], query: &str) -> Vec<(String, String, String)> {
    let q = query.to_ascii_lowercase();
    build_crypto_pool(live_markets)
        .into_iter()
        .filter(|(code, ko, en, aliases)| {
            code.to_ascii_lowercase().contains(&q)
                || ko.to_ascii_lowercase().contains(&q)
                || en.to_ascii_lowercase().contains(&q)
                || aliases.iter().any(|a| a.to_ascii_lowercase().contains(&q))
        })
        .map(|(code, ko, en, _)| (code, ko, en))
        .collect()
}

fn filter_stocks(query: &str) -> Vec<&'static AssetEntry> {
    let q = query.to_ascii_lowercase();
    STOCK_PRESETS
        .iter()
        .chain(STOCK_EXTENDED.iter())
        .filter(|e| {
            e.code.to_ascii_lowercase().contains(&q)
                || e.name_ko.to_ascii_lowercase().contains(&q)
                || e.name_en.to_ascii_lowercase().contains(&q)
                || e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(&q))
        })
        .collect()
}

/// 입력값이 코인 마켓인지 주식 심볼인지 휴리스틱으로 판별한다 (관심 종목 추가용).
pub fn is_crypto_like(input: &str) -> bool {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.contains('-') && !trimmed.contains('.') {
        return true;
    }
    if trimmed.contains('.')
        || (trimmed.len() == 6 && trimmed.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    let matches_stock = STOCK_PRESETS
        .iter()
        .chain(STOCK_EXTENDED.iter())
        .any(|e| {
            e.code.eq_ignore_ascii_case(trimmed)
                || e.name_ko.to_ascii_lowercase().contains(&lower)
                || e.name_en.to_ascii_lowercase().contains(&lower)
                || e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(&lower))
        });
    if matches_stock {
        return false;
    }
    CRYPTO_PRESETS
        .iter()
        .chain(CRYPTO_EXTENDED.iter())
        .any(|e| {
            let ticker = e.code.strip_prefix("KRW-").unwrap_or(e.code);
            e.code.eq_ignore_ascii_case(trimmed)
                || ticker.eq_ignore_ascii_case(trimmed)
                || e.name_ko.to_ascii_lowercase().contains(&lower)
                || e.name_en.to_ascii_lowercase().contains(&lower)
                || e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(&lower))
        })
}

/// 프롬프트 실시간 자동완성 제안용 통합 종목 검색 (`(code, label, kind)`).
pub fn suggest_assets(query: &str, max: usize) -> Vec<(String, String, &'static str)> {
    let q = query.trim().to_ascii_lowercase();
    let mut hits = Vec::new();
    for e in CRYPTO_PRESETS.iter().chain(CRYPTO_EXTENDED.iter()) {
        let ticker = e.code.strip_prefix("KRW-").unwrap_or(e.code).to_ascii_lowercase();
        if q.is_empty()
            || e.code.to_ascii_lowercase().contains(&q)
            || ticker.contains(&q)
            || e.name_ko.to_ascii_lowercase().contains(&q)
            || e.name_en.to_ascii_lowercase().contains(&q)
            || e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(&q))
        {
            hits.push((e.code.to_string(), e.name_ko.to_string(), "CRYPTO"));
            if hits.len() >= max {
                return hits;
            }
        }
    }
    for e in STOCK_PRESETS.iter().chain(STOCK_EXTENDED.iter()) {
        if q.is_empty()
            || e.code.to_ascii_lowercase().contains(&q)
            || e.name_ko.to_ascii_lowercase().contains(&q)
            || e.name_en.to_ascii_lowercase().contains(&q)
            || e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(&q))
        {
            hits.push((e.code.to_string(), e.name_ko.to_string(), "STOCK"));
            if hits.len() >= max {
                return hits;
            }
        }
    }
    hits
}

fn expand_range_token(tok: &str, max_len: usize) -> Option<Vec<usize>> {
    let (a, b) = tok.split_once('-')?;
    let start: usize = a.trim().parse().ok()?;
    let end: usize = b.trim().parse().ok()?;
    if start < 1 || end > max_len || start > end {
        return None;
    }
    Some((start..=end).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_crypto_numbers_names_and_tickers() {
        assert_eq!(resolve_crypto_market("1"), "KRW-BTC");
        assert_eq!(resolve_crypto_market("4"), "KRW-SOL");
        assert_eq!(resolve_crypto_market("솔"), "KRW-SOL");
        assert_eq!(resolve_crypto_market("이더"), "KRW-ETH");
        assert_eq!(resolve_crypto_market("btc"), "KRW-BTC");
        assert_eq!(resolve_crypto_market("pepe"), "KRW-PEPE");
    }

    #[test]
    fn stock_autocomplete_supports_exact_and_disambiguation() {
        assert_eq!(resolve_stock_symbol("1"), "005930.KS");
        assert_eq!(resolve_stock_symbol("8"), "NVDA");
        assert_eq!(resolve_stock_symbol("하이"), "000660.KS");
        assert_eq!(resolve_stock_symbol("엔비"), "NVDA");
        assert_eq!(resolve_stock_symbol("005930"), "005930.KS");

        match match_stock_query("삼성") {
            MatchOutcome::Candidates(list) => {
                assert!(list.len() >= 2);
                assert_eq!(list[0].0, "005930.KS");
            }
            other => panic!("expected candidates, got {other:?}"),
        }
    }

    #[test]
    fn list_resolvers_support_all_kr_us_and_ranges() {
        let all_c = resolve_crypto_list("all");
        assert_eq!(all_c.len(), CRYPTO_PRESETS.len());
        assert_eq!(resolve_crypto_list("1-3"), vec!["KRW-BTC", "KRW-ETH", "KRW-XRP"]);
        assert!(resolve_crypto_list("-").is_empty());

        let all_s = resolve_stock_list("all");
        assert_eq!(all_s.len(), STOCK_PRESETS.len());
        let kr = resolve_stock_list("kr");
        assert_eq!(kr.len(), 6);
        let us = resolve_stock_list("us");
        assert_eq!(us.len(), 6);
        assert_eq!(resolve_stock_list("1,8"), vec!["005930.KS", "NVDA"]);
    }

    #[test]
    fn generates_powershell_and_bash_completions() {
        let ps = completion_script("powershell").expect("ps completion");
        assert!(ps.contains("Register-ArgumentCompleter"));
        assert!(ps.contains("KRW-BTC"));
        assert!(ps.contains("005930.KS"));

        let bash = completion_script("bash").expect("bash completion");
        assert!(bash.contains("complete -W"));
        assert!(bash.contains("pipeline"));
        assert!(completion_script("zsh").is_none());
    }
}
