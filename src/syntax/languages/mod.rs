pub mod python;
pub mod rust;
pub mod json;
pub mod markdown;
pub mod html;
pub mod css;
pub mod javascript;

use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Language, Query};

/// Supported languages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedLanguage {
    Python,
    Rust,
    Json,
    Markdown,
    Html,
    Css,
    JavaScript,
}

impl SupportedLanguage {
    /// Detect language from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "py" | "pyw" | "pyi" => Some(Self::Python),
                "rs" => Some(Self::Rust),
                "json" => Some(Self::Json),
                "md" | "markdown" => Some(Self::Markdown),
                "html" | "htm" => Some(Self::Html),
                "css" => Some(Self::Css),
                "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
                _ => None,
            })
    }

    /// Get the tree-sitter language
    pub fn language(&self) -> Language {
        match self {
            Self::Python => python::language(),
            Self::Rust => rust::language(),
            Self::Json => json::language(),
            Self::Markdown => markdown::language(),
            Self::Html => html::language(),
            Self::Css => css::language(),
            Self::JavaScript => javascript::language(),
        }
    }

    /// Get the highlighting query
    pub fn query(&self) -> Result<Query, tree_sitter::QueryError> {
        match self {
            Self::Python => python::query(),
            Self::Rust => rust::query(),
            Self::Json => json::query(),
            Self::Markdown => markdown::query(),
            Self::Html => html::query(),
            Self::Css => css::query(),
            Self::JavaScript => javascript::query(),
        }
    }

    /// Get capture name to token type mapping
    pub fn capture_names(&self) -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
        match self {
            Self::Python => python::capture_names(),
            Self::Rust => rust::capture_names(),
            Self::Json => json::capture_names(),
            Self::Markdown => markdown::capture_names(),
            Self::Html => html::capture_names(),
            Self::Css => css::capture_names(),
            Self::JavaScript => javascript::capture_names(),
        }
    }
}
