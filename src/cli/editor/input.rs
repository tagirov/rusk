//! Event dispatch for the interactive editor. Each handler takes the mutable
//! editor pieces it needs and returns a high-level [`Action`] so the main
//! loop can centralize I/O, validation, and exit semantics.

use anyhow::Result;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use super::clipboard::EditorClipboard;
use super::history::{History, OpKind};
use super::mouse::{self, ClickTracker};
use super::state::EditorState;
use super::text_ops;
use super::view;

pub(super) enum Action {
    Continue,
    Save,
    Cancel,
    Abort,
    ShowHelp,
}

/// Slice of the outer editor state that the input dispatch needs to read.
pub(super) struct EditorContext<'a> {
    pub prompt_width: usize,
    pub editor_row: u16,
    pub prefill_lines: &'a [String],
}

pub(super) fn handle_key(
    ev: KeyEvent,
    state: &mut EditorState,
    history: &mut History,
    clipboard: &mut EditorClipboard,
    ctx: &EditorContext<'_>,
) -> Result<Action> {
    if ev.kind != KeyEventKind::Press {
        return Ok(Action::Continue);
    }
    let KeyEvent { code, modifiers, .. } = ev;
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let vw = view::visible_width(ctx.prompt_width);

    // Used by movement arms to preserve the selection anchor when shift is
    // held. The outer loop clears the anchor on non-shift moves.
    let mut selection_consumed = false;

    match (code, ctrl, alt) {
        // ── Clipboard ───────────────────────────────────────────────────────
        (KeyCode::Char('c'), true, _) => {
            if let Some(text) = state.selection_text() {
                clipboard.copy(&text);
                history.break_run();
            } else {
                return Ok(Action::Abort);
            }
        }
        (KeyCode::Char('x'), true, _) => {
            if let Some(text) = state.selection_text() {
                clipboard.copy(&text);
                history.record(state.snapshot(), OpKind::Other);
                state.delete_selection();
                state.recompute_desired(vw);
            }
        }
        (KeyCode::Char('v'), true, _) => {
            let pasted = clipboard.paste();
            if !pasted.is_empty() {
                history.record(state.snapshot(), OpKind::Other);
                state.insert_str(&pasted, vw);
            }
        }

        // ── Undo / redo ─────────────────────────────────────────────────────
        (KeyCode::Char('z'), true, _) => {
            if let Some(s) = history.undo(state.snapshot()) {
                state.restore(s, vw);
            }
        }
        (KeyCode::Char('y'), true, _) => {
            if let Some(s) = history.redo(state.snapshot()) {
                state.restore(s, vw);
            }
        }

        // ── Select all ──────────────────────────────────────────────────────
        (KeyCode::Char('a'), true, _) => {
            state.select_all(vw);
            history.break_run();
            selection_consumed = true;
        }

        // ── Save / help / reset ─────────────────────────────────────────────
        (KeyCode::Char('s'), true, _) => return Ok(Action::Save),
        (KeyCode::Char('g'), true, _) | (KeyCode::F(1), _, _) => {
            history.break_run();
            return Ok(Action::ShowHelp);
        }
        (KeyCode::Char('r'), true, _) => {
            history.record(state.snapshot(), OpKind::Other);
            state.reset_to_prefill(ctx.prefill_lines, vw);
        }
        (KeyCode::Esc, _, _) => return Ok(Action::Cancel),

        // ── Structural editing ──────────────────────────────────────────────
        (KeyCode::Enter, _, _) => {
            history.record(state.snapshot(), OpKind::Other);
            state.insert_newline(vw);
        }
        (KeyCode::Tab, _, _) => {
            history.record(state.snapshot(), OpKind::Other);
            state.insert_tab(vw);
        }

        // ── Word movement ───────────────────────────────────────────────────
        (KeyCode::Left, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.word_left(vw);
            history.break_run();
        }
        (KeyCode::Right, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.word_right(vw);
            history.break_run();
        }

        // ── Character movement ──────────────────────────────────────────────
        (KeyCode::Left, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.move_left(vw);
            history.break_run();
        }
        (KeyCode::Right, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.move_right(vw);
            history.break_run();
        }

        // ── Vertical movement ───────────────────────────────────────────────
        (KeyCode::Up, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.soft_up_n(5, vw);
            history.break_run();
        }
        (KeyCode::Down, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.soft_down_n(5, vw);
            history.break_run();
        }
        (KeyCode::Up, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.soft_up(vw);
            history.break_run();
        }
        (KeyCode::Down, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.soft_down(vw);
            history.break_run();
        }

        // ── Page / buffer jumps ─────────────────────────────────────────────
        (KeyCode::PageUp, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.goto_buffer_start();
            history.break_run();
        }
        (KeyCode::PageDown, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.goto_buffer_end(vw);
            history.break_run();
        }
        (KeyCode::PageUp, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.soft_up_n(view::page_rows(), vw);
            history.break_run();
        }
        (KeyCode::PageDown, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.soft_down_n(view::page_rows(), vw);
            history.break_run();
        }
        (KeyCode::Home, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.goto_buffer_start();
            history.break_run();
        }
        (KeyCode::End, true, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.goto_buffer_end(vw);
            history.break_run();
        }
        (KeyCode::Home, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.smart_home(vw);
            history.break_run();
        }
        (KeyCode::End, _, _) => {
            if shift {
                state.start_selection_if_needed();
                selection_consumed = true;
            }
            state.goto_line_end(vw);
            history.break_run();
        }

        // ── Delete-word / line-kill ─────────────────────────────────────────
        // Ctrl+H is the legacy BS byte that many terminals send for Ctrl+Backspace.
        (KeyCode::Char('w'), true, _)
        | (KeyCode::Char('h'), true, _)
        | (KeyCode::Backspace, true, _) => {
            history.record(state.snapshot(), OpKind::Other);
            state.delete_word_left(vw);
        }
        (KeyCode::Delete, true, _) => {
            history.record(state.snapshot(), OpKind::Other);
            state.delete_word_right(vw);
        }
        (KeyCode::Char('k'), true, _) => {
            history.record(state.snapshot(), OpKind::Other);
            if shift {
                state.delete_line(vw);
            } else {
                state.kill_to_eol(vw);
            }
        }
        (KeyCode::Char('u'), true, _) => {
            history.record(state.snapshot(), OpKind::Other);
            state.kill_to_bol(vw);
        }

        // ── Character-level editing ─────────────────────────────────────────
        (KeyCode::Backspace, _, _) => {
            let kind = if state.anchor.is_some() {
                OpKind::Other
            } else {
                OpKind::Backspace
            };
            history.record(state.snapshot(), kind);
            state.backspace(vw);
        }
        (KeyCode::Delete, _, _) => {
            let kind = if state.anchor.is_some() {
                OpKind::Other
            } else {
                OpKind::DeleteChar
            };
            history.record(state.snapshot(), kind);
            state.delete(vw);
        }
        (KeyCode::Char(ch), _, _) => {
            history.record(state.snapshot(), OpKind::InsertChar);
            state.insert_char(ch, vw);
        }

        _ => history.break_run(),
    }

    if !shift && !selection_consumed {
        state.clear_selection();
    }

    Ok(Action::Continue)
}

