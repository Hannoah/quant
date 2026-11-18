use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveDown, MoveToColumn, MoveUp, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};

use crate::style::{Theme, Tone, display_width, pad_cell, truncate_cols};

const MAX_VISIBLE_ITEMS: usize = 10;

/// 선택 메뉴 항목.
#[derive(Debug, Clone)]
pub struct SelectItem {
    pub label: String,
    pub hint: String,
}

/// 자동완성 제안 항목.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub label: String,
    pub hint: String,
}

/// 대화형 프롬프트 입력 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptInput {
    Line(String),
    Eof,
    Interrupted,
}

/// 표준 입출력이 모두 대화형 터미널인지 확인한다.
pub fn is_interactive_tty() -> bool {
    !cfg!(test)
        && std::env::var_os("QUANT_PLAIN_PROMPT").is_none()
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, Show);
    }
}

/// 터미널의 사용 가능한 가로 열 폭을 반환한다 (마지막 열 줄바꿈 방지용 -1).
fn term_cols() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| usize::from(w).saturating_sub(1))
        .unwrap_or(96)
        .max(40)
}

/// 커서 이동이 가능한 순수 UTF-8 한 줄 입력 버퍼.
#[derive(Debug, Default, Clone)]
pub struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
}

impl LineEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn as_str(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn set(&mut self, text: &str) {
        self.chars = text.chars().collect();
        self.cursor = self.chars.len();
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, ch: char) {
        let pos = self.cursor.min(self.chars.len());
        self.chars.insert(pos, ch);
        self.cursor = pos + 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 && !self.chars.is_empty() {
            self.cursor -= 1;
            self.chars.remove(self.cursor.min(self.chars.len()));
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.chars.len();
    }

    pub fn delete_word_back(&mut self) {
        let end = self.cursor.min(self.chars.len());
        let mut i = end;
        while i > 0 && self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        self.chars.drain(i..end);
        self.cursor = i;
    }

    pub fn before_cursor(&self) -> String {
        self.chars[..self.cursor.min(self.chars.len())]
            .iter()
            .collect()
    }
}

/// 단일 항목 화살표 + 실시간 검색 필터 선택 UI (`Esc` 취소 시 `Ok(None)`).
/// 비TTY 환경(파이프/테스트)에서는 번호 목록과 `fallback_reader`로 동작한다.
pub fn select_one<R: BufRead, W: Write>(
    theme: &Theme,
    title: &str,
    items: &[SelectItem],
    default_idx: usize,
    fallback_reader: &mut R,
    out: &mut W,
) -> io::Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }
    if !is_interactive_tty() {
        return select_one_fallback(theme, title, items, default_idx, fallback_reader, out);
    }

    let _guard = RawModeGuard::enter()?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;

    let mut query = String::new();
    let mut idx = default_idx.min(items.len() - 1);
    let mut drawn: u16 = 0;

    let result = loop {
        let visible = filter_indices(items, &query);
        if visible.is_empty() {
            idx = 0;
        } else {
            idx = idx.min(visible.len() - 1);
        }

        drawn = draw_select_panel(
            theme,
            title,
            items,
            &visible,
            None,
            idx,
            &query,
            drawn,
            &mut stdout,
        )?;

        let Event::Key(k) = event::read()? else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);

        match k.code {
            KeyCode::Esc => break None,
            KeyCode::Char('c') if ctrl => break None,
            KeyCode::Enter => {
                if let Some(&orig) = visible.get(idx) {
                    break Some(orig);
                }
            }
            KeyCode::Up => {
                if !visible.is_empty() {
                    idx = if idx == 0 { visible.len() - 1 } else { idx - 1 };
                }
            }
            KeyCode::Down => {
                if !visible.is_empty() {
                    idx = if idx + 1 >= visible.len() { 0 } else { idx + 1 };
                }
            }
            KeyCode::PageUp => idx = idx.saturating_sub(MAX_VISIBLE_ITEMS),
            KeyCode::PageDown => {
                if !visible.is_empty() {
                    idx = (idx + MAX_VISIBLE_ITEMS).min(visible.len() - 1);
                }
            }
            KeyCode::Home => idx = 0,
            KeyCode::End => {
                if !visible.is_empty() {
                    idx = visible.len() - 1;
                }
            }
            KeyCode::Backspace => {
                query.pop();
                idx = 0;
            }
            KeyCode::Char(ch) if !ctrl && !alt => {
                query.push(ch);
                idx = 0;
            }
            _ => {}
        }
    };

    clear_drawn_panel(drawn, &mut stdout)?;
    Ok(result)
}

