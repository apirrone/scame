use std::path::PathBuf;

/// Unique identifier for a buffer in the workspace
pub type BufferId = usize;

/// Position in a text document (line and column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Requests sent from main thread to LSP background task
#[derive(Debug)]
pub enum LspRequest {
    /// Notify LSP that a document was opened
    DidOpen {
        buffer_id: BufferId,
        path: PathBuf,
        content: String,
        language: String,
    },
    /// Notify LSP that a document was changed
    DidChange {
        buffer_id: BufferId,
        path: PathBuf,
        content: String,
        version: i32,
    },
    /// Notify LSP that a document was saved
    DidSave {
        buffer_id: BufferId,
        path: PathBuf,
    },
    /// Request to jump to definition
    GotoDefinition {
        buffer_id: BufferId,
        path: PathBuf,
        position: Position,
    },
    /// Request completions at a position
    Completion {
        buffer_id: BufferId,
        path: PathBuf,
        position: Position,
    },
    /// Shutdown the LSP client
    Shutdown,
}

/// Responses sent from LSP background task to main thread
#[derive(Debug, Clone)]
pub enum LspResponse {
    /// Diagnostics published by the language server
    Diagnostics {
        buffer_id: BufferId,
        diagnostics: Vec<Diagnostic>,
    },
    /// Result of goto definition request
    GotoDefinition {
        location: Location,
    },
    /// Result of completion request
    Completion {
        items: Vec<CompletionItem>,
    },
    /// Error occurred in LSP
    Error {
        message: String,
    },
}

/// A diagnostic (error, warning, info) from the language server
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub range: (Position, Position),
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// Severity of a diagnostic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// A location in a source file
#[derive(Debug, Clone)]
pub struct Location {
    pub path: PathBuf,
    pub position: Position,
}

/// A completion item from the language server
#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub kind: Option<CompletionItemKind>,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
}

/// Kind of completion item (function, variable, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionItemKind {
    Function,
    Method,
    Variable,
    Field,
    Keyword,
    Module,
    Struct,
    Enum,
    Interface,
    Constant,
    Other,
}
