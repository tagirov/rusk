//! Undo / redo ring buffers for the interactive editor.

#[derive(Clone)]
pub(super) struct Snapshot {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub anchor: Option<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OpKind {
    InsertChar,
    Backspace,
    DeleteChar,
    Other,
}

pub(super) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: OpKind,
    cap: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            last: OpKind::Other,
            cap: 200,
        }
    }

    /// Push `snap` onto the undo stack unless this op should be coalesced
    /// with the previous one (consecutive single-char edits of the same kind).
    pub fn record(&mut self, snap: Snapshot, op: OpKind) {
        let coalesce =
            matches!(op, OpKind::InsertChar | OpKind::Backspace) && self.last == op;
        if !coalesce {
            self.undo.push(snap);
            if self.undo.len() > self.cap {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.last = op;
    }

    pub fn break_run(&mut self) {
        self.last = OpKind::Other;
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let s = self.undo.pop()?;
        self.redo.push(current);
        self.last = OpKind::Other;
        Some(s)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let s = self.redo.pop()?;
        self.undo.push(current);
        self.last = OpKind::Other;
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str) -> Snapshot {
        Snapshot {
            lines: vec![text.to_string()],
            row: 0,
            col: text.len(),
            anchor: None,
        }
    }

    #[test]
    fn record_coalesces_consecutive_same_kind_inserts() {
        let mut h = History::new();
        h.record(snap("a"), OpKind::InsertChar);
        h.record(snap("ab"), OpKind::InsertChar);
        h.record(snap("abc"), OpKind::InsertChar);
        let current = snap("abcd");
        let restored = h.undo(current.clone()).expect("undo");
        // Only the first snapshot is preserved due to coalescing.
        assert_eq!(restored.lines, vec!["a".to_string()]);
        assert!(h.undo(restored).is_none());
    }

    #[test]
    fn record_does_not_coalesce_across_op_kinds() {
        let mut h = History::new();
        h.record(snap("a"), OpKind::InsertChar);
        h.record(snap("ab"), OpKind::Other);
        let current = snap("abc");
        let first = h.undo(current).expect("undo 1");
        assert_eq!(first.lines, vec!["ab".to_string()]);
        let second = h.undo(first).expect("undo 2");
        assert_eq!(second.lines, vec!["a".to_string()]);
    }

    #[test]
    fn record_does_not_coalesce_delete_char() {
        let mut h = History::new();
        h.record(snap("abc"), OpKind::DeleteChar);
        h.record(snap("ab"), OpKind::DeleteChar);
        let current = snap("a");
        let first = h.undo(current).expect("undo 1");
        assert_eq!(first.lines, vec!["ab".to_string()]);
        let second = h.undo(first).expect("undo 2");
        assert_eq!(second.lines, vec!["abc".to_string()]);
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut h = History::new();
        h.record(snap("a"), OpKind::Other);
        let current = snap("ab");
        let undone = h.undo(current.clone()).expect("undo");
        assert_eq!(undone.lines, vec!["a".to_string()]);
        let redone = h.redo(undone).expect("redo");
        assert_eq!(redone.lines, current.lines);
    }

    #[test]
    fn break_run_prevents_next_coalesce() {
        let mut h = History::new();
        h.record(snap("a"), OpKind::InsertChar);
        h.break_run();
        h.record(snap("ab"), OpKind::InsertChar);
        let current = snap("abc");
        let first = h.undo(current).expect("undo 1");
        assert_eq!(first.lines, vec!["ab".to_string()]);
        let second = h.undo(first).expect("undo 2");
        assert_eq!(second.lines, vec!["a".to_string()]);
    }

    #[test]
    fn recording_clears_redo_stack() {
        let mut h = History::new();
        h.record(snap("a"), OpKind::Other);
        let _ = h.undo(snap("ab"));
        // Now `redo` has one entry. A fresh record() should wipe it.
        h.record(snap("ax"), OpKind::Other);
        assert!(h.redo(snap("ay")).is_none());
    }
}