/// 다중 항목 체크박스 화살표 + 실시간 필터 선택 UI (`Space` 토글, `Ctrl+A` 전체).
/// 비TTY 환경(파이프/테스트)에서는 쉼표 구분 번호/all 입력으로 동작한다.
pub fn select_many<R: BufRead, W: Write>(
    theme: &Theme,
    title: &str,
    items: &[SelectItem],
    initial_checked: &[bool],
    fallback_reader: &mut R,
    out: &mut W,
) -> io::Result<Option<Vec<usize>>> {
    if items.is_empty() {
        return Ok(Some(Vec::new()));
    }
    if !is_interactive_tty() {
        return select_many_fallback(theme, title, items, initial_checked, fallback_reader, out);
    }

    let _guard = RawModeGuard::enter()?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;

    let mut checked: Vec<bool> = (0..items.len())
        .map(|i| initial_checked.get(i).copied().unwrap_or(false))
        .collect();
    let mut query = String::new();
    let mut idx = 0usize;
    let mut drawn: u16 = 0;

    let result = loop {
        let visible = filter_indices(items, &query);
        if visible.is_empty() {
            idx = 0;
        } else {
            idx = idx.min(visible.len() - 1);
        }

        drawn = draw_select_panel(
            theme,
            title,
            items,
            &visible,
            Some(&checked),
            idx,
            &query,
            drawn,
            &mut stdout,
        )?;

        let Event::Key(k) = event::read()? else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);

        match k.code {
            KeyCode::Esc => break None,
            KeyCode::Char('c') if ctrl => break None,
            KeyCode::Enter => {
                let picked: Vec<usize> = checked
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &on)| on.then_some(i))
                    .collect();
                break Some(picked);
            }
            KeyCode::Char(' ') => {
                if let Some(&orig) = visible.get(idx) {
                    checked[orig] = !checked[orig];
                }
            }
            KeyCode::Tab => {
                toggle_all_visible(&mut checked, &visible);
            }
            KeyCode::Char('a') if ctrl => {
                toggle_all_visible(&mut checked, &visible);
            }
            KeyCode::Up => {
                if !visible.is_empty() {
                    idx = if idx == 0 { visible.len() - 1 } else { idx - 1 };
                }
            }
            KeyCode::Down => {
                if !visible.is_empty() {
                    idx = if idx + 1 >= visible.len() { 0 } else { idx + 1 };
                }
            }
            KeyCode::PageUp => idx = idx.saturating_sub(MAX_VISIBLE_ITEMS),
            KeyCode::PageDown => {
                if !visible.is_empty() {
                    idx = (idx + MAX_VISIBLE_ITEMS).min(visible.len() - 1);
                }
            }
            KeyCode::Home => idx = 0,
            KeyCode::End => {
                if !visible.is_empty() {
                    idx = visible.len() - 1;
                }
            }
            KeyCode::Backspace => {
                query.pop();
                idx = 0;
            }
            KeyCode::Char(ch) if !ctrl && !alt => {
                query.push(ch);
                idx = 0;
            }
            _ => {}
        }
    };

    clear_drawn_panel(drawn, &mut stdout)?;
    Ok(result)
}

