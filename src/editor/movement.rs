use crate::buffer::TextBuffer;
use crate::editor::EditorState;

/// Cursor movement operations
pub struct Movement;

impl Movement {
    /// Move cursor left
    pub fn move_left(state: &mut EditorState, buffer: &TextBuffer) {
        if state.cursor.column > 0 {
            state.cursor.move_horizontal(state.cursor.column - 1);
        } else if state.cursor.line > 0 {
            // Move to end of previous line
            state.cursor.line -= 1;
            let line_len = buffer.line_len(state.cursor.line);
            state.cursor.move_horizontal(line_len);
        }
        state.ensure_cursor_visible();
    }

    /// Move cursor right
    pub fn move_right(state: &mut EditorState, buffer: &TextBuffer) {
        let line_len = buffer.line_len(state.cursor.line);
        if state.cursor.column < line_len {
            state.cursor.move_horizontal(state.cursor.column + 1);
        } else if state.cursor.line + 1 < buffer.len_lines() {
            // Move to start of next line
            state.cursor.line += 1;
            state.cursor.move_horizontal(0);
        }
        state.ensure_cursor_visible();
    }

    /// Move cursor up
    pub fn move_up(state: &mut EditorState, buffer: &TextBuffer) {
        if state.cursor.line > 0 {
            let new_line = state.cursor.line - 1;
            let line_len = buffer.line_len(new_line);
            state.cursor.move_vertical(new_line, line_len);
        }
        state.ensure_cursor_visible();
    }

    /// Move cursor down
    pub fn move_down(state: &mut EditorState, buffer: &TextBuffer) {
        if state.cursor.line + 1 < buffer.len_lines() {
            let new_line = state.cursor.line + 1;
            let line_len = buffer.line_len(new_line);
            state.cursor.move_vertical(new_line, line_len);
        }
        state.ensure_cursor_visible();
    }

    /// Move to start of line
    pub fn move_to_line_start(state: &mut EditorState) {
        state.cursor.move_horizontal(0);
        state.ensure_cursor_visible();
    }

    /// Move to end of line
    pub fn move_to_line_end(state: &mut EditorState, buffer: &TextBuffer) {
        let line_len = buffer.line_len(state.cursor.line);
        state.cursor.move_horizontal(line_len);
        state.ensure_cursor_visible();
    }

    /// Move to start of buffer
    pub fn move_to_start(state: &mut EditorState) {
        state.cursor.move_to(0, 0);
        state.ensure_cursor_visible();
    }

    /// Move to end of buffer
    pub fn move_to_end(state: &mut EditorState, buffer: &TextBuffer) {
        let last_line = buffer.len_lines().saturating_sub(1);
        let line_len = buffer.line_len(last_line);
        state.cursor.move_to(last_line, line_len);
        state.ensure_cursor_visible();
    }

    /// Jump to a specific line
    pub fn jump_to_line(state: &mut EditorState, buffer: &TextBuffer, line: usize) {
        let line = line.min(buffer.len_lines().saturating_sub(1));
        let line_len = buffer.line_len(line);
        let column = state.cursor.desired_column.min(line_len);
        state.cursor.move_to(line, column);
        state.viewport.center_on_line(line);
    }

    /// Page up
    pub fn page_up(state: &mut EditorState, buffer: &TextBuffer) {
        let page_size = state.viewport.height as usize;
        let new_line = state.cursor.line.saturating_sub(page_size);
        let line_len = buffer.line_len(new_line);
        state.cursor.move_vertical(new_line, line_len);
        state.viewport.scroll_up(page_size);
    }

    /// Page down
    pub fn page_down(state: &mut EditorState, buffer: &TextBuffer) {
        let page_size = state.viewport.height as usize;
        let new_line = (state.cursor.line + page_size).min(buffer.len_lines().saturating_sub(1));
        let line_len = buffer.line_len(new_line);
        state.cursor.move_vertical(new_line, line_len);
        state.viewport.scroll_down(page_size);
    }
}
