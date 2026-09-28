//! Rendering of the alternate-screen editor: soft-wrap visuals, selection
//! and date-header coloring, footer with scroll indicators and dirty marker.

use anyhow::Result;
use colored::*;
use crossterm::{
    QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    style::Print,
    terminal::{Clear, ClearType, size},
};
use std::io::{self, Write};

use super::text_ops;
use crate::config::theme;

/// Terminal size with a single (80, 24) fallback shared by every caller, so
/// layout helpers and [`render`] can never diverge on fallback geometry. A
/// size of zero is no size: a pty nobody set a window size on reports 0x0,
/// and laying text out in it gave one-cell rows under the footer.
pub(super) fn term_size() -> (u16, u16) {
    fallback_size(size().ok())
}

fn fallback_size(size: Option<(u16, u16)>) -> (u16, u16) {
    match size {
        Some((cols, rows)) if cols > 0 && rows > 0 => (cols, rows),
        _ => (80, 24),
    }
}

/// The hotkey hint, longest first: the footer shows the first one that fits
/// next to the dirty glyph.
const ML_FOOTER_VARIANTS: [&str; 3] = [
    "^S save  ·  ^G help  ·  Esc cancel",
    "^S save · ^G help · Esc cancel",
    "^S · ^G · Esc",
];
/// Cells the dirty glyph takes after the hint: a space and the glyph.
const STATUS_GLYPH_W: usize = 2;
/// Lower the footer vs the prior 5-row band (wide: less padding under footer; compact: more gap above).
const ML_FOOTER_SHIFT_DOWN: usize = 2;
/// Blank full rows between the footer line and the bottom row of the terminal (wide layout only).
const ML_FOOTER_FROM_BOTTOM: usize = 5 - ML_FOOTER_SHIFT_DOWN;
/// Blank lines above the editor text (content starts at this row) when layout allows.
pub(super) const ML_TOP_MARGIN: u16 = 5;
/// Below this width, skip the tall decorated vertical block (footer docked, top margin).
const MIN_TERM_COLS_FOR_MARGINS: u16 = 50;
/// Minimum text rows that must fit for top/bottom decorative spacing; otherwise use compact.
const MIN_TEXT_ROWS_FOR_VERTICAL_DECOR: usize = 7;
/// Max character columns for the editable text (soft-wrap width) on wide terminals.
const ML_MAX_TEXT_COLS: usize = 80;
/// Minimum blank columns reserved on each side of the full line block (prompt + text).
const ML_MIN_HPAD: usize = 2;
/// Minimum one blank row from the top of the terminal to the first text line (compact layout).
const ML_MIN_VPAD_TOP: u16 = 1;
/// Minimum one full blank text row between the last text line and the footer line.
const ML_MIN_VPAD_TEXT_TO_FOOTER: usize = 1;
/// Blank full row between the last text line and the footer in wide (docked) layout.
const ML_VPAD_TEXT_TO_FOOTER_WIDE: usize = 1;

/// Soft-wrap width and left padding to center the text block in the terminal.
/// `(editor_row, footer_gap, available_text)`: on wide terminals tall enough to show
/// at least [`MIN_TEXT_ROWS_FOR_VERTICAL_DECOR`] text rows with margins, text starts
/// at `ML_TOP_MARGIN`, the footer is docked in [`render`], and `footer_gap` is 0 (spacing is
/// reserved in the row budget via [`ML_VPAD_TEXT_TO_FOOTER_WIDE`]). Otherwise a compact layout
/// with at least one empty row at the top and one above the footer when the height allows; very
/// short terminals use best-effort.
pub(super) fn vertical_layout(term_cols: u16, term_rows: u16) -> (u16, usize, usize) {
    let t = term_rows as usize;
    if term_cols < MIN_TERM_COLS_FOR_MARGINS {
        return compact_vertical_layout(t);
    }
    let v = ML_TOP_MARGIN as usize;
    // Rows: v + text + gap + footer + ML_FOOTER_FROM_BOTTOM (blank to bottom)
    let a = t.saturating_sub(v + ML_VPAD_TEXT_TO_FOOTER_WIDE + 1 + ML_FOOTER_FROM_BOTTOM);
    if a < MIN_TEXT_ROWS_FOR_VERTICAL_DECOR {
        return compact_vertical_layout(t);
    }
    (ML_TOP_MARGIN, 0, a.max(1))
}

