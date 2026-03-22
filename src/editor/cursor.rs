/// Cursor position, selection, and cursor state for the editor.

/// A 0-indexed position in a text document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    pub fn zero() -> Self {
        Self { line: 0, col: 0 }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line.cmp(&other.line).then(self.col.cmp(&other.col))
    }
}

/// A selection in the document.
///
/// `anchor` is where the selection started, `head` is where the cursor currently is.
/// When the user shifts-arrows to the left, head < anchor; to the right, head > anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    /// Returns `true` if the selection is empty (anchor == head).
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns `(start, end)` in document order regardless of anchor/head direction.
    pub fn ordered(&self) -> (Position, Position) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Returns `true` if the given position is inside the selection (inclusive of start,
    /// exclusive of end).
    pub fn contains(&self, pos: Position) -> bool {
        let (start, end) = self.ordered();
        pos >= start && pos < end
    }
}

/// Full cursor state including position, optional selection, and the desired column
/// for vertical movement.
///
/// `desired_col` preserves the column when moving vertically through lines that are
/// shorter than the column the user started on. For example, moving down from column 20
/// through a line that is only 5 characters long should restore column 20 on the next
/// sufficiently long line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorState {
    pub position: Position,
    pub selection: Option<Selection>,
    pub desired_col: usize,
}

impl CursorState {
    pub fn new() -> Self {
        Self {
            position: Position::zero(),
            selection: None,
            desired_col: 0,
        }
    }

    /// Create a cursor state at the given position with no selection.
    pub fn at(line: usize, col: usize) -> Self {
        Self {
            position: Position::new(line, col),
            selection: None,
            desired_col: col,
        }
    }

    /// Move the cursor to a new position, clearing any selection and updating
    /// `desired_col`.
    pub fn move_to(&mut self, line: usize, col: usize) {
        self.position = Position::new(line, col);
        self.selection = None;
        self.desired_col = col;
    }

    /// Move the cursor vertically, clamping the column to the given line length but
    /// preserving `desired_col` so that moving back to a longer line restores it.
    pub fn move_vertically(&mut self, line: usize, line_len: usize) {
        let col = self.desired_col.min(line_len);
        self.position = Position::new(line, col);
        self.selection = None;
        // desired_col intentionally NOT updated
    }

    /// Start or extend a selection to the current position.
    pub fn select_to(&mut self, line: usize, col: usize) {
        let anchor = match &self.selection {
            Some(sel) => sel.anchor,
            None => self.position,
        };
        self.position = Position::new(line, col);
        self.selection = Some(Selection::new(anchor, self.position));
        self.desired_col = col;
    }

    /// Clear the current selection without moving the cursor.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Returns `true` if there is a non-empty selection.
    pub fn has_selection(&self) -> bool {
        self.selection.map_or(false, |s| !s.is_empty())
    }
}

