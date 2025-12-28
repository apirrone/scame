use super::rope_buffer::Position;
use anyhow::Result;

/// Represents a single change to the buffer
#[derive(Debug, Clone)]
pub enum Change {
    /// Insert text at a position
    Insert { pos: Position, text: String },
    /// Delete text at a position
    Delete { pos: Position, text: String },
    /// A compound change (multiple changes grouped together)
    Compound(Vec<Change>),
}

impl Change {
    /// Get the inverse of this change (for undo/redo)
    pub fn inverse(&self) -> Self {
        match self {
            Change::Insert { pos, text } => Change::Delete {
                pos: *pos,
                text: text.clone(),
            },
            Change::Delete { pos, text } => Change::Insert {
                pos: *pos,
                text: text.clone(),
            },
            Change::Compound(changes) => {
                let inverted: Vec<_> = changes.iter().rev().map(|c| c.inverse()).collect();
                Change::Compound(inverted)
            }
        }
    }
}

/// Manages undo/redo history for a text buffer
pub struct UndoManager {
    /// History of changes
    history: Vec<Change>,
    /// Current position in history
    current: usize,
    /// Maximum number of changes to keep
    max_history: usize,
    /// Whether we're currently in an undo/redo operation
    in_undo_redo: bool,
}

impl UndoManager {
    /// Create a new undo manager
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            current: 0,
            max_history,
            in_undo_redo: false,
        }
    }

    /// Record a change
    pub fn record(&mut self, change: Change) {
        // Don't record changes during undo/redo operations
        if self.in_undo_redo {
            return;
        }

        // Remove any redo history when making a new change
        self.history.truncate(self.current);

        // Add the change
        self.history.push(change);
        self.current += 1;

        // Limit history size
        if self.history.len() > self.max_history {
            let remove_count = self.history.len() - self.max_history;
            self.history.drain(0..remove_count);
            self.current = self.current.saturating_sub(remove_count);
        }
    }

    /// Undo the last change and return it
    pub fn undo(&mut self) -> Option<Change> {
        if self.current == 0 {
            return None;
        }

        self.in_undo_redo = true;
        self.current -= 1;
        let change = self.history[self.current].clone();
        Some(change.inverse())
    }

    /// Redo the next change and return it
    pub fn redo(&mut self) -> Option<Change> {
        if self.current >= self.history.len() {
            return None;
        }

        self.in_undo_redo = true;
        let change = self.history[self.current].clone();
        self.current += 1;
        Some(change)
    }

    /// Finish an undo/redo operation
    pub fn finish_undo_redo(&mut self) {
        self.in_undo_redo = false;
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.current > 0
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.current < self.history.len()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.history.clear();
        self.current = 0;
        self.in_undo_redo = false;
    }

    /// Get the number of changes in history
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Get the current position in history
    pub fn current_position(&self) -> usize {
        self.current
    }
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new(1000) // Default to 1000 operations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_redo() {
        let mut manager = UndoManager::new(100);

        // Record some changes
        manager.record(Change::Insert {
            pos: Position::new(0, 0),
            text: "Hello".to_string(),
        });
        manager.record(Change::Insert {
            pos: Position::new(0, 5),
            text: " World".to_string(),
        });

        assert!(manager.can_undo());
        assert!(!manager.can_redo());

        // Undo
        let change = manager.undo().unwrap();
        assert!(matches!(change, Change::Delete { .. }));
        assert!(manager.can_redo());

        // Redo
        let change = manager.redo().unwrap();
        assert!(matches!(change, Change::Insert { .. }));

        manager.finish_undo_redo();
    }

    #[test]
    fn test_max_history() {
        let mut manager = UndoManager::new(3);

        manager.record(Change::Insert {
            pos: Position::new(0, 0),
            text: "A".to_string(),
        });
        manager.record(Change::Insert {
            pos: Position::new(0, 1),
            text: "B".to_string(),
        });
        manager.record(Change::Insert {
            pos: Position::new(0, 2),
            text: "C".to_string(),
        });
        manager.record(Change::Insert {
            pos: Position::new(0, 3),
            text: "D".to_string(),
        });

        // Should only keep the last 3
        assert_eq!(manager.history_len(), 3);
    }

    #[test]
    fn test_new_change_clears_redo() {
        let mut manager = UndoManager::new(100);

        manager.record(Change::Insert {
            pos: Position::new(0, 0),
            text: "A".to_string(),
        });
        manager.record(Change::Insert {
            pos: Position::new(0, 1),
            text: "B".to_string(),
        });

        manager.undo();
        manager.finish_undo_redo();
        assert!(manager.can_redo());

        // New change should clear redo history
        manager.record(Change::Insert {
            pos: Position::new(0, 1),
            text: "C".to_string(),
        });
        assert!(!manager.can_redo());
    }
}
