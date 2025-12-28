use crate::lsp::protocol::{BufferId, Diagnostic};
use std::collections::HashMap;

/// Store for diagnostics from the language server
pub struct DiagnosticsStore {
    diagnostics: HashMap<BufferId, Vec<Diagnostic>>,
}

impl DiagnosticsStore {
    /// Create a new diagnostics store
    pub fn new() -> Self {
        Self {
            diagnostics: HashMap::new(),
        }
    }

    /// Update diagnostics for a buffer
    pub fn update(&mut self, buffer_id: BufferId, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            self.diagnostics.remove(&buffer_id);
        } else {
            self.diagnostics.insert(buffer_id, diagnostics);
        }
    }

    /// Get diagnostics for a buffer
    pub fn get(&self, buffer_id: BufferId) -> Option<&[Diagnostic]> {
        self.diagnostics.get(&buffer_id).map(|v| v.as_slice())
    }

    /// Get diagnostics that overlap with a specific line
    pub fn get_for_line(&self, buffer_id: BufferId, line: usize) -> Vec<&Diagnostic> {
        if let Some(diagnostics) = self.diagnostics.get(&buffer_id) {
            diagnostics
                .iter()
                .filter(|diag| {
                    let (start, end) = diag.range;
                    line >= start.line && line <= end.line
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Count errors and warnings for a buffer
    pub fn count_errors_warnings(&self, buffer_id: BufferId) -> (usize, usize) {
        if let Some(diagnostics) = self.diagnostics.get(&buffer_id) {
            let errors = diagnostics
                .iter()
                .filter(|d| matches!(d.severity, crate::lsp::protocol::DiagnosticSeverity::Error))
                .count();
            let warnings = diagnostics
                .iter()
                .filter(|d| matches!(d.severity, crate::lsp::protocol::DiagnosticSeverity::Warning))
                .count();
            (errors, warnings)
        } else {
            (0, 0)
        }
    }

    /// Clear all diagnostics
    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }
}

impl Default for DiagnosticsStore {
    fn default() -> Self {
        Self::new()
    }
}
