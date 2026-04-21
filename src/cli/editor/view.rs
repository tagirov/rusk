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

pub(super) const ML_FOOTER: &str = "^S save  ·  ^G help  ·  Esc cancel";
const ML_FOOTER_GAP: usize = 2;
const ML_RIGHT_MARGIN: usize = 4;

/// Split buffer lines into soft-wrapped visual rows. Each tuple is
/// `(buffer_idx, chunk_content, start_char_offset_in_buffer_line)`.
pub(super) fn compute_visuals(lines: &[String], vw: usize) -> Vec<(usize, String, usize)> {
    let mut out: Vec<(usize, String, usize)> = Vec::new();
    for (buf_idx, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            out.push((buf_idx, String::new(), 0));
            continue;
        }
        let mut offset = 0usize;
        while offset < chars.len() {
            let end = (offset + vw).min(chars.len());
            let chunk: String = chars[offset..end].iter().collect();
            out.push((buf_idx, chunk, offset));
            offset = end;
        }
    }
    out
}

/// Print one visual chunk with selection + optional validation colors and
/// a bold-green highlight for a leading date token on the first logical line.
fn print_visual_chunk(
    stdout: &mut io::Stdout,
    lines: &[String],
    buf_idx: usize,
    start_char: usize,
    content: &str,
    sel: Option<((usize, usize), (usize, usize))>,
    full_text_valid: Option<bool>,
) -> Result<()> {
    let chunk_chars: Vec<char> = content.chars().collect();
    let chunk_len = chunk_chars.len();
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
        if a >= b { None } else { Some((a - start_char, b - start_char)) }
    });

    let date_end_in_chunk: usize = if buf_idx == 0 {
        let date_end_in_line = text_ops::leading_date_char_len(&lines[0]);
        date_end_in_line.saturating_sub(start_char).min(chunk_len)
    } else {
        0
    };

    let emit = |stdout: &mut io::Stdout,
                text: &str,
                selected: bool,
                is_date: bool|
     -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        if selected {
            stdout.queue(Print(text.reversed()))?;
        } else if is_date {
            stdout.queue(Print(text.green().bold()))?;
        } else {
            match full_text_valid {
                Some(true) => stdout.queue(Print(text.green()))?,
                Some(false) => stdout.queue(Print(text.red()))?,
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

    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a == b {
            continue;
        }
        let text: String = chunk_chars[a..b].iter().collect();
        let selected = match sel_in_chunk {
            Some((ss, se)) => a >= ss && b <= se,
            None => false,
        };
        let is_date = date_end_in_chunk > 0 && b <= date_end_in_chunk;
        emit(stdout, &text, selected, is_date)?;
    }
    Ok(())
}

pub(super) struct RenderInput<'a> {
    pub editor_row: u16,
    pub prompt: &'a str,
    pub prompt_width: usize,
    pub first_line_colored: Option<&'a str>,
    pub lines: &'a [String],
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub view_top: &'a mut usize,
    pub validate: Option<&'a fn(&str) -> bool>,
    pub selection: Option<((usize, usize), (usize, usize))>,
    pub dirty: bool,
}

