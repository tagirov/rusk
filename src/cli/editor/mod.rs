//! Interactive multi-line editor.
//!
//! The module is sliced into small, focused files:
//!
//! * [`text_ops`]  — pure text helpers (available without `interactive`).
//! * [`state`]     — [`state::EditorState`] with movement / editing methods.
//! * [`history`]   — undo / redo ring buffers.
//! * [`clipboard`] — system-clipboard wrapper with a process-local fallback.
//! * [`mouse`]     — click tracker and screen → buffer coordinate mapping.
//! * [`view`]      — soft-wrapping, rendering, footer / scroll / dirty glyphs.
//! * [`terminal`]  — alt-screen / raw-mode control, help, confirm-discard.
//! * [`draft`]     — [`draft::EditorExtras`] and crash-safe draft autosave.
//! * [`input`]     — event dispatch producing a high-level [`input::Action`].
//!
//! The event loop lives in [`run_editor`] (entry point: `HandlerCLI::run_multi_line_editor`)
//! and is intentionally narrow: setup → poll → dispatch → render → teardown.

#[cfg_attr(not(feature = "interactive"), allow(dead_code))]
pub(crate) mod text_ops;

#[cfg(feature = "interactive")]
mod clipboard;
#[cfg(feature = "interactive")]
pub(crate) mod draft;
#[cfg(feature = "interactive")]
mod history;
#[cfg(feature = "interactive")]
mod input;
#[cfg(feature = "interactive")]
mod mouse;
#[cfg(feature = "interactive")]
mod state;
#[cfg(feature = "interactive")]
mod terminal;
#[cfg(feature = "interactive")]
mod view;

#[cfg(feature = "interactive")]
pub(crate) use draft::EditorExtras;

#[cfg(feature = "interactive")]
use anyhow::Result;
#[cfg(feature = "interactive")]
use crossterm::event::{self, Event};
#[cfg(feature = "interactive")]
use crossterm::{
    QueueableCommand,
    terminal::{Clear, ClearType},
};
#[cfg(feature = "interactive")]
use std::io::{self, Write};
#[cfg(feature = "interactive")]
use std::time::{Duration, Instant};

#[cfg(feature = "interactive")]
use input::Action;

use super::HandlerCLI;

// ── Public entry point ──────────────────────────────────────────────────────

#[cfg(feature = "interactive")]
pub(crate) fn run_editor(
    prompt: &str,
    prefill: &str,
    cursor_at_start: bool,
    validate: Option<fn(&str) -> bool>,
    allow_skip: bool,
    extras: EditorExtras,
) -> Result<String> {
    let mut stdout = io::stdout();
    terminal::enter(&mut stdout)?;

    let prompt_width = prompt.chars().count();
    let prefill_lines = text_ops::split_multi_line_prefill(prefill);
    let editor_row: u16 = 0;

    let initial_vw = view::visible_width(prompt_width);
    let mut state = state::EditorState::from_prefill(&prefill_lines, cursor_at_start, initial_vw);

    let mut history = history::History::new();
    let mut clipboard = clipboard::EditorClipboard::new();
    let mut click_tracker = mouse::ClickTracker::default();
    let mut last_autosave = Instant::now();
    let mut last_saved_content = prefill.to_string();

    let ctx = input::EditorContext {
        prompt_width,
        editor_row,
        prefill_lines: &prefill_lines,
    };

    let first_line_colored = extras.first_line_colored.clone();

    let render = |stdout: &mut io::Stdout, state: &mut state::EditorState| -> Result<()> {
        let selection = state.selection_range();
        let dirty = state.dirty_vs(prefill);
        view::render(
            stdout,
            view::RenderInput {
                editor_row,
                prompt,
                prompt_width,
                first_line_colored: first_line_colored.as_deref(),
                lines: &state.lines,
                cursor_row: state.row,
                cursor_col: state.col,
                view_top: &mut state.view_top,
                validate: validate.as_ref(),
                selection,
                dirty,
            },
        )
    };

    render(&mut stdout, &mut state)?;

    loop {
        draft::tick(
            &extras,
            &state.lines,
            prefill,
            &mut last_saved_content,
            &mut last_autosave,
        );

        let Some(ev) = poll_event(Duration::from_millis(500))? else {
            continue;
        };

        let action = match ev {
            Event::Resize(_, _) => {
                stdout.queue(Clear(ClearType::All))?;
                stdout.flush().ok();
                Action::Continue
            }
            Event::Key(k) => input::handle_key(k, &mut state, &mut history, &mut clipboard, &ctx)?,
            Event::Mouse(m) => input::handle_mouse(
                m,
                &mut state,
                &mut history,
                &mut clipboard,
                &mut click_tracker,
                &ctx,
            )?,
            Event::Paste(s) => input::handle_paste(s, &mut state, &mut history, &ctx)?,
            _ => Action::Continue,
        };

        match action {
            Action::Continue => {}
            Action::ShowHelp => {
                terminal::show_help(&mut stdout)?;
            }
            Action::Save => {
                let joined = state.joined();
                if validate.is_some_and(|v| {
                    !joined.trim().is_empty() && !v(joined.as_str())
                }) {
                    print!("\x07");
                    stdout.flush().ok();
                    continue;
                }
                terminal::finish(&mut stdout)?;
                draft::cleanup(&extras);
                return Ok(joined);
            }
            Action::Cancel => {
                if state.dirty_vs(prefill) && !terminal::confirm_discard(&mut stdout)? {
                    render(&mut stdout, &mut state)?;
                    continue;
                }
                terminal::finish(&mut stdout)?;
                draft::cleanup(&extras);
                return Err(if allow_skip {
                    crate::error::AppError::SkipTask.into()
                } else {
                    crate::error::AppError::UserCancel.into()
                });
            }
            Action::Abort => {
                terminal::finish(&mut stdout)?;
                println!("\n");
                return Err(crate::error::AppError::UserAbort.into());
            }
        }

        render(&mut stdout, &mut state)?;
    }
}

