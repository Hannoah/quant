use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};

use crate::style::{Theme, Tone};
use crate::tui::Spinner;

const REPO_FALLBACK: &str = env!("CARGO_PKG_REPOSITORY");

/// `quant update` 실행 옵션.
#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub check: bool,
    pub force: bool,
    pub no_pull: bool,
    pub from: Option<String>,
    pub repo: Option<PathBuf>,
    pub branch: String,
}

/// `"v0.1.0"` 또는 `"0.1.0-beta"` 형태의 시맨틱 버전을 `(major, minor, patch)`로 파싱한다.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let clean = s
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()?;
    let mut parts = clean.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor, patch))
}

/// `candidate` 버전이 `current`보다 높으면 `true`를 반환한다.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    matches!(
        (parse_version(current), parse_version(candidate)),
        (Some(c), Some(n)) if n > c
    )
}

/// `Cargo.toml` 본문의 `[package]` 섹션에서 `version = "..."` 값을 추출한다.
pub fn parse_cargo_toml_version(content: &str) -> Option<String> {
    let mut in_package = false;
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((key, val)) = line.split_once('=')
            && key.trim() == "version"
        {
            let trimmed = val.trim().trim_matches(['"', '\'']);
            if parse_version(trimmed).is_some() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// GitHub 저장소 URL을 raw `Cargo.toml` URL로 변환한다.
pub fn raw_cargo_toml_url(repo: &str, branch: &str) -> Option<String> {
    let trimmed = repo.trim().trim_end_matches('/').trim_end_matches(".git");
    let path = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))?;
    let mut segs = path.split('/').filter(|s| !s.is_empty());
    let owner = segs.next()?;
    let name = segs.next()?;
    Some(format!(
        "https://raw.githubusercontent.com/{owner}/{name}/{}/Cargo.toml",
        branch.trim()
    ))
}

/// 이전 업데이트 때 남긴 `quant.exe.old-*` 임시 바이너리를 시작 시 정리한다.
pub fn cleanup_old_binaries() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let (Some(dir), Some(name)) = (exe.parent(), exe.file_name()) else {
        return;
    };
    let prefix = format!("{}.old-", name.to_string_lossy());
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Windows에서 실행 중인 바이너리를 안전하게 교체한다 (`AgentSquad` 방식).
pub fn swap_binary(new_bin: &Path, target_bin: &Path) -> anyhow::Result<()> {
    if let Some(parent) = target_bin.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    if target_bin.exists() {
        let mut old = target_bin.as_os_str().to_owned();
        old.push(format!(".old-{}", std::process::id()));
        let old = PathBuf::from(old);
        fs::rename(target_bin, &old)
            .with_context(|| format!("cannot move {} aside", target_bin.display()))?;
        if let Err(err) = fs::copy(new_bin, target_bin) {
            let _ = fs::remove_file(target_bin);
            let _ = fs::rename(&old, target_bin);
            return Err(err).with_context(|| format!("cannot write {}", target_bin.display()));
        }
        let _ = fs::remove_file(&old);
    } else {
        fs::copy(new_bin, target_bin)
            .with_context(|| format!("cannot copy to {}", target_bin.display()))?;
    }
    Ok(())
}

