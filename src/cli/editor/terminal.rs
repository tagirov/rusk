//! Low-level terminal control: enter/leave alt-screen, raw-mode teardown,
//! help overlay, and the "discard changes?" confirmation popup.

use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind, read,
    },
    style::Print,
    terminal::{
        Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};
use std::io::{self, Write};

struct ShowCursorOnDrop;
impl Drop for ShowCursorOnDrop {
    fn drop(&mut self) {
        let _ = std::io::stdout().execute(Show);
    }
}

/// Enter the alternate screen + raw mode with mouse capture and no line wrap.
pub(super) fn enter(stdout: &mut io::Stdout) -> Result<()> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    stdout.queue(EnterAlternateScreen)?;
    stdout.queue(EnableMouseCapture)?;
    stdout.queue(DisableLineWrap)?;
    stdout.queue(Clear(ClearType::All))?;
    stdout.flush().ok();
    Ok(())
}

/// Leave the alternate screen and restore normal terminal state.
pub(super) fn finish(stdout: &mut io::Stdout) -> Result<()> {
    stdout.queue(EnableLineWrap).ok();
    stdout.queue(DisableMouseCapture).ok();
    stdout.queue(LeaveAlternateScreen)?;
    stdout.flush().ok();
    disable_raw_mode().ok();
    Ok(())
}