#[cfg(feature = "interactive")]
fn poll_event(timeout: Duration) -> Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

// ── `HandlerCLI` shims (stable API used by handlers.rs and tests) ──────────

impl HandlerCLI {
    /// Full-screen multi-line TUI editor: alternate screen, raw mode, mouse, undo, drafts.
    #[cfg(feature = "interactive")]
    pub(crate) fn run_multi_line_editor(
        prompt: &str,
        prefill: &str,
        cursor_at_start: bool,
        validate: Option<fn(&str) -> bool>,
        allow_skip: bool,
        extras: EditorExtras,
    ) -> Result<String> {
        run_editor(prompt, prefill, cursor_at_start, validate, allow_skip, extras)
    }

    #[cfg(feature = "interactive")]
    pub(crate) fn draft_path_for(dir: &std::path::Path) -> std::path::PathBuf {
        draft::path_for(dir)
    }

    #[cfg(feature = "interactive")]
    pub(crate) fn read_draft_for(path: &std::path::Path, key: &str) -> Option<String> {
        draft::read_for(path, key)
    }

    // Public helpers re-exported for tests and external tooling.
    // These stay unconditional to match the previous API surface.

    #[doc(hidden)]
    pub fn prev_char_boundary(s: &str, byte_idx: usize) -> usize {
        text_ops::prev_char_boundary(s, byte_idx)
    }

    #[doc(hidden)]
    pub fn next_char_boundary(s: &str, byte_idx: usize) -> usize {
        text_ops::next_char_boundary(s, byte_idx)
    }

    #[doc(hidden)]
    pub fn byte_idx_to_char_count(s: &str, byte_idx: usize) -> usize {
        text_ops::byte_idx_to_char_count(s, byte_idx)
    }

    #[doc(hidden)]
    pub fn is_word_char(c: char) -> bool {
        text_ops::is_word_char(c)
    }

    #[doc(hidden)]
    pub fn jump_prev_word(buffer: &str, cursor: usize) -> usize {
        text_ops::jump_prev_word(buffer, cursor)
    }

    #[doc(hidden)]
    pub fn jump_next_word(buffer: &str, cursor: usize) -> usize {
        text_ops::jump_next_word(buffer, cursor)
    }

    #[doc(hidden)]
    pub fn first_non_space(line: &str) -> usize {
        text_ops::first_non_space(line)
    }

    #[doc(hidden)]
    pub fn word_bounds(line: &str, byte_idx: usize) -> (usize, usize) {
        text_ops::word_bounds(line, byte_idx)
    }

    #[doc(hidden)]
    pub fn split_multi_line_prefill(prefill: &str) -> Vec<String> {
        text_ops::split_multi_line_prefill(prefill)
    }

    #[doc(hidden)]
    pub fn ml_char_to_byte(line: &str, target_char: usize) -> usize {
        text_ops::ml_char_to_byte(line, target_char)
    }

    #[doc(hidden)]
    pub fn ml_move_left(lines: &[String], row: usize, col: usize) -> (usize, usize) {
        text_ops::ml_move_left(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_move_right(lines: &[String], row: usize, col: usize) -> (usize, usize) {
        text_ops::ml_move_right(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_word_left(lines: &[String], row: usize, col: usize) -> (usize, usize) {
        text_ops::ml_word_left(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_word_right(lines: &[String], row: usize, col: usize) -> (usize, usize) {
        text_ops::ml_word_right(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_soft_up(
        lines: &[String],
        row: usize,
        col: usize,
        desired_vis_col: usize,
        vw: usize,
    ) -> (usize, usize) {
        text_ops::ml_soft_up(lines, row, col, desired_vis_col, vw)
    }

    #[doc(hidden)]
    pub fn ml_soft_down(
        lines: &[String],
        row: usize,
        col: usize,
        desired_vis_col: usize,
        vw: usize,
    ) -> (usize, usize) {
        text_ops::ml_soft_down(lines, row, col, desired_vis_col, vw)
    }

    #[doc(hidden)]
    pub fn ml_backspace(lines: &mut Vec<String>, row: &mut usize, col: &mut usize) {
        text_ops::ml_backspace(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_delete(lines: &mut Vec<String>, row: usize, col: &mut usize) {
        text_ops::ml_delete(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_delete_word_left(lines: &mut Vec<String>, row: &mut usize, col: &mut usize) {
        text_ops::ml_delete_word_left(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_delete_word_right(lines: &mut Vec<String>, row: usize, col: &mut usize) {
        text_ops::ml_delete_word_right(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_kill_to_eol(lines: &mut Vec<String>, row: usize, col: &mut usize) {
        text_ops::ml_kill_to_eol(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_kill_to_bol(lines: &mut [String], row: usize, col: &mut usize) {
        text_ops::ml_kill_to_bol(lines, row, col)
    }

    #[doc(hidden)]
    pub fn ml_delete_line(lines: &mut Vec<String>, row: &mut usize, col: &mut usize) {
        text_ops::ml_delete_line(lines, row, col)
    }
}
