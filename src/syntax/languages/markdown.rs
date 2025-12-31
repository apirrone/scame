use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the Markdown language
pub fn language() -> Language {
    tree_sitter_md::language()
}

/// Get the Markdown highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    // Very minimal query to avoid version-specific node types
    // tree-sitter-md has different node types across versions
    // If this fails, syntax highlighting will be disabled for markdown (gracefully)
    let query_source = r#"
; Basic code highlighting (most stable across versions)
(code_span) @string
(fenced_code_block) @string
    "#;

    Query::new(&language(), query_source)
}

/// Get capture name to token type mapping
pub fn capture_names() -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
    let query = query()?;
    let mut map = HashMap::new();

    for (i, name) in query.capture_names().iter().enumerate() {
        let token_type = match name.as_ref() {
            "keyword" => TokenType::Keyword,
            "string" => TokenType::String,
            "function" => TokenType::Function,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