/// One row from the top to the first line, one between the last line and the footer, one for the
/// footer when `t >= 4`. For `t == 3` one text row fits with top margin but no line above the
/// footer. Row budget: `1 + av + 1 + 1` = `t` for `t >= 4`.
fn compact_vertical_layout(t: usize) -> (u16, usize, usize) {
    if t < 3 {
        return (0, 0, 1);
    }
    if t < 4 {
        return (ML_MIN_VPAD_TOP, 0, 1);
    }
    let gap = ML_MIN_VPAD_TEXT_TO_FOOTER + ML_FOOTER_SHIFT_DOWN;
    let av = t.saturating_sub(ML_MIN_VPAD_TOP as usize + gap + 1);
    (ML_MIN_VPAD_TOP, gap, av.max(1))
}

fn compute_footer_row(
    term_rows: usize,
    term_cols: usize,
    editor_row: u16,
    footer_gap: usize,
    available_text: usize,
    view_top: usize,
    visuals_len: usize,
) -> u16 {
    let visible_count = available_text.min(visuals_len.saturating_sub(view_top));
    let t = term_rows;
    let dock_footer =
        term_cols >= MIN_TERM_COLS_FOR_MARGINS as usize && editor_row == ML_TOP_MARGIN;
    if dock_footer {
        t.saturating_sub(1)
            .saturating_sub(ML_FOOTER_FROM_BOTTOM)
            .max(editor_row as usize) as u16
    } else {
        let content_end_row = editor_row as usize + visible_count;
        let default_footer_row = t.saturating_sub(1);
        let floating_footer_row = content_end_row + footer_gap;
        floating_footer_row
            .min(default_footer_row)
            .max(editor_row as usize) as u16
    }
}

/// Row of the help footer, matching the current [`render`] layout.
pub(super) fn footer_row_for_state(
    lines: &[String],
    view_top: usize,
    prompt_width: usize,
    term_size: (u16, u16),
) -> u16 {
    let (term_cols_u16, term_rows_u16) = term_size;
    let term_rows = term_rows_u16 as usize;
    let term_cols = term_cols_u16 as usize;
    let (editor_row, footer_gap, available_text) = vertical_layout(term_cols_u16, term_rows_u16);
    let (visible_width, _) = editor_text_layout(term_cols, prompt_width);
    let visuals = compute_visuals(lines, visible_width);
    compute_footer_row(
        term_rows,
        term_cols,
        editor_row,
        footer_gap,
        available_text,
        view_top,
        visuals.len(),
    )
}

/// Row for the "discard changes?" prompt when the band below the footer is non-empty: centered
/// between the footer line and the bottom of the window. `None` when the footer sits on the last
/// row (no gap); the caller should draw the dialog on the last line, replacing the footer.
pub(super) fn discard_dialog_row(footer_row: u16, term_rows: u16) -> Option<u16> {
    let last = term_rows.saturating_sub(1);
    if footer_row < last {
        let span = (last - footer_row) as usize;
        let off = 1 + span.saturating_sub(1) / 2;
        Some(((footer_row as usize) + off).min(last as usize) as u16)
    } else {
        None
    }
}

/// Soft-wrap width and horizontal offset to center the full line block (prompt + text) in the terminal.
pub(super) fn editor_text_layout(term_cols: usize, prompt_width: usize) -> (usize, usize) {
    // Center the text column (not the prompt+text block) so the blank gap on the left of
    // the text equals the gap on the right. The prompt sits inside the left margin, so the
    // reservation on each side must be at least `prompt_width` (plus `ML_MIN_HPAD`).
    let side = prompt_width.max(ML_MIN_HPAD);
    let visible_width = term_cols
        .saturating_sub(2 * side)
        .clamp(1, ML_MAX_TEXT_COLS);
    let text_left = term_cols.saturating_sub(visible_width) / 2;
    let content_left = text_left.saturating_sub(prompt_width);
    (visible_width, content_left)
}

/// A soft-wrapped visual row: the buffer line it belongs to and the byte
/// range it covers inside that line.
pub(super) type VisualRow = (usize, std::ops::Range<usize>);