/// A single row in the help overlay body.
enum HelpRow {
    Section(&'static str),
    Pair(&'static str, &'static str),
    Note(&'static str),
    Blank,
}

const HELP_TITLE: &str = "Rusk Interactive Editor";
const HELP_HINT: &str = "press any key to return";

const HELP_ROWS: &[HelpRow] = &[
    HelpRow::Section("Navigation"),
    HelpRow::Pair("← / →", "move by character"),
    HelpRow::Pair("Ctrl+← / Ctrl+→", "jump by word"),
    HelpRow::Pair("↑ / ↓", "move by visual row"),
    HelpRow::Pair("Ctrl+↑ / Ctrl+↓", "move by 5 visual rows"),
    HelpRow::Pair("Home / End", "smart line start / line end"),
    HelpRow::Pair("Ctrl+Home / Ctrl+End", "buffer start / buffer end"),
    HelpRow::Pair("PageUp / PageDown", "scroll one page"),
    HelpRow::Pair("Ctrl+PageUp / Ctrl+PageDown", "buffer start / buffer end"),
    HelpRow::Blank,
    HelpRow::Section("Selection"),
    HelpRow::Pair("Shift + <movement>", "extend selection"),
    HelpRow::Pair("Ctrl+A", "select all"),
    HelpRow::Blank,
    HelpRow::Section("Editing"),
    HelpRow::Pair("Enter", "insert newline"),
    HelpRow::Pair("Tab", "insert four spaces"),
    HelpRow::Pair("Backspace / Delete", "delete char or selection"),
    HelpRow::Pair("Ctrl+W, Ctrl+Backspace", "delete word to the left"),
    HelpRow::Pair("Ctrl+Delete", "delete word to the right"),
    HelpRow::Pair("Ctrl+K", "kill to end of line"),
    HelpRow::Pair("Ctrl+Shift+K", "delete whole line"),
    HelpRow::Pair("Ctrl+U", "kill to beginning of line"),
    HelpRow::Pair("Ctrl+R", "restore original text"),
    HelpRow::Blank,
    HelpRow::Section("Clipboard & History"),
    HelpRow::Pair("Ctrl+C / Ctrl+X / Ctrl+V", "copy / cut / paste"),
    HelpRow::Pair("Ctrl+Z / Ctrl+Y", "undo / redo"),
    HelpRow::Blank,
    HelpRow::Section("Mouse"),
    HelpRow::Pair("Click / drag", "move cursor / extend selection"),
    HelpRow::Pair("Double-click", "select word"),
    HelpRow::Pair("Triple-click", "select line"),
    HelpRow::Pair("Shift + click", "extend selection to click point"),
    HelpRow::Pair("Middle-click", "paste at cursor"),
    HelpRow::Pair("Wheel", "scroll view"),
    HelpRow::Blank,
    HelpRow::Section("Session"),
    HelpRow::Pair("Ctrl+S", "save and exit"),
    HelpRow::Pair("Esc", "cancel / skip (confirms when dirty)"),
    HelpRow::Pair("Ctrl+G / F1", "show this help"),
    HelpRow::Blank,
    HelpRow::Section("Due date — first token of the first line"),
    HelpRow::Pair("DD-MM-YYYY", "absolute (also `/` or `.`; short year ok)"),
    HelpRow::Pair("2d 2w 3m 1q 1y", "relative from today (combine: 10d5w)"),
    HelpRow::Pair(
        "+2w, +10d5w",
        "relative to current due date (today if none)",
    ),
    HelpRow::Pair("_", "clear the due date"),
    HelpRow::Note("Recognized tokens are colored: green = today/future, red = past."),
];

fn pad_right_chars(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count >= width {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + (width - count));
    out.push_str(s);
    for _ in 0..(width - count) {
        out.push(' ');
    }
    out
}

/// Render the in-editor help overlay, wait for a key or click to dismiss it,
/// and then clear the screen so the caller can re-render the editor.
pub(super) fn show_help(stdout: &mut io::Stdout) -> Result<()> {
    /// title + rule + blank + blank before hint + hint
    const CHROME: usize = 5;
    /// Indent applied to every body row so sections visually group the pairs.
    const BODY_INDENT: usize = 2;

    let body = HELP_ROWS;

    let key_col = body
        .iter()
        .filter_map(|r| match r {
            HelpRow::Pair(k, _) => Some(k.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let row_visible_width = |r: &HelpRow| -> usize {
        match r {
            HelpRow::Section(s) => s.chars().count(),
            HelpRow::Pair(_, d) => BODY_INDENT + key_col + 2 + d.chars().count(),
            HelpRow::Note(n) => BODY_INDENT + n.chars().count(),
            HelpRow::Blank => 0,
        }
    };
    let body_width = body.iter().map(row_visible_width).max().unwrap_or(0);

    let mut body_scroll: usize = 0;
    let _show_cursor = ShowCursorOnDrop;
    stdout.queue(Hide)?;
    stdout.flush().ok();

    let paint = |stdout: &mut io::Stdout, body_scroll: &mut usize| -> Result<()> {
        let (term_cols_u16, term_rows_u16) = size().unwrap_or((80, 24));
        let term_cols = term_cols_u16 as usize;
        let term_rows = term_rows_u16 as usize;

        let max_body = term_rows.saturating_sub(CHROME);
        let overflow = body.len() > max_body;
        let visible_body = if overflow { max_body } else { body.len() };
        let max_scroll = if overflow && visible_body > 0 {
            body.len().saturating_sub(visible_body)
        } else {
            0
        };
        *body_scroll = (*body_scroll).min(max_scroll);

        let start_row = if overflow {
            0
        } else {
            let block_height = CHROME + body.len();
            term_rows.saturating_sub(block_height) / 2
        };
        let body_start_col = term_cols.saturating_sub(body_width) / 2;

        stdout.queue(Clear(ClearType::All))?;

        let title_col = term_cols.saturating_sub(HELP_TITLE.chars().count()) / 2;
        stdout.queue(MoveTo(title_col as u16, start_row as u16))?;
        stdout.queue(Print(HELP_TITLE.truecolor(235, 235, 240).bold()))?;

        // Thin underline beneath the title, as wide as the body block.
        let rule_width = body_width.max(HELP_TITLE.chars().count());
        let rule: String = "─".repeat(rule_width);
        let rule_col = term_cols.saturating_sub(rule_width) / 2;
        stdout.queue(MoveTo(rule_col as u16, (start_row + 1) as u16))?;
        stdout.queue(Print(rule.truecolor(90, 90, 95)))?;

        let first = if overflow { *body_scroll } else { 0 };
        let n = if overflow {
            (body.len().saturating_sub(first)).min(visible_body)
        } else {
            body.len()
        };

        for (i, row) in body.iter().skip(first).take(n).enumerate() {
            let y = (start_row + 3 + i) as u16;
            match row {
                HelpRow::Section(s) => {
                    stdout.queue(MoveTo(body_start_col as u16, y))?;
                    stdout.queue(Print(s.truecolor(120, 200, 230).bold()))?;
                }
                HelpRow::Pair(k, d) => {
                    stdout.queue(MoveTo(body_start_col as u16, y))?;
                    stdout.queue(Print(" ".repeat(BODY_INDENT)))?;
                    let padded = pad_right_chars(k, key_col);
                    stdout.queue(Print(padded.truecolor(230, 230, 235).bold()))?;
                    stdout.queue(Print("  "))?;
                    stdout.queue(Print(d.truecolor(175, 175, 180)))?;
                }
                HelpRow::Note(text) => {
                    stdout.queue(MoveTo(body_start_col as u16, y))?;
                    stdout.queue(Print(" ".repeat(BODY_INDENT)))?;
                    stdout.queue(Print(text.truecolor(150, 150, 155).italic()))?;
                }
                HelpRow::Blank => {}
            }
        }

        let last_body_row = if n == 0 {
            start_row + 2
        } else {
            start_row + 3 + (n - 1)
        };
        // At least one blank line between the last help line and the hint.
        let hint_row = last_body_row + 2;
        let hint_col = term_cols.saturating_sub(HELP_HINT.chars().count()) / 2;
        stdout.queue(MoveTo(hint_col as u16, hint_row as u16))?;
        stdout.queue(Print(HELP_HINT.truecolor(128, 128, 128).italic()))?;
        stdout.flush().ok();
        Ok(())
    };

    paint(stdout, &mut body_scroll)?;

    // Dismiss on any key Press or left mouse button Down; redraw on resize.
    // When content overflows, Up/Down/PageUp/PageDown/Home/End/scroll wheel move the view.
    // Drain queued follow-up events so they never reach the editor loop.
    loop {
        let (_, term_rows_u16) = size().unwrap_or((80, 24));
        let term_rows = term_rows_u16 as usize;
        let max_body = term_rows.saturating_sub(CHROME);
        let visible_body = if body.len() > max_body {
            max_body
        } else {
            body.len()
        };
        let max_scroll = if body.len() > max_body && visible_body > 0 {
            body.len().saturating_sub(visible_body)
        } else {
            0
        };
        let page_step = visible_body.max(1);

        match read()? {
            Event::Resize(_, _) => {
                body_scroll = body_scroll.min(max_scroll);
                paint(stdout, &mut body_scroll)?;
            }
            Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) if body.len() > max_body => match code {
                KeyCode::Up => {
                    body_scroll = body_scroll.saturating_sub(1).min(max_scroll);
                    paint(stdout, &mut body_scroll)?;
                }
                KeyCode::Down => {
                    body_scroll = (body_scroll + 1).min(max_scroll);
                    paint(stdout, &mut body_scroll)?;
                }
                KeyCode::PageUp => {
                    body_scroll = body_scroll.saturating_sub(page_step).min(max_scroll);
                    paint(stdout, &mut body_scroll)?;
                }
                KeyCode::PageDown => {
                    body_scroll = (body_scroll + page_step).min(max_scroll);
                    paint(stdout, &mut body_scroll)?;
                }
                KeyCode::Home => {
                    body_scroll = 0;
                    paint(stdout, &mut body_scroll)?;
                }
                KeyCode::End => {
                    body_scroll = max_scroll;
                    paint(stdout, &mut body_scroll)?;
                }
                _ => break,
            },
            Event::Key(KeyEvent {
                kind: KeyEventKind::Press,
                ..
            }) => break,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            }) => break,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                ..
            }) if body.len() > max_body => {
                body_scroll = body_scroll.saturating_sub(3).min(max_scroll);
                paint(stdout, &mut body_scroll)?;
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..
            }) if body.len() > max_body => {
                body_scroll = (body_scroll + 3).min(max_scroll);
                paint(stdout, &mut body_scroll)?;
            }
            _ => {}
        }
    }
    while crossterm::event::poll(std::time::Duration::from_millis(0))? {
        let _ = read()?;
    }

    stdout.queue(Clear(ClearType::All))?;
    stdout.flush().ok();
    Ok(())
}

