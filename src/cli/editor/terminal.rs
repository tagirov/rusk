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

/// Render the in-editor help overlay, wait for a key or click to dismiss it,
/// and then clear the screen so the caller can re-render the editor.
pub(super) fn show_help(stdout: &mut io::Stdout) -> Result<()> {
    let lines: &[&str] = &[
        "Rusk Interactive Editor Keys",
        "",
        "  Enter              new line",
        "  Ctrl+S             save",
        "  Esc                cancel / skip (confirms when dirty)",
        "  Ctrl+R             restore original text",
        "  Tab                insert four spaces",
        "  Up / Down          move between lines",
        "  Ctrl+Up/Down       move by 5 lines",
        "  Left / Right       move by char",
        "  Ctrl+Left/Right    jump by word",
        "  Home / End         line start (smart) / end",
        "  Ctrl+Home/End      buffer start / end",
        "  PageUp / PageDown  scroll one page",
        "  Shift+Arrows       extend selection",
        "  Ctrl+A             select all",
        "  Ctrl+C / X / V     copy / cut / paste",
        "  Ctrl+Z / Ctrl+Y    undo / redo",
        "  Ctrl+W, Ctrl+Bksp  delete word left",
        "  Ctrl+Delete        delete word right",
        "  Ctrl+K             kill to end of line",
        "  Ctrl+Shift+K       delete whole line",
        "  Ctrl+U             kill to beginning of line",
        "  Double-click       select word",
        "  Triple-click       select line",
        "  Middle-click       paste from clipboard",
        "",
        "  Due date: only the first line, at the very start, as the first",
        "    token. Absolute: DD-MM-YYYY, DD/MM/YYYY, or DD.MM.YYYY (short year",
        "    ok), or relative from today (2d, 2w, 10d5w, …), same as rusk add -d.",
        "    A recognized token is shown in color on that line (green = today or",
        "    later, red = before today; invalid or non-date text is not colored).",
        "    Use `_` alone as the first token to clear. No date token = no due date;",
        "    keep the preloaded first line, or Ctrl+R to restore the original.",
        "  Ctrl+G / F1        show this help",
    ];
    let hint = "(press any key to return)";
    /// title + blank + blank before hint + hint
    const CHROME: usize = 4;

    let body: &[&str] = &lines[2..];
    let body_width = || {
        body
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0)
    };

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
        let body_start_col = term_cols.saturating_sub(body_width()) / 2;

        stdout.queue(Clear(ClearType::All))?;

        let title = lines[0];
        let title_col = term_cols.saturating_sub(title.chars().count()) / 2;
        stdout.queue(MoveTo(title_col as u16, start_row as u16))?;
        stdout.queue(Print(title.bold()))?;

        let first = if overflow { *body_scroll } else { 0 };
        let n = if overflow {
            (body.len().saturating_sub(first)).min(visible_body)
        } else {
            body.len()
        };

        for (i, line) in body.iter().skip(first).take(n).enumerate() {
            let row = start_row + 2 + i;
            stdout.queue(MoveTo(body_start_col as u16, row as u16))?;
            stdout.queue(Print(line.truecolor(200, 200, 200)))?;
        }

        let last_body_row = if n == 0 {
            start_row + 1
        } else {
            start_row + 2 + (n - 1)
        };
        // At least one blank line between the last help line and the hint.
        let hint_row = last_body_row + 2;
        let hint_col = term_cols.saturating_sub(hint.chars().count()) / 2;
        stdout.queue(MoveTo(hint_col as u16, hint_row as u16))?;
        stdout.queue(Print(hint.truecolor(128, 128, 128)))?;
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
            Event::Key(KeyEvent { kind: KeyEventKind::Press, .. }) => break,
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
        if let Event::Key(KeyEvent { code, kind, modifiers, .. }) = read()? {
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
