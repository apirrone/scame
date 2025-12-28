// Quick test to see if tree-sitter queries parse correctly
use tree_sitter::{Language, Query};

fn main() {
    // Test Python
    let py_lang = tree_sitter_python::language();
    let py_query_source = r#"
        ; Keywords
        [
          "def"
          "class"
          "if"
          "else"
          "for"
          "while"
          "return"
        ] @keyword

        ; Function definitions
        (function_definition
          name: (identifier) @function)

        ; Strings
        (string) @string

        ; Comments
        (comment) @comment
    "#;

    match Query::new(&py_lang, py_query_source) {
        Ok(_) => println!("✓ Python query is valid"),
        Err(e) => println!("✗ Python query error: {:?}", e),
    }

    // Test Rust
    let rs_lang = tree_sitter_rust::language();
    let rs_query_source = r#"
        ; Keywords
        [
          "fn"
          "let"
          "mut"
          "struct"
          "impl"
        ] @keyword

        ; Function definitions
        (function_item
          name: (identifier) @function)

        ; Strings
        (string_literal) @string

        ; Comments
        (line_comment) @comment
    "#;

    match Query::new(&rs_lang, rs_query_source) {
        Ok(_) => println!("✓ Rust query is valid"),
        Err(e) => println!("✗ Rust query error: {:?}", e),
    }
}
