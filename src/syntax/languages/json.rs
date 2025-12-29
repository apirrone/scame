use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the JSON language
pub fn language() -> Language {
    tree_sitter_json::language()
}

/// Get the JSON highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Strings (keys and values)
(string) @string

; Numbers
(number) @number

; Constants
(true) @constant
(false) @constant
(null) @constant

; Keys in objects
(pair
  key: (string) @property)
    "#;

    Query::new(&language(), query_source)
}

/// Get capture name to token type mapping
pub fn capture_names() -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
    let query = query()?;
    let mut map = HashMap::new();

    for (i, name) in query.capture_names().iter().enumerate() {
        let token_type = match name.as_ref() {
            "string" => TokenType::String,
            "number" => TokenType::Number,
            "constant" => TokenType::Constant,
            "property" => TokenType::Property,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