/// Split buffer lines into soft-wrapped visual rows, measured in terminal
/// cells: `vw` is a cell budget, not a count of `char`s, and a row is only
/// ever cut between grapheme clusters. Wrapping is word-aware: a row breaks
/// after the last whitespace that fits, so words move to the next row whole;
/// a single word wider than `vw` is hard-broken. Rows always partition the
/// line exactly (every byte belongs to one row), which the cursor and mouse
/// mapping rely on.
/// `vw` must be at least 1 (guaranteed by `editor_text_layout`).
pub(super) fn compute_visuals(lines: &[String], vw: usize) -> Vec<VisualRow> {
    let mut out: Vec<VisualRow> = Vec::new();
    for (buf_idx, line) in lines.iter().enumerate() {
        if line.is_empty() {
            out.push((buf_idx, 0..0));
            continue;
        }
        let mut start = 0usize; // byte offset of the current row start
        while start < line.len() {
            let rest = &line[start..];
            let fits = crate::width::prefix_within(rest, vw);
            if start + fits == line.len() {
                break;
            }
            // The first cluster that does not fit decides the break: if it is
            // whitespace the full window ends on a word boundary; otherwise
            // break after the last whitespace inside the window.
            let next_is_space = rest[fits..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
            let brk = if next_is_space {
                fits
            } else {
                last_space_end(&rest[..fits]).unwrap_or(fits) // one long word: hard break
            };
            out.push((buf_idx, start..start + brk));
            start += brk;
        }
        out.push((buf_idx, start..line.len()));
    }
    out
}

/// Byte offset just past the last whitespace char in `s`, if any.
fn last_space_end(s: &str) -> Option<usize> {
    s.char_indices()
        .rfind(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
}

/// Visual `(row index in visuals, column in cells)` of a buffer position
/// given by its byte offset in the line. A position on a row boundary
/// belongs to the next visual row (the cursor shows at its start), except at
/// the end of the buffer line where it stays on the last row of that line.
pub(super) fn visual_pos(
    visuals: &[VisualRow],
    lines: &[String],
    row: usize,
    col_byte: usize,
) -> (usize, usize) {
    let mut last_of_row: Option<(usize, usize)> = None;
    for (vi, (buf_idx, range)) in visuals.iter().enumerate() {
        if *buf_idx != row {
            continue;
        }
        if col_byte >= range.start && col_byte < range.end {
            return (vi, crate::width::cells_before(&lines[row][range.clone()], col_byte - range.start));
        }
        last_of_row = Some((vi, range.start));
    }
    match last_of_row {
        Some((vi, start)) => (
            vi,
            crate::width::cells_before(&lines[row][start..], col_byte.saturating_sub(start)),
        ),
        None => (0, 0),
    }
}

/// Visual column (in cells) of a buffer position under the current wrap (the
/// value preserved as the desired column for vertical movement).
pub(super) fn visual_col(lines: &[String], vw: usize, row: usize, col_byte: usize) -> usize {
    let visuals = compute_visuals(lines, vw);
    visual_pos(&visuals, lines, row, col_byte).1
}

/// One visual row up (`up == true`) or down, keeping `desired_vis_col` when
/// the target row is long enough. Returns the position unchanged at the
/// buffer edges.
pub(super) fn soft_vertical_move(
    lines: &[String],
    vw: usize,
    row: usize,
    col_byte: usize,
    desired_vis_col: usize,
    up: bool,
) -> (usize, usize) {
    let visuals = compute_visuals(lines, vw);
    let (vi, _) = visual_pos(&visuals, lines, row, col_byte);
    let target = if up {
        vi.checked_sub(1)
    } else if vi + 1 < visuals.len() {
        Some(vi + 1)
    } else {
        None
    };
    let Some(ti) = target else {
        return (row, col_byte);
    };
    let (buf_idx, range) = &visuals[ti];
    let chunk = &lines[*buf_idx][range.clone()];
    let mut in_chunk = crate::width::byte_at_cell(chunk, desired_vis_col);
    // On the last row of a line the cursor may sit past the final cluster;
    // on wrapped rows it must stay before the boundary (the boundary
    // position renders on the following row).
    let is_last_of_line = visuals
        .get(ti + 1)
        .is_none_or(|(next_buf, _)| next_buf != buf_idx);
    if !is_last_of_line && in_chunk >= chunk.len() && !chunk.is_empty() {
        in_chunk = crate::width::cluster_start(chunk, chunk.len() - 1);
    }
    (*buf_idx, range.start + in_chunk)
}

/// Print one visual chunk with selection + optional validation colors and
/// a bold date highlight (green, or red if the leading date is before today) on the first line.
#[allow(clippy::too_many_arguments)]
fn print_visual_chunk(
    stdout: &mut io::Stdout,
    lines: &[String],
    buf_idx: usize,
    start_byte: usize,
    content: &str,
    sel: Option<((usize, usize), (usize, usize))>,
    date_len: usize,
    date_past: bool,
) -> Result<()> {
    let chunk_end = start_byte + content.len();

    let sel_line_range: Option<(usize, usize)> = sel.and_then(|(s, e)| {
        if buf_idx < s.0 || buf_idx > e.0 {
            return None;
        }
        let start_in_line = if buf_idx == s.0 { s.1 } else { 0 };
        let end_in_line = if buf_idx == e.0 {
            e.1
        } else {
            lines[buf_idx].len()
        };
        if start_in_line >= end_in_line {
            return None;
        }
        Some((start_in_line, end_in_line))
    });

    let sel_in_chunk: Option<(usize, usize)> = sel_line_range.and_then(|(s, e)| {
        let a = s.max(start_byte);
        let b = e.min(chunk_end);
        if a >= b {
            None
        } else {
            Some((a - start_byte, b - start_byte))
        }
    });

    let date_end_in_chunk: usize = if buf_idx == 0 {
        date_len.saturating_sub(start_byte).min(content.len())
    } else {
        0
    };
    let date_past = buf_idx == 0 && date_past;

    let emit = |stdout: &mut io::Stdout, text: &str, selected: bool, is_date: bool| -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        if selected {
            stdout.queue(Print(text.reversed()))?;
        } else if is_date {
            if date_past {
                stdout.queue(Print(theme().date_overdue.paint(text).bold()))?;
            } else {
                stdout.queue(Print(theme().editor_date.paint(text).bold()))?;
            }
        } else {
            stdout.queue(Print(text))?;
        }
        Ok(())
    };

    for (range, selected, is_date) in
        chunk_runs(content.len(), sel_in_chunk, date_end_in_chunk)
    {
        emit(stdout, &content[range], selected, is_date)?;
    }
    Ok(())
}

/// Break a visual row at the date / selection boundaries so each run is
/// homogeneous and can be printed with a single color: `(byte range in the
/// row, selected, is date)`. Every boundary the caller hands in is a char
/// boundary, so slicing the row by these ranges is always valid.
fn chunk_runs(
    chunk_len: usize,
    sel_in_chunk: Option<(usize, usize)>,
    date_end_in_chunk: usize,
) -> Vec<(std::ops::Range<usize>, bool, bool)> {
    let mut stops: Vec<usize> = vec![0, chunk_len];
    if date_end_in_chunk > 0 && date_end_in_chunk < chunk_len {
        stops.push(date_end_in_chunk);
    }
    if let Some((a, b)) = sel_in_chunk {
        if a > 0 && a < chunk_len {
            stops.push(a);
        }
        if b > 0 && b < chunk_len {
            stops.push(b);
        }
    }
    stops.sort_unstable();
    stops.dedup();

    let mut runs = Vec::new();
    for i in 1..stops.len() {
        let (a, b) = (stops[i - 1], stops[i]);
        if a == b {
            continue;
        }
        let selected = match sel_in_chunk {
            Some((ss, se)) => a >= ss && b <= se,
            None => false,
        };
        let is_date = date_end_in_chunk > 0 && b <= date_end_in_chunk;
        runs.push((a..b, selected, is_date));
    }
    runs
}

/// Paint the footer line: centered help text, scroll arrows in the left gap,
/// and the dirty/clean glyph to the right of the text block.
#[allow(clippy::too_many_arguments)]
fn render_footer(
    stdout: &mut io::Stdout,
    footer_row: u16,
    term_cols: usize,
    text_left: usize,
    text_right_excl: usize,
    has_up: bool,
    has_down: bool,
    dirty: bool,
) -> Result<()> {
    stdout.queue(MoveTo(0, footer_row))?;
    stdout.queue(Clear(ClearType::CurrentLine))?;
    let footer_text = footer_hint(term_cols);
    let footer_width = crate::width::width(footer_text);
    let mut footer_x = term_cols.saturating_sub(footer_width) / 2;

    // The glyph goes between the hint and the right edge of the text when they leave a gap; otherwise
    // hint and glyph are centered together, so the glyph never lands on a
    // letter of the hint.
    let footer_end = footer_x + footer_width;
    let status_x = if footer_end < text_right_excl {
        (footer_end + text_right_excl - 1) / 2
    } else {
        footer_x = term_cols.saturating_sub(footer_width + STATUS_GLYPH_W) / 2;
        footer_x + footer_width + STATUS_GLYPH_W - 1
    }
    .min(term_cols.saturating_sub(1));

    stdout.queue(MoveTo(footer_x as u16, footer_row))?;
    stdout.queue(Print(theme().editor_footer.paint(footer_text)))?;

    if has_up || has_down {
        // Two fixed columns: down (left), up (right). Order and placement never swap.
        const ARROW_PAIR_W: usize = 2;
        if footer_x > text_left + ARROW_PAIR_W.saturating_sub(1) {
            let arrow_start = text_left + (footer_x - text_left - ARROW_PAIR_W) / 2;
            stdout.queue(MoveTo(
                arrow_start.min(u16::MAX as usize) as u16,
                footer_row,
            ))?;
            if has_down {
                stdout.queue(Print(theme().editor_footer.paint("↓")))?;
            } else {
                stdout.queue(Print(" "))?;
            }
            stdout.queue(MoveTo(
                (arrow_start + 1).min(u16::MAX as usize) as u16,
                footer_row,
            ))?;
            if has_up {
                stdout.queue(Print(theme().editor_footer.paint("↑")))?;
            } else {
                stdout.queue(Print(" "))?;
            }
        }
    }

    stdout.queue(MoveTo(status_x as u16, footer_row))?;
    if dirty {
        stdout.queue(Print(theme().editor_dirty.paint("●")))?;
    } else {
        stdout.queue(Print(theme().editor_clean.paint("○")))?;
    }
    Ok(())
}

/// The longest hotkey hint that fits into `term_cols` beside the dirty
/// glyph; on a terminal too narrow even for the shortest, as much of it as
/// fits.
fn footer_hint(term_cols: usize) -> &'static str {
    let room = term_cols.saturating_sub(STATUS_GLYPH_W);
    ML_FOOTER_VARIANTS
        .iter()
        .find(|v| crate::width::width(v) <= room)
        .copied()
        .unwrap_or_else(|| {
            let shortest = ML_FOOTER_VARIANTS[ML_FOOTER_VARIANTS.len() - 1];
            // `prefix_within` always keeps one cluster; no room means none.
            let take = if room == 0 { 0 } else { crate::width::prefix_within(shortest, room) };
            // Cut after a whole key, not on a dangling separator.
            shortest[..take].trim_end_matches([' ', '·'])
        })
}

