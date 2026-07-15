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
/// `cols`/`rows` hold the terminal size read once per event, so all layout
/// math within one event sees the same geometry.
pub(super) struct EditorContext<'a> {
    pub prompt_width: usize,
    pub editor_row: u16,
    pub cols: u16,
    pub rows: u16,
    pub prefill_lines: &'a [String],
}

/// Shared frame of every movement arm: when shift is held, anchor the
/// selection (if not yet anchored) before moving. The selection survives
/// because the epilogue only clears it on non-shift moves.
fn selectable_move(
    state: &mut EditorState,
    history: &mut History,
    shift: bool,
    mv: impl FnOnce(&mut EditorState),
) {
    if shift {
        state.start_selection_if_needed();
    }
    mv(state);
    history.break_run();
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
    let KeyEvent {
        code, modifiers, ..
    } = ev;
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let vw = view::editor_text_layout(ctx.cols as usize, ctx.prompt_width).0;

    // Set by arms that create a selection without shift (Ctrl+A) so the
    // epilogue below does not immediately clear it.
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
        (KeyCode::Left, true, _) => selectable_move(state, history, shift, |s| s.word_left(vw)),
        (KeyCode::Right, true, _) => selectable_move(state, history, shift, |s| s.word_right(vw)),

        // ── Character movement ──────────────────────────────────────────────
        (KeyCode::Left, _, _) => selectable_move(state, history, shift, |s| s.move_left(vw)),
        (KeyCode::Right, _, _) => selectable_move(state, history, shift, |s| s.move_right(vw)),

        // ── Vertical movement ───────────────────────────────────────────────
        (KeyCode::Up, true, _) => selectable_move(state, history, shift, |s| s.soft_up_n(5, vw)),
        (KeyCode::Down, true, _) => {
            selectable_move(state, history, shift, |s| s.soft_down_n(5, vw));
        }
        (KeyCode::Up, _, _) => selectable_move(state, history, shift, |s| s.soft_up(vw)),
        (KeyCode::Down, _, _) => selectable_move(state, history, shift, |s| s.soft_down(vw)),

        // ── Page / buffer jumps ─────────────────────────────────────────────
        (KeyCode::PageUp | KeyCode::Home, true, _) => {
            selectable_move(state, history, shift, |s| s.goto_buffer_start());
        }
        (KeyCode::PageDown | KeyCode::End, true, _) => {
            selectable_move(state, history, shift, |s| s.goto_buffer_end(vw));
        }
        (KeyCode::PageUp, _, _) => {
            let page = view::vertical_layout(ctx.cols, ctx.rows).2;
            selectable_move(state, history, shift, |s| s.soft_up_n(page, vw));
        }
        (KeyCode::PageDown, _, _) => {
            let page = view::vertical_layout(ctx.cols, ctx.rows).2;
            selectable_move(state, history, shift, |s| s.soft_down_n(page, vw));
        }
        (KeyCode::Home, _, _) => selectable_move(state, history, shift, |s| s.smart_home(vw)),
        (KeyCode::End, _, _) => selectable_move(state, history, shift, |s| s.goto_line_end(vw)),

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

/// Map a mouse event at screen `(column, mrow)` to buffer `(row, byte_col)`
/// using the same layout as the current render.
fn screen_to_buffer_pos(
    state: &EditorState,
    ctx: &EditorContext<'_>,
    vw: usize,
    content_left: usize,
    column: u16,
    mrow: u16,
) -> (usize, usize) {
    let visuals = view::compute_visuals(&state.lines, vw);
    mouse::ScreenToBuffer {
        lines: &state.lines,
        visuals: &visuals,
        screen_x: column,
        screen_y: mrow,
        editor_row: ctx.editor_row,
        view_top: state.view_top,
        prompt_width: ctx.prompt_width,
        content_left,
    }
    .resolve()
}

pub(super) fn handle_mouse(
    ev: MouseEvent,
    state: &mut EditorState,
    history: &mut History,
    clipboard: &mut EditorClipboard,
    click_tracker: &mut ClickTracker,
    ctx: &EditorContext<'_>,
) -> Result<Action> {
    let MouseEvent {
        kind,
        column,
        row: mrow,
        modifiers,
    } = ev;
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let (vw, content_left) = view::editor_text_layout(ctx.cols as usize, ctx.prompt_width);

    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let clicks = click_tracker.click(column, mrow);
            let (r, c) = screen_to_buffer_pos(state, ctx, vw, content_left, column, mrow);
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
                let (r, c) = screen_to_buffer_pos(state, ctx, vw, content_left, column, mrow);
                state.row = r;
                state.col = c;
                state.anchor = None;
                state.insert_str(&pasted, vw);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let (r, c) = screen_to_buffer_pos(state, ctx, vw, content_left, column, mrow);
            state.goto(r, c, vw);
            history.break_run();
        }
        MouseEventKind::Up(MouseButton::Left)
            if state.anchor == Some((state.row, state.col)) =>
        {
            state.anchor = None;
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let (vwm, av, vlen) =
                view::layout_metrics_for_buffer(&state.lines, ctx.prompt_width, ctx.cols, ctx.rows);
            if vlen > av {
                if matches!(kind, MouseEventKind::ScrollUp) {
                    state.soft_up_n(3, vwm);
                } else {
                    state.soft_down_n(3, vwm);
                }
                state.anchor = None;
                history.break_run();
            }
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
    let vw = view::editor_text_layout(ctx.cols as usize, ctx.prompt_width).0;
    history.record(state.snapshot(), OpKind::Other);
    state.insert_str(&pasted, vw);
    Ok(Action::Continue)
}
