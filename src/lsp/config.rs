use std::path::Path;

/// Supported programming languages for LSP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
}

impl Language {
    /// Detect language from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "rs" => Some(Language::Rust),
                "py" => Some(Language::Python),
                _ => None,
            })
    }

    /// Get the language identifier for LSP
    pub fn language_id(&self) -> &str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
        }
    }

    /// Get the command to start the language server
    pub fn server_command(&self) -> (&str, Vec<&str>) {
        match self {
            Language::Rust => ("rust-analyzer", vec![]),
            // Try different possible pyright installations
            Language::Python => ("pyright-langserver", vec!["--stdio"]),
        }
    }

    /// Get alternative commands to try if the primary fails
    pub fn alternative_commands(&self) -> Vec<(&str, Vec<&str>)> {
        match self {
            Language::Rust => vec![],
            Language::Python => vec![
                ("pyright", vec!["--stdio"]),
                ("pylsp", vec![]), // python-lsp-server as fallback
            ],
        }
    }
}