/// First visual row to show: the current one, brought to the cursor when
/// the view follows it, and never past the point where the text ends above
/// the bottom of the window (a view scrolled in a small window must not stay
/// scrolled once the window grows back).
pub(super) fn settle_view_top(
    view_top: usize,
    cursor_vis_row: usize,
    follow_cursor: bool,
    available_text: usize,
    visuals_len: usize,
) -> usize {
    let mut top = view_top;
    if follow_cursor {
        if cursor_vis_row < top {
            top = cursor_vis_row;
        }
        if cursor_vis_row >= top.saturating_add(available_text) {
            top = cursor_vis_row + 1 - available_text;
        }
    }
    top.min(visuals_len.saturating_sub(available_text))
}

pub(super) struct RenderInput<'a> {
    pub prompt: &'a str,
    pub prompt_width: usize,
    pub term_size: (u16, u16),
    pub first_line_colored: Option<&'a str>,
    pub lines: &'a [String],
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub view_top: &'a mut usize,
    pub follow_cursor: bool,
    pub selection: Option<((usize, usize), (usize, usize))>,
    pub dirty: bool,
    pub relative_date_base: Option<chrono::NaiveDate>,
}

pub(super) fn render(stdout: &mut io::Stdout, r: RenderInput<'_>) -> Result<()> {
    let (term_cols_u16, term_rows_u16) = r.term_size;
    let term_cols = term_cols_u16 as usize;
    let (editor_row, footer_gap, available_text) = vertical_layout(term_cols_u16, term_rows_u16);
    let (visible_width, content_left) = editor_text_layout(term_cols, r.prompt_width);
    let content_left_u16 = content_left.min(u16::MAX as usize) as u16;

    let visuals = compute_visuals(r.lines, visible_width);

    let (cursor_vis_row, cursor_vis_col) =
        visual_pos(&visuals, r.lines, r.cursor_row, r.cursor_col);

    *r.view_top = settle_view_top(
        *r.view_top,
        cursor_vis_row,
        r.follow_cursor,
        available_text,
        visuals.len(),
    );

    // Clear from the top; text starts at `editor_row` (vertically centered when width allows).
    stdout.queue(MoveTo(0, 0))?;
    stdout.queue(Clear(ClearType::FromCursorDown))?;

    let sel_range = r.selection.map(|(a, b)| {
        if text_ops::pos_cmp(a, b) != std::cmp::Ordering::Greater {
            (a, b)
        } else {
            (b, a)
        }
    });

    // Parse the leading date token once per frame; it is constant across chunks.
    let ld = text_ops::leading_date(&r.lines[0], r.relative_date_base);
    let date_len = ld.map_or(0, |(n, _)| n);
    let date_past = ld.is_some_and(|(_, d)| d < chrono::Local::now().date_naive());

    let visible_count = available_text.min(visuals.len().saturating_sub(*r.view_top));
    let pad: String = " ".repeat(r.prompt_width);
    for v_i in 0..visible_count {
        let (buf_idx, range) = &visuals[*r.view_top + v_i];
        let content = &r.lines[*buf_idx][range.start..range.end];
        stdout.queue(MoveTo(content_left_u16, editor_row + v_i as u16))?;
        if *buf_idx == 0 && range.start == 0 {
            if let Some(colored) = r.first_line_colored {
                stdout.queue(Print(colored))?;
            } else {
                stdout.queue(Print(r.prompt))?;
            }
        } else {
            stdout.queue(Print(&pad))?;
        }

        print_visual_chunk(
            stdout,
            r.lines,
            *buf_idx,
            range.start,
            content,
            sel_range,
            date_len,
            date_past,
        )?;
    }

    // Wide: footer is fixed ML_FOOTER_FROM_BOTTOM full rows above the last line; narrow: under last text.
    let footer_row = compute_footer_row(
        term_rows_u16 as usize,
        term_cols,
        editor_row,
        footer_gap,
        available_text,
        *r.view_top,
        visuals.len(),
    );
    let text_left = content_left;
    let text_right_excl = content_left + r.prompt_width + visible_width;
    let has_up = *r.view_top > 0;
    let has_down = *r.view_top + available_text < visuals.len();
    // A one-row window has room for the text or the footer, not both.
    if term_rows_u16 >= 2 {
        render_footer(
            stdout,
            footer_row,
            term_cols,
            text_left,
            text_right_excl,
            has_up,
            has_down,
            r.dirty,
        )?;
    }

    // The wheel may have left the cursor outside the view: then it is not
    // drawn anywhere, rather than on the footer or the top margin.
    let on_screen = cursor_vis_row >= *r.view_top && cursor_vis_row < *r.view_top + visible_count;
    if on_screen {
        let cur_visual_row_on_screen = cursor_vis_row - *r.view_top;
        let mut x = (content_left + r.prompt_width + cursor_vis_col) as u16;
        if term_cols_u16 > 0 && x >= term_cols_u16 {
            x = term_cols_u16.saturating_sub(1);
        }
        let y = editor_row.saturating_add(cur_visual_row_on_screen as u16);
        stdout.queue(MoveTo(x, y))?;
        stdout.queue(Show)?;
    } else {
        stdout.queue(Hide)?;
    }
    stdout.flush().ok();
    Ok(())
}

