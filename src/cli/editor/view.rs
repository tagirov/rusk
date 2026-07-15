//! Rendering of the alternate-screen editor: soft-wrap visuals, selection
//! and date-header coloring, footer with scroll indicators and dirty marker.

use anyhow::Result;
use colored::*;
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    style::Print,
    terminal::{Clear, ClearType, size},
};
use std::io::{self, Write};

use super::text_ops;
use crate::config::theme;

/// Terminal size with a single (80, 24) fallback shared by every caller, so
/// layout helpers and [`render`] can never diverge on fallback geometry.
pub(super) fn term_size() -> (u16, u16) {
    size().unwrap_or((80, 24))
}

pub(super) const ML_FOOTER: &str = "^S save  ·  ^G help  ·  Esc cancel";
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

/// Split buffer lines into soft-wrapped visual rows. Each tuple is
/// `(buffer_idx, byte_range_in_buffer_line, start_char_offset_in_buffer_line)`.
/// Wrapping is word-aware: a row breaks after the last whitespace that fits,
/// so words move to the next row whole; a single word longer than `vw` is
/// hard-broken. Chunks always partition the line exactly (every char belongs
/// to one row), which the cursor and mouse mapping rely on.
/// `vw` must be at least 1 (guaranteed by `editor_text_layout`).
pub(super) fn compute_visuals(
    lines: &[String],
    vw: usize,
) -> Vec<(usize, std::ops::Range<usize>, usize)> {
    let mut out: Vec<(usize, std::ops::Range<usize>, usize)> = Vec::new();
    for (buf_idx, line) in lines.iter().enumerate() {
        if line.is_empty() {
            out.push((buf_idx, 0..0, 0));
            continue;
        }
        // (byte offset, char) pairs so break positions map back to bytes.
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let total = chars.len();
        let mut start = 0usize; // char index of the current row start
        while total - start > vw {
            // The first char that does not fit decides the break: if it is
            // whitespace the full window ends on a word boundary; otherwise
            // break after the last whitespace inside the window.
            let brk = if chars[start + vw].1.is_whitespace() {
                vw
            } else {
                (0..vw)
                    .rev()
                    .find(|i| chars[start + *i].1.is_whitespace())
                    .map(|i| i + 1)
                    .unwrap_or(vw) // one long word: hard break
            };
            let end = start + brk;
            out.push((buf_idx, chars[start].0..chars[end].0, start));
            start = end;
        }
        out.push((buf_idx, chars[start].0..line.len(), start));
    }
    out
}

/// Visual `(row index in visuals, column in chars)` of a buffer position.
/// A position on a row boundary belongs to the next visual row (the cursor
/// shows at its start), except at the end of the buffer line where it stays
/// on the last row of that line.
pub(super) fn visual_pos(
    visuals: &[(usize, std::ops::Range<usize>, usize)],
    lines: &[String],
    row: usize,
    char_col: usize,
) -> (usize, usize) {
    let mut last_of_row: Option<(usize, usize)> = None;
    for (vi, (buf_idx, range, start_char)) in visuals.iter().enumerate() {
        if *buf_idx != row {
            continue;
        }
        let chunk_len = lines[row][range.clone()].chars().count();
        if char_col >= *start_char && char_col < start_char + chunk_len {
            return (vi, char_col - start_char);
        }
        last_of_row = Some((vi, *start_char));
    }
    match last_of_row {
        Some((vi, start_char)) => (vi, char_col.saturating_sub(start_char)),
        None => (0, 0),
    }
}

/// Visual column of a buffer position under the current wrap (the value
/// preserved as the desired column for vertical movement).
pub(super) fn visual_col(lines: &[String], vw: usize, row: usize, col_byte: usize) -> usize {
    let visuals = compute_visuals(lines, vw);
    let char_col = text_ops::byte_idx_to_char_count(&lines[row], col_byte);
    visual_pos(&visuals, lines, row, char_col).1
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
    let char_col = text_ops::byte_idx_to_char_count(&lines[row], col_byte);
    let (vi, _) = visual_pos(&visuals, lines, row, char_col);
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
    let (buf_idx, range, start_char) = &visuals[ti];
    let chunk_len = lines[*buf_idx][range.clone()].chars().count();
    // On the last row of a line the cursor may sit past the final char;
    // on wrapped rows it must stay before the boundary (the boundary
    // position renders on the following row).
    let is_last_of_line = visuals
        .get(ti + 1)
        .is_none_or(|(next_buf, _, _)| next_buf != buf_idx);
    let max_col = if is_last_of_line {
        chunk_len
    } else {
        chunk_len.saturating_sub(1)
    };
    let target_char = start_char + desired_vis_col.min(max_col);
    (*buf_idx, text_ops::ml_char_to_byte(&lines[*buf_idx], target_char))
}

