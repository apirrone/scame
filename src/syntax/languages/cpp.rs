use crate::syntax::theme::TokenType;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Get the C++ language
pub fn language() -> Language {
    tree_sitter_cpp::language()
}

/// Get the C++ highlighting query
pub fn query() -> Result<Query, tree_sitter::QueryError> {
    let query_source = r##"
; Comments
(comment) @comment

; Strings
(string_literal) @string
(raw_string_literal) @string
(char_literal) @string
(system_lib_string) @string

; Numbers
(number_literal) @number

; Constants
(true) @constant
(false) @constant
(null) @constant

"nullptr" @constant

; Preprocessor
(preproc_directive) @keyword

; Functions
(function_declarator
  declarator: (identifier) @function)

(function_declarator
  declarator: (field_identifier) @function)

(function_declarator
  declarator: (qualified_identifier
    name: (identifier) @function))

(call_expression
  function: (identifier) @function)

(call_expression
  function: (field_expression
    field: (field_identifier) @function))

(call_expression
  function: (qualified_identifier
    name: (identifier) @function))

(template_function
  name: (identifier) @function)

; Types
(class_specifier
  name: (type_identifier) @type)

(struct_specifier
  name: (type_identifier) @type)

(union_specifier
  name: (type_identifier) @type)

(enum_specifier
  name: (type_identifier) @type)

(type_identifier) @type
(primitive_type) @type
(sized_type_specifier) @type
(auto) @type

; Keywords
"if" @keyword
"else" @keyword
"switch" @keyword
"case" @keyword
"default" @keyword
"for" @keyword
"while" @keyword
"do" @keyword
"return" @keyword
"break" @keyword
"continue" @keyword
"goto" @keyword
"struct" @keyword
"class" @keyword
"union" @keyword
"enum" @keyword
"typedef" @keyword
"namespace" @keyword
"template" @keyword
"typename" @keyword
"using" @keyword
"public" @keyword
"private" @keyword
"protected" @keyword
"static" @keyword
"const" @keyword
"extern" @keyword
"inline" @keyword
"new" @keyword
"delete" @keyword
"try" @keyword
"catch" @keyword
"throw" @keyword
"sizeof" @keyword

(this) @keyword
    "##;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_compiles() {
        query().expect("C++ query should compile");
    }

    #[test]
    fn parses_sample() {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language()).unwrap();
        let src = r#"
#include <iostream>
namespace foo {
class Bar {
public:
    int baz(int x) const { return x + 1; }
};
}
int main() { foo::Bar b; return b.baz(41); }
"#;
        let tree = parser.parse(src, None).unwrap();
        assert!(!tree.root_node().has_error(), "should parse without errors");
    }
}