/// 실시간 자동완성(`Tab`), 히스토리(`↑`/`↓`), 커서 아래 실시간 제안을 지원하는 REPL 프롬프트.
pub fn ask_command<R: BufRead, W: Write, F, C>(
    theme: &Theme,
    prompt_plain: &str,
    history: &[String],
    suggest_fn: F,
    complete_fn: C,
    fallback_reader: &mut R,
    out: &mut W,
) -> io::Result<PromptInput>
where
    F: Fn(&str) -> Vec<Suggestion>,
    C: Fn(&str) -> Option<String>,
{
    if !is_interactive_tty() {
        write!(out, "{} ", theme.style(prompt_plain, Tone::Info, true))?;
        out.flush()?;
        let mut line = String::new();
        let bytes = fallback_reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(PromptInput::Eof);
        }
        let cleaned = line.trim().trim_start_matches('\u{feff}').trim().to_string();
        return Ok(PromptInput::Line(cleaned));
    }

    let _guard = RawModeGuard::enter()?;
    let mut stdout = io::stdout();

    let mut editor = LineEditor::new();
    let mut draft = String::new();
    let mut hist_idx: Option<usize> = None;
    let mut armed_ctrl_c = false;
    let mut drawn_below: u16 = 0;
    let mut notice: Option<String> = None;

    loop {
        drawn_below = draw_prompt_with_suggestions(
            theme,
            prompt_plain,
            &editor,
            &suggest_fn,
            notice.as_deref(),
            drawn_below,
            &mut stdout,
        )?;
        notice = None;

        let Event::Key(k) = event::read()? else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);

        if !(ctrl && k.code == KeyCode::Char('c')) {
            armed_ctrl_c = false;
        }

        match k.code {
            KeyCode::Enter => {
                clear_prompt_suggestions(drawn_below, &mut stdout)?;
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                return Ok(PromptInput::Line(editor.as_str().trim().to_string()));
            }
            KeyCode::Char('d') if ctrl && editor.is_empty() => {
                clear_prompt_suggestions(drawn_below, &mut stdout)?;
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                return Ok(PromptInput::Eof);
            }
            KeyCode::Char('c') if ctrl => {
                if !editor.is_empty() {
                    editor.clear();
                    hist_idx = None;
                } else if !armed_ctrl_c {
                    armed_ctrl_c = true;
                    notice = Some("(Ctrl+C 한 번 더 누르거나 /exit 입력 시 종료)".to_string());
                } else {
                    clear_prompt_suggestions(drawn_below, &mut stdout)?;
                    write!(stdout, "\r\n")?;
                    stdout.flush()?;
                    return Ok(PromptInput::Interrupted);
                }
            }
            KeyCode::Char('l') if ctrl => {
                clear_prompt_suggestions(drawn_below, &mut stdout)?;
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                return Ok(PromptInput::Line("/clear".to_string()));
            }
            KeyCode::Tab => {
                if let Some(completed) = complete_fn(&editor.as_str()) {
                    editor.set(&completed);
                    hist_idx = None;
                }
            }
            KeyCode::Up => {
                if !history.is_empty() {
                    let next_idx = match hist_idx {
                        None => {
                            draft = editor.as_str();
                            0
                        }
                        Some(i) => (i + 1).min(history.len() - 1),
                    };
                    hist_idx = Some(next_idx);
                    editor.set(&history[next_idx]);
                }
            }
            KeyCode::Down => {
                if let Some(i) = hist_idx {
                    if i == 0 {
                        hist_idx = None;
                        editor.set(&draft);
                    } else {
                        let next_idx = i - 1;
                        hist_idx = Some(next_idx);
                        editor.set(&history[next_idx]);
                    }
                }
            }
            KeyCode::Left => editor.move_left(),
            KeyCode::Right => editor.move_right(),
            KeyCode::Home => editor.move_home(),
            KeyCode::End => editor.move_end(),
            KeyCode::Char('a') if ctrl => editor.move_home(),
            KeyCode::Char('e') if ctrl => editor.move_end(),
            KeyCode::Char('u') if ctrl => {
                editor.clear();
                hist_idx = None;
            }
            KeyCode::Char('w') if ctrl => {
                editor.delete_word_back();
                hist_idx = None;
            }
            KeyCode::Backspace => {
                editor.backspace();
                hist_idx = None;
            }
            KeyCode::Delete => {
                editor.delete();
                hist_idx = None;
            }
            KeyCode::Esc => {
                editor.clear();
                hist_idx = None;
            }
            KeyCode::Char(ch) if !ctrl && !alt => {
                editor.insert(ch);
                hist_idx = None;
            }
            _ => {}
        }
    }
}

