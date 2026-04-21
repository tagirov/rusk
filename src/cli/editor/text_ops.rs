//! Pure, side-effect-free helpers used by the interactive editor.
//!
//! Everything here is a small, easily unit-testable function that operates
//! on `&str` or `Vec<String>` without touching the terminal or any global
//! state. Helpers are intentionally kept free-standing (not methods on a
//! type) so they can be used both by the editor internals and by the
//! public `HandlerCLI` shims re-exported in `editor/mod.rs`.

use std::cmp::Ordering;

// ── Byte / char helpers ─────────────────────────────────────────────────────

pub fn prev_char_boundary(s: &str, byte_idx: usize) -> usize {
    if byte_idx == 0 {
        return 0;
    }
    let mut idx = byte_idx;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    if idx > 0 {
        if let Some((prev_idx, _)) = s.char_indices().take_while(|(i, _)| *i < idx).last() {
            prev_idx
        } else {
            0
        }
    } else {
        0
    }
}

pub fn next_char_boundary(s: &str, byte_idx: usize) -> usize {
    let len = s.len();
    if byte_idx >= len {
        return len;
    }
    let mut idx = byte_idx + 1;
    while idx < len && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx.min(len)
}

pub fn byte_idx_to_char_count(s: &str, byte_idx: usize) -> usize {
    s.char_indices().take_while(|(i, _)| *i < byte_idx).count()
}

