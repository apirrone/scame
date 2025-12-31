use anyhow::Result;
use crossterm::style::Color;
use similar::{ChangeTag, TextDiff};
use std::path::{Path, PathBuf};

/// Represents a line in the diff view with its change type
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub change_type: ChangeType,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Unchanged,
    Added,
    Deleted,
    Modified,
}

impl ChangeType {
    pub fn color(&self) -> Color {
        match self {
            ChangeType::Unchanged => Color::Reset,
            ChangeType::Added => Color::Rgb { r: 34, g: 139, b: 34 },    // Green
            ChangeType::Deleted => Color::Rgb { r: 220, g: 38, b: 38 },  // Red
            ChangeType::Modified => Color::Rgb { r: 255, g: 191, b: 0 }, // Yellow/Amber
        }
    }

    pub fn bg_color(&self) -> Color {
        match self {
            ChangeType::Unchanged => Color::Reset,
            ChangeType::Added => Color::Rgb { r: 0, g: 64, b: 0 },     // Dark green
            ChangeType::Deleted => Color::Rgb { r: 64, g: 0, b: 0 },   // Dark red
            ChangeType::Modified => Color::Rgb { r: 64, g: 48, b: 0 }, // Dark yellow
        }
    }
}

/// Represents the diff between two files
pub struct DiffView {
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub left_lines: Vec<DiffLine>,
    pub right_lines: Vec<DiffLine>,
    pub left_content: String,
    pub right_content: String,
    pub scroll_offset: usize,
}

impl DiffView {
    /// Create a new diff view from two files
    pub fn new(left_path: PathBuf, right_path: PathBuf) -> Result<Self> {
        let left_content = std::fs::read_to_string(&left_path)?;
        let right_content = std::fs::read_to_string(&right_path)?;

        let (left_lines, right_lines) = Self::compute_diff(&left_content, &right_content);

        Ok(Self {
            left_path,
            right_path,
            left_lines,
            right_lines,
            left_content,
            right_content,
            scroll_offset: 0,
        })
    }

    /// Get the file extension
    pub fn file_extension(&self) -> Option<&str> {
        self.left_path.extension().and_then(|e| e.to_str())
    }

    /// Check if syntax highlighting is supported for this file type
    pub fn supports_syntax_highlighting(&self) -> bool {
        matches!(self.file_extension(), Some("py") | Some("rs"))
    }

    /// Compute the diff between two strings and return paired lines
    fn compute_diff(left: &str, right: &str) -> (Vec<DiffLine>, Vec<DiffLine>) {
        let diff = TextDiff::from_lines(left, right);

        let mut left_lines = Vec::new();
        let mut right_lines = Vec::new();
        let mut old_line_num = 1;
        let mut new_line_num = 1;

        for change in diff.iter_all_changes() {
            let content = change.value().trim_end_matches(&['\n', '\r'][..]).to_string();

            match change.tag() {
                ChangeTag::Equal => {
                    // Unchanged line - appears in both sides
                    left_lines.push(DiffLine {
                        content: content.clone(),
                        change_type: ChangeType::Unchanged,
                        old_line_num: Some(old_line_num),
                        new_line_num: Some(new_line_num),
                    });
                    right_lines.push(DiffLine {
                        content,
                        change_type: ChangeType::Unchanged,
                        old_line_num: Some(old_line_num),
                        new_line_num: Some(new_line_num),
                    });
                    old_line_num += 1;
                    new_line_num += 1;
                }
                ChangeTag::Delete => {
                    // Line deleted from left - show on left side only
                    left_lines.push(DiffLine {
                        content,
                        change_type: ChangeType::Deleted,
                        old_line_num: Some(old_line_num),
                        new_line_num: None,
                    });
                    // Add empty placeholder on right side to keep alignment
                    right_lines.push(DiffLine {
                        content: String::new(),
                        change_type: ChangeType::Unchanged,
                        old_line_num: None,
                        new_line_num: None,
                    });
                    old_line_num += 1;
                }
                ChangeTag::Insert => {
                    // Line added on right - show on right side only
                    // Add empty placeholder on left side to keep alignment
                    left_lines.push(DiffLine {
                        content: String::new(),
                        change_type: ChangeType::Unchanged,
                        old_line_num: None,
                        new_line_num: None,
                    });
                    right_lines.push(DiffLine {
                        content,
                        change_type: ChangeType::Added,
                        old_line_num: None,
                        new_line_num: Some(new_line_num),
                    });
                    new_line_num += 1;
                }
            }
        }

        (left_lines, right_lines)
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, n: usize, max_visible: usize) {
        let max_scroll = self.left_lines.len().saturating_sub(max_visible);
        self.scroll_offset = (self.scroll_offset + n).min(max_scroll);
    }

    /// Get the visible lines for display
    pub fn visible_lines(&self, height: usize) -> (&[DiffLine], &[DiffLine]) {
        let end = (self.scroll_offset + height).min(self.left_lines.len());
        let left = &self.left_lines[self.scroll_offset..end];
        let right = &self.right_lines[self.scroll_offset..end];
        (left, right)
    }
}