/// API 요청 중 터미널이 멈춘 것처럼 보이지 않도록 표시하는 점자 스피너.
pub struct Spinner {
    stopped: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
    active: bool,
}

impl Spinner {
    pub fn start(label: &str) -> Self {
        let active = std::io::stderr().is_terminal()
            && std::env::var_os("QUANT_PLAIN_PROMPT").is_none();
        let stopped = Arc::new(AtomicBool::new(false));
        if !active {
            return Self {
                stopped,
                handle: None,
                active: false,
            };
        }

        let flag = Arc::clone(&stopped);
        let msg = label.to_string();
        let handle = tokio::spawn(async move {
            const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let started = Instant::now();
            tokio::time::sleep(Duration::from_millis(150)).await;
            let mut idx = 0usize;
            while !flag.load(Ordering::Relaxed) {
                let secs = started.elapsed().as_secs_f32();
                let frame = FRAMES[idx % FRAMES.len()];
                idx = idx.wrapping_add(1);
                let mut err = io::stderr();
                let _ = write!(err, "\r\x1b[2K\x1b[36m{frame}\x1b[0m {msg} \x1b[90m{secs:.1}s\x1b[0m");
                let _ = err.flush();
                tokio::time::sleep(Duration::from_millis(90)).await;
            }
        });

        Self {
            stopped,
            handle: Some(handle),
            active: true,
        }
    }