/// Print one visual chunk with selection + optional validation colors and
/// a bold date highlight (green, or red if the leading date is before today) on the first line.
#[allow(clippy::too_many_arguments)]
fn print_visual_chunk(
    stdout: &mut io::Stdout,
    lines: &[String],
    buf_idx: usize,
    start_char: usize,
    content: &str,
    sel: Option<((usize, usize), (usize, usize))>,
    full_text_valid: Option<bool>,
    date_len: usize,
    date_past: bool,
) -> Result<()> {
    let chunk_len = content.chars().count();
    let chunk_end = start_char + chunk_len;

    let sel_line_range: Option<(usize, usize)> = sel.and_then(|(s, e)| {
        let line_chars = lines[buf_idx].chars().count();
        if buf_idx < s.0 || buf_idx > e.0 {
            return None;
        }
        let start_char_in_line = if buf_idx == s.0 {
            text_ops::byte_idx_to_char_count(&lines[buf_idx], s.1)
        } else {
            0
        };
        let end_char_in_line = if buf_idx == e.0 {
            text_ops::byte_idx_to_char_count(&lines[buf_idx], e.1)
        } else {
            line_chars
        };
        if start_char_in_line >= end_char_in_line {
            return None;
        }
        Some((start_char_in_line, end_char_in_line))
    });

    let sel_in_chunk: Option<(usize, usize)> = sel_line_range.and_then(|(s, e)| {
        let a = s.max(start_char);
        let b = e.min(chunk_end);
        if a >= b {
            None
        } else {
            Some((a - start_char, b - start_char))
        }
    });

    let date_end_in_chunk: usize = if buf_idx == 0 {
        date_len.saturating_sub(start_char).min(chunk_len)
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
            match full_text_valid {
                Some(true) => stdout.queue(Print(theme().success.paint(text)))?,
                Some(false) => stdout.queue(Print(theme().error.paint(text)))?,
                _ => stdout.queue(Print(text))?,
            };
        }
        Ok(())
    };

    // Break the chunk at date / selection boundaries so each run is
    // homogeneous and can be printed with a single color.
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

    // Map sorted char stops to byte offsets in a single pass over the chunk.
    let mut byte_stops: Vec<usize> = vec![content.len(); stops.len()];
    let mut si = 0usize;
    for (ci, (bi, _)) in content.char_indices().enumerate() {
        if si >= stops.len() {
            break;
        }
        if stops[si] == ci {
            byte_stops[si] = bi;
            si += 1;
        }
    }

    for i in 1..stops.len() {
        let (a, b) = (stops[i - 1], stops[i]);
        if a == b {
            continue;
        }
        let text = &content[byte_stops[i - 1]..byte_stops[i]];
        let selected = match sel_in_chunk {
            Some((ss, se)) => a >= ss && b <= se,
            None => false,
        };
        let is_date = date_end_in_chunk > 0 && b <= date_end_in_chunk;
        emit(stdout, text, selected, is_date)?;
    }
    Ok(())
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
    let mut footer_text = ML_FOOTER.to_string();
    if footer_text.chars().count() > term_cols.max(1) {
        footer_text = footer_text.chars().take(term_cols.max(1)).collect();
    }
    let footer_width = footer_text.chars().count();
    let footer_x = term_cols.saturating_sub(footer_width) / 2;
    let footer_end = footer_x + footer_width;
    stdout.queue(MoveTo(footer_x as u16, footer_row))?;
    stdout.queue(Print(theme().editor_footer.paint(&footer_text)))?;

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

    let status_x = if text_right_excl < footer_x {
        let lo = text_right_excl;
        let hi = footer_x.saturating_sub(1);
        if lo <= hi {
            (lo + hi) / 2
        } else {
            text_right_excl.saturating_sub(1)
        }
    } else if footer_end < text_right_excl {
        let lo = footer_end;
        let hi = text_right_excl.saturating_sub(1);
        if lo <= hi { (lo + hi) / 2 } else { hi }
    } else {
        text_right_excl.saturating_sub(1)
    }
    .min(term_cols.saturating_sub(1));
    stdout.queue(MoveTo(status_x as u16, footer_row))?;
    if dirty {
        stdout.queue(Print(theme().editor_dirty.paint("●")))?;
    } else {
        stdout.queue(Print(theme().editor_clean.paint("○")))?;
    }
    Ok(())
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
    pub validate: Option<&'a fn(&str) -> bool>,
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

    let cursor_char_in_line =
        text_ops::byte_idx_to_char_count(&r.lines[r.cursor_row], r.cursor_col);
    let (cursor_vis_row, cursor_vis_col) =
        visual_pos(&visuals, r.lines, r.cursor_row, cursor_char_in_line);

    if cursor_vis_row < *r.view_top {
        *r.view_top = cursor_vis_row;
    }
    if cursor_vis_row >= r.view_top.saturating_add(available_text) {
        *r.view_top = cursor_vis_row + 1 - available_text;
    }

    let full_text_valid = r.validate.map(|v| {
        let full = r.lines.join("\n");
        full.trim().is_empty() || v(full.as_str())
    });

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
        let (buf_idx, range, start_char) = &visuals[*r.view_top + v_i];
        let content = &r.lines[*buf_idx][range.start..range.end];
        stdout.queue(MoveTo(content_left_u16, editor_row + v_i as u16))?;
        if *buf_idx == 0 && *start_char == 0 {
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
            *start_char,
            content,
            sel_range,
            full_text_valid,
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

    let cur_visual_row_on_screen = cursor_vis_row.saturating_sub(*r.view_top);
    let mut x = (content_left + r.prompt_width + cursor_vis_col) as u16;
    if term_cols_u16 > 0 && x >= term_cols_u16 {
        x = term_cols_u16.saturating_sub(1);
    }
    let y = editor_row.saturating_add(cur_visual_row_on_screen as u16);
    stdout.queue(MoveTo(x, y))?;
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
            .map(|(bi, range, _)| lines[bi][range].to_string())
            .collect()
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
        let l = lines(&["a few words here", "", "loooooooooongword and more"]);
        for vw in 1..=30 {
            let visuals = compute_visuals(&l, vw);
            let mut expected_start = vec![0usize; l.len()];
            for (bi, range, start_char) in &visuals {
                assert_eq!(range.start, expected_start[*bi], "vw={vw}");
                assert!(
                    l[*bi][range.clone()].chars().count() <= vw.max(1),
                    "vw={vw}: row wider than the limit"
                );
                let _ = start_char;
                expected_start[*bi] = range.end;
            }
            for (bi, line) in l.iter().enumerate() {
                assert_eq!(expected_start[bi], line.len(), "vw={vw}: line {bi} not fully covered");
            }
        }
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
}