impl Default for CursorState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Position tests ──────────────────────────────────────────────

    #[test]
    fn position_new_and_zero() {
        let p = Position::new(3, 7);
        assert_eq!(p.line, 3);
        assert_eq!(p.col, 7);

        let z = Position::zero();
        assert_eq!(z.line, 0);
        assert_eq!(z.col, 0);
    }

    #[test]
    fn position_ordering() {
        let a = Position::new(0, 5);
        let b = Position::new(1, 0);
        let c = Position::new(1, 3);
        let d = Position::new(1, 3);

        assert!(a < b);
        assert!(b < c);
        assert_eq!(c, d);
        assert!(a < c);
    }

    #[test]
    fn position_ordering_same_line() {
        let a = Position::new(2, 0);
        let b = Position::new(2, 5);
        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn position_eq() {
        assert_eq!(Position::new(0, 0), Position::zero());
        assert_ne!(Position::new(0, 1), Position::zero());
    }

    // ── Selection tests ─────────────────────────────────────────────

    #[test]
    fn selection_is_empty() {
        let sel = Selection::new(Position::new(1, 2), Position::new(1, 2));
        assert!(sel.is_empty());

        let sel2 = Selection::new(Position::new(1, 2), Position::new(1, 3));
        assert!(!sel2.is_empty());
    }

    #[test]
    fn selection_ordered_forward() {
        let anchor = Position::new(1, 0);
        let head = Position::new(2, 5);
        let sel = Selection::new(anchor, head);
        let (start, end) = sel.ordered();
        assert_eq!(start, anchor);
        assert_eq!(end, head);
    }

    #[test]
    fn selection_ordered_backward() {
        let anchor = Position::new(3, 10);
        let head = Position::new(1, 2);
        let sel = Selection::new(anchor, head);
        let (start, end) = sel.ordered();
        assert_eq!(start, head);
        assert_eq!(end, anchor);
    }

    #[test]
    fn selection_contains_inside() {
        let sel = Selection::new(Position::new(1, 0), Position::new(1, 10));
        assert!(sel.contains(Position::new(1, 0)));
        assert!(sel.contains(Position::new(1, 5)));
        assert!(sel.contains(Position::new(1, 9)));
    }

    #[test]
    fn selection_contains_excludes_end() {
        let sel = Selection::new(Position::new(1, 0), Position::new(1, 10));
        assert!(!sel.contains(Position::new(1, 10)));
    }

    #[test]
    fn selection_contains_outside() {
        let sel = Selection::new(Position::new(1, 5), Position::new(3, 2));
        assert!(!sel.contains(Position::new(0, 0)));
        assert!(!sel.contains(Position::new(1, 4)));
        assert!(!sel.contains(Position::new(3, 2)));
        assert!(!sel.contains(Position::new(4, 0)));
    }

    #[test]
    fn selection_contains_multiline() {
        let sel = Selection::new(Position::new(1, 5), Position::new(3, 2));
        assert!(sel.contains(Position::new(1, 5)));
        assert!(sel.contains(Position::new(2, 0)));
        assert!(sel.contains(Position::new(2, 100)));
        assert!(sel.contains(Position::new(3, 1)));
    }

    #[test]
    fn selection_contains_backward_selection() {
        // anchor after head — should still work via ordered()
        let sel = Selection::new(Position::new(3, 2), Position::new(1, 5));
        assert!(sel.contains(Position::new(1, 5)));
        assert!(sel.contains(Position::new(2, 0)));
        assert!(!sel.contains(Position::new(3, 2)));
    }

    // ── CursorState tests ───────────────────────────────────────────

    #[test]
    fn cursor_state_default() {
        let cs = CursorState::new();
        assert_eq!(cs.position, Position::zero());
        assert!(cs.selection.is_none());
        assert_eq!(cs.desired_col, 0);
    }

    #[test]
    fn cursor_state_at() {
        let cs = CursorState::at(5, 10);
        assert_eq!(cs.position, Position::new(5, 10));
        assert_eq!(cs.desired_col, 10);
        assert!(cs.selection.is_none());
    }

    #[test]
    fn cursor_state_move_to() {
        let mut cs = CursorState::at(0, 0);
        cs.select_to(0, 5);
        assert!(cs.has_selection());

        cs.move_to(2, 3);
        assert_eq!(cs.position, Position::new(2, 3));
        assert!(!cs.has_selection());
        assert_eq!(cs.desired_col, 3);
    }

    #[test]
    fn cursor_state_move_vertically_preserves_desired_col() {
        let mut cs = CursorState::at(0, 20);
        // Move to a short line (length 5): column clamped but desired_col preserved
        cs.move_vertically(1, 5);
        assert_eq!(cs.position, Position::new(1, 5));
        assert_eq!(cs.desired_col, 20);

        // Move to a longer line: column restored
        cs.move_vertically(2, 30);
        assert_eq!(cs.position, Position::new(2, 20));
        assert_eq!(cs.desired_col, 20);
    }

    #[test]
    fn cursor_state_move_vertically_clears_selection() {
        let mut cs = CursorState::at(0, 5);
        cs.select_to(0, 10);
        assert!(cs.has_selection());

        cs.move_vertically(1, 20);
        assert!(!cs.has_selection());
    }

    #[test]
    fn cursor_state_select_to_creates_selection() {
        let mut cs = CursorState::at(1, 0);
        cs.select_to(1, 5);
        assert!(cs.has_selection());
        let sel = cs.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(1, 0));
        assert_eq!(sel.head, Position::new(1, 5));
    }

    #[test]
    fn cursor_state_select_to_extends_selection() {
        let mut cs = CursorState::at(1, 0);
        cs.select_to(1, 5);
        cs.select_to(2, 3);
        let sel = cs.selection.unwrap();
        // Anchor stays at original position
        assert_eq!(sel.anchor, Position::new(1, 0));
        assert_eq!(sel.head, Position::new(2, 3));
    }

    #[test]
    fn cursor_state_clear_selection() {
        let mut cs = CursorState::at(0, 0);
        cs.select_to(1, 5);
        assert!(cs.has_selection());
        cs.clear_selection();
        assert!(!cs.has_selection());
        // Position unchanged
        assert_eq!(cs.position, Position::new(1, 5));
    }

    #[test]
    fn cursor_state_has_selection_false_for_empty() {
        let mut cs = CursorState::at(1, 5);
        cs.select_to(1, 5); // same position => empty selection
        assert!(!cs.has_selection());
    }

    #[test]
    fn cursor_state_default_trait() {
        let cs: CursorState = Default::default();
        assert_eq!(cs, CursorState::new());
    }
}
