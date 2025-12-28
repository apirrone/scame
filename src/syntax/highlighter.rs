use super::theme::{Theme, TokenType};
use anyhow::Result;
use std::collections::HashMap;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

/// A highlighted span with token type
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub token_type: TokenType,
}

/// Cache entry for parsed syntax tree
struct CacheEntry {
    tree: Tree,
    text_hash: u64,
}

/// Syntax highlighter using tree-sitter
pub struct Highlighter {
    parser: Parser,
    theme: Theme,
    cache: HashMap<String, CacheEntry>,
}

impl Highlighter {
    /// Create a new highlighter
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            theme: Theme::default(),
            cache: HashMap::new(),
        }
    }

    /// Set the language for parsing
    pub fn set_language(&mut self, language: &Language) -> Result<()> {
        self.parser.set_language(language)?;
        Ok(())
    }

    /// Parse text and return syntax tree
    fn parse(&mut self, text: &str, file_id: &str) -> Result<Option<Tree>> {
        // Simple hash for cache invalidation
        let text_hash = Self::simple_hash(text);

        // Check cache
        if let Some(entry) = self.cache.get(file_id) {
            if entry.text_hash == text_hash {
                return Ok(Some(entry.tree.clone()));
            }
        }

        // Parse
        let tree = self.parser.parse(text, None);

        // Cache the result
        if let Some(tree) = &tree {
            self.cache.insert(
                file_id.to_string(),
                CacheEntry {
                    tree: tree.clone(),
                    text_hash,
                },
            );
        }

        Ok(tree)
    }

    /// Highlight text and return spans
    pub fn highlight(
        &mut self,
        text: &str,
        file_id: &str,
        query: &Query,
        query_capture_names: &HashMap<usize, TokenType>,
    ) -> Result<Vec<HighlightSpan>> {
        let mut spans = Vec::new();

        let Some(tree) = self.parse(text, file_id)? else {
            return Ok(spans);
        };

        let root_node = tree.root_node();
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(query, root_node, text.as_bytes());

        for m in matches {
            for capture in m.captures {
                let capture_index = capture.index as usize;
                if let Some(token_type) = query_capture_names.get(&capture_index) {
                    spans.push(HighlightSpan {
                        start_byte: capture.node.start_byte(),
                        end_byte: capture.node.end_byte(),
                        token_type: *token_type,
                    });
                }
            }
        }

        // Sort spans by start position
        spans.sort_by_key(|s| s.start_byte);

        Ok(spans)
    }

    /// Get the theme
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Simple hash function for cache invalidation
    fn simple_hash(text: &str) -> u64 {
        let mut hash: u64 = 5381;
        for byte in text.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Clear cache for specific file
    pub fn clear_file_cache(&mut self, file_id: &str) {
        self.cache.remove(file_id);
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}