/// 원격 버전 확인, Git 동기화, 릴리스 빌드 및 바이너리 교체를 수행한다.
pub async fn exec_update<W: Write>(
    client: &reqwest::Client,
    theme: &Theme,
    opts: &UpdateOptions,
    out: &mut W,
) -> anyhow::Result<()> {
    let current_ver = env!("CARGO_PKG_VERSION");
    let install_dir = install_base_dir();
    let _ = fs::create_dir_all(&install_dir);
    let url_cfg = install_dir.join("repo_url.txt");
    let dir_cfg = install_dir.join("repo_dir.txt");

    let git_url = opts
        .from
        .clone()
        .or_else(|| {
            fs::read_to_string(&url_cfg)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| REPO_FALLBACK.to_string());

    theme.banner_to(out, "QUANT CLI SELF-UPDATE", &git_url)?;
    theme.kv_to(out, "Running Version", &format!("v{current_ver}"))?;

    let spinner = Spinner::start("원격 저장소 최신 버전을 확인하는 중...");
    let remote_res = fetch_remote_version(client, &git_url, &opts.branch).await;
    spinner.finish().await;

    let remote_ver = match remote_res {
        Ok(Some(ver)) => {
            theme.kv_to(
                out,
                "Remote Version",
                &format!("v{ver} ({})", opts.branch),
            )?;
            Some(ver)
        }
        Ok(None) => {
            theme.kv_to(out, "Remote Version", "unavailable (no package version)")?;
            None
        }
        Err(err) => {
            theme.warn_to(out, &format!("원격 버전 확인 실패: {err}")).ok();
            None
        }
    };

    let mut repo_root = resolve_repo_root(opts.repo.as_deref(), &dir_cfg, &install_dir);
    if opts.from.is_some() && !repo_root.join(".git").is_dir() {
        repo_root = install_dir.join("repo");
    }

    if let Some(ref url) = opts.from {
        let _ = fs::write(&url_cfg, url);
    }

    let mut workspace_ver = read_workspace_version(&repo_root);
    if let Some(ref ver) = workspace_ver {
        theme.kv_to(
            out,
            "Workspace",
            &format!("{} (v{ver})", repo_root.display()),
        )?;
    } else {
        theme.kv_to(out, "Workspace", "(local source not found)")?;
    }

    if opts.check {
        return report_check_only(
            theme,
            current_ver,
            remote_ver.as_deref(),
            workspace_ver.as_deref(),
            out,
        );
    }

    if workspace_ver.is_none() {
        theme.section_to(out, "Step 1/3 · Git Clone")?;
        let clone_dir = install_dir.join("repo");
        clone_repo(&git_url, &opts.branch, &clone_dir)?;
        repo_root = clone_dir;
        workspace_ver = read_workspace_version(&repo_root);
        theme.ok_to(out, &format!("Cloned {git_url} -> {}", repo_root.display()))?;
    }

    if !repo_root.join("Cargo.toml").is_file() {
        bail!(
            "Cargo.toml not found at {}. Pass `--repo <path>` or `--from <git-url>`.",
            repo_root.display()
        );
    }
    let _ = fs::write(&dir_cfg, repo_root.display().to_string());

    let head_before = git_head(&repo_root);
    if !opts.no_pull && has_git_origin(&repo_root) {
        theme.section_to(out, "Step 1/3 · Git Pull (--ff-only)")?;
        match run_git_pull(&repo_root) {
            Ok(()) => {
                theme.ok_to(out, "원격 저장소와 동기화했습니다.")?;
                workspace_ver = read_workspace_version(&repo_root);
            }
            Err(err) => {
                theme.warn_to(out, &format!("git pull 건너뜀 ({err}) — 로컬 소스로 계속합니다."))?;
            }
        }
    }
    let head_after = git_head(&repo_root);
    let pulled_new_commits = head_before.is_some() && head_after.is_some() && head_before != head_after;

    let remote_has_newer = remote_ver
        .as_deref()
        .is_some_and(|rv| is_newer(current_ver, rv));
    let workspace_has_newer = workspace_ver
        .as_deref()
        .is_some_and(|wv| is_newer(current_ver, wv));

    if !opts.force && !remote_has_newer && !workspace_has_newer && !pulled_new_commits {
        writeln!(out)?;
        theme.ok_to(out, &format!("quant v{current_ver} 은(는) 이미 최신 버전입니다."))?;
        theme.hint_to(
            out,
            "quant update --force",
            "버전이 같아도 현재 소스로 다시 빌드 및 교체",
        )?;
        return Ok(());
    }

    if remote_has_newer && !workspace_has_newer {
        theme.warn_to(
            out,
            &format!(
                "원격 버전(v{})이 로컬 워크스페이스(v{})보다 높습니다. 워크스페이스 소스를 먼저 확인하세요.",
                remote_ver.as_deref().unwrap_or("-"),
                workspace_ver.as_deref().unwrap_or("-")
            ),
        )?;
    }

    theme.section_to(out, "Step 2/3 · Building Release Binary")?;
    let build_status = Command::new("cargo")
        .current_dir(&repo_root)
        .args(["build", "--release"])
        .status()
        .context("failed to spawn `cargo build --release`")?;
    if !build_status.success() {
        bail!("cargo build --release failed (exit {:?})", build_status.code());
    }

    let exe_name = if cfg!(windows) { "quant.exe" } else { "quant" };
    let built_bin = repo_root.join("target").join("release").join(exe_name);
    if !built_bin.is_file() {
        bail!("built binary not found at {}", built_bin.display());
    }
    let built_ver = Command::new(&built_bin)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| format!("quant {}", workspace_ver.as_deref().unwrap_or(current_ver)));
    theme.ok_to(out, &format!("Release build ready: {built_ver}"))?;

    theme.section_to(out, "Step 3/3 · Installing Binary")?;
    let mut targets = Vec::new();
    if let Some(cargo_bin) = cargo_bin_exe(exe_name) {
        targets.push(cargo_bin);
    }
    if !cfg!(debug_assertions)
        && let Ok(cur_exe) = std::env::current_exe()
    {
        let already = targets
            .iter()
            .any(|t| same_path(t, &cur_exe));
        if !already {
            targets.push(cur_exe);
        }
    }
    if targets.is_empty() {
        bail!("could not determine target binary path (~/.cargo/bin/{exe_name})");
    }

    for target in &targets {
        swap_binary(&built_bin, target)?;
        theme.ok_to(out, &format!("Installed -> {}", target.display()))?;
    }

    writeln!(out)?;
    theme.kpi_strip_to(
        out,
        &[
            ("FROM", format!("v{current_ver}"), Tone::Muted),
            ("INSTALLED", built_ver, Tone::Ok),
        ],
    )?;
    theme.hint_to(
        out,
        "quant --version",
        "새 버전을 적용하려면 quant를 다시 실행하세요",
    )?;
    Ok(())
}

