//! Mouse-related state: multi-click tracking and screen-to-buffer mapping.

use std::time::Instant;

const DOUBLE_CLICK_MS: u128 = 400;

#[derive(Default)]
pub(super) struct ClickTracker {
    last_time: Option<Instant>,
    last_pos: (u16, u16),
    count: u8,
}

impl ClickTracker {
    /// Register a click at `(x, y)`. Returns the current consecutive click
    /// count capped at 3 (triple-click is the highest meaningful gesture).
    pub fn click(&mut self, x: u16, y: u16) -> u8 {
        let now = Instant::now();
        let within = self
            .last_time
            .map(|t| now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS)
            .unwrap_or(false);
        if within && self.last_pos == (x, y) {
            self.count = self.count.saturating_add(1).min(3);
        } else {
            self.count = 1;
        }
        self.last_time = Some(now);
        self.last_pos = (x, y);
        self.count
    }
}

/// Which visual rows a mouse event may land on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Reach {
    /// Only the rows on screen: a click on the footer, the margin below the
    /// text or the one above it picks the nearest visible row, never one
    /// scrolled out of sight.
    Visible,
    /// One row past either edge of the view, so a drag that leaves the text
    /// area extends the selection there and the view scrolls after it.
    OneBeyond,
}

/// Arguments for mapping a screen cell `(x, y)` to buffer `(row, byte_col)`.
pub(super) struct ScreenToBuffer<'a> {
    pub lines: &'a [String],
    pub visuals: &'a [super::view::VisualRow],
    pub screen_x: u16,
    pub screen_y: u16,
    pub editor_row: u16,
    pub view_top: usize,
    /// Text rows the window has room for (`vertical_layout`).
    pub available_text: usize,
    pub prompt_width: usize,
    /// Left padding of the text block (centering); mouse X is relative to the full screen.
    pub content_left: usize,
    pub reach: Reach,
}

impl ScreenToBuffer<'_> {
    pub(super) fn resolve(self) -> (usize, usize) {
        let ScreenToBuffer {
            lines,
            visuals,
            screen_x,
            screen_y,
            editor_row,
            view_top,
            available_text,
            prompt_width,
            content_left,
            reach,
        } = self;
        if visuals.is_empty() || lines.is_empty() {
            return (0, 0);
        }
        let last = visuals.len() - 1;
        let top = view_top.min(last);
        let bottom = (top + available_text.max(1) - 1).min(last);
        let (lo, hi) = match reach {
            Reach::Visible => (top, bottom),
            Reach::OneBeyond => (top.saturating_sub(1), (bottom + 1).min(last)),
        };
        let rel = screen_y as isize - editor_row as isize;
        let vis_idx = top.saturating_add_signed(rel).clamp(lo, hi);
        let (buf_idx, ref range) = visuals[vis_idx];
        let rel_x = (screen_x as usize).saturating_sub(content_left);
        // Columns are cells, not chars: a click lands in front of the
        // character drawn under it, whatever its width. Clicks past the end
        // of a wrapped row clamp to that row's end (rows vary in length
        // under word wrap).
        let cell_in_row = rel_x.saturating_sub(prompt_width);
        let chunk = &lines[buf_idx][range.clone()];
        (buf_idx, range.start + crate::width::byte_at_cell(chunk, cell_in_row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::editor::view;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn click(lines: &[String], visuals: &[view::VisualRow], x: u16, y: u16) -> (usize, usize) {
        ScreenToBuffer {
            lines,
            visuals,
            screen_x: x,
            screen_y: y,
            editor_row: 5,
            view_top: 0,
            available_text: 10,
            prompt_width: 4,
            content_left: 0,
            reach: Reach::Visible,
        }
        .resolve()
    }

    fn click_in_view(visuals: &[view::VisualRow], y: u16, view_top: usize, reach: Reach) -> usize {
        let l: Vec<String> = (0..visuals.len()).map(|i| format!("line {i}")).collect();
        ScreenToBuffer {
            lines: &l,
            visuals,
            screen_x: 4,
            screen_y: y,
            editor_row: 5,
            view_top,
            available_text: 10,
            prompt_width: 4,
            content_left: 0,
            reach,
        }
        .resolve()
        .0
    }

    /// REVIEW №194: a click on the footer or the margin below the text put
    /// the cursor on a row scrolled out of sight, and the view jumped to it.
    /// A click lands on the nearest visible row; only a drag reaches one
    /// row past the edge, which is what scrolls the view while selecting.
    #[test]
    fn clicks_stay_on_the_visible_rows() {
        let l: Vec<String> = (0..60).map(|i| format!("line {i}")).collect();
        let v = view::compute_visuals(&l, 40);
        // Rows 5..=14 show visual rows 20..=29.
        assert_eq!(click_in_view(&v, 5, 20, Reach::Visible), 20);
        assert_eq!(click_in_view(&v, 14, 20, Reach::Visible), 29);
        assert_eq!(click_in_view(&v, 21, 20, Reach::Visible), 29); // footer
        assert_eq!(click_in_view(&v, 1, 20, Reach::Visible), 20); // top margin
        assert_eq!(click_in_view(&v, 21, 20, Reach::OneBeyond), 30);
        assert_eq!(click_in_view(&v, 1, 20, Reach::OneBeyond), 19);
        // At the ends of the text there is nothing beyond.
        assert_eq!(click_in_view(&v, 1, 0, Reach::OneBeyond), 0);
        assert_eq!(click_in_view(&v, 21, 50, Reach::OneBeyond), 59);
    }

    /// REVIEW №10: the mapping added the column to the row's first char
    /// index, so every wide character on the row shifted the target. A
    /// column now lands in front of the character drawn under it.
    #[test]
    fn a_click_lands_in_front_of_the_character_under_it() {
        let l = lines(&["日本語 text"]);
        let v = view::compute_visuals(&l, 20);
        // Text starts at content_left + prompt_width = column 4.
        assert_eq!(click(&l, &v, 4, 5), (0, 0));
        // Both cells of a wide char point at its start.
        assert_eq!(click(&l, &v, 5, 5), (0, 0));
        assert_eq!(click(&l, &v, 6, 5), (0, 3));
        assert_eq!(click(&l, &v, 10, 5), (0, 9)); // the space
        assert_eq!(click(&l, &v, 11, 5), (0, 10)); // "t"
        // Past the end of the line clamps to its end.
        assert_eq!(click(&l, &v, 60, 5), (0, l[0].len()));
    }

    /// A click on a wrapped row is relative to that row, and a click below
    /// the last row lands on it.
    #[test]
    fn clicks_follow_the_wrapped_rows() {
        let l = lines(&["日本語のテキスト"]); // 16 cells
        let v = view::compute_visuals(&l, 10); // "日本語のテ" / "キスト"
        assert_eq!(v.len(), 2);
        assert_eq!(click(&l, &v, 4, 6), (0, "日本語のテ".len()));
        assert_eq!(click(&l, &v, 6, 6), (0, "日本語のテキ".len()));
        assert_eq!(click(&l, &v, 4, 99), (0, "日本語のテ".len()));
    }
}
