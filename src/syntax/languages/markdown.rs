use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the Markdown language
pub fn language() -> Language {
    tree_sitter_md::language()
}

/// Get the Markdown highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Headings
(atx_heading) @keyword

; Emphasis
(emphasis) @emphasis
(strong_emphasis) @strong

; Code
(code_span) @string
(fenced_code_block) @string
(indented_code_block) @string

; Links
(link) @function
(image) @function

; Lists
(list_marker) @keyword

; Inline code
(code_fence_content) @string
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
            "emphasis" => TokenType::String,
            "strong" => TokenType::Keyword,
            "string" => TokenType::String,
            "function" => TokenType::Function,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
