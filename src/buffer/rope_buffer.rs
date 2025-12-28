use anyhow::Result;
use ropey::Rope;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    LF,   // Unix: \n
    CRLF, // Windows: \r\n
    CR,   // Old Mac: \r
}

impl LineEnding {
    pub fn as_str(&self) -> &str {
        match self {
            LineEnding::LF => "\n",
            LineEnding::CRLF => "\r\n",
            LineEnding::CR => "\r",
        }
    }

    pub fn detect(text: &str) -> Self {
        if text.contains("\r\n") {
            LineEnding::CRLF
        } else if text.contains('\n') {
            LineEnding::LF
        } else if text.contains('\r') {
            LineEnding::CR
        } else {
            // Default to system line ending
            #[cfg(windows)]
            return LineEnding::CRLF;
            #[cfg(not(windows))]
            return LineEnding::LF;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    pub fn zero() -> Self {
        Self { line: 0, column: 0 }
    }
}

pub struct TextBuffer {
    rope: Rope,
    file_path: Option<PathBuf>,
    modified: bool,
    line_ending: LineEnding,
}

impl TextBuffer {
    /// Create a new empty text buffer
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            file_path: None,
            modified: false,
            line_ending: LineEnding::LF,
        }
    }

    /// Load a file into the buffer
    pub fn from_file(path: PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(&path)?;
        let line_ending = LineEnding::detect(&content);
        let rope = Rope::from_str(&content);

        Ok(Self {
            rope,
            file_path: Some(path),
            modified: false,
            line_ending,
        })
    }

    /// Save the buffer to its file
    pub fn save(&mut self) -> Result<()> {
        if let Some(path) = &self.file_path {
            let content = self.rope.to_string();
            std::fs::write(path, content)?;
            self.modified = false;
            Ok(())
        } else {
            anyhow::bail!("No file path set for buffer")
        }
    }

    /// Save the buffer to a specific file
    pub fn save_as(&mut self, path: PathBuf) -> Result<()> {
        let content = self.rope.to_string();
        std::fs::write(&path, content)?;
        self.file_path = Some(path);
        self.modified = false;
        Ok(())
    }

    /// Insert text at a position
    pub fn insert(&mut self, pos: Position, text: &str) -> Result<()> {
        let char_idx = self.pos_to_char(pos)?;
        self.rope.insert(char_idx, text);
        self.modified = true;
        Ok(())
    }

    /// Insert a character at a position
    pub fn insert_char(&mut self, pos: Position, ch: char) -> Result<()> {
        let char_idx = self.pos_to_char(pos)?;
        self.rope.insert_char(char_idx, ch);
        self.modified = true;
        Ok(())
    }

    /// Delete a range of text
    pub fn delete_range(&mut self, start: Position, end: Position) -> Result<String> {
        let start_idx = self.pos_to_char(start)?;
        let end_idx = self.pos_to_char(end)?;

        if start_idx >= end_idx {
            return Ok(String::new());
        }

        let deleted = self.rope.slice(start_idx..end_idx).to_string();
        self.rope.remove(start_idx..end_idx);
        self.modified = true;
        Ok(deleted)
    }

    /// Delete a single character at position
    pub fn delete_char(&mut self, pos: Position) -> Result<Option<char>> {
        let char_idx = self.pos_to_char(pos)?;

        if char_idx >= self.rope.len_chars() {
            return Ok(None);
        }

        let ch = self.rope.char(char_idx);
        self.rope.remove(char_idx..char_idx + 1);
        self.modified = true;
        Ok(Some(ch))
    }

    /// Get a line of text
    pub fn get_line(&self, line: usize) -> Option<String> {
        if line >= self.rope.len_lines() {
            return None;
        }
        Some(self.rope.line(line).to_string())
    }

    /// Get a range of lines
    pub fn get_lines(&self, start: usize, end: usize) -> Vec<String> {
        let end = end.min(self.rope.len_lines());
        (start..end)
            .map(|i| self.rope.line(i).to_string())
            .collect()
    }

