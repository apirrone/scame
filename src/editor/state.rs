use crate::buffer::Position;

/// Cursor state in the editor
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
    /// Desired column for vertical movement (preserves column when moving through shorter lines)
    pub desired_column: usize,
}

impl Cursor {
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            desired_column: column,
        }
    }

    pub fn zero() -> Self {
        Self {
            line: 0,
            column: 0,
            desired_column: 0,
        }
    }

    pub fn position(&self) -> Position {
        Position::new(self.line, self.column)
    }

    pub fn set_position(&mut self, pos: Position) {
        self.line = pos.line;
        self.column = pos.column;
        self.desired_column = pos.column;
    }

    pub fn move_to(&mut self, line: usize, column: usize) {
        self.line = line;
        self.column = column;
        self.desired_column = column;
    }

    /// Move vertically while preserving desired column
    pub fn move_vertical(&mut self, line: usize, max_column: usize) {
        self.line = line;
        self.column = self.desired_column.min(max_column);
    }

    /// Move horizontally and update desired column
    pub fn move_horizontal(&mut self, column: usize) {
        self.column = column;
        self.desired_column = column;
    }
}

/// Text selection in the editor
#[derive(Debug, Clone, Copy)]
pub struct Selection {
    /// The anchor point where selection started
    pub anchor: Position,
    /// The current cursor position (head of selection)
    pub head: Position,
}

impl Selection {
    pub fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    /// Get the start and end positions in document order
    pub fn range(&self) -> (Position, Position) {
        if self.anchor.line < self.head.line
            || (self.anchor.line == self.head.line && self.anchor.column <= self.head.column)
        {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Check if selection is empty (anchor == head)
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Get the start position
    pub fn start(&self) -> Position {
        self.range().0
    }

    /// Get the end position
    pub fn end(&self) -> Position {
        self.range().1
    }
}

/// Viewport state (visible area of the buffer)
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: u16,
    pub height: u16,
    /// Top visible line
    pub top_line: usize,
    /// Left visible column (for horizontal scrolling)
    pub left_column: usize,
}

impl Viewport {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            top_line: 0,
            left_column: 0,
        }
    }

    /// Get the bottom visible line
    pub fn bottom_line(&self) -> usize {
        self.top_line + self.height as usize
    }

    /// Check if a line is visible
    pub fn is_line_visible(&self, line: usize) -> bool {
        line >= self.top_line && line < self.bottom_line()
    }

    /// Scroll to make a line visible
    pub fn scroll_to_line(&mut self, line: usize) {
        if line < self.top_line {
            self.top_line = line;
        } else if line >= self.bottom_line() {
            self.top_line = line.saturating_sub(self.height as usize - 1);
        }
    }

    /// Center the viewport on a line
    pub fn center_on_line(&mut self, line: usize) {
        self.top_line = line.saturating_sub(self.height as usize / 2);
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        self.top_line = self.top_line.saturating_sub(n);
    }

    /// Scroll down by n lines (clamped to reasonable bounds)
    pub fn scroll_down(&mut self, n: usize, max_lines: usize) {
        // Allow scrolling slightly past the last line for better UX,
        // but not infinitely
        let max_top_line = max_lines.saturating_sub(1);
        self.top_line = (self.top_line + n).min(max_top_line);
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }
}

/// Editor mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// Normal editing mode
    Normal,
    /// Insert mode
    Insert,
    /// Command mode (for command palette)
    Command,
    /// Search mode
    Search,
}

/// Complete editor state
pub struct EditorState {
    pub cursor: Cursor,
    pub selection: Option<Selection>,
    pub viewport: Viewport,
    pub mode: EditorMode,
}

impl EditorState {
    pub fn new(viewport_width: u16, viewport_height: u16) -> Self {
        Self {
            cursor: Cursor::zero(),
            selection: None,
            viewport: Viewport::new(viewport_width, viewport_height),
            mode: EditorMode::Normal,
        }
    }

    /// Start a selection at the current cursor position
    pub fn start_selection(&mut self) {
        if self.selection.is_none() {
            self.selection = Some(Selection::new(
                self.cursor.position(),
                self.cursor.position(),
            ));
        }
    }

    /// Update selection to current cursor position
    pub fn update_selection(&mut self) {
        if let Some(selection) = &mut self.selection {
            selection.head = self.cursor.position();
        }
    }

    /// Clear the selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        self.selection.is_some() && !self.selection.as_ref().unwrap().is_empty()
    }

    /// Ensure cursor is visible in viewport
    pub fn ensure_cursor_visible(&mut self) {
        self.viewport.scroll_to_line(self.cursor.line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_movement() {
        let mut cursor = Cursor::zero();
        cursor.move_to(5, 10);
        assert_eq!(cursor.line, 5);
        assert_eq!(cursor.column, 10);
        assert_eq!(cursor.desired_column, 10);
    }

    #[test]
    fn test_cursor_vertical_movement() {
        let mut cursor = Cursor::new(0, 10);
        cursor.move_vertical(1, 5); // Move to shorter line
        assert_eq!(cursor.line, 1);
        assert_eq!(cursor.column, 5); // Clamped to max
        assert_eq!(cursor.desired_column, 10); // Preserved

        cursor.move_vertical(2, 20); // Move to longer line
        assert_eq!(cursor.column, 10); // Restored to desired
    }

    #[test]
    fn test_selection_range() {
        let sel = Selection::new(Position::new(5, 10), Position::new(3, 5));
        let (start, end) = sel.range();
        assert_eq!(start, Position::new(3, 5));
        assert_eq!(end, Position::new(5, 10));
    }

    #[test]
    fn test_viewport_scrolling() {
        let mut viewport = Viewport::new(80, 24);
        viewport.scroll_to_line(30);
        assert!(viewport.is_line_visible(30));

        viewport.center_on_line(50);
        assert_eq!(viewport.top_line, 50 - 12); // Centered
    }

    #[test]
    fn test_editor_selection() {
        let mut state = EditorState::new(80, 24);
        assert!(!state.has_selection());

        state.start_selection();
        assert!(!state.has_selection()); // Empty selection

        state.cursor.move_to(1, 5);
        state.update_selection();
        assert!(state.has_selection()); // Now has selection
    }
}
