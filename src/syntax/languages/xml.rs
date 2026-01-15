use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the XML language
pub fn language() -> Language {
    tree_sitter_xml::language_xml().into()
}

/// Get the XML highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Tag names
(STag (Name) @keyword)
(ETag (Name) @keyword)
(EmptyElemTag (Name) @keyword)

; Attribute names
(Attribute (Name) @property)

; Attribute values
(AttValue) @string

; Text content
(CharData) @string

; Comments
(Comment) @comment

; XML Declaration
(XMLDecl) @keyword

; CDATA sections
(CDSect) @string
    "#;

    Query::new(&language(), query_source)
}

/// Get capture name to token type mapping
pub fn capture_names() -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
    let query = query()?;
    let mut map = HashMap::new();

    for (i, name) in query.capture_names().iter().enumerate() {
        let token_type = match name.as_ref() {
            "keyword" => TokenType::Keyword,      // Tags
            "property" => TokenType::Property,    // Attributes
            "string" => TokenType::String,        // Attribute values, text, CDATA
            "comment" => TokenType::Comment,      // Comments
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