    /// Get the entire content as a string
    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }

    /// Get the number of lines
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Convert a line number to a byte offset
    pub fn line_to_byte(&self, line: usize) -> usize {
        self.rope.line_to_byte(line)
    }

    /// Get the number of characters
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// Get the length of a specific line (in characters)
    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let line_start = self.rope.line_to_char(line);
        let line_end = if line + 1 < self.rope.len_lines() {
            self.rope.line_to_char(line + 1)
        } else {
            self.rope.len_chars()
        };

        // Subtract line ending characters
        let mut len = line_end - line_start;
        if len > 0 {
            let last_char = self.rope.char(line_end - 1);
            if last_char == '\n' {
                len -= 1;
                if len > 0 && self.rope.char(line_end - 2) == '\r' {
                    len -= 1;
                }
            }
        }
        len
    }

    /// Convert a Position to a char index
    pub fn pos_to_char(&self, pos: Position) -> Result<usize> {
        if pos.line >= self.rope.len_lines() {
            anyhow::bail!("Line {} out of bounds (max {})", pos.line, self.rope.len_lines());
        }

        let line_start = self.rope.line_to_char(pos.line);
        let line_len = self.line_len(pos.line);
        let column = pos.column.min(line_len);

        Ok(line_start + column)
    }

    /// Convert a char index to a Position
    pub fn char_to_pos(&self, char_idx: usize) -> Position {
        let char_idx = char_idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let column = char_idx - line_start;

        Position { line, column }
    }

    /// Check if buffer is modified
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Get the file path
    pub fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    /// Set the file path
    pub fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }

    /// Get the line ending type
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.rope = Rope::new();
        self.modified = true;
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_buffer() {
        let buffer = TextBuffer::new();
        assert_eq!(buffer.len_lines(), 1); // Empty rope has 1 line
        assert_eq!(buffer.len_chars(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_insert() {
        let mut buffer = TextBuffer::new();
        buffer.insert(Position::zero(), "Hello").unwrap();
        assert_eq!(buffer.to_string(), "Hello");
        assert!(buffer.is_modified());
    }

    #[test]
    fn test_delete_range() {
        let mut buffer = TextBuffer::new();
        buffer.insert(Position::zero(), "Hello World").unwrap();
        buffer.delete_range(Position::new(0, 0), Position::new(0, 5)).unwrap();
        assert_eq!(buffer.to_string(), " World");
    }

    #[test]
    fn test_multiline() {
        let mut buffer = TextBuffer::new();
        buffer.insert(Position::zero(), "Line 1\nLine 2\nLine 3").unwrap();
        assert_eq!(buffer.len_lines(), 3);
        assert_eq!(buffer.get_line(0).unwrap(), "Line 1\n");
        assert_eq!(buffer.get_line(1).unwrap(), "Line 2\n");
        assert_eq!(buffer.get_line(2).unwrap(), "Line 3");
    }

    #[test]
    fn test_pos_to_char() {
        let mut buffer = TextBuffer::new();
        buffer.insert(Position::zero(), "ABC\nDEF\nGHI").unwrap();

        assert_eq!(buffer.pos_to_char(Position::new(0, 0)).unwrap(), 0);
        assert_eq!(buffer.pos_to_char(Position::new(0, 3)).unwrap(), 3);
        assert_eq!(buffer.pos_to_char(Position::new(1, 0)).unwrap(), 4);
        assert_eq!(buffer.pos_to_char(Position::new(2, 2)).unwrap(), 10);
    }

    #[test]
    fn test_line_ending_detection() {
        assert_eq!(LineEnding::detect("Hello\nWorld"), LineEnding::LF);
        assert_eq!(LineEnding::detect("Hello\r\nWorld"), LineEnding::CRLF);
        assert_eq!(LineEnding::detect("Hello\rWorld"), LineEnding::CR);
    }
}
