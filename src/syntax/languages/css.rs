use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the CSS language
pub fn language() -> Language {
    tree_sitter_css::language()
}

/// Get the CSS highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Comments
(comment) @comment

; Properties
(property_name) @property

; String values
(string_value) @string

; Numbers
(integer_value) @number
(float_value) @number
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
            "type" => TokenType::Type,
            "function" => TokenType::Function,
            "property" => TokenType::Property,
            "string" => TokenType::String,
            "number" => TokenType::Number,
            "constant" => TokenType::Constant,
            "comment" => TokenType::Comment,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
