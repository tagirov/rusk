//! Mutable editor state: buffer, cursor, selection anchor, view scroll.
//!
//! All operations are expressed as methods so the event loop stays slim and
//! doesn't repeat the same "delete selection → mutate → recompute desired
//! column" boilerplate across a dozen branches.

use super::history::Snapshot;
use super::text_ops;

pub(super) struct EditorState {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    /// Desired visual column (in chars) to preserve while moving vertically.
    pub desired_col_char: usize,
    pub anchor: Option<(usize, usize)>,
    pub view_top: usize,
}

impl EditorState {
    pub fn from_prefill(prefill_lines: &[String], cursor_at_start: bool, vw: usize) -> Self {
        let mut lines: Vec<String> = prefill_lines.to_vec();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let (row, col) = if cursor_at_start {
            (0usize, 0usize)
        } else {
            let r = lines.len().saturating_sub(1);
            let c = lines[r].len();
            (r, c)
        };
        let desired_col_char = text_ops::byte_idx_to_char_count(&lines[row], col) % vw.max(1);
        Self {
            lines,
            row,
            col,
            desired_col_char,
            anchor: None,
            view_top: 0,
        }
    }

    // ── Snapshot / selection ────────────────────────────────────────────────

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
            anchor: self.anchor,
        }
    }

    pub fn restore(&mut self, snap: Snapshot, vw: usize) {
        self.lines = snap.lines;
        self.row = snap.row;
        self.col = snap.col;
        self.anchor = snap.anchor;
        self.recompute_desired(vw);
    }

    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        text_ops::selection_range(self.anchor, self.row, self.col)
    }

    pub fn selection_text(&self) -> Option<String> {
        let (s, e) = self.selection_range()?;
        Some(text_ops::selection_text(&self.lines, s, e))
    }

    pub fn start_selection_if_needed(&mut self) {
        if self.anchor.is_none() {
            self.anchor = Some((self.row, self.col));
        }
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Delete the current selection if any; returns `true` when something was
    /// removed so callers can skip further work.
    pub fn delete_selection(&mut self) -> bool {
        if let Some((s, e)) = self.selection_range() {
            let (r, c) = text_ops::delete_selection_range(&mut self.lines, s, e);
            self.row = r;
            self.col = c;
            self.anchor = None;
            true
        } else {
            false
        }
    }

    pub fn joined(&self) -> String {
        self.lines.join("\n")
    }

    pub fn dirty_vs(&self, prefill: &str) -> bool {
        self.joined() != prefill
    }

    pub fn recompute_desired(&mut self, vw: usize) {
        self.desired_col_char =
            text_ops::byte_idx_to_char_count(&self.lines[self.row], self.col) % vw.max(1);
    }

    // ── Movement ────────────────────────────────────────────────────────────

    pub fn move_left(&mut self, vw: usize) {
        let (r, c) = text_ops::ml_move_left(&self.lines, self.row, self.col);
        self.row = r;
        self.col = c;
        self.recompute_desired(vw);
    }

    pub fn move_right(&mut self, vw: usize) {
        let (r, c) = text_ops::ml_move_right(&self.lines, self.row, self.col);
        self.row = r;
        self.col = c;
        self.recompute_desired(vw);
    }

    pub fn word_left(&mut self, vw: usize) {
        let (r, c) = text_ops::ml_word_left(&self.lines, self.row, self.col);
        self.row = r;
        self.col = c;
        self.recompute_desired(vw);
    }

    pub fn word_right(&mut self, vw: usize) {
        let (r, c) = text_ops::ml_word_right(&self.lines, self.row, self.col);
        self.row = r;
        self.col = c;
        self.recompute_desired(vw);
    }

    pub fn soft_up(&mut self, vw: usize) {
        let (r, c) =
            text_ops::ml_soft_up(&self.lines, self.row, self.col, self.desired_col_char, vw);
        self.row = r;
        self.col = c;
    }

    pub fn soft_down(&mut self, vw: usize) {
        let (r, c) =
            text_ops::ml_soft_down(&self.lines, self.row, self.col, self.desired_col_char, vw);
        self.row = r;
        self.col = c;
    }

    pub fn soft_up_n(&mut self, n: usize, vw: usize) {
        for _ in 0..n {
            let (r, c) =
                text_ops::ml_soft_up(&self.lines, self.row, self.col, self.desired_col_char, vw);
            if (r, c) == (self.row, self.col) {
                break;
            }
            self.row = r;
            self.col = c;
        }
    }

    pub fn soft_down_n(&mut self, n: usize, vw: usize) {
        for _ in 0..n {
            let (r, c) =
                text_ops::ml_soft_down(&self.lines, self.row, self.col, self.desired_col_char, vw);
            if (r, c) == (self.row, self.col) {
                break;
            }
            self.row = r;
            self.col = c;
        }
    }

    pub fn goto_buffer_start(&mut self) {
        self.row = 0;
        self.col = 0;
        self.desired_col_char = 0;
    }

    pub fn goto_buffer_end(&mut self, vw: usize) {
        self.row = self.lines.len().saturating_sub(1);
        self.col = self.lines[self.row].len();
        self.recompute_desired(vw);
    }

    /// Smart Home: first non-space on the current line, then column 0.
    pub fn smart_home(&mut self, vw: usize) {
        let first = text_ops::first_non_space(&self.lines[self.row]);
        self.col = if self.col == first { 0 } else { first };
        self.recompute_desired(vw);
    }

    pub fn goto_line_end(&mut self, vw: usize) {
        self.col = self.lines[self.row].len();
        self.recompute_desired(vw);
    }

    pub fn select_all(&mut self, vw: usize) {
        self.anchor = Some((0, 0));
        self.row = self.lines.len().saturating_sub(1);
        self.col = self.lines[self.row].len();
        self.recompute_desired(vw);
    }

    pub fn goto(&mut self, row: usize, col: usize, vw: usize) {
        self.row = row;
        self.col = col;
        self.recompute_desired(vw);
    }

    // ── Editing ─────────────────────────────────────────────────────────────

    pub fn insert_char(&mut self, ch: char, vw: usize) {
        self.delete_selection();
        self.lines[self.row].insert(self.col, ch);
        self.col += ch.len_utf8();
        self.recompute_desired(vw);
    }

    pub fn insert_str(&mut self, text: &str, vw: usize) {
        self.delete_selection();
        let (r, c) = text_ops::insert_string(&mut self.lines, self.row, self.col, text);
        self.row = r;
        self.col = c;
        self.recompute_desired(vw);
    }

    pub fn insert_newline(&mut self, vw: usize) {
        self.delete_selection();
        let tail = self.lines[self.row].split_off(self.col);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
        self.desired_col_char = 0;
        let _ = vw;
    }

    pub fn insert_tab(&mut self, vw: usize) {
        self.insert_str("    ", vw);
    }

    pub fn backspace(&mut self, vw: usize) {
        if !self.delete_selection() {
            text_ops::ml_backspace(&mut self.lines, &mut self.row, &mut self.col);
        }
        self.recompute_desired(vw);
    }

    pub fn delete(&mut self, vw: usize) {
        if !self.delete_selection() {
            text_ops::ml_delete(&mut self.lines, self.row, &mut self.col);
        }
        self.recompute_desired(vw);
    }

    pub fn delete_word_left(&mut self, vw: usize) {
        if !self.delete_selection() {
            text_ops::ml_delete_word_left(&mut self.lines, &mut self.row, &mut self.col);
        }
        self.recompute_desired(vw);
    }

    pub fn delete_word_right(&mut self, vw: usize) {
        if !self.delete_selection() {
            text_ops::ml_delete_word_right(&mut self.lines, self.row, &mut self.col);
        }
        self.recompute_desired(vw);
    }

    pub fn kill_to_eol(&mut self, vw: usize) {
        text_ops::ml_kill_to_eol(&mut self.lines, self.row, &mut self.col);
        self.anchor = None;
        self.recompute_desired(vw);
    }

    pub fn kill_to_bol(&mut self, vw: usize) {
        text_ops::ml_kill_to_bol(&mut self.lines, self.row, &mut self.col);
        self.anchor = None;
        self.recompute_desired(vw);
    }

    pub fn delete_line(&mut self, vw: usize) {
        text_ops::ml_delete_line(&mut self.lines, &mut self.row, &mut self.col);
        self.anchor = None;
        self.recompute_desired(vw);
    }

    pub fn reset_to_prefill(&mut self, prefill_lines: &[String], vw: usize) {
        self.lines = if prefill_lines.is_empty() {
            vec![String::new()]
        } else {
            prefill_lines.to_vec()
        };
        self.row = self.lines.len().saturating_sub(1);
        self.col = self.lines[self.row].len();
        self.anchor = None;
        self.recompute_desired(vw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VW: usize = 40;

    fn state_with(lines: &[&str], row: usize, col: usize) -> EditorState {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        EditorState {
            lines: owned,
            row,
            col,
            desired_col_char: col,
            anchor: None,
            view_top: 0,
        }
    }

    #[test]
    fn from_prefill_empty_makes_single_empty_line() {
        let s = EditorState::from_prefill(&[], false, VW);
        assert_eq!(s.lines, vec![String::new()]);
        assert_eq!((s.row, s.col), (0, 0));
    }

    #[test]
    fn from_prefill_cursor_at_start_sets_origin() {
        let prefill = vec!["a".into(), "bb".into()];
        let s = EditorState::from_prefill(&prefill, true, VW);
        assert_eq!((s.row, s.col), (0, 0));
    }

    #[test]
    fn from_prefill_cursor_at_end_sets_end() {
        let prefill = vec!["a".into(), "bb".into()];
        let s = EditorState::from_prefill(&prefill, false, VW);
        assert_eq!((s.row, s.col), (1, 2));
    }

    #[test]
    fn insert_char_advances_cursor() {
        let mut s = state_with(&[""], 0, 0);
        s.insert_char('a', VW);
        s.insert_char('b', VW);
        assert_eq!(s.joined(), "ab");
        assert_eq!((s.row, s.col), (0, 2));
    }

    #[test]
    fn insert_newline_splits_line() {
        let mut s = state_with(&["hello world"], 0, 5);
        s.insert_newline(VW);
        assert_eq!(s.lines, vec!["hello".to_string(), " world".to_string()]);
        assert_eq!((s.row, s.col), (1, 0));
    }

    #[test]
    fn insert_str_with_newlines_moves_cursor_to_tail_of_last_part() {
        let mut s = state_with(&["abc"], 0, 3);
        s.insert_str("X\nY", VW);
        assert_eq!(s.lines, vec!["abcX".to_string(), "Y".to_string()]);
        assert_eq!((s.row, s.col), (1, 1));
    }

    #[test]
    fn insert_char_replaces_selection() {
        let mut s = state_with(&["hello"], 0, 4);
        s.anchor = Some((0, 1));
        s.insert_char('_', VW);
        assert_eq!(s.joined(), "h_o");
        assert!(s.anchor.is_none());
    }

    #[test]
    fn backspace_deletes_selection_when_present() {
        let mut s = state_with(&["abcdef"], 0, 5);
        s.anchor = Some((0, 1));
        s.backspace(VW);
        assert_eq!(s.joined(), "af");
        assert!(s.anchor.is_none());
    }

    #[test]
    fn delete_word_left_joins_lines_at_start() {
        let mut s = state_with(&["abc", "def"], 1, 0);
        s.delete_word_left(VW);
        assert_eq!(s.lines, vec!["abcdef".to_string()]);
        assert_eq!((s.row, s.col), (0, 3));
    }

    #[test]
    fn kill_to_eol_truncates_line() {
        let mut s = state_with(&["hello world"], 0, 5);
        s.kill_to_eol(VW);
        assert_eq!(s.joined(), "hello");
    }

    #[test]
    fn kill_to_bol_drops_prefix() {
        let mut s = state_with(&["hello"], 0, 3);
        s.kill_to_bol(VW);
        assert_eq!(s.joined(), "lo");
        assert_eq!(s.col, 0);
    }

    #[test]
    fn delete_line_removes_row() {
        let mut s = state_with(&["a", "b", "c"], 1, 1);
        s.delete_line(VW);
        assert_eq!(s.lines, vec!["a".to_string(), "c".to_string()]);
        assert_eq!(s.row, 1);
    }

    #[test]
    fn delete_line_on_single_line_clears_it() {
        let mut s = state_with(&["hello"], 0, 2);
        s.delete_line(VW);
        assert_eq!(s.lines, vec![String::new()]);
        assert_eq!((s.row, s.col), (0, 0));
    }

    #[test]
    fn select_all_covers_whole_buffer() {
        let mut s = state_with(&["a", "bb", "ccc"], 0, 0);
        s.select_all(VW);
        assert_eq!(s.anchor, Some((0, 0)));
        assert_eq!((s.row, s.col), (2, 3));
        assert!(s.selection_range().is_some());
    }

    #[test]
    fn smart_home_toggles_between_indent_and_col0() {
        let mut s = state_with(&["    hi"], 0, 6);
        s.smart_home(VW);
        assert_eq!(s.col, 4);
        s.smart_home(VW);
        assert_eq!(s.col, 0);
    }

    #[test]
    fn goto_line_end_jumps_to_eol() {
        let mut s = state_with(&["hello"], 0, 0);
        s.goto_line_end(VW);
        assert_eq!(s.col, 5);
    }

    #[test]
    fn reset_to_prefill_restores_original() {
        let mut s = state_with(&["edited"], 0, 6);
        let prefill = vec!["a".to_string(), "b".to_string()];
        s.reset_to_prefill(&prefill, VW);
        assert_eq!(s.lines, prefill);
        assert_eq!((s.row, s.col), (1, 1));
    }

    #[test]
    fn snapshot_restore_roundtrip_keeps_anchor() {
        let mut s = state_with(&["abc", "def"], 1, 2);
        s.anchor = Some((0, 1));
        let snap = s.snapshot();
        s.insert_str("XYZ", VW);
        assert_ne!(s.joined(), "abc\ndef");
        s.restore(snap, VW);
        assert_eq!(s.joined(), "abc\ndef");
        assert_eq!(s.anchor, Some((0, 1)));
        assert_eq!((s.row, s.col), (1, 2));
    }

    #[test]
    fn dirty_vs_detects_change() {
        let s = state_with(&["hello"], 0, 0);
        assert!(!s.dirty_vs("hello"));
        assert!(s.dirty_vs("world"));
    }

    #[test]
    fn selection_range_none_when_anchor_equals_head() {
        let mut s = state_with(&["abc"], 0, 1);
        s.anchor = Some((0, 1));
        assert!(s.selection_range().is_none());
    }

    #[test]
    fn selection_text_returns_selected_substring() {
        let mut s = state_with(&["abcdef"], 0, 4);
        s.anchor = Some((0, 1));
        assert_eq!(s.selection_text().as_deref(), Some("bcd"));
    }

    #[test]
    fn soft_up_stops_at_buffer_start() {
        let mut s = state_with(&["line"], 0, 2);
        s.soft_up(VW);
        assert_eq!((s.row, s.col), (0, 2));
    }

    #[test]
    fn soft_down_stops_at_buffer_end() {
        let mut s = state_with(&["line"], 0, 2);
        s.soft_down(VW);
        assert_eq!((s.row, s.col), (0, 2));
    }

    #[test]
    fn soft_up_n_bails_out_when_no_movement_possible() {
        // If no vertical movement is possible we must not loop forever.
        let mut s = state_with(&["one"], 0, 1);
        s.soft_up_n(100, VW);
        assert_eq!((s.row, s.col), (0, 1));
    }

    #[test]
    fn goto_buffer_start_and_end() {
        let mut s = state_with(&["a", "bc", "def"], 0, 0);
        s.goto_buffer_end(VW);
        assert_eq!((s.row, s.col), (2, 3));
        s.goto_buffer_start();
        assert_eq!((s.row, s.col), (0, 0));
    }
}