fn report_check_only<W: Write>(
    theme: &Theme,
    current_ver: &str,
    remote_ver: Option<&str>,
    workspace_ver: Option<&str>,
    out: &mut W,
) -> anyhow::Result<()> {
    writeln!(out)?;
    if let Some(rv) = remote_ver
        && is_newer(current_ver, rv)
    {
        theme.warn_to(
            out,
            &format!("새 원격 버전 v{rv} 이(가) 있습니다 (현재: v{current_ver})."),
        )?;
        theme.hint_to(out, "quant update", "최신 버전 다운로드 및 설치")?;
        return Ok(());
    }
    if let Some(wv) = workspace_ver
        && is_newer(current_ver, wv)
    {
        theme.warn_to(
            out,
            &format!("로컬 워크스페이스에 새 버전 v{wv} 이(가) 있습니다 (현재: v{current_ver})."),
        )?;
        theme.hint_to(out, "quant update --no-pull", "로컬 워크스페이스 기준으로 업데이트")?;
        return Ok(());
    }
    theme.ok_to(out, &format!("현재 최신 버전(v{current_ver})을 사용 중입니다."))?;
    Ok(())
}

async fn fetch_remote_version(
    client: &reqwest::Client,
    repo: &str,
    branch: &str,
) -> anyhow::Result<Option<String>> {
    let Some(url) = raw_cargo_toml_url(repo, branch) else {
        return Ok(None);
    };
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("HTTP {}", resp.status().as_u16());
    }
    let body = resp.text().await?;
    Ok(parse_cargo_toml_version(&body))
}