pub fn ml_char_to_byte(line: &str, target_char: usize) -> usize {
    for (count, (i, _)) in line.char_indices().enumerate() {
        if count == target_char {
            return i;
        }
    }
    line.len()
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

pub fn first_non_space(line: &str) -> usize {
    for (i, c) in line.char_indices() {
        if !c.is_whitespace() {
            return i;
        }
    }
    line.len()
}

// ── Word jumps ──────────────────────────────────────────────────────────────

pub fn jump_prev_word(buffer: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut pos = cursor;
    if !buffer.is_char_boundary(pos) {
        pos = prev_char_boundary(buffer, pos);
    }
    let chars: Vec<(usize, char)> = buffer
        .char_indices()
        .take_while(|(idx, _)| *idx < pos)
        .collect();
    if chars.is_empty() {
        return 0;
    }
    let mut i = chars.len() - 1;
    while i > 0 && is_word_char(chars[i].1) {
        i -= 1;
    }
    while i > 0 && !is_word_char(chars[i].1) {
        i -= 1;
    }
    if i < chars.len() && is_word_char(chars[i].1) {
        while i > 0 && is_word_char(chars[i - 1].1) {
            i -= 1;
        }
        chars[i].0
    } else if i + 1 < chars.len() {
        chars[i + 1].0
    } else {
        chars[i].0
    }
}

pub fn jump_next_word(buffer: &str, cursor: usize) -> usize {
    let len = buffer.len();
    if cursor >= len {
        return len;
    }
    let mut pos = cursor;
    if !buffer.is_char_boundary(pos) {
        pos = next_char_boundary(buffer, pos);
    }
    let chars: Vec<(usize, char)> = buffer
        .char_indices()
        .skip_while(|(idx, _)| *idx < pos)
        .collect();
    if chars.is_empty() {
        return len;
    }
    let mut i = 0;
    while i < chars.len() && is_word_char(chars[i].1) {
        i += 1;
    }
    while i < chars.len() && !is_word_char(chars[i].1) {
        i += 1;
    }
    if i < chars.len() { chars[i].0 } else { len }
}

pub fn word_bounds(line: &str, byte_idx: usize) -> (usize, usize) {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let pos = chars
        .iter()
        .position(|(i, _)| *i >= byte_idx)
        .unwrap_or(chars.len());
    let anchor = pos.min(chars.len().saturating_sub(1));
    let is_word = is_word_char(chars[anchor].1);
    let mut start = anchor;
    while start > 0 && is_word_char(chars[start - 1].1) == is_word {
        start -= 1;
    }
    let mut end = anchor;
    while end < chars.len() && is_word_char(chars[end].1) == is_word {
        end += 1;
    }
    let start_byte = chars[start].0;
    let end_byte = if end >= chars.len() {
        line.len()
    } else {
        chars[end].0
    };
    (start_byte, end_byte)
}

// ── Prefill / date header ───────────────────────────────────────────────────

pub fn split_multi_line_prefill(prefill: &str) -> Vec<String> {
    if prefill.is_empty() {
        return Vec::new();
    }
    let normalized = prefill.replace("\r\n", "\n").replace('\r', "\n");
    normalized.split('\n').map(|s| s.to_string()).collect()
}

/// Character length of a leading date prefix on the first logical line.
/// Returns 0 if the leading whitespace-delimited token is not a valid CLI date.
pub fn leading_date_char_len(line: &str) -> usize {
    let token: String = line.chars().take_while(|c| !c.is_whitespace()).collect();
    if token.is_empty() {
        return 0;
    }
    if crate::parse_cli_date(&token).is_ok() {
        token.chars().count()
    } else {
        0
    }
}

// ── Position / selection helpers ────────────────────────────────────────────

pub fn pos_cmp(a: (usize, usize), b: (usize, usize)) -> Ordering {
    match a.0.cmp(&b.0) {
        Ordering::Equal => a.1.cmp(&b.1),
        o => o,
    }
}

/// Returns `(start, end)` (start <= end in buffer order) if anchor is set
/// and differs from the head; `None` otherwise.
pub fn selection_range(
    anchor: Option<(usize, usize)>,
    head_row: usize,
    head_col: usize,
) -> Option<((usize, usize), (usize, usize))> {
    let a = anchor?;
    let h = (head_row, head_col);
    if a == h {
        return None;
    }
    Some(if pos_cmp(a, h) != Ordering::Greater {
        (a, h)
    } else {
        (h, a)
    })
}

pub fn selection_text(
    lines: &[String],
    start: (usize, usize),
    end: (usize, usize),
) -> String {
    if start.0 == end.0 {
        return lines[start.0][start.1..end.1].to_string();
    }
    let mut out = String::new();
    out.push_str(&lines[start.0][start.1..]);
    out.push('\n');
    for line in lines.iter().take(end.0).skip(start.0 + 1) {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&lines[end.0][..end.1]);
    out
}

pub fn delete_selection_range(
    lines: &mut Vec<String>,
    start: (usize, usize),
    end: (usize, usize),
) -> (usize, usize) {
    if start.0 == end.0 {
        lines[start.0].drain(start.1..end.1);
    } else {
        let end_suffix = lines[end.0][end.1..].to_string();
        lines[start.0].truncate(start.1);
        lines[start.0].push_str(&end_suffix);
        lines.drain((start.0 + 1)..=end.0);
    }
    start
}

pub fn insert_string(
    lines: &mut Vec<String>,
    row: usize,
    col: usize,
    text: &str,
) -> (usize, usize) {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let parts: Vec<&str> = normalized.split('\n').collect();
    if parts.len() == 1 {
        lines[row].insert_str(col, parts[0]);
        return (row, col + parts[0].len());
    }
    let tail = lines[row].split_off(col);
    lines[row].push_str(parts[0]);
    let mut r = row;
    let mut c = col;
    for (i, part) in parts.iter().enumerate().skip(1) {
        let last = i == parts.len() - 1;
        r += 1;
        if last {
            let mut new_line = part.to_string();
            c = new_line.len();
            new_line.push_str(&tail);
            lines.insert(r, new_line);
        } else {
            lines.insert(r, part.to_string());
        }
    }
    (r, c)
}

// ── Movement ────────────────────────────────────────────────────────────────

pub fn ml_move_left(lines: &[String], row: usize, col: usize) -> (usize, usize) {
    if col > 0 {
        (row, prev_char_boundary(&lines[row], col))
    } else if row > 0 {
        let new_row = row - 1;
        (new_row, lines[new_row].len())
    } else {
        (row, col)
    }
}

pub fn ml_move_right(lines: &[String], row: usize, col: usize) -> (usize, usize) {
    if col < lines[row].len() {
        (row, next_char_boundary(&lines[row], col))
    } else if row + 1 < lines.len() {
        (row + 1, 0)
    } else {
        (row, col)
    }
}

pub fn ml_word_left(lines: &[String], row: usize, col: usize) -> (usize, usize) {
    if col == 0 {
        if row > 0 {
            let new_row = row - 1;
            return (new_row, lines[new_row].len());
        }
        return (row, 0);
    }
    (row, jump_prev_word(&lines[row], col))
}

pub fn ml_word_right(lines: &[String], row: usize, col: usize) -> (usize, usize) {
    if col >= lines[row].len() {
        if row + 1 < lines.len() {
            return (row + 1, 0);
        }
        return (row, col);
    }
    (row, jump_next_word(&lines[row], col))
}

pub fn ml_soft_up(
    lines: &[String],
    row: usize,
    col: usize,
    desired_vis_col: usize,
    vw: usize,
) -> (usize, usize) {
    let line_chars = lines[row].chars().count();
    let cursor_char = byte_idx_to_char_count(&lines[row], col).min(line_chars);
    let cur_vis_row = cursor_char / vw.max(1);
    if cur_vis_row > 0 {
        let new_vis_row = cur_vis_row - 1;
        let row_end = ((new_vis_row + 1) * vw).min(line_chars);
        let target = (new_vis_row * vw + desired_vis_col).min(row_end);
        (row, ml_char_to_byte(&lines[row], target))
    } else if row > 0 {
        let prev = row - 1;
        let prev_chars = lines[prev].chars().count();
        let last_vis_row = if prev_chars == 0 {
            0
        } else {
            prev_chars.saturating_sub(1) / vw.max(1)
        };
        let row_end = ((last_vis_row + 1) * vw).min(prev_chars);
        let target = (last_vis_row * vw + desired_vis_col).min(row_end);
        (prev, ml_char_to_byte(&lines[prev], target))
    } else {
        (row, col)
    }
}

pub fn ml_soft_down(
    lines: &[String],
    row: usize,
    col: usize,
    desired_vis_col: usize,
    vw: usize,
) -> (usize, usize) {
    let line_chars = lines[row].chars().count();
    let cursor_char = byte_idx_to_char_count(&lines[row], col).min(line_chars);
    let cur_vis_row = cursor_char / vw.max(1);
    let last_vis_row = if line_chars == 0 {
        0
    } else {
        line_chars.saturating_sub(1) / vw.max(1)
    };
    if cur_vis_row < last_vis_row {
        let new_vis_row = cur_vis_row + 1;
        let row_end = ((new_vis_row + 1) * vw).min(line_chars);
        let target = (new_vis_row * vw + desired_vis_col).min(row_end);
        (row, ml_char_to_byte(&lines[row], target))
    } else if row + 1 < lines.len() {
        let next = row + 1;
        let next_chars = lines[next].chars().count();
        let row_end = vw.min(next_chars);
        let target = desired_vis_col.min(row_end);
        (next, ml_char_to_byte(&lines[next], target))
    } else {
        (row, col)
    }
}

// ── Editing primitives ──────────────────────────────────────────────────────

pub fn ml_backspace(lines: &mut Vec<String>, row: &mut usize, col: &mut usize) {
    if *col > 0 {
        let prev = prev_char_boundary(&lines[*row], *col);
        lines[*row].drain(prev..*col);
        *col = prev;
    } else if *row > 0 {
        let current = lines.remove(*row);
        *row -= 1;
        *col = lines[*row].len();
        lines[*row].push_str(&current);
    }
}

pub fn ml_delete(lines: &mut Vec<String>, row: usize, col: &mut usize) {
    let line_len = lines[row].len();
    if *col < line_len {
        let next = next_char_boundary(&lines[row], *col);
        lines[row].drain(*col..next);
    } else if row + 1 < lines.len() {
        let next_line = lines.remove(row + 1);
        lines[row].push_str(&next_line);
    }
}

pub fn ml_delete_word_left(lines: &mut Vec<String>, row: &mut usize, col: &mut usize) {
    if *col > 0 {
        let new_col = jump_prev_word(&lines[*row], *col);
        lines[*row].drain(new_col..*col);
        *col = new_col;
    } else if *row > 0 {
        let current = lines.remove(*row);
        *row -= 1;
        *col = lines[*row].len();
        lines[*row].push_str(&current);
    }
}

pub fn ml_delete_word_right(lines: &mut Vec<String>, row: usize, col: &mut usize) {
    let line_len = lines[row].len();
    if *col < line_len {
        let new_col = jump_next_word(&lines[row], *col);
        lines[row].drain(*col..new_col);
    } else if row + 1 < lines.len() {
        let next_line = lines.remove(row + 1);
        lines[row].push_str(&next_line);
    }
}

pub fn ml_kill_to_eol(lines: &mut Vec<String>, row: usize, col: &mut usize) {
    let line_len = lines[row].len();
    if *col < line_len {
        lines[row].truncate(*col);
    } else if row + 1 < lines.len() {
        let next_line = lines.remove(row + 1);
        lines[row].push_str(&next_line);
    }
}

pub fn ml_kill_to_bol(lines: &mut [String], row: usize, col: &mut usize) {
    if *col > 0 {
        lines[row].drain(0..*col);
        *col = 0;
    }
}

pub fn ml_delete_line(lines: &mut Vec<String>, row: &mut usize, col: &mut usize) {
    if lines.len() == 1 {
        lines[0].clear();
        *col = 0;
        return;
    }
    lines.remove(*row);
    if *row >= lines.len() {
        *row = lines.len().saturating_sub(1);
    }
    *col = (*col).min(lines[*row].len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ── selection_range ─────────────────────────────────────────────────────

    #[test]
    fn selection_range_none_without_anchor() {
        assert!(selection_range(None, 0, 0).is_none());
    }

    #[test]
    fn selection_range_normalizes_reverse_order() {
        let a = Some((2, 0));
        let r = selection_range(a, 0, 3).unwrap();
        assert_eq!(r, ((0, 3), (2, 0)));
    }

    #[test]
    fn selection_range_collapsed_returns_none() {
        assert!(selection_range(Some((1, 2)), 1, 2).is_none());
    }

    // ── selection_text ──────────────────────────────────────────────────────

    #[test]
    fn selection_text_single_line() {
        let l = lines(&["hello world"]);
        assert_eq!(selection_text(&l, (0, 6), (0, 11)), "world");
    }

    #[test]
    fn selection_text_multi_line() {
        let l = lines(&["first", "second", "third"]);
        assert_eq!(
            selection_text(&l, (0, 3), (2, 2)),
            "st\nsecond\nth".to_string()
        );
    }

    // ── delete_selection_range ──────────────────────────────────────────────

    #[test]
    fn delete_selection_same_line() {
        let mut l = lines(&["abcdef"]);
        let pos = delete_selection_range(&mut l, (0, 1), (0, 4));
        assert_eq!(l, vec!["aef".to_string()]);
        assert_eq!(pos, (0, 1));
    }

    #[test]
    fn delete_selection_spans_lines() {
        let mut l = lines(&["aaa", "bbb", "ccc"]);
        let pos = delete_selection_range(&mut l, (0, 1), (2, 2));
        assert_eq!(l, vec!["ac".to_string()]);
        assert_eq!(pos, (0, 1));
    }

    // ── insert_string ───────────────────────────────────────────────────────

    #[test]
    fn insert_string_no_newlines_inline() {
        let mut l = lines(&["hello"]);
        let (r, c) = insert_string(&mut l, 0, 2, "XX");
        assert_eq!(l, vec!["heXXllo".to_string()]);
        assert_eq!((r, c), (0, 4));
    }

    #[test]
    fn insert_string_with_newlines_splits_and_advances() {
        let mut l = lines(&["tail"]);
        let (r, c) = insert_string(&mut l, 0, 1, "A\nB\nC");
        assert_eq!(l, vec!["tA".to_string(), "B".to_string(), "Cail".to_string()]);
        assert_eq!((r, c), (2, 1));
    }

    #[test]
    fn insert_string_normalizes_crlf() {
        let mut l = lines(&[""]);
        let (r, c) = insert_string(&mut l, 0, 0, "a\r\nb\rc");
        assert_eq!(l, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!((r, c), (2, 1));
    }

    // ── word_bounds ─────────────────────────────────────────────────────────

    #[test]
    fn word_bounds_at_word_start() {
        assert_eq!(word_bounds("hello world", 0), (0, 5));
    }

    #[test]
    fn word_bounds_in_whitespace() {
        assert_eq!(word_bounds("a   b", 2), (1, 4));
    }

    #[test]
    fn word_bounds_empty_line() {
        assert_eq!(word_bounds("", 0), (0, 0));
    }

    // ── first_non_space ─────────────────────────────────────────────────────

    #[test]
    fn first_non_space_with_indent() {
        assert_eq!(first_non_space("    hi"), 4);
    }

    #[test]
    fn first_non_space_blank_returns_len() {
        assert_eq!(first_non_space("   "), 3);
    }

    // ── pos_cmp ─────────────────────────────────────────────────────────────

    #[test]
    fn pos_cmp_orders_by_row_first() {
        use std::cmp::Ordering;
        assert_eq!(pos_cmp((0, 5), (1, 0)), Ordering::Less);
        assert_eq!(pos_cmp((1, 5), (1, 3)), Ordering::Greater);
        assert_eq!(pos_cmp((2, 4), (2, 4)), Ordering::Equal);
    }
}
