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
use std::time::Duration;

#[cfg(feature = "interactive")]
use input::Action;

use super::HandlerCLI;

// ── Public entry point ──────────────────────────────────────────────────────

#[cfg(feature = "interactive")]
pub(crate) fn run_editor(
    prompt: &str,
    prefill: &str,
    restored: Option<&str>,
    cursor_at_start: bool,
    allow_skip: bool,
    extras: EditorExtras,
) -> Result<String> {
    let mut stdout = io::stdout();
    // From here on the terminal is the editor's, and the guard is what
    // gives it back — on the way out below, but also on a `?`, a panic or
    // a SIGTERM. Nothing between here and the end may restore it by hand.
    let mut guard = terminal::TerminalGuard::enter(&mut stdout)?;

    let prompt_width = crate::width::width(prompt);
    let prefill_lines = text_ops::split_multi_line_prefill(prefill);
    // What the task holds now: line endings unified and tabs expanded.
    // Everything that asks "did the user change anything?" — the dirty
    // glyph, the discard prompt, the draft autosave — compares against this,
    // so merely opening a task written with tabs or CRLF is not an edit,
    // and a buffer that starts from a restored draft is dirty from the
    // first frame, because against the stored task it is.
    let baseline = prefill_lines.join("\n");
    let opening_lines = match restored {
        Some(text) => text_ops::split_multi_line_prefill(text),
        None => prefill_lines.clone(),
    };

    let init_size = view::term_size();
    let initial_vw = view::editor_text_layout(init_size.0 as usize, prompt_width).0;
    let mut state = state::EditorState::from_prefill(&opening_lines, cursor_at_start, initial_vw);

    let mut history = history::History::new();
    let mut clipboard = clipboard::EditorClipboard::new();
    let mut click_tracker = mouse::ClickTracker::default();
    let mut autosave = draft::Autosave::default();

    let first_line_colored = extras.first_line_colored.clone();

    let render = |stdout: &mut io::Stdout,
                  state: &mut state::EditorState,
                  term_size: (u16, u16)|
     -> Result<()> {
        let selection = state.selection_range();
        let dirty = state.dirty_vs(&baseline);
        view::render(
            stdout,
            view::RenderInput {
                prompt,
                prompt_width,
                term_size,
                first_line_colored: first_line_colored.as_deref(),
                lines: &state.lines,
                cursor_row: state.row,
                cursor_col: state.col,
                view_top: &mut state.view_top,
                follow_cursor: state.follow_cursor,
                selection,
                dirty,
                relative_date_base: extras.relative_date_base,
            },
        )
    };

    render(&mut stdout, &mut state, init_size)?;

    loop {
        draft::tick(&extras, &state.lines, &baseline, &mut autosave);

        // A SIGTERM from a window that was closed, a SIGHUP from a
        // connection that dropped: the terminal goes back the way it was
        // and the buffer is kept, exactly as a Ctrl+C would.
        if let Some(signum) = terminal::caught_signal() {
            draft::flush(&extras, &state.lines, &baseline, &mut autosave);
            guard.finish(&mut stdout);
            terminal::exit_after_signal(signum, draft_note(&extras, &autosave).as_deref());
        }

        let Some(ev) = poll_event(Duration::from_millis(500))? else {
            continue;
        };
        // One terminal-size read per event: input dispatch and render share the
        // same geometry even during a resize burst.
        let mut term_size = view::term_size();
        let (cols, rows) = term_size;
        let ctx = input::EditorContext {
            prompt_width,
            editor_row: view::vertical_layout(cols, rows).0,
            cols,
            rows,
            prefill_lines: &prefill_lines,
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
                // Overlays block on read() and swallow resize events, so the
                // pre-overlay size may be stale by the time they return.
                term_size = view::term_size();
            }
            Action::Save => {
                let joined = state.joined();
                // The draft stays until the text is somewhere safer: the
                // caller removes it once the database holds it, and a save
                // that fails afterwards leaves the text right here.
                draft::flush(&extras, &state.lines, &baseline, &mut autosave);
                guard.finish(&mut stdout);
                warn_about_drafts(&extras, &autosave);
                return Ok(joined);
            }
            Action::Cancel => {
                let fr = view::footer_row_for_state(
                    &state.lines,
                    state.view_top,
                    prompt_width,
                    term_size,
                );
                let dr = view::discard_dialog_row(fr, rows);
                if state.dirty_vs(&baseline) {
                    match terminal::confirm_discard(&mut stdout, dr)? {
                        terminal::Discard::No => {
                            term_size = view::term_size();
                            render(&mut stdout, &mut state, term_size)?;
                            continue;
                        }
                        terminal::Discard::Abort => {
                            return abort(&mut guard, &mut stdout, &extras, &state, &baseline, &mut autosave);
                        }
                        terminal::Discard::Yes => {}
                    }
                }
                // Discarded on purpose: there is nothing unsaved left.
                if let Some(slot) = &extras.draft {
                    draft::remove(slot);
                }
                guard.finish(&mut stdout);
                warn_about_drafts(&extras, &autosave);
                return Err(if allow_skip {
                    crate::error::AppError::SkipTask.into()
                } else {
                    crate::error::AppError::UserCancel.into()
                });
            }
            Action::Abort => {
                return abort(&mut guard, &mut stdout, &extras, &state, &baseline, &mut autosave);
            }
        }

        render(&mut stdout, &mut state, term_size)?;
    }
}

/// Ctrl+C. It ends the session on the spot — that is what it is for — but
/// not by throwing away what was typed: the buffer goes to the draft first,
/// so the next `rusk edit` can offer it back.
#[cfg(feature = "interactive")]
fn abort(
    guard: &mut terminal::TerminalGuard,
    stdout: &mut io::Stdout,
    extras: &EditorExtras,
    state: &state::EditorState,
    baseline: &str,
    autosave: &mut draft::Autosave,
) -> Result<String> {
    draft::flush(extras, &state.lines, baseline, autosave);
    guard.finish(stdout);
    // Aborting already: nothing more to do about an output that fails.
    crate::out!("\n\n").ok();
    if let Some(note) = draft_note(extras, autosave) {
        crate::errln!("{note}");
    }
    Err(crate::error::AppError::UserAbort.into())
}

/// What to say about the draft once the terminal is back: that the typed
/// text was kept, or that it could not be.
#[cfg(feature = "interactive")]
fn draft_note(extras: &EditorExtras, autosave: &draft::Autosave) -> Option<String> {
    if let Some(error) = &autosave.error {
        return Some(format!("Warning: {error}"));
    }
    let slot = extras.draft.as_ref()?;
    slot.path
        .exists()
        .then(|| "Your text was kept as a draft; the next edit of this task offers it back.".to_string())
}

/// An autosave that failed is worth one line, once the editor is no longer
/// holding the screen.
#[cfg(feature = "interactive")]
fn warn_about_drafts(extras: &EditorExtras, autosave: &draft::Autosave) {
    let _ = extras;
    if let Some(error) = &autosave.error {
        crate::backend::warn_once(&format!("Warning: {error}"));
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
        restored: Option<&str>,
        cursor_at_start: bool,
        allow_skip: bool,
        extras: EditorExtras,
    ) -> Result<String> {
        run_editor(prompt, prefill, restored, cursor_at_start, allow_skip, extras)
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
}
