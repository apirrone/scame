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

    /// Helper: check if character is a word boundary
    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Move to the start of the previous word
    pub fn move_word_left(state: &mut EditorState, buffer: &TextBuffer) {
        let mut line = state.cursor.line;
        let mut col = state.cursor.column;

        // Get current line text
        if let Some(line_text) = buffer.get_line(line) {
            let chars: Vec<char> = line_text.chars().collect();

            // Skip trailing whitespace at current position
            while col > 0 && col <= chars.len() && chars.get(col.saturating_sub(1)).map_or(false, |&c| c.is_whitespace()) {
                col -= 1;
            }

            // Move to start of current word
            if col > 0 && col <= chars.len() {
                let in_word = chars.get(col.saturating_sub(1)).map_or(false, |&c| Self::is_word_char(c));
                while col > 0 {
                    if let Some(&prev_char) = chars.get(col - 1) {
                        if Self::is_word_char(prev_char) != in_word {
                            break;
                        }
                        col -= 1;
                    } else {
                        break;
                    }
                }
            }

            // If we're at the start of the line, move to previous line
            if col == 0 && line > 0 {
                line -= 1;
                col = buffer.line_len(line);
            }
        }

        state.cursor.move_to(line, col);
        state.ensure_cursor_visible();
    }

    /// Move to the start of the next word
    pub fn move_word_right(state: &mut EditorState, buffer: &TextBuffer) {
        let mut line = state.cursor.line;
        let mut col = state.cursor.column;

        // Get current line text
        if let Some(line_text) = buffer.get_line(line) {
            let chars: Vec<char> = line_text.chars().collect();
            let line_len = chars.len();

            // Skip current word
            if col < line_len {
                let in_word = chars.get(col).map_or(false, |&c| Self::is_word_char(c));
                while col < line_len {
                    if let Some(&current_char) = chars.get(col) {
                        if Self::is_word_char(current_char) != in_word {
                            break;
                        }
                        col += 1;
                    } else {
                        break;
                    }
                }
            }

            // Skip whitespace
            while col < line_len && chars.get(col).map_or(false, |&c| c.is_whitespace()) {
                col += 1;
            }

            // If we're at the end of the line, move to next line
            if col >= line_len && line + 1 < buffer.len_lines() {
                line += 1;
                col = 0;
            }
        }

        state.cursor.move_to(line, col);
        state.ensure_cursor_visible();
    }

    /// Helper: check if line is blank (only whitespace)
    fn is_blank_line(buffer: &TextBuffer, line: usize) -> bool {
        if let Some(line_text) = buffer.get_line(line) {
            line_text.trim().is_empty()
        } else {
            true
        }
    }

    /// Move up to the previous code block (previous blank line or start of buffer)
    pub fn move_block_up(state: &mut EditorState, buffer: &TextBuffer) {
        let mut line = state.cursor.line;

        if line == 0 {
            return;
        }

        // If we're on a blank line, skip all blank lines
        while line > 0 && Self::is_blank_line(buffer, line) {
            line -= 1;
        }

        // Now skip non-blank lines to find the previous blank line
        while line > 0 && !Self::is_blank_line(buffer, line) {
            line -= 1;
        }

        // Move to the first non-blank line of this block
        if line > 0 || !Self::is_blank_line(buffer, 0) {
            while line > 0 && Self::is_blank_line(buffer, line) {
                line -= 1;
            }
        }

        state.cursor.move_to(line, 0);
        state.ensure_cursor_visible();
    }

    /// Move down to the next code block (next blank line or end of buffer)
    pub fn move_block_down(state: &mut EditorState, buffer: &TextBuffer) {
        let mut line = state.cursor.line;
        let max_line = buffer.len_lines().saturating_sub(1);

        if line >= max_line {
            return;
        }

        // If we're on a blank line, skip all blank lines
        while line < max_line && Self::is_blank_line(buffer, line) {
            line += 1;
        }

        // Now skip non-blank lines to find the next blank line
        while line < max_line && !Self::is_blank_line(buffer, line) {
            line += 1;
        }

        // Skip blank lines to get to the start of next block
        while line < max_line && Self::is_blank_line(buffer, line) {
            line += 1;
        }

        state.cursor.move_to(line, 0);
        state.ensure_cursor_visible();
    }
}
