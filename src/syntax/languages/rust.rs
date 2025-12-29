use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the Rust language
pub fn language() -> Language {
    tree_sitter_rust::language()
}

/// Get the Rust highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
; Comments
(line_comment) @comment
(block_comment) @comment

; Strings
(string_literal) @string
(char_literal) @string
(raw_string_literal) @string

; Numbers
(integer_literal) @number
(float_literal) @number

; Constants
(boolean_literal) @constant

; Functions
(function_item
  name: (identifier) @function)

(call_expression
  function: (identifier) @function)

(call_expression
  function: (field_expression
    field: (field_identifier) @function))

(macro_invocation
  macro: (identifier) @function)

; Types
(struct_item
  name: (type_identifier) @type)

(enum_item
  name: (type_identifier) @type)

(impl_item
  type: (type_identifier) @type)

(type_identifier) @type
(primitive_type) @type

; Keywords - only the ones that work
"fn" @keyword
"let" @keyword
"const" @keyword
"static" @keyword
"struct" @keyword
"enum" @keyword
"impl" @keyword
"trait" @keyword
"if" @keyword
"else" @keyword
"match" @keyword
"for" @keyword
"while" @keyword
"loop" @keyword
"return" @keyword
"break" @keyword
"continue" @keyword
"use" @keyword
"mod" @keyword
"pub" @keyword
"async" @keyword
"await" @keyword
"where" @keyword
"unsafe" @keyword
"extern" @keyword
"type" @keyword
"as" @keyword
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
            "function" => TokenType::Function,
            "type" => TokenType::Type,
            "string" => TokenType::String,
            "number" => TokenType::Number,
            "comment" => TokenType::Comment,
            "operator" => TokenType::Operator,
            "variable" => TokenType::Variable,
            "constant" => TokenType::Constant,
            "parameter" => TokenType::Parameter,
            "property" => TokenType::Property,
            _ => continue,
        };
        map.insert(i, token_type);
    }

    Ok(map)
}
