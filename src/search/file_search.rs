use crate::workspace::FileTree;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;

/// Result from a file search
#[derive(Debug, Clone)]
pub struct FileSearchResult {
    pub path: PathBuf,
    pub score: i64,
    pub display_path: String,
}

/// File search engine with fuzzy matching
pub struct FileSearch {
    matcher: SkimMatcherV2,
}

impl FileSearch {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Search for files matching a pattern, optionally prioritizing by extension
    pub fn search(&self, file_tree: &FileTree, pattern: &str, priority_extension: Option<&str>) -> Vec<FileSearchResult> {
        // If pattern is empty, return all files in alphabetical order
        if pattern.is_empty() {
            let mut results: Vec<FileSearchResult> = file_tree
                .relative_files()
                .into_iter()
                .map(|path| {
                    let path_str = path.to_string_lossy().to_string();
                    FileSearchResult {
                        path: file_tree.root().join(&path),
                        score: 0, // No score needed for alphabetical listing
                        display_path: path_str,
                    }
                })
                .collect();

            // Sort alphabetically by display path
            results.sort_by(|a, b| a.display_path.cmp(&b.display_path));

            // Limit results to 100 files (more than fuzzy search since they're all relevant)
            results.truncate(100);

            return results;
        }

        // Remove spaces from the pattern for more flexible fuzzy matching
        // This allows "python sdk" to match "python-sdk.md"
        let normalized_pattern = pattern.replace(' ', "");

        let mut results: Vec<FileSearchResult> = file_tree
            .relative_files()
            .into_iter()
            .filter_map(|path| {
                let path_str = path.to_string_lossy().to_string();

                // First, try to match against the filename (highest priority)
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                let filename_score = self.matcher.fuzzy_match(filename, &normalized_pattern);

                // Also try matching against the full path (lower priority)
                let path_score = self.matcher.fuzzy_match(&path_str, &normalized_pattern);

                // Use the better of the two scores, but heavily prioritize filename matches
                let base_score = match (filename_score, path_score) {
                    (Some(fs), Some(ps)) => {
                        // If filename matches, boost it significantly (5x)
                        // This makes filename matches always rank higher than path matches
                        if fs > 0 {
                            fs * 5
                        } else {
                            ps
                        }
                    }
                    (Some(fs), None) => fs * 5,
                    (None, Some(ps)) => ps,
                    (None, None) => return None,
                };

                let mut score = base_score;

                // Boost score if extension matches
                if let Some(priority_ext) = priority_extension {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == priority_ext {
                            // Boost score significantly (3x) for matching extensions
                            score = score * 3;
                        }
                    }
                }

                Some(FileSearchResult {
                    path: file_tree.root().join(&path),
                    score,
                    display_path: path_str,
                })
            })
            .collect();

        // Sort by score (highest first)
        results.sort_by(|a, b| b.score.cmp(&a.score));

        // Limit results
        results.truncate(20);

        results
    }
}

impl Default for FileSearch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_search() {
        let search = FileSearch::new();
        let matcher = &search.matcher;

        // Test fuzzy matching
        assert!(matcher.fuzzy_match("src/main.rs", "main").is_some());
        assert!(matcher.fuzzy_match("src/main.rs", "sr").is_some());
        assert!(matcher.fuzzy_match("src/main.rs", "srs").is_some());

        // Test space normalization
        let normalized = "python sdk".replace(' ', "");
        assert_eq!(normalized, "pythonsdk");
        assert!(matcher.fuzzy_match("python-sdk.md", &normalized).is_some());
        assert!(matcher.fuzzy_match("python_sdk.py", &normalized).is_some());
    }
}
