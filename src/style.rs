use std::io::{self, IsTerminal, Write};
use std::time::SystemTime;

use clap::builder::styling::{AnsiColor, Effects, Styles};

/// Clap v4 도움말 색상 팔레트 (`deal-cli` 스타일과 통일).
pub fn clap_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightCyan.on_default() | Effects::BOLD)
        .usage(AnsiColor::BrightCyan.on_default() | Effects::BOLD)
        .literal(AnsiColor::BrightGreen.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Yellow.on_default())
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Red.on_default() | Effects::BOLD)
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Err,
    Info,
    Accent,
    Muted,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub color: bool,
}

impl Theme {
    /// 표준 출력 TTY 및 `NO_COLOR` / `QUANT_NO_COLOR` / `FORCE_COLOR` 환경을 감지한다.
    pub fn auto() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let force_color = std::env::var_os("FORCE_COLOR").is_some();
        let no_color =
            std::env::var_os("NO_COLOR").is_some() || std::env::var_os("QUANT_NO_COLOR").is_some();
        Self {
            color: (is_tty || force_color) && !no_color,
        }
    }

    /// 색상 비활성화 테마 (테스트 및 파이프 출력용).
    #[cfg(test)]
    pub fn plain() -> Self {
        Self { color: false }
    }

    pub fn style(&self, text: &str, tone: Tone, bold: bool) -> String {
        if !self.color {
            return text.to_string();
        }
        let code = match tone {
            Tone::Ok => "32",
            Tone::Warn => "33",
            Tone::Err => "31",
            Tone::Info => "36",
            Tone::Accent => "94",
            Tone::Muted => "90",
        };
        if bold {
            format!("\x1b[1;{code}m{text}\x1b[0m")
        } else {
            format!("\x1b[{code}m{text}\x1b[0m")
        }
    }

    pub fn bold(&self, text: &str) -> String {
        if self.color {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        self.style(text, Tone::Muted, false)
    }

    pub fn highlight_row(&self, text: &str) -> String {
        if self.color {
            format!("\x1b[48;5;237;1;36m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn badge(&self, label: &str, tone: Tone) -> String {
        let bracketed = format!("[{label}]");
        self.style(&bracketed, tone, true)
    }

    pub fn banner_to<W: Write>(&self, out: &mut W, title: &str, subtitle: &str) -> io::Result<()> {
        let inner_w = display_width(title).max(display_width(subtitle)).max(56);
        let box_w = inner_w + 4;
        let top = format!("╭{}╮", "─".repeat(box_w));
        let bot = format!("╰{}╯", "─".repeat(box_w));

        writeln!(out, "{}", self.style(&top, Tone::Info, false))?;
        writeln!(
            out,
            "{}  {}  {}",
            self.style("│", Tone::Info, false),
            pad_cell(&self.style(title, Tone::Info, true), inner_w, false),
            self.style("│", Tone::Info, false)
        )?;
        if !subtitle.is_empty() {
            writeln!(
                out,
                "{}  {}  {}",
                self.style("│", Tone::Info, false),
                pad_cell(&self.style(subtitle, Tone::Muted, false), inner_w, false),
                self.style("│", Tone::Info, false)
            )?;
        }
        writeln!(out, "{}", self.style(&bot, Tone::Info, false))
    }

    pub fn section_to<W: Write>(&self, out: &mut W, title: &str) -> io::Result<()> {
        writeln!(out)?;
        let line = format!("── {title} ");
        let fill = 72usize.saturating_sub(display_width(&line));
        writeln!(
            out,
            "{}{}",
            self.style(&line, Tone::Info, true),
            self.style(&"─".repeat(fill), Tone::Muted, false)
        )
    }

    pub fn kv_to<W: Write>(&self, out: &mut W, key: &str, value: &str) -> io::Result<()> {
        let padded = pad_cell(key, 18, false);
        writeln!(
            out,
            "  {} {}",
            self.style(&padded, Tone::Muted, false),
            value
        )
    }

    pub fn kpi_strip_to<W: Write>(
        &self,
        out: &mut W,
        items: &[(&str, String, Tone)],
    ) -> io::Result<()> {
        let mut parts = Vec::with_capacity(items.len());
        for (label, val, tone) in items {
            let text = format!("{label}: {val}");
            parts.push(self.badge(&text, *tone));
        }
        writeln!(out, "  {}", parts.join("  "))
    }

    pub fn ok_to<W: Write>(&self, out: &mut W, msg: &str) -> io::Result<()> {
        writeln!(out, "  {} {}", self.badge("OK", Tone::Ok), msg)
    }

    pub fn warn_to<W: Write>(&self, out: &mut W, msg: &str) -> io::Result<()> {
        writeln!(out, "  {} {}", self.badge("WARN", Tone::Warn), msg)
    }

    pub fn err_to<W: Write>(&self, out: &mut W, msg: &str) -> io::Result<()> {
        writeln!(out, "  {} {}", self.badge("ERR", Tone::Err), msg)
    }

    pub fn hint_to<W: Write>(&self, out: &mut W, cmd: &str, desc: &str) -> io::Result<()> {
        writeln!(
            out,
            "  {} {}  {}",
            self.style("-> Next:", Tone::Info, true),
            self.bold(cmd),
            self.style(&format!("# {desc}"), Tone::Muted, false)
        )
    }

    /// ANSI 코드와 한글 전각 폭을 인식하여 정렬된 표를 출력한다.
    pub fn table_to<W: Write>(
        &self,
        out: &mut W,
        headers: &[&str],
        rows: &[Vec<String>],
        right_align_cols: &[usize],
    ) -> io::Result<()> {
        if headers.is_empty() {
            return Ok(());
        }
        let ncols = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| display_width(h)).collect();
        for row in rows {
            for (i, cell) in row.iter().take(ncols).enumerate() {
                widths[i] = widths[i].max(display_width(cell));
            }
        }

        let mut hdr_cells = Vec::with_capacity(ncols);
        for (i, h) in headers.iter().enumerate() {
            let padded = pad_cell(h, widths[i], right_align_cols.contains(&i));
            hdr_cells.push(self.style(&padded, Tone::Info, true));
        }
        writeln!(out, "  {}", hdr_cells.join("  "))?;

        let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
        writeln!(out, "  {}", self.style(&sep.join("  "), Tone::Muted, false))?;

        for row in rows {
            let mut line_cells = Vec::with_capacity(ncols);
            for (i, w) in widths.iter().enumerate().take(ncols) {
                let raw = row.get(i).map(String::as_str).unwrap_or("");
                line_cells.push(pad_cell(raw, *w, right_align_cols.contains(&i)));
            }
            writeln!(out, "  {}", line_cells.join("  "))?;
        }
        Ok(())
    }
}

/// 가격 규모(비트코인 억 단위 ~ 저가 코인 0.0001원대)에 맞춘 천 단위 콤마 + 적응형 소수점 포맷.
pub fn format_price(val: f64) -> String {
    if !val.is_finite() {
        return "-".to_string();
    }
    let abs = val.abs();
    if abs == 0.0 {
        return "0".to_string();
    }
    if abs >= 1000.0 {
        if (val - val.round()).abs() < 1e-4 {
            format_commas_int(val.round() as i64)
        } else {
            format_commas_fixed(val, 2)
        }
    } else if abs >= 1.0 {
        format_commas_fixed(val, 2)
    } else if abs >= 0.01 {
        format!("{val:.4}")
    } else {
        format!("{val:.6}")
    }
}

/// 거래량 포맷 (천 단위 콤마, 소수점 코인 거래량은 필요한 만큼만 표시).
pub fn format_volume(val: f64) -> String {
    if !val.is_finite() {
        return "-".to_string();
    }
    let abs = val.abs();
    if abs == 0.0 {
        return "0".to_string();
    }
    if (val - val.round()).abs() < 1e-4 {
        format_commas_int(val.round() as i64)
    } else if abs >= 1.0 {
        format_commas_fixed(val, 2)
    } else {
        format!("{val:.4}")
    }
}

/// 정수를 천 단위 콤마로 포맷한다 (`188700` -> `"188,700"`).
pub fn format_commas_int(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let digits = n.unsigned_abs().to_string();
    format!("{sign}{}", insert_commas(&digits))
}

fn format_commas_fixed(val: f64, decimals: usize) -> String {
    let formatted = format!("{val:.decimals$}");
    if let Some((int_part, frac_part)) = formatted.split_once('.') {
        let (sign, digits) = match int_part.strip_prefix('-') {
            Some(rest) => ("-", rest),
            None => ("", int_part),
        };
        format!("{sign}{}.{frac_part}", insert_commas(digits))
    } else {
        formatted
    }
}

fn insert_commas(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// 시계열 값들을 유니코드 미니 스파크라인(`▁▂▃▄▅▆▇█`)으로 렌더링한다.
pub fn sparkline(values: &[f64], width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() || width == 0 {
        return String::new();
    }
    let sampled: Vec<f64> = if values.len() <= width {
        values.to_vec()
    } else {
        (0..width)
            .map(|i| {
                let idx = i * (values.len() - 1) / (width - 1).max(1);
                values[idx]
            })
            .collect()
    };

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in &sampled {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return String::new();
    }
    let span = max - min;
    sampled
        .into_iter()
        .map(|v| {
            if span <= f64::EPSILON {
                BARS[3]
            } else {
                let norm = ((v - min) / span).clamp(0.0, 1.0);
                let idx = (norm * 7.0).round() as usize;
                BARS[idx.min(7)]
            }
        })
        .collect()
}

/// 진행률 바 (`[████░░░░░░] 40%`).
pub fn render_progress_bar(done: usize, total: usize, width: usize) -> String {
    if total == 0 || width == 0 {
        return String::new();
    }
    let ratio = (done as f64 / total as f64).clamp(0.0, 1.0);
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let pct = (ratio * 100.0).round() as usize;
    format!("[{}{}] {pct:>3}%", "█".repeat(filled), "░".repeat(empty))
}

/// 파일 수정 시각을 사람이 읽기 쉬운 상대 시간으로 포맷한다.
pub fn format_relative_age(modified: SystemTime) -> String {
    let Ok(elapsed) = SystemTime::now().duration_since(modified) else {
        return "방금".to_string();
    };
    let secs = elapsed.as_secs();
    if secs < 60 {
        "방금".to_string()
    } else if secs < 3600 {
        format!("{}분 전", secs / 60)
    } else if secs < 86400 {
        format!("{}시간 전", secs / 3600)
    } else {
        format!("{}일 전", secs / 86400)
    }
}

/// ANSI 이스케이프 시퀀스를 제외하고 한글/CJK 전각 문자를 2칸으로 계산한다.
pub fn display_width(s: &str) -> usize {
    let mut w = 0usize;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for e in chars.by_ref() {
                if e == 'm' {
                    break;
                }
            }
            continue;
        }
        w += if is_zero_width_char(c) {
            0
        } else if is_wide_char(c) {
            2
        } else {
            1
        };
    }
    w
}

