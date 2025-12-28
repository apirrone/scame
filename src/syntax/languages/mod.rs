pub mod python;
pub mod rust;

use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Language, Query};

/// Supported languages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedLanguage {
    Python,
    Rust,
}

impl SupportedLanguage {
    /// Detect language from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "py" | "pyw" => Some(Self::Python),
                "rs" => Some(Self::Rust),
                _ => None,
            })
    }

    /// Get the tree-sitter language
    pub fn language(&self) -> Language {
        match self {
            Self::Python => python::language(),
            Self::Rust => rust::language(),
        }
    }

    /// Get the highlighting query
    pub fn query(&self) -> Result<Query, tree_sitter::QueryError> {
        match self {
            Self::Python => python::query(),
            Self::Rust => rust::query(),
        }
    }

    /// Get capture name to token type mapping
    pub fn capture_names(&self) -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
        match self {
            Self::Python => python::capture_names(),
            Self::Rust => rust::capture_names(),
        }
    }
}
