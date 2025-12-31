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
; Function definitions
(function_definition
  name: (identifier) @function)

; Function calls
(call
  function: (identifier) @function)

; Method calls (e.g., obj.method())
(call
  function: (attribute
    attribute: (identifier) @function))

; Class definitions
(class_definition
  name: (identifier) @type)

; Strings
(string) @string

; Comments
(comment) @comment

; Numbers
(integer) @number
(float) @number

; Constants
(true) @constant
(false) @constant
(none) @constant

; 'self' parameter (VSCode shows this in blue)
((identifier) @variable
  (#eq? @variable "self"))

; 'cls' parameter (for class methods)
((identifier) @variable
  (#eq? @variable "cls"))

; Attribute access - the property part (e.g., 'attribute' in self.attribute)
(attribute
  attribute: (identifier) @property)

; Import statements - module names in teal/green
(import_statement
  name: (dotted_name) @type)

(import_from_statement
  module_name: (dotted_name) @type)

(aliased_import
  name: (dotted_name) @type)

; Imported names as variables
(import_statement
  name: (dotted_name (identifier) @type))

(import_from_statement
  name: (dotted_name (identifier) @variable))

; Function parameters
(parameters
  (identifier) @parameter)

(typed_parameter
  (identifier) @parameter)

(default_parameter
  name: (identifier) @parameter)

(typed_default_parameter
  name: (identifier) @parameter)

; All other identifiers as variables (this is the catch-all)
(identifier) @variable

; Keywords
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
"in" @keyword
"is" @keyword
"not" @keyword
"and" @keyword
"or" @keyword
"assert" @keyword
"del" @keyword
"global" @keyword
"nonlocal" @keyword
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