pub(super) fn render(stdout: &mut io::Stdout, r: RenderInput<'_>) -> Result<()> {
    let (term_cols_u16, term_rows_u16) = size().unwrap_or((0, 0));
    let term_cols = term_cols_u16 as usize;
    let term_rows = term_rows_u16.max(3) as usize;

    let available_text = term_rows
        .saturating_sub(r.editor_row as usize + 1 + ML_FOOTER_GAP)
        .max(1);
    let visible_width = term_cols
        .saturating_sub(r.prompt_width.saturating_add(ML_RIGHT_MARGIN))
        .max(1);

    let visuals = compute_visuals(r.lines, visible_width);

    let cursor_char_in_line =
        text_ops::byte_idx_to_char_count(&r.lines[r.cursor_row], r.cursor_col);
    let mut visual_start_of_cursor_line: usize = 0;
    for (vi, (bi, _, start_char)) in visuals.iter().enumerate() {
        if *bi == r.cursor_row && *start_char == 0 {
            visual_start_of_cursor_line = vi;
            break;
        }
    }
    let cursor_vis_row =
        visual_start_of_cursor_line + (cursor_char_in_line / visible_width);
    let cursor_vis_col = cursor_char_in_line % visible_width;

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

    stdout.queue(MoveTo(0, r.editor_row))?;
    stdout.queue(Clear(ClearType::FromCursorDown))?;

    let sel_range = r.selection.map(|(a, b)| {
        if text_ops::pos_cmp(a, b) != std::cmp::Ordering::Greater {
            (a, b)
        } else {
            (b, a)
        }
    });

    let visible_count = available_text.min(visuals.len().saturating_sub(*r.view_top));
    for v_i in 0..visible_count {
        let (buf_idx, content, start_char) = &visuals[*r.view_top + v_i];
        stdout.queue(MoveTo(0, r.editor_row + v_i as u16))?;
        if *buf_idx == 0 && *start_char == 0 {
            if let Some(colored) = r.first_line_colored {
                stdout.queue(Print(colored))?;
            } else {
                stdout.queue(Print(r.prompt))?;
            }
        } else {
            let pad: String = " ".repeat(r.prompt_width);
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
        )?;
    }

    // Footer floats a couple of rows below the text when the buffer is
    // short and docks to the last screen row otherwise.
    let content_end_row = r.editor_row as usize + visible_count;
    let default_footer_row = term_rows_u16.saturating_sub(1) as usize;
    let floating_footer_row = content_end_row + ML_FOOTER_GAP;
    let footer_row =
        floating_footer_row.min(default_footer_row).max(r.editor_row as usize) as u16;
    stdout.queue(MoveTo(0, footer_row))?;
    stdout.queue(Clear(ClearType::CurrentLine))?;
    let mut footer_text = ML_FOOTER.to_string();
    if footer_text.chars().count() > term_cols.max(1) {
        let trimmed: String = footer_text.chars().take(term_cols.max(1)).collect();
        footer_text = trimmed;
    }
    let footer_width = footer_text.chars().count();
    let footer_x = term_cols.saturating_sub(footer_width) / 2;
    stdout.queue(MoveTo(footer_x as u16, footer_row))?;
    stdout.queue(Print(footer_text.truecolor(128, 128, 128)))?;

    let has_up = *r.view_top > 0;
    let has_down = *r.view_top + available_text < visuals.len();
    let arrow_x = (footer_x / 2) as u16;
    if has_up {
        stdout.queue(MoveTo(arrow_x, footer_row))?;
        stdout.queue(Print("↑".truecolor(128, 128, 128)))?;
    }
    if has_down {
        stdout.queue(MoveTo(arrow_x + 1, footer_row))?;
        stdout.queue(Print("↓".truecolor(128, 128, 128)))?;
    }

    let status_x = term_cols.saturating_sub(1).saturating_sub(arrow_x as usize);
    let footer_end = footer_x + footer_width;
    if status_x > footer_end {
        stdout.queue(MoveTo(status_x as u16, footer_row))?;
        if r.dirty {
            stdout.queue(Print("●".truecolor(220, 170, 60)))?;
        } else {
            stdout.queue(Print("○".truecolor(100, 100, 100)))?;
        }
    }

    let cur_visual_row_on_screen = cursor_vis_row.saturating_sub(*r.view_top);
    let mut x = (r.prompt_width + cursor_vis_col) as u16;
    if term_cols_u16 > 0 && x >= term_cols_u16 {
        x = term_cols_u16.saturating_sub(1);
    }
    let y = r.editor_row.saturating_add(cur_visual_row_on_screen as u16);
    stdout.queue(MoveTo(x, y))?;
    stdout.flush().ok();
    Ok(())
}

/// Visible-width available for text, matching the formula used by `render`.
pub(super) fn visible_width(prompt_width: usize) -> usize {
    let (cols, _) = size().unwrap_or((80, 24));
    (cols as usize)
        .saturating_sub(prompt_width.saturating_add(ML_RIGHT_MARGIN))
        .max(1)
}

/// How many visual rows fit on screen (used by PageUp/PageDown).
pub(super) fn page_rows() -> usize {
    let (_, rows) = size().unwrap_or((80, 24));
    (rows.saturating_sub(3) as usize).max(1)
}
