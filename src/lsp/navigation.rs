use crate::lsp::protocol::Location;

/// Navigation history for jump back functionality
/// Maintains a stack of previous cursor positions when jumping to definitions
pub struct NavigationHistory {
    stack: Vec<Location>,
    max_size: usize,
}

impl NavigationHistory {
    /// Create a new navigation history with a maximum stack size
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            max_size: 50, // Store up to 50 jump locations
        }
    }

    /// Push a location onto the history stack
    /// Called before jumping to a definition
    pub fn push(&mut self, location: Location) {
        self.stack.push(location);

        // Limit stack size to prevent unbounded growth
        if self.stack.len() > self.max_size {
            self.stack.remove(0);
        }
    }

    /// Pop the most recent location from the history stack
    /// Returns None if the stack is empty
    pub fn pop(&mut self) -> Option<Location> {
        self.stack.pop()
    }

    /// Check if the history stack is empty
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get the current stack depth
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.stack.clear();
    }
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::protocol::Position;
    use std::path::PathBuf;

    #[test]
    fn test_push_pop() {
        let mut history = NavigationHistory::new();

        let loc1 = Location {
            path: PathBuf::from("file1.rs"),
            position: Position::new(10, 5),
        };

        let loc2 = Location {
            path: PathBuf::from("file2.rs"),
            position: Position::new(20, 15),
        };

        history.push(loc1.clone());
        history.push(loc2.clone());

        assert_eq!(history.depth(), 2);

        let popped = history.pop().unwrap();
        assert_eq!(popped.path, loc2.path);
        assert_eq!(popped.position.line, loc2.position.line);

        let popped = history.pop().unwrap();
        assert_eq!(popped.path, loc1.path);
        assert_eq!(popped.position.line, loc1.position.line);

        assert!(history.is_empty());
        assert!(history.pop().is_none());
    }

    #[test]
    fn test_max_size() {
        let mut history = NavigationHistory::new();

        // Push more than max_size locations
        for i in 0..60 {
            history.push(Location {
                path: PathBuf::from(format!("file{}.rs", i)),
                position: Position::new(i, 0),
            });
        }

        // Should be limited to max_size
        assert_eq!(history.depth(), 50);
    }
}
