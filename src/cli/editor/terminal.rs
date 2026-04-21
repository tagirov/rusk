//! Low-level terminal control: enter/leave alt-screen, raw-mode teardown,
//! help overlay, and the "discard changes?" confirmation popup.

use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
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
    let lines: [&str; 27] = [
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
        "  Ctrl+G / F1        show this help",
    ];
    let hint = "(press any key to return)";

    let (term_cols_u16, term_rows_u16) = size().unwrap_or((80, 24));
    let term_cols = term_cols_u16 as usize;
    let term_rows = term_rows_u16 as usize;

    let block_height = lines.len() + 2;
    let block_width = lines
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(hint.chars().count());
    let start_row = term_rows.saturating_sub(block_height) / 2;
    let start_col = term_cols.saturating_sub(block_width) / 2;

    stdout.queue(Clear(ClearType::All))?;
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            let title_col = start_col
                + block_width.saturating_sub(line.chars().count()) / 2;
            stdout.queue(MoveTo(title_col as u16, (start_row + i) as u16))?;
            stdout.queue(Print(line.bold()))?;
        } else {
            stdout.queue(MoveTo(start_col as u16, (start_row + i) as u16))?;
            stdout.queue(Print(line.truecolor(200, 200, 200)))?;
        }
    }
    let hint_row = start_row + lines.len() + 1;
    let hint_col = term_cols.saturating_sub(hint.chars().count()) / 2;
    stdout.queue(MoveTo(hint_col as u16, hint_row as u16))?;
    stdout.queue(Print(hint.truecolor(128, 128, 128)))?;
    stdout.flush().ok();

    // Dismiss on any key Press or left mouse button Down, then drain any
    // queued follow-up events so they never reach the editor loop.
    loop {
        match read()? {
            Event::Key(KeyEvent { kind: KeyEventKind::Press, .. }) => break,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            }) => break,
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

/// Overlay "Discard changes? [y/N]" near the footer.
/// Returns `Ok(true)` if the user confirmed with `y`, `Ok(false)` otherwise.
/// A Ctrl+C aborts by returning `AppError::UserAbort`.
pub(super) fn confirm_discard(stdout: &mut io::Stdout) -> Result<bool> {
    let (cols_u16, rows_u16) = size().unwrap_or((80, 24));
    let rows = rows_u16 as usize;
    let cols = cols_u16 as usize;
    let prompt = " Discard changes? [y/N] ";
    let row = rows.saturating_sub(1) as u16;
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
