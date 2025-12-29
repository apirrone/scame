use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use crate::buffer::Position;

/// Request for AI completion
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Path to the file being edited
    pub file_path: PathBuf,

    /// Programming language (e.g., "rust", "python", "javascript")
    pub language: String,

    /// Code before the cursor (up to 2000 chars)
    pub code_before_cursor: String,

    /// Code after the cursor (up to 500 chars)
    pub code_after_cursor: String,

    /// Cursor position in the file
    pub cursor_position: Position,
}

/// Response from AI completion provider
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// Completion text to insert
    pub text: String,

    /// Provider that generated this completion
    pub provider: String,
}

/// Trait for AI completion providers
#[async_trait]
pub trait CompletionProvider: Send + Sync {
    /// Get completion suggestion
    async fn get_completion(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Provider name (e.g., "copilot", "openai", "claude", "local")
    fn name(&self) -> &str;
}