pub(super) fn handle_mouse(
    ev: MouseEvent,
    state: &mut EditorState,
    history: &mut History,
    clipboard: &mut EditorClipboard,
    click_tracker: &mut ClickTracker,
    ctx: &EditorContext<'_>,
) -> Result<Action> {
    let MouseEvent { kind, column, row: mrow, modifiers } = ev;
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let vw = view::visible_width(ctx.prompt_width);

    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let clicks = click_tracker.click(column, mrow);
            let visuals = view::compute_visuals(&state.lines, vw);
            let (r, c) = mouse::ScreenToBuffer {
                lines: &state.lines,
                visuals: &visuals,
                vw,
                screen_x: column,
                screen_y: mrow,
                editor_row: ctx.editor_row,
                view_top: state.view_top,
                prompt_width: ctx.prompt_width,
            }
            .resolve();
            match clicks {
                2 => {
                    let (ws, we) = text_ops::word_bounds(&state.lines[r], c);
                    state.anchor = Some((r, ws));
                    state.row = r;
                    state.col = we;
                }
                3 => {
                    state.anchor = Some((r, 0));
                    state.row = r;
                    state.col = state.lines[r].len();
                }
                _ => {
                    if shift {
                        state.start_selection_if_needed();
                        state.row = r;
                        state.col = c;
                    } else {
                        state.row = r;
                        state.col = c;
                        state.anchor = Some((r, c));
                    }
                }
            }
            state.recompute_desired(vw);
            history.break_run();
        }
        MouseEventKind::Down(MouseButton::Middle) => {
            let pasted = clipboard.paste();
            if !pasted.is_empty() {
                history.record(state.snapshot(), OpKind::Other);
                let visuals = view::compute_visuals(&state.lines, vw);
                let (r, c) = mouse::ScreenToBuffer {
                    lines: &state.lines,
                    visuals: &visuals,
                    vw,
                    screen_x: column,
                    screen_y: mrow,
                    editor_row: ctx.editor_row,
                    view_top: state.view_top,
                    prompt_width: ctx.prompt_width,
                }
                .resolve();
                state.row = r;
                state.col = c;
                state.anchor = None;
                state.insert_str(&pasted, vw);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let visuals = view::compute_visuals(&state.lines, vw);
            let (r, c) = mouse::ScreenToBuffer {
                lines: &state.lines,
                visuals: &visuals,
                vw,
                screen_x: column,
                screen_y: mrow,
                editor_row: ctx.editor_row,
                view_top: state.view_top,
                prompt_width: ctx.prompt_width,
            }
            .resolve();
            state.goto(r, c, vw);
            history.break_run();
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if state.anchor == Some((state.row, state.col)) {
                state.anchor = None;
            }
        }
        MouseEventKind::ScrollUp => {
            state.soft_up_n(3, vw);
            if state.view_top > 0 {
                state.view_top = state.view_top.saturating_sub(3);
            }
            state.anchor = None;
            history.break_run();
        }
        MouseEventKind::ScrollDown => {
            let visuals = view::compute_visuals(&state.lines, vw);
            state.soft_down_n(3, vw);
            if state.view_top + 1 < visuals.len() {
                state.view_top = (state.view_top + 3).min(visuals.len() - 1);
            }
            state.anchor = None;
            history.break_run();
        }
        _ => {}
    }
    Ok(Action::Continue)
}

pub(super) fn handle_paste(
    pasted: String,
    state: &mut EditorState,
    history: &mut History,
    ctx: &EditorContext<'_>,
) -> Result<Action> {
    let vw = view::visible_width(ctx.prompt_width);
    history.record(state.snapshot(), OpKind::Other);
    state.insert_str(&pasted, vw);
    Ok(Action::Continue)
}
