//! Text measured in terminal cells instead of `char`s.
//!
//! Every layout decision that ends up on a terminal - the wrapped task text
//! in `rusk list`, the delete dialog, the editor's soft wrap, its cursor and
//! its mouse mapping - asks this module how wide a string is. A `char` is not
//! a cell: CJK and most emoji take two, combining marks and joiners take
//! none. Text is also only ever cut between grapheme clusters, so a family
//! emoji, a flag or a letter with its accent never splits across two rows.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Display width of `s` in terminal cells.
pub fn width(s: &str) -> usize {
    s.graphemes(true).map(cluster_width).sum()
}

/// Display width of one grapheme cluster. Whole-cluster measurement is what
/// makes ZWJ sequences (`👨‍👩‍👧`) and flags count as the two cells a terminal
/// gives them rather than the sum of their parts.
pub fn cluster_width(cluster: &str) -> usize {
    UnicodeWidthStr::width(cluster)
}

/// Grapheme clusters of `s` as `(byte offset, cluster, width in cells)`.
pub fn clusters(s: &str) -> impl Iterator<Item = (usize, &str, usize)> {
    s.grapheme_indices(true)
        .map(|(offset, g)| (offset, g, cluster_width(g)))
}

/// Cells taken by `s[..byte_idx]`. `byte_idx` may land inside a cluster (the
/// editor moves the cursor by `char`): the part before it is measured as it
/// stands. An index inside a `char` is rounded down to its start rather than
/// panicking on the slice.
pub fn cells_before(s: &str, byte_idx: usize) -> usize {
    let mut idx = byte_idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    width(&s[..idx])
}

/// Byte length of the longest prefix of `s` that fits into `max` cells.
/// Always includes at least one cluster when `s` is non-empty, so a caller
/// splitting text into chunks cannot loop forever on a cluster wider than
/// `max`.
pub fn prefix_within(s: &str, max: usize) -> usize {
    let mut used = 0usize;
    let mut end = 0usize;
    for (offset, g, w) in clusters(s) {
        if end > 0 && used + w > max {
            return end;
        }
        used += w;
        end = offset + g.len();
    }
    end
}

/// Byte index of the cluster boundary at cell column `cell`, clamped to the
/// end of `s`. A column inside a wide cluster maps to that cluster's start,
/// so a click anywhere on a CJK character puts the cursor in front of it.
pub fn byte_at_cell(s: &str, cell: usize) -> usize {
    let mut used = 0usize;
    for (offset, _, w) in clusters(s) {
        if used + w > cell {
            return offset;
        }
        used += w;
    }
    s.len()
}

/// Byte index of the cluster boundary at or before `byte_idx`.
pub fn cluster_start(s: &str, byte_idx: usize) -> usize {
    let idx = byte_idx.min(s.len());
    let mut start = 0usize;
    for (offset, g) in s.grapheme_indices(true) {
        if offset >= idx {
            break;
        }
        if offset + g.len() > idx {
            return offset;
        }
        start = offset + g.len();
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_not_a_char() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("日本語"), 6);
        assert_eq!(width("🍎"), 2);
        // ZWJ sequence and flag: one cluster, two cells.
        assert_eq!(width("👨\u{200d}👩\u{200d}👧"), 2);
        assert_eq!(width("🇺🇸"), 2);
        // Base plus combining accent: one cell.
        assert_eq!(width("e\u{301}"), 1);
        assert_eq!(width(""), 0);
    }

    #[test]
    fn prefixes_stop_on_cluster_boundaries() {
        let s = "日本語";
        assert_eq!(prefix_within(s, 0), "日".len()); // never empty
        assert_eq!(prefix_within(s, 1), "日".len()); // half a cell does not fit
        assert_eq!(prefix_within(s, 2), "日".len());
        assert_eq!(prefix_within(s, 5), "日本".len());
        assert_eq!(prefix_within(s, 6), s.len());
        let fam = "👨\u{200d}👩\u{200d}👧x";
        assert_eq!(prefix_within(fam, 2), fam.len() - 1);
    }

    #[test]
    fn cells_and_bytes_map_both_ways() {
        let s = "a日x";
        assert_eq!(cells_before(s, 0), 0);
        assert_eq!(cells_before(s, 1), 1);
        assert_eq!(cells_before(s, 1 + "日".len()), 3);
        assert_eq!(byte_at_cell(s, 0), 0);
        assert_eq!(byte_at_cell(s, 1), 1);
        // Both cells of the wide char point at its start.
        assert_eq!(byte_at_cell(s, 2), 1);
        assert_eq!(byte_at_cell(s, 3), 1 + "日".len());
        assert_eq!(byte_at_cell(s, 99), s.len());
    }

    #[test]
    fn cluster_start_walks_back_into_a_cluster() {
        let s = "e\u{301}x";
        assert_eq!(cluster_start(s, 0), 0);
        assert_eq!(cluster_start(s, 1), 0); // inside the accented cluster
        assert_eq!(cluster_start(s, 3), 3);
    }
}
