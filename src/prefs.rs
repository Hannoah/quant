use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_HISTORY: usize = 200;

/// 사용자 설정 및 관심 종목(Watchlist).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UserPrefs {
    #[serde(default)]
    pub watchlist_crypto: Vec<String>,
    #[serde(default)]
    pub watchlist_stocks: Vec<String>,
    #[serde(default)]
    pub last_crypto_interval: Option<String>,
    #[serde(default)]
    pub last_crypto_count: Option<u32>,
    #[serde(default)]
    pub last_stock_interval: Option<String>,
    #[serde(default)]
    pub last_stock_range: Option<String>,
    #[serde(default)]
    pub last_out_dir: Option<String>,
}

impl UserPrefs {
    pub fn add_crypto(&mut self, code: &str) -> bool {
        if self.watchlist_crypto.iter().any(|c| c.eq_ignore_ascii_case(code)) {
            return false;
        }
        self.watchlist_crypto.push(code.to_string());
        true
    }

    pub fn add_stock(&mut self, code: &str) -> bool {
        if self.watchlist_stocks.iter().any(|s| s.eq_ignore_ascii_case(code)) {
            return false;
        }
        self.watchlist_stocks.push(code.to_string());
        true
    }

    pub fn remove_target(&mut self, code: &str) -> bool {
        let before = self.watchlist_crypto.len() + self.watchlist_stocks.len();
        self.watchlist_crypto
            .retain(|c| !c.eq_ignore_ascii_case(code));
        self.watchlist_stocks
            .retain(|s| !s.eq_ignore_ascii_case(code));
        let after = self.watchlist_crypto.len() + self.watchlist_stocks.len();
        after < before
    }

    pub fn total_watchlist(&self) -> usize {
        self.watchlist_crypto.len() + self.watchlist_stocks.len()
    }
}

pub fn data_config_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("quant")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("quant")
    } else {
        PathBuf::from(".quant")
    }
}

pub fn prefs_path() -> PathBuf {
    data_config_dir().join("prefs.json")
}

pub fn history_path() -> PathBuf {
    data_config_dir().join("history.txt")
}

pub fn load_prefs() -> UserPrefs {
    if cfg!(test) {
        return UserPrefs::default();
    }
    load_prefs_from(&prefs_path())
}

pub fn save_prefs(prefs: &UserPrefs) {
    if cfg!(test) {
        return;
    }
    let _ = save_prefs_to(&prefs_path(), prefs);
}

pub fn load_prefs_from(path: &Path) -> UserPrefs {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_prefs_to(path: &Path, prefs: &UserPrefs) -> anyhow::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(prefs)?;
    fs::write(path, json)?;
    Ok(())
}

/// 명령어 이력을 최신순으로 불러온다.
pub fn load_history() -> Vec<String> {
    if cfg!(test) {
        return Vec::new();
    }
    load_history_from(&history_path())
}

/// 새 명령어를 최신순 이력 맨 앞에 추가하고 디스크에 저장한다.
pub fn record_history(history: &mut Vec<String>, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if history.first().map(String::as_str) != Some(trimmed) {
        history.retain(|h| h != trimmed);
        history.insert(0, trimmed.to_string());
        history.truncate(MAX_HISTORY);
        if !cfg!(test) {
            let _ = save_history_to(&history_path(), history);
        }
    }
}

pub fn load_history_from(path: &Path) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(MAX_HISTORY)
        .map(ToString::to_string)
        .collect()
}

pub fn save_history_to(path: &Path, history: &[String]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let body = history.iter().take(MAX_HISTORY).cloned().collect::<Vec<_>>().join("\n");
    fs::write(path, body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_roundtrip_and_watchlist_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("prefs.json");

        let mut prefs = UserPrefs::default();
        assert!(prefs.add_crypto("KRW-BTC"));
        assert!(!prefs.add_crypto("krw-btc"));
        assert!(prefs.add_stock("005930.KS"));
        prefs.last_crypto_count = Some(60);
        save_prefs_to(&path, &prefs).expect("save prefs");

        let mut loaded = load_prefs_from(&path);
        assert_eq!(loaded, prefs);
        assert!(loaded.remove_target("KRW-BTC"));
        assert_eq!(loaded.total_watchlist(), 1);
    }

    #[test]
    fn history_roundtrip_deduplicates_and_keeps_newest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("history.txt");

        let hist = vec!["/status".to_string(), "/fetch --market btc".to_string()];
        save_history_to(&path, &hist).expect("save hist");
        let loaded = load_history_from(&path);
        assert_eq!(loaded, hist);
    }
}