/// Overlay "Discard changes? [y/N]". When `dialog_row` is `Some(r)`, the prompt is on row `r`
/// (in the space between the footer and the window bottom). When `None`, it is shown on the last
/// row, replacing the footer.
/// Returns `Ok(true)` if the user confirmed with `y`, `Ok(false)` otherwise.
/// A Ctrl+C aborts by returning `AppError::UserAbort`.
pub(super) fn confirm_discard(stdout: &mut io::Stdout, dialog_row: Option<u16>) -> Result<bool> {
    let _show_cursor = ShowCursorOnDrop;
    stdout.queue(Hide)?;
    stdout.flush().ok();
    let (cols_u16, rows_u16) = size().unwrap_or((80, 24));
    let cols = cols_u16 as usize;
    let last = rows_u16.saturating_sub(1);
    let prompt = " Discard changes? [y/N] ";
    let row = dialog_row.unwrap_or(last);
    let col = cols.saturating_sub(prompt.chars().count()) / 2;
    stdout.queue(MoveTo(0, row))?;
    stdout.queue(Clear(ClearType::CurrentLine))?;
    stdout.queue(MoveTo(col as u16, row))?;
    stdout.queue(Print(prompt.truecolor(255, 165, 0)))?;
    stdout.flush().ok();
    loop {
        if let Event::Key(KeyEvent {
            code,
            kind,
            modifiers,
            ..
        }) = read()?
        {
            if kind != KeyEventKind::Press {
                continue;
            }
            match (code, modifiers) {
                (KeyCode::Char('y') | KeyCode::Char('Y'), _) => return Ok(true),
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    finish(stdout).ok();
                    return Err(crate::error::AppError::UserAbort.into());
                }
                _ => return Ok(false),
            }
        }
    }
}
