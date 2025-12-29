use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the JavaScript language
pub fn language() -> Language {
    tree_sitter_javascript::language()
}

/// Get the JavaScript highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Comments
(comment) @comment

; Strings
(string) @string

; Numbers
(number) @number

; Keywords
"const" @keyword
"let" @keyword
"var" @keyword
"function" @keyword
"class" @keyword
"if" @keyword
"else" @keyword
"for" @keyword
"while" @keyword
"return" @keyword
"async" @keyword
"await" @keyword
"import" @keyword
"export" @keyword
"try" @keyword
"catch" @keyword
"throw" @keyword
"new" @keyword
"this" @keyword

; Constants
(true) @constant
(false) @constant
(null) @constant
(undefined) @constant

; Function declarations
(function_declaration
  name: (identifier) @function)

; Function calls
(call_expression
  function: (identifier) @function)

; Properties
(property_identifier) @property
    "#;

    Query::new(&language(), query_source)
}

/// Get capture name to token type mapping
pub fn capture_names() -> Result<HashMap<usize, TokenType>, tree_sitter::QueryError> {
    let query = query()?;
    let mut map = HashMap::new();

    for (i, name) in query.capture_names().iter().enumerate() {
        let token_type = match name.as_ref() {
            "comment" => TokenType::Comment,
            "string" => TokenType::String,
            "number" => TokenType::Number,
            "constant" => TokenType::Constant,
            "keyword" => TokenType::Keyword,
            "function" => TokenType::Function,
            "type" => TokenType::Type,
            "property" => TokenType::Property,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
