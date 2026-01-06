use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the Markdown language
pub fn language() -> Language {
    tree_sitter_md::language()
}

/// Get the Markdown highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    // Query based on actual tree-sitter-md node types
    let query_source = r#"
; Headings
(atx_heading) @keyword
(atx_h1_marker) @keyword
(atx_h2_marker) @keyword
(atx_h3_marker) @keyword
(atx_h4_marker) @keyword
(atx_h5_marker) @keyword
(atx_h6_marker) @keyword

; Code blocks
(fenced_code_block) @string
(code_fence_content) @string
(indented_code_block) @string

; Code fence info string (language)
(info_string) @type

; Lists
(list_marker_minus) @keyword
(list_marker_plus) @keyword
(list_marker_star) @keyword
(list_marker_dot) @keyword
(list_marker_parenthesis) @keyword

; Blockquotes
(block_quote) @comment
(block_quote_marker) @comment
    "#;

    Query::new(&language(), query_source)
}

/// Get capture name to token type mapping
pub fn capture_names() -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
    let query = query()?;
    let mut map = HashMap::new();

    for (i, name) in query.capture_names().iter().enumerate() {
        let token_type = match name.as_ref() {
            "keyword" => TokenType::Keyword,      // Headings, list markers, hr
            "string" => TokenType::String,        // Code blocks and inline code
            "function" => TokenType::Function,    // Links, images, strong emphasis
            "type" => TokenType::Type,            // Emphasis (italic)
            "comment" => TokenType::Comment,      // Blockquotes
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