fn install_base_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("quant")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("quant")
    } else {
        PathBuf::from(".").join(".quant")
    }
}

fn resolve_repo_root(
    explicit: Option<&Path>,
    dir_cfg: &Path,
    install_dir: &Path,
) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Ok(env_repo) = std::env::var("QUANT_REPO") {
        let p = PathBuf::from(env_repo);
        if p.join("Cargo.toml").is_file() {
            return p;
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest_dir.join("Cargo.toml").is_file() {
        return manifest_dir;
    }
    if let Ok(saved) = fs::read_to_string(dir_cfg) {
        let p = PathBuf::from(saved.trim());
        if p.join("Cargo.toml").is_file() {
            return p;
        }
    }
    install_dir.join("repo")
}

fn read_workspace_version(repo_root: &Path) -> Option<String> {
    let content = fs::read_to_string(repo_root.join("Cargo.toml")).ok()?;
    parse_cargo_toml_version(&content)
}

fn cargo_bin_exe(exe_name: &str) -> Option<PathBuf> {
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        return Some(PathBuf::from(cargo_home).join("bin").join(exe_name));
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)?;
    Some(home.join(".cargo").join("bin").join(exe_name))
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

fn has_git_origin(repo_root: &Path) -> bool {
    repo_root.join(".git").is_dir()
        && Command::new("git")
            .args([
                "-C",
                &repo_root.display().to_string(),
                "remote",
                "get-url",
                "origin",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn git_head(repo_root: &Path) -> Option<String> {
    if !repo_root.join(".git").is_dir() {
        return None;
    }
    Command::new("git")
        .args([
            "-C",
            &repo_root.display().to_string(),
            "rev-parse",
            "HEAD",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn run_git_pull(repo_root: &Path) -> anyhow::Result<()> {
    let out = Command::new("git")
        .args([
            "-C",
            &repo_root.display().to_string(),
            "pull",
            "--ff-only",
        ])
        .output()
        .context("failed to run git pull")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("{}", stderr.trim());
    }
    Ok(())
}

fn clone_repo(url: &str, branch: &str, dest: &Path) -> anyhow::Result<()> {
    if dest.exists() {
        let _ = fs::remove_dir_all(dest);
    }
    let status = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            branch,
            url,
            &dest.display().to_string(),
        ])
        .status()
        .context("failed to spawn git clone")?;
    if !status.success() {
        bail!("git clone failed from {url}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_and_compare() {
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_version("bad"), None);
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.9", "v0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.1.9"));
    }

    #[test]
    fn parses_package_version_from_cargo_toml() {
        let toml = r#"
[package]
name = "quant"
version = "0.2.0"
edition = "2024"

[dependencies]
tokio = { version = "1" }
"#;
        assert_eq!(parse_cargo_toml_version(toml).as_deref(), Some("0.2.0"));
    }

    #[test]
    fn builds_raw_github_url() {
        assert_eq!(
            raw_cargo_toml_url("https://github.com/Hannoah/quant.git", "main").as_deref(),
            Some("https://raw.githubusercontent.com/Hannoah/quant/main/Cargo.toml")
        );
        assert_eq!(
            raw_cargo_toml_url("https://github.com/Hannoah/quant/", "dev").as_deref(),
            Some("https://raw.githubusercontent.com/Hannoah/quant/dev/Cargo.toml")
        );
    }

    #[test]
    fn swap_binary_replaces_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let new_bin = dir.path().join("new.bin");
        let target_bin = dir.path().join("quant.bin");
        fs::write(&target_bin, b"old-binary").expect("write old");
        fs::write(&new_bin, b"new-binary").expect("write new");

        swap_binary(&new_bin, &target_bin).expect("swap must succeed");
        let got = fs::read(&target_bin).expect("read target");
        assert_eq!(got, b"new-binary");
    }
}
