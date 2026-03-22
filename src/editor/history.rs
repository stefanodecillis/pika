/// Undo/redo history with grouped edits.

/// A single atomic edit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Text was inserted at the given (line, col) position.
    Insert { pos: (usize, usize), text: String },
    /// Text was deleted starting at the given (line, col) position.
    Delete { pos: (usize, usize), text: String },
}

impl Edit {
    /// Create an Insert edit.
    pub fn insert(line: usize, col: usize, text: impl Into<String>) -> Self {
        Edit::Insert {
            pos: (line, col),
            text: text.into(),
        }
    }

    /// Create a Delete edit.
    pub fn delete(line: usize, col: usize, text: impl Into<String>) -> Self {
        Edit::Delete {
            pos: (line, col),
            text: text.into(),
        }
    }

    /// Return the inverse of this edit (for undo).
    pub fn invert(&self) -> Self {
        match self {
            Edit::Insert { pos, text } => Edit::Delete {
                pos: *pos,
                text: text.clone(),
            },
            Edit::Delete { pos, text } => Edit::Insert {
                pos: *pos,
                text: text.clone(),
            },
        }
    }
}

/// Undo/redo history that groups related edits together.
///
/// Each entry in the undo/redo stacks is a `Vec<Edit>` representing a group
/// of related edits that should be undone/redone as a unit.
#[derive(Debug)]
pub struct UndoHistory {
    undo_stack: Vec<Vec<Edit>>,
    redo_stack: Vec<Vec<Edit>>,
    /// When `Some`, edits are being accumulated into a group.
    current_group: Option<Vec<Edit>>,
}

