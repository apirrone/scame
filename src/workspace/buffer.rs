use crate::buffer::{Change, TextBuffer, UndoManager};
use crate::editor::EditorState;
use anyhow::Result;
use std::path::PathBuf;

/// Unique identifier for a buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(pub usize);

/// A single buffer containing text, editor state, and undo history
pub struct Buffer {
    id: BufferId,
    text_buffer: TextBuffer,
    editor_state: EditorState,
    undo_manager: UndoManager,
}

impl Buffer {
    /// Create a new empty buffer
    pub fn new(id: BufferId, width: u16, height: u16) -> Self {
        Self {
            id,
            text_buffer: TextBuffer::new(),
            editor_state: EditorState::new(width, height),
            undo_manager: UndoManager::new(1000),
        }
    }

    /// Create a buffer from a file
    pub fn from_file(id: BufferId, path: PathBuf, width: u16, height: u16) -> Result<Self> {
        Ok(Self {
            id,
            text_buffer: TextBuffer::from_file(path)?,
            editor_state: EditorState::new(width, height),
            undo_manager: UndoManager::new(1000),
        })
    }

    /// Get the buffer ID
    pub fn id(&self) -> BufferId {
        self.id
    }

    /// Get the text buffer
    pub fn text_buffer(&self) -> &TextBuffer {
        &self.text_buffer
    }

    /// Get mutable text buffer
    pub fn text_buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.text_buffer
    }

    /// Get the editor state
    pub fn editor_state(&self) -> &EditorState {
        &self.editor_state
    }

    /// Get mutable editor state
    pub fn editor_state_mut(&mut self) -> &mut EditorState {
        &mut self.editor_state
    }

    /// Get the undo manager
    pub fn undo_manager(&self) -> &UndoManager {
        &self.undo_manager
    }

    /// Get mutable undo manager
    pub fn undo_manager_mut(&mut self) -> &mut UndoManager {
        &mut self.undo_manager
    }

    /// Record a change to the undo manager
    pub fn record_change(&mut self, change: Change) {
        self.undo_manager.record(change);
    }

    /// Apply a change to the buffer
    pub fn apply_change(&mut self, change: &Change) -> Result<()> {
        match change {
            Change::Insert { pos, text } => {
                self.text_buffer.insert(*pos, text)?;
                let char_idx = self.text_buffer.pos_to_char(*pos)? + text.len();
                self.editor_state.cursor.set_position(self.text_buffer.char_to_pos(char_idx));
            }
            Change::Delete { pos, text } => {
                let end_idx = self.text_buffer.pos_to_char(*pos)? + text.len();
                let end_pos = self.text_buffer.char_to_pos(end_idx);
                self.text_buffer.delete_range(*pos, end_pos)?;
                self.editor_state.cursor.set_position(*pos);
            }
            Change::Compound(changes) => {
                for change in changes {
                    self.apply_change(change)?;
                }
            }
        }
        self.editor_state.ensure_cursor_visible();
        Ok(())
    }

    /// Get the file path
    pub fn file_path(&self) -> Option<&PathBuf> {
        self.text_buffer.file_path()
    }

    /// Get the display name for this buffer
    pub fn display_name(&self) -> String {
        if let Some(path) = self.file_path() {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unnamed")
                .to_string()
        } else {
            format!("[Buffer {}]", self.id.0)
        }
    }

    /// Check if buffer is modified
    pub fn is_modified(&self) -> bool {
        self.text_buffer.is_modified()
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: u16, height: u16) {
        self.editor_state.viewport.resize(width, height);
    }

    /// Get mutable references to both text buffer and editor state
    /// This is needed to avoid borrow checker issues when both are needed simultaneously
    pub fn split_mut(&mut self) -> (&mut TextBuffer, &mut EditorState, &mut UndoManager) {
        (&mut self.text_buffer, &mut self.editor_state, &mut self.undo_manager)
    }
}
