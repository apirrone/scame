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
(function_item
  name: (identifier) @function)

(call_expression
  function: (identifier) @function)

(macro_invocation
  macro: (identifier) @function)

(struct_item
  name: (type_identifier) @type)

(enum_item
  name: (type_identifier) @type)

(type_identifier) @type

(string_literal) @string
(char_literal) @string

(line_comment) @comment
(block_comment) @comment

(integer_literal) @number
(float_literal) @number

(boolean_literal) @constant

"fn" @keyword
"let" @keyword
"mut" @keyword
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
"crate" @keyword
"self" @keyword
"super" @keyword
"async" @keyword
"await" @keyword
"move" @keyword
"ref" @keyword
"where" @keyword
"unsafe" @keyword
"extern" @keyword
"type" @keyword
"dyn" @keyword
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