impl UndoHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_group: None,
        }
    }

    /// Record a single edit. If a group is in progress, the edit is added to
    /// that group. Otherwise the edit becomes its own single-item group on the
    /// undo stack.
    ///
    /// Recording any new edit clears the redo stack.
    pub fn record(&mut self, edit: Edit) {
        self.redo_stack.clear();

        if let Some(ref mut group) = self.current_group {
            group.push(edit);
        } else {
            self.undo_stack.push(vec![edit]);
        }
    }

    /// Begin accumulating edits into a group. Nested calls are not supported;
    /// calling `begin_group` while a group is already open is a no-op.
    pub fn begin_group(&mut self) {
        if self.current_group.is_none() {
            self.current_group = Some(Vec::new());
        }
    }

    /// End the current group and push it onto the undo stack. If the group is
    /// empty, nothing is pushed. If no group is open, this is a no-op.
    pub fn end_group(&mut self) {
        if let Some(group) = self.current_group.take() {
            if !group.is_empty() {
                self.redo_stack.clear();
                self.undo_stack.push(group);
            }
        }
    }

    /// Pop the most recent edit group from the undo stack, push its inverse
    /// onto the redo stack, and return the inverted edits (in reverse order)
    /// so the caller can apply them.
    pub fn undo(&mut self) -> Option<Vec<Edit>> {
        let group = self.undo_stack.pop()?;
        // Invert and reverse: the last edit applied should be undone first.
        let inverted: Vec<Edit> = group.iter().rev().map(|e| e.invert()).collect();
        self.redo_stack.push(group);
        Some(inverted)
    }

    /// Pop the most recent edit group from the redo stack, push it back onto
    /// the undo stack, and return the original edits so the caller can
    /// re-apply them.
    pub fn redo(&mut self) -> Option<Vec<Edit>> {
        let group = self.redo_stack.pop()?;
        let edits = group.clone();
        self.undo_stack.push(group);
        Some(edits)
    }

    /// Returns `true` if there are entries on the undo stack.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns `true` if there are entries on the redo stack.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_group = None;
    }
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Edit tests ──────────────────────────────────────────────────

    #[test]
    fn edit_insert_constructor() {
        let e = Edit::insert(1, 2, "abc");
        assert_eq!(
            e,
            Edit::Insert {
                pos: (1, 2),
                text: "abc".to_string()
            }
        );
    }

    #[test]
    fn edit_delete_constructor() {
        let e = Edit::delete(3, 0, "xyz");
        assert_eq!(
            e,
            Edit::Delete {
                pos: (3, 0),
                text: "xyz".to_string()
            }
        );
    }

    #[test]
    fn edit_invert_insert_to_delete() {
        let e = Edit::insert(0, 5, "hello");
        let inv = e.invert();
        assert_eq!(
            inv,
            Edit::Delete {
                pos: (0, 5),
                text: "hello".to_string()
            }
        );
    }

    #[test]
    fn edit_invert_delete_to_insert() {
        let e = Edit::delete(2, 3, "world");
        let inv = e.invert();
        assert_eq!(
            inv,
            Edit::Insert {
                pos: (2, 3),
                text: "world".to_string()
            }
        );
    }

    #[test]
    fn edit_double_invert_is_identity() {
        let e = Edit::insert(0, 0, "hi");
        assert_eq!(e.invert().invert(), e);
    }

    // ── UndoHistory: basic record/undo/redo ─────────────────────────

    #[test]
    fn empty_history() {
        let h = UndoHistory::new();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn record_single_edit_creates_own_group() {
        let mut h = UndoHistory::new();
        h.record(Edit::insert(0, 0, "a"));
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_returns_inverted_edit() {
        let mut h = UndoHistory::new();
        h.record(Edit::insert(0, 0, "a"));
        let undone = h.undo().unwrap();
        assert_eq!(undone.len(), 1);
        assert_eq!(undone[0], Edit::delete(0, 0, "a"));
        assert!(!h.can_undo());
        assert!(h.can_redo());
    }

    #[test]
    fn redo_returns_original_edit() {
        let mut h = UndoHistory::new();
        h.record(Edit::insert(0, 0, "a"));
        h.undo();
        let redone = h.redo().unwrap();
        assert_eq!(redone.len(), 1);
        assert_eq!(redone[0], Edit::insert(0, 0, "a"));
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_on_empty_returns_none() {
        let mut h = UndoHistory::new();
        assert!(h.undo().is_none());
    }

    #[test]
    fn redo_on_empty_returns_none() {
        let mut h = UndoHistory::new();
        assert!(h.redo().is_none());
    }

    // ── New edit clears redo stack ───────────────────────────────────

    #[test]
    fn new_edit_clears_redo() {
        let mut h = UndoHistory::new();
        h.record(Edit::insert(0, 0, "a"));
        h.record(Edit::insert(0, 1, "b"));
        h.undo(); // redo has "b"
        assert!(h.can_redo());

        h.record(Edit::insert(0, 1, "c")); // new edit clears redo
        assert!(!h.can_redo());
    }

    // ── Grouped edits ───────────────────────────────────────────────

    #[test]
    fn grouped_edits_undo_together() {
        let mut h = UndoHistory::new();
        h.begin_group();
        h.record(Edit::insert(0, 0, "a"));
        h.record(Edit::insert(0, 1, "b"));
        h.record(Edit::insert(0, 2, "c"));
        h.end_group();

        let undone = h.undo().unwrap();
        assert_eq!(undone.len(), 3);
        // Should be reversed and inverted
        assert_eq!(undone[0], Edit::delete(0, 2, "c"));
        assert_eq!(undone[1], Edit::delete(0, 1, "b"));
        assert_eq!(undone[2], Edit::delete(0, 0, "a"));
    }

    #[test]
    fn grouped_edits_redo_together() {
        let mut h = UndoHistory::new();
        h.begin_group();
        h.record(Edit::insert(0, 0, "x"));
        h.record(Edit::insert(0, 1, "y"));
        h.end_group();

        h.undo();
        let redone = h.redo().unwrap();
        assert_eq!(redone.len(), 2);
        assert_eq!(redone[0], Edit::insert(0, 0, "x"));
        assert_eq!(redone[1], Edit::insert(0, 1, "y"));
    }

    #[test]
    fn empty_group_not_pushed() {
        let mut h = UndoHistory::new();
        h.begin_group();
        h.end_group();
        assert!(!h.can_undo());
    }

    #[test]
    fn end_group_without_begin_is_noop() {
        let mut h = UndoHistory::new();
        h.end_group(); // should not panic
        assert!(!h.can_undo());
    }

    #[test]
    fn nested_begin_group_is_noop() {
        let mut h = UndoHistory::new();
        h.begin_group();
        h.record(Edit::insert(0, 0, "a"));
        h.begin_group(); // no-op, group already open
        h.record(Edit::insert(0, 1, "b"));
        h.end_group();

        // Both edits should be in the same group
        let undone = h.undo().unwrap();
        assert_eq!(undone.len(), 2);
    }

    // ── Multiple undo/redo cycles ───────────────────────────────────

    #[test]
    fn multiple_undo_redo() {
        let mut h = UndoHistory::new();
        h.record(Edit::insert(0, 0, "first"));
        h.record(Edit::insert(0, 5, "second"));

        let u1 = h.undo().unwrap();
        assert_eq!(u1[0], Edit::delete(0, 5, "second"));

        let u2 = h.undo().unwrap();
        assert_eq!(u2[0], Edit::delete(0, 0, "first"));

        assert!(h.undo().is_none());

        let r1 = h.redo().unwrap();
        assert_eq!(r1[0], Edit::insert(0, 0, "first"));

        let r2 = h.redo().unwrap();
        assert_eq!(r2[0], Edit::insert(0, 5, "second"));

        assert!(h.redo().is_none());
    }

    // ── Clear ───────────────────────────────────────────────────────

    #[test]
    fn clear_removes_all_history() {
        let mut h = UndoHistory::new();
        h.record(Edit::insert(0, 0, "a"));
        h.undo();
        assert!(h.can_redo());

        h.clear();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn clear_aborts_open_group() {
        let mut h = UndoHistory::new();
        h.begin_group();
        h.record(Edit::insert(0, 0, "a"));
        h.clear();
        assert!(!h.can_undo());
        // end_group after clear should be a no-op
        h.end_group();
        assert!(!h.can_undo());
    }

    // ── Default trait ───────────────────────────────────────────────

    #[test]
    fn default_trait() {
        let h: UndoHistory = Default::default();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    // ── Mixed grouped and ungrouped ─────────────────────────────────

    #[test]
    fn mixed_grouped_and_ungrouped() {
        let mut h = UndoHistory::new();

        // Ungrouped edit
        h.record(Edit::insert(0, 0, "a"));

        // Grouped edit
        h.begin_group();
        h.record(Edit::insert(0, 1, "b"));
        h.record(Edit::insert(0, 2, "c"));
        h.end_group();

        // Another ungrouped edit
        h.record(Edit::insert(0, 3, "d"));

        // Undo "d" (single)
        let u1 = h.undo().unwrap();
        assert_eq!(u1.len(), 1);

        // Undo "bc" (group)
        let u2 = h.undo().unwrap();
        assert_eq!(u2.len(), 2);

        // Undo "a" (single)
        let u3 = h.undo().unwrap();
        assert_eq!(u3.len(), 1);

        assert!(h.undo().is_none());
    }
}
