use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the Python language
pub fn language() -> Language {
    tree_sitter_python::language()
}

/// Get the Python highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r#"
(function_definition
  name: (identifier) @function)

(call
  function: (identifier) @function)

(class_definition
  name: (identifier) @type)

(string) @string

(comment) @comment

(integer) @number
(float) @number

(true) @constant
(false) @constant
(none) @constant

"def" @keyword
"class" @keyword
"if" @keyword
"else" @keyword
"elif" @keyword
"for" @keyword
"while" @keyword
"return" @keyword
"import" @keyword
"from" @keyword
"as" @keyword
"with" @keyword
"try" @keyword
"except" @keyword
"finally" @keyword
"raise" @keyword
"break" @keyword
"continue" @keyword
"pass" @keyword
"lambda" @keyword
"async" @keyword
"await" @keyword
"yield" @keyword
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