/// 화면 표시 폭 `max_cols` 이내로 문자열을 자르고 필요시 `…`를 붙인다.
pub fn truncate_cols(s: &str, max_cols: usize) -> String {
    if display_width(s) <= max_cols {
        return s.to_string();
    }
    if max_cols <= 1 {
        return "…".chars().take(max_cols).collect();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = if is_zero_width_char(ch) {
            0
        } else if is_wide_char(ch) {
            2
        } else {
            1
        };
        if w + cw + 1 > max_cols {
            out.push('…');
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

fn is_zero_width_char(c: char) -> bool {
    matches!(
        c,
        '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{1F3FB}'..='\u{1F3FF}'
    )
}

fn is_wide_char(c: char) -> bool {
    matches!(
        c,
        '\u{1100}'..='\u{115F}'
            | '\u{2E80}'..='\u{A4CF}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FE30}'..='\u{FE4F}'
            | '\u{FF00}'..='\u{FFEF}'
            | '\u{2600}'..='\u{26FF}'
            | '\u{2700}'..='\u{27BF}'
            | '\u{2B00}'..='\u{2BFF}'
            | '\u{1F300}'..='\u{1FAFF}'
    )
}

pub fn pad_cell(s: &str, width: usize, right_align: bool) -> String {
    let w = display_width(s);
    if w >= width {
        return s.to_string();
    }
    let pad = " ".repeat(width - w);
    if right_align {
        format!("{pad}{s}")
    } else {
        format!("{s}{pad}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_ignores_ansi_and_handles_hangul() {
        assert_eq!(display_width("ABC"), 3);
        assert_eq!(display_width("한글"), 4);
        assert_eq!(display_width("\x1b[32m[OK]\x1b[0m 코인"), 9);
    }

    #[test]
    fn table_to_aligns_columns_and_numbers() {
        let theme = Theme::plain();
        let mut out = Vec::new();
        theme
            .table_to(
                &mut out,
                &["ASSET", "BARS"],
                &[vec!["KRW-BTC".into(), "30".into()]],
                &[1],
            )
            .expect("table write must succeed");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("ASSET    BARS"));
        assert!(text.contains("KRW-BTC    30"));
    }

    #[test]
    fn formats_prices_volumes_and_sparklines_cleanly() {
        assert_eq!(format_price(188_700.0), "188,700");
        assert_eq!(format_price(1_234.56), "1,234.56");
        assert_eq!(format_price(227.5), "227.50");
        assert_eq!(format_price(0.0315), "0.0315");
        assert_eq!(format_price(0.000412), "0.000412");
        assert_eq!(format_volume(35_516_270.0), "35,516,270");
        assert_eq!(format_volume(120.5), "120.50");
        assert_eq!(sparkline(&[10.0, 20.0, 30.0], 3), "▁▅█");
        assert_eq!(render_progress_bar(5, 10, 10), "[█████░░░░░]  50%");
    }
}
