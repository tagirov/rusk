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
    // A key is about the text at the cursor: the view goes back to it.
    state.follow_cursor = true;
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
                // Copying is not a move: what was selected stays selected,
                // so the next character replaces it the way it would in any
                // other editor — and a second Ctrl+C is still a copy, not
                // the abort that used to throw the text away.
                selection_consumed = true;
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
            // Nothing left once cleaned: no edit, no undo step, and the
            // selection is not replaced by nothing.
            if !text_ops::clean_input(&pasted).is_empty() {
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
        // A page turns the view and the cursor together, so the cursor
        // keeps its place on the screen instead of dragging the view behind.
        (KeyCode::PageUp, _, _) => {
            let (_, page, vlen) =
                view::layout_metrics_for_buffer(&state.lines, ctx.prompt_width, ctx.cols, ctx.rows);
            selectable_move(state, history, shift, |s| {
                s.soft_up_n(page, vw);
                s.scroll_view(-(page as isize), vlen.saturating_sub(page));
            });
        }
        (KeyCode::PageDown, _, _) => {
            let (_, page, vlen) =
                view::layout_metrics_for_buffer(&state.lines, ctx.prompt_width, ctx.cols, ctx.rows);
            selectable_move(state, history, shift, |s| {
                s.soft_down_n(page, vw);
                s.scroll_view(page as isize, vlen.saturating_sub(page));
            });
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
        // Any other control character a terminal reports as one has no place
        // in the buffer (`insert_char` drops it): no undo step for nothing.
        (KeyCode::Char(ch), _, _)
            if crate::printable::is_control(ch) && !matches!(ch, '\t' | '\n' | '\r') =>
        {
            history.break_run();
        }
        (KeyCode::Char(ch), _, _) => {
            // A terminal that reports Tab or a line break as a character
            // gets the key's own undo step too, not one coalesced with the
            // typing around it.
            let kind = if ch.is_control() {
                OpKind::Other
            } else {
                OpKind::InsertChar
            };
            history.record(state.snapshot(), kind);
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
/// using the same layout as the current render. `reach` is how far past the
/// visible rows the event may land (see [`mouse::Reach`]).
fn screen_to_buffer_pos(
    state: &EditorState,
    ctx: &EditorContext<'_>,
    vw: usize,
    content_left: usize,
    column: u16,
    mrow: u16,
    reach: mouse::Reach,
) -> (usize, usize) {
    let visuals = view::compute_visuals(&state.lines, vw);
    mouse::ScreenToBuffer {
        lines: &state.lines,
        visuals: &visuals,
        screen_x: column,
        screen_y: mrow,
        editor_row: ctx.editor_row,
        view_top: state.view_top,
        available_text: view::vertical_layout(ctx.cols, ctx.rows).2,
        prompt_width: ctx.prompt_width,
        content_left,
        reach,
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
    let at = |state: &EditorState, reach| {
        screen_to_buffer_pos(state, ctx, vw, content_left, column, mrow, reach)
    };

    match kind {
        // A click, a drag or a paste puts the cursor under the pointer and
        // the view follows it again (the wheel alone leaves it behind).
        MouseEventKind::Down(MouseButton::Left) => {
            state.follow_cursor = true;
            let clicks = click_tracker.click(column, mrow);
            let (r, c) = at(state, mouse::Reach::Visible);
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
            // Nothing left once cleaned: no edit, no undo step, and the
            // selection is not replaced by nothing.
            if !text_ops::clean_input(&pasted).is_empty() {
                history.record(state.snapshot(), OpKind::Other);
                state.follow_cursor = true;
                let (r, c) = at(state, mouse::Reach::Visible);
                state.row = r;
                state.col = c;
                state.anchor = None;
                state.insert_str(&pasted, vw);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            // Dragging past an edge selects one row beyond it; the view
            // follows the cursor there, so it scrolls a row per event.
            let (r, c) = at(state, mouse::Reach::OneBeyond);
            state.follow_cursor = true;
            state.goto(r, c, vw);
            history.break_run();
        }
        MouseEventKind::Up(MouseButton::Left)
            if state.anchor == Some((state.row, state.col)) =>
        {
            state.anchor = None;
        }
        // The wheel scrolls the view and nothing else: the cursor and the
        // selection stay put, even out of sight, until a key brings the view
        // back to them.
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            const WHEEL_ROWS: isize = 3;
            let (_, av, vlen) =
                view::layout_metrics_for_buffer(&state.lines, ctx.prompt_width, ctx.cols, ctx.rows);
            if vlen > av {
                let delta = if matches!(kind, MouseEventKind::ScrollUp) {
                    -WHEEL_ROWS
                } else {
                    WHEEL_ROWS
                };
                state.follow_cursor = false;
                state.scroll_view(delta, vlen - av);
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
    if text_ops::clean_input(&pasted).is_empty() {
        return Ok(Action::Continue);
    }
    history.record(state.snapshot(), OpKind::Other);
    state.follow_cursor = true;
    state.insert_str(&pasted, vw);
    Ok(Action::Continue)
}
