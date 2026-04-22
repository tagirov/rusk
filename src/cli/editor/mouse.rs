//! Mouse-related state: multi-click tracking and screen-to-buffer mapping.

use std::time::Instant;

use super::text_ops;

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

/// Arguments for mapping a screen cell `(x, y)` to buffer `(row, byte_col)`.
pub(super) struct ScreenToBuffer<'a> {
    pub lines: &'a [String],
    pub visuals: &'a [(usize, String, usize)],
    pub vw: usize,
    pub screen_x: u16,
    pub screen_y: u16,
    pub editor_row: u16,
    pub view_top: usize,
    pub prompt_width: usize,
    /// Left padding of the text block (centering); mouse X is relative to the full screen.
    pub content_left: usize,
}

impl ScreenToBuffer<'_> {
    pub(super) fn resolve(self) -> (usize, usize) {
        let ScreenToBuffer {
            lines,
            visuals,
            vw,
            screen_x,
            screen_y,
            editor_row,
            view_top,
            prompt_width,
            content_left,
        } = self;
        if visuals.is_empty() || lines.is_empty() {
            return (0, 0);
        }
        let rel = (screen_y as i32 - editor_row as i32).max(0) as usize;
        let vis_idx = (view_top + rel).min(visuals.len() - 1);
        let (buf_idx, _, start_char) = visuals[vis_idx];
        let rel_x = (screen_x as usize).saturating_sub(content_left);
        let col_in_row = rel_x.saturating_sub(prompt_width);
        let target_char = start_char + col_in_row;
        let line_chars = lines[buf_idx].chars().count();
        let clamped = target_char.min(line_chars).min(start_char + vw);
        let clamped = clamped.min(line_chars);
        (buf_idx, text_ops::ml_char_to_byte(&lines[buf_idx], clamped))
    }
}