    pub async fn finish(mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
        if self.active {
            let mut err = io::stderr();
            let _ = write!(err, "\r\x1b[2K");
            let _ = err.flush();
            self.active = false;
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if self.active {
            let mut err = io::stderr();
            let _ = write!(err, "\r\x1b[2K");
            let _ = err.flush();
        }
    }
}

fn filter_indices(items: &[SelectItem], query: &str) -> Vec<usize> {
    let q = query.trim().to_ascii_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            q.is_empty()
                || item.label.to_ascii_lowercase().contains(&q)
                || item.hint.to_ascii_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

fn toggle_all_visible(checked: &mut [bool], visible: &[usize]) {
    if visible.is_empty() {
        return;
    }
    let all_on = visible.iter().all(|&i| checked[i]);
    for &i in visible {
        checked[i] = !all_on;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_select_panel(
    theme: &Theme,
    title: &str,
    items: &[SelectItem],
    visible: &[usize],
    checked: Option<&[bool]>,
    idx: usize,
    query: &str,
    prev_drawn: u16,
    stdout: &mut io::Stdout,
) -> io::Result<u16> {
    if prev_drawn > 0 {
        execute!(stdout, MoveUp(prev_drawn), MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
    } else {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
    }

    let cols = term_cols();
    let start = idx
        .saturating_sub(4)
        .min(visible.len().saturating_sub(MAX_VISIBLE_ITEMS));
    let shown = &visible[start..visible.len().min(start + MAX_VISIBLE_ITEMS)];
    let label_w = shown
        .iter()
        .map(|&orig| display_width(&items[orig].label))
        .max()
        .unwrap_or(10)
        .min(34);

    let mut lines = Vec::new();
    let filter_note = if query.is_empty() {
        "입력으로 필터".to_string()
    } else {
        format!("검색: {query}▏")
    };
    let checked_note = match checked {
        Some(c) => format!(" · {}개 선택", c.iter().filter(|&&b| b).count()),
        None => String::new(),
    };
    let header_raw = truncate_cols(&format!("{title}  ·  {filter_note}{checked_note}"), cols.saturating_sub(4));
    lines.push(format!(
        "{}{}",
        theme.style("╭─ ", Tone::Info, false),
        theme.bold(&header_raw)
    ));

    let bar = theme.style("│ ", Tone::Info, false);
    if start > 0 {
        lines.push(format!("{bar}{}", theme.dim(&format!("↑ {}개 더 있음", start))));
    }

    if shown.is_empty() {
        lines.push(format!("{bar}{}", theme.dim("(일치하는 항목 없음)")));
    } else {
        for (offset, &orig) in shown.iter().enumerate() {
            let is_cur = start + offset == idx;
            let item = &items[orig];
            let check_prefix = match checked {
                Some(c) if c[orig] => "[✓] ",
                Some(_) => "[ ] ",
                None => "",
            };
            let cursor_prefix = if is_cur { "❯ " } else { "  " };
            let prefix_w = display_width(cursor_prefix) + display_width(check_prefix);
            let hint_budget = cols.saturating_sub(2 + prefix_w + label_w + 2);
            let hint_str = if !item.hint.is_empty() && hint_budget >= 6 {
                format!("  {}", truncate_cols(&item.hint, hint_budget))
            } else {
                String::new()
            };
            let padded_lbl = pad_cell(&truncate_cols(&item.label, label_w), label_w, false);
            let row_plain = format!("{cursor_prefix}{check_prefix}{padded_lbl}{hint_str}");
            let clipped = truncate_cols(&row_plain, cols.saturating_sub(2));
            if is_cur {
                lines.push(format!("{bar}{}", theme.highlight_row(&pad_cell(&clipped, cols.saturating_sub(2).min(76), false))));
            } else {
                let colored_lbl = format!(
                    "{cursor_prefix}{}{padded_lbl}{}",
                    if checked.is_some_and(|c| c[orig]) {
                        theme.style(check_prefix, Tone::Ok, true)
                    } else {
                        check_prefix.to_string()
                    },
                    theme.dim(&hint_str)
                );
                lines.push(format!("{bar}{colored_lbl}"));
            }
        }
    }

    if start + MAX_VISIBLE_ITEMS < visible.len() {
        lines.push(format!(
            "{bar}{}",
            theme.dim(&format!("↓ {}개 더 있음", visible.len() - start - MAX_VISIBLE_ITEMS))
        ));
    }

    let help_text = if checked.is_some() {
        "↑↓ 이동 · Space 체크 · Ctrl+A 전체 · Enter 확정 · Esc 취소"
    } else {
        "↑↓ 이동 · Enter 선택 · Esc 취소"
    };
    let pos_info = if visible.is_empty() {
        String::new()
    } else {
        format!("  ({}/{})", idx + 1, visible.len())
    };
    lines.push(format!(
        "{}{}",
        theme.style("╰─ ", Tone::Info, false),
        theme.dim(&truncate_cols(&format!("{help_text}{pos_info}"), cols.saturating_sub(4)))
    ));

    for l in &lines {
        write!(stdout, "{l}\r\n")?;
    }
    stdout.flush()?;
    Ok(lines.len() as u16)
}

fn clear_drawn_panel(drawn: u16, stdout: &mut io::Stdout) -> io::Result<()> {
    if drawn > 0 {
        execute!(stdout, MoveUp(drawn), MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
        stdout.flush()?;
    }
    Ok(())
}

fn draw_prompt_with_suggestions<F>(
    theme: &Theme,
    prompt_plain: &str,
    editor: &LineEditor,
    suggest_fn: &F,
    notice: Option<&str>,
    prev_below: u16,
    stdout: &mut io::Stdout,
) -> io::Result<u16>
where
    F: Fn(&str) -> Vec<Suggestion>,
{
    let cols = term_cols();
    let full_text = editor.as_str();
    let prompt_styled = format!("{} ", theme.style(prompt_plain, Tone::Info, true));
    let prompt_w = display_width(prompt_plain) + 1;
    let cursor_col = (prompt_w + display_width(&editor.before_cursor())).min(cols.saturating_sub(1));

    // 이전 제안 영역 지우기 + 현재 입력 줄 다시 그리기
    execute!(stdout, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
    write!(
        stdout,
        "{prompt_styled}{}",
        truncate_cols(&full_text, cols.saturating_sub(prompt_w))
    )?;

    let mut below_lines: Vec<String> = Vec::new();
    if let Some(msg) = notice {
        below_lines.push(format!("  {}", theme.style(msg, Tone::Warn, false)));
    } else {
        let items = suggest_fn(&full_text);
        let label_w = items
            .iter()
            .take(6)
            .map(|s| display_width(&s.label))
            .max()
            .unwrap_or(10)
            .min(26);
        for item in items.into_iter().take(6) {
            let lbl = pad_cell(&truncate_cols(&item.label, label_w), label_w, false);
            let hint_budget = cols.saturating_sub(4 + label_w + 2);
            let hint = if item.hint.is_empty() || hint_budget < 4 {
                String::new()
            } else {
                format!("  {}", theme.dim(&truncate_cols(&item.hint, hint_budget)))
            };
            below_lines.push(format!("  {}{}", theme.style(&lbl, Tone::Info, false), hint));
        }
    }

    let _ = prev_below;
    if !below_lines.is_empty() {
        for bl in &below_lines {
            write!(stdout, "\r\n{bl}")?;
        }
        execute!(stdout, MoveUp(below_lines.len() as u16))?;
    }
    execute!(stdout, MoveToColumn(cursor_col as u16))?;
    stdout.flush()?;
    Ok(below_lines.len() as u16)
}

fn clear_prompt_suggestions(drawn_below: u16, stdout: &mut io::Stdout) -> io::Result<()> {
    if drawn_below > 0 {
        execute!(
            stdout,
            MoveToColumn(0),
            MoveDown(1),
            Clear(ClearType::FromCursorDown),
            MoveUp(1),
            MoveToColumn(0)
        )?;
        stdout.flush()?;
    }
    Ok(())
}

fn select_one_fallback<R: BufRead, W: Write>(
    theme: &Theme,
    title: &str,
    items: &[SelectItem],
    default_idx: usize,
    reader: &mut R,
    out: &mut W,
) -> io::Result<Option<usize>> {
    theme.section_to(out, title)?;
    for (i, item) in items.iter().enumerate() {
        writeln!(
            out,
            "  [{}] {:<18} {}",
            i + 1,
            item.label,
            theme.dim(&item.hint)
        )?;
    }
    let def_num = default_idx.min(items.len() - 1) + 1;
    write!(out, "  번호 선택 [{def_num}]: ")?;
    out.flush()?;
    let mut buf = String::new();
    let bytes = reader.read_line(&mut buf)?;
    if bytes == 0 {
        return Ok(None);
    }
    let trimmed = buf.trim().trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(Some(def_num - 1));
    }
    if matches!(trimmed.to_ascii_lowercase().as_str(), "q" | "esc" | "cancel") {
        return Ok(None);
    }
    if let Ok(num) = trimmed.parse::<usize>()
        && num >= 1
        && num <= items.len()
    {
        return Ok(Some(num - 1));
    }
    let q = trimmed.to_ascii_lowercase();
    if let Some(pos) = items
        .iter()
        .position(|it| it.label.to_ascii_lowercase().contains(&q) || it.hint.to_ascii_lowercase().contains(&q))
    {
        return Ok(Some(pos));
    }
    Ok(None)
}

fn select_many_fallback<R: BufRead, W: Write>(
    theme: &Theme,
    title: &str,
    items: &[SelectItem],
    initial_checked: &[bool],
    reader: &mut R,
    out: &mut W,
) -> io::Result<Option<Vec<usize>>> {
    theme.section_to(out, title)?;
    for (i, item) in items.iter().enumerate() {
        let mark = if initial_checked.get(i).copied().unwrap_or(false) {
            "*"
        } else {
            " "
        };
        writeln!(
            out,
            "  [{mark}][{}] {:<16} {}",
            i + 1,
            item.label,
            theme.dim(&item.hint)
        )?;
    }
    let defaults: Vec<String> = initial_checked
        .iter()
        .enumerate()
        .filter(|&(_, &on)| on)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    let def_str = if defaults.is_empty() {
        "-".to_string()
    } else {
        defaults.join(",")
    };
    write!(out, "  번호 쉼표 구분 (all / -) [{def_str}]: ")?;
    out.flush()?;
    let mut buf = String::new();
    let bytes = reader.read_line(&mut buf)?;
    if bytes == 0 {
        return Ok(None);
    }
    let trimmed = buf.trim().trim_start_matches('\u{feff}').trim();
    let effective = if trimmed.is_empty() {
        def_str.as_str()
    } else {
        trimmed
    };
    if matches!(effective.to_ascii_lowercase().as_str(), "q" | "esc" | "cancel") {
        return Ok(None);
    }
    if effective == "-" || effective.eq_ignore_ascii_case("none") {
        return Ok(Some(Vec::new()));
    }
    if effective.eq_ignore_ascii_case("all") || effective == "*" {
        return Ok(Some((0..items.len()).collect()));
    }

    let mut picked = Vec::new();
    for tok in effective.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(num) = tok.parse::<usize>()
            && num >= 1
            && num <= items.len()
            && !picked.contains(&(num - 1))
        {
            picked.push(num - 1);
        }
    }
    Ok(Some(picked))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn line_editor_handles_utf8_cursor_and_word_delete() {
        let mut ed = LineEditor::new();
        ed.set("fetch 삼성전자");
        assert_eq!(ed.as_str(), "fetch 삼성전자");
        ed.move_left();
        ed.backspace();
        assert_eq!(ed.as_str(), "fetch 삼성자");
        ed.move_home();
        ed.move_right();
        ed.delete();
        assert_eq!(ed.before_cursor(), "f");
        assert_eq!(ed.as_str(), "ftch 삼성자");
        ed.move_end();
        ed.delete_word_back();
        assert_eq!(ed.as_str(), "ftch ");
        ed.clear();
        assert!(ed.is_empty());
    }

    #[test]
    fn filter_toggle_and_ask_command_fallback_work() {
        let items = vec![
            SelectItem { label: "KRW-BTC".into(), hint: "비트코인".into() },
            SelectItem { label: "KRW-SOL".into(), hint: "솔라나".into() },
            SelectItem { label: "005930.KS".into(), hint: "삼성전자".into() },
        ];
        let visible = filter_indices(&items, "솔라");
        assert_eq!(visible, vec![1]);

        let all = filter_indices(&items, "");
        let mut checked = vec![false; items.len()];
        toggle_all_visible(&mut checked, &all);
        assert_eq!(checked, vec![true, true, true]);
        toggle_all_visible(&mut checked, &visible);
        assert_eq!(checked, vec![true, false, true]);

        let theme = Theme::plain();
        let mut out = Vec::new();
        let mut reader = Cursor::new("\u{feff}  /status  \n".as_bytes());
        let first = ask_command(
            &theme,
            "quant>",
            &[],
            |_| Vec::new(),
            |_| None,
            &mut reader,
            &mut out,
        )
        .expect("ask ok");
        assert_eq!(first, PromptInput::Line("/status".to_string()));

        let second = ask_command(
            &theme,
            "quant>",
            &[],
            |_| Vec::new(),
            |_| None,
            &mut reader,
            &mut out,
        )
        .expect("eof ok");
        assert_eq!(second, PromptInput::Eof);
    }

    #[test]
    fn fallback_select_one_and_many_parse_numbers_and_keywords() {
        let theme = Theme::plain();
        let items = vec![
            SelectItem { label: "KRW-BTC".into(), hint: "비트코인".into() },
            SelectItem { label: "KRW-SOL".into(), hint: "솔라나".into() },
        ];
        let mut out = Vec::new();
        let mut r1 = Cursor::new("솔라나\n".as_bytes());
        let picked = select_one_fallback(&theme, "Crypto", &items, 0, &mut r1, &mut out)
            .expect("ok");
        assert_eq!(picked, Some(1));

        let mut r2 = Cursor::new(b"all\n".as_slice());
        let many = select_many_fallback(&theme, "Multi", &items, &[true, false], &mut r2, &mut out)
            .expect("ok");
        assert_eq!(many, Some(vec![0, 1]));
    }
}
