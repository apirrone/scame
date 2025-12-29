use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the HTML language
pub fn language() -> Language {
    tree_sitter_html::language()
}

/// Get the HTML highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Tags
(tag_name) @keyword
(erroneous_end_tag_name) @keyword

; Attributes
(attribute_name) @property

; Attribute values
(attribute_value) @string
(quoted_attribute_value) @string

; Text content
(text) @default

; Comments
(comment) @comment

; Doctype
(doctype) @keyword
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
            "property" => TokenType::Property,
            "string" => TokenType::String,
            "comment" => TokenType::Comment,
            "default" => continue,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