/// Same `editor_text_layout` + `vertical_layout` + `compute_visuals` as [`render`] for the
/// given terminal size. Use for wheel and other logic that must match on-screen line counts.
pub(super) fn layout_metrics_for_buffer(
    lines: &[String],
    prompt_width: usize,
    cols: u16,
    rows: u16,
) -> (usize, usize, usize) {
    let (_, _, av) = vertical_layout(cols, rows);
    let (vw, _) = editor_text_layout(cols as usize, prompt_width);
    let vlen = compute_visuals(lines, vw).len();
    (vw, av, vlen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Rendered text of every visual row.
    fn rows(lines: &[String], vw: usize) -> Vec<String> {
        compute_visuals(lines, vw)
            .into_iter()
            .map(|(bi, range)| lines[bi][range].to_string())
            .collect()
    }

    /// REVIEW №192: `size()` answers Ok((0, 0)) on a pty without a window
    /// size, and only an error used to fall back.
    #[test]
    fn a_zero_size_falls_back() {
        assert_eq!(fallback_size(Some((0, 0))), (80, 24));
        assert_eq!(fallback_size(Some((80, 0))), (80, 24));
        assert_eq!(fallback_size(None), (80, 24));
        assert_eq!(fallback_size(Some((30, 6))), (30, 6));
    }

    /// REVIEW №193 and №190: the view is clamped to the text on every
    /// frame (a window that grows back shows text again), and it only
    /// chases the cursor while it follows it.
    #[test]
    fn the_view_settles_inside_the_text() {
        // 60 rows, 10 on screen: the last page starts at 50.
        assert_eq!(settle_view_top(59, 59, true, 10, 60), 50);
        // The window grew to 30 rows: the view comes back down to the text.
        assert_eq!(settle_view_top(59, 59, true, 30, 60), 30);
        assert_eq!(settle_view_top(5, 3, true, 100, 60), 0);
        // Following: the cursor pulls the view either way.
        assert_eq!(settle_view_top(20, 5, true, 10, 60), 5);
        assert_eq!(settle_view_top(0, 25, true, 10, 60), 16);
        // Scrolled away by the wheel: the cursor stays out of sight.
        assert_eq!(settle_view_top(20, 5, false, 10, 60), 20);
        assert_eq!(settle_view_top(55, 5, false, 10, 60), 50);
    }

    /// REVIEW №195: the hint used to be cut at the terminal width, and the
    /// dirty glyph was then drawn over one of its letters.
    #[test]
    fn the_footer_hint_shortens_to_fit_beside_the_glyph() {
        assert_eq!(footer_hint(80), ML_FOOTER_VARIANTS[0]);
        assert_eq!(footer_hint(36), ML_FOOTER_VARIANTS[0]);
        assert_eq!(footer_hint(35), ML_FOOTER_VARIANTS[1]);
        assert_eq!(footer_hint(30), "^S · ^G · Esc");
        assert_eq!(footer_hint(12), "^S · ^G");
        assert_eq!(footer_hint(6), "^S");
        assert_eq!(footer_hint(2), "");
        for cols in 0..40 {
            assert!(crate::width::width(footer_hint(cols)) + STATUS_GLYPH_W <= cols.max(STATUS_GLYPH_W));
        }
    }

    #[test]
    fn wraps_at_word_boundaries() {
        let l = lines(&["hello brave new world"]);
        assert_eq!(rows(&l, 12), vec!["hello brave ", "new world"]);
        assert_eq!(rows(&l, 6), vec!["hello ", "brave ", "new ", "world"]);
    }

    #[test]
    fn word_ending_exactly_at_the_boundary_is_kept_whole() {
        // "aaa bbbb" is exactly 8 chars; the following space breaks the row.
        let l = lines(&["aaa bbbb x"]);
        assert_eq!(rows(&l, 8), vec!["aaa bbbb", " x"]);
    }

    #[test]
    fn long_words_are_hard_broken() {
        let l = lines(&["abcdefghij end"]);
        assert_eq!(rows(&l, 4), vec!["abcd", "efgh", "ij ", "end"]);
    }

    #[test]
    fn chunks_partition_every_line_exactly() {
        let l = lines(&[
            "a few words here",
            "",
            "loooooooooongword and more",
            "日本語のテキスト 🍎🍎🍎 e\u{301}nd",
        ]);
        for vw in 1..=30 {
            let visuals = compute_visuals(&l, vw);
            let mut expected_start = vec![0usize; l.len()];
            for (bi, range) in &visuals {
                assert_eq!(range.start, expected_start[*bi], "vw={vw}");
                let row = &l[*bi][range.clone()];
                // One cluster wider than the whole budget is the only way a
                // row may exceed it.
                let cluster_count = crate::width::clusters(row).count();
                assert!(
                    crate::width::width(row) <= vw.max(1) || cluster_count == 1,
                    "vw={vw}: row {row:?} wider than the limit"
                );
                expected_start[*bi] = range.end;
            }
            for (bi, line) in l.iter().enumerate() {
                assert_eq!(expected_start[bi], line.len(), "vw={vw}: line {bi} not fully covered");
            }
        }
    }

    #[test]
    fn wide_characters_wrap_by_cells() {
        // Eight CJK chars are 16 cells: only five fit a 10-cell row.
        let l = lines(&["日本語のテキスト"]);
        assert_eq!(rows(&l, 10), vec!["日本語のテ", "キスト"]);
        // A cell budget an odd number of cells wide leaves one cell unused.
        assert_eq!(rows(&l, 5), vec!["日本", "語の", "テキ", "スト"]);
    }

    #[test]
    fn clusters_are_never_split() {
        // A family emoji is one cluster of two cells; a base with a
        // combining accent is one cluster of one cell.
        let fam = "👨\u{200d}👩\u{200d}👧";
        let l = vec![format!("{fam}{fam}"), "e\u{301}e\u{301}".to_string()];
        assert_eq!(
            rows(&l, 3),
            vec![fam.to_string(), fam.to_string(), "e\u{301}e\u{301}".to_string()]
        );
        // Even a budget narrower than the cluster keeps it whole.
        assert_eq!(rows(&lines(&[fam]), 1), vec![fam.to_string()]);
    }

    #[test]
    fn empty_lines_get_one_empty_row() {
        assert_eq!(rows(&lines(&["", "x"]), 10), vec!["", "x"]);
    }

    #[test]
    fn visual_pos_maps_cursor_into_wrapped_rows() {
        let l = lines(&["hello brave new world"]);
        let visuals = compute_visuals(&l, 12); // "hello brave " + "new world"
        assert_eq!(visual_pos(&visuals, &l, 0, 0), (0, 0));
        assert_eq!(visual_pos(&visuals, &l, 0, 11), (0, 11));
        // The row boundary belongs to the next visual row.
        assert_eq!(visual_pos(&visuals, &l, 0, 12), (1, 0));
        // End of line stays on the last row, one past its final char.
        assert_eq!(visual_pos(&visuals, &l, 0, 21), (1, 9));
    }

    #[test]
    fn soft_vertical_move_walks_wrapped_rows_and_lines() {
        let l = lines(&["hello brave new world", "tail"]);
        // vw=12: rows are "hello brave " / "new world" / "tail".
        // Down from the first row keeps the desired column.
        assert_eq!(soft_vertical_move(&l, 12, 0, 3, 3, false), (0, 15));
        // Down from the wrapped row crosses into the next buffer line.
        assert_eq!(soft_vertical_move(&l, 12, 0, 15, 3, false), (1, 3));
        // Up from the second buffer line lands on the last wrapped row.
        assert_eq!(soft_vertical_move(&l, 12, 1, 3, 3, true), (0, 15));
        // Edges stay put.
        assert_eq!(soft_vertical_move(&l, 12, 0, 3, 3, true), (0, 3));
        assert_eq!(soft_vertical_move(&l, 12, 1, 3, 3, false), (1, 3));
        // A desired column past the target row clamps inside it.
        assert_eq!(soft_vertical_move(&l, 12, 0, 15, 11, true), (0, 11));
    }

    #[test]
    fn visual_col_is_relative_to_the_wrapped_row() {
        let l = lines(&["hello brave new world"]);
        assert_eq!(visual_col(&l, 12, 0, 3), 3);
        assert_eq!(visual_col(&l, 12, 0, 15), 3); // "new" starts the second row
    }

    #[test]
    fn colored_runs_follow_byte_offsets() {
        // Row "日本語 text": a selection over the three wide chars and a
        // date of the same byte length must not be confused with each other
        // - both are byte offsets into the row now, not char counts.
        let nine = "日本語".len();
        assert_eq!(
            chunk_runs(14, Some((0, nine)), 0),
            vec![(0..nine, true, false), (nine..14, false, false)]
        );
        // A date shorter than the row colors only its own bytes.
        assert_eq!(
            chunk_runs(14, None, nine),
            vec![(0..nine, false, true), (nine..14, false, false)]
        );
        // Selection inside the date: the date splits around it, the tail
        // after the date stays plain.
        assert_eq!(
            chunk_runs(14, Some((3, 6)), nine),
            vec![
                (0..3, false, true),
                (3..6, true, true),
                (6..nine, false, true),
                (nine..14, false, false)
            ]
        );
        // A whole row that is date, and a row with neither.
        assert_eq!(chunk_runs(9, None, 9), vec![(0..9, false, true)]);
        assert_eq!(chunk_runs(9, None, 0), vec![(0..9, false, false)]);
        assert_eq!(chunk_runs(0, None, 0), vec![]);
    }

    #[test]
    fn visual_columns_count_cells_not_chars() {
        let l = lines(&["日本語 text"]);
        let visuals = compute_visuals(&l, 20);
        // Three CJK chars are nine bytes and six cells.
        assert_eq!(visual_pos(&visuals, &l, 0, 9), (0, 6));
        assert_eq!(visual_col(&l, 20, 0, "日本語 text".len()), 11);
        // Vertical movement keeps the cell column, landing on a boundary.
        let two = lines(&["日本語abc", "abcdef"]);
        assert_eq!(soft_vertical_move(&two, 20, 1, 4, 4, true), (0, 6));
        // A column inside a wide char goes in front of it.
        assert_eq!(soft_vertical_move(&two, 20, 1, 3, 3, true), (0, 3));
    }
}
