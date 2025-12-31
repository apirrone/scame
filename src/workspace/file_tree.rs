use anyhow::Result;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Represents a project file tree
pub struct FileTree {
    root: PathBuf,
    files: Vec<PathBuf>,
}

impl FileTree {
    /// Create a new file tree from a root directory
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: Vec::new(),
        }
    }

    /// Scan the directory and build the file list
    /// Automatically respects .gitignore, .ignore, and other ignore files
    pub fn scan(&mut self) -> Result<()> {
        self.files.clear();

        // Use the ignore crate which automatically respects:
        // - .gitignore
        // - .ignore
        // - .git/info/exclude
        // - global gitignore
        for result in WalkBuilder::new(&self.root)
            .hidden(false)        // Include hidden files (false = don't filter them out)
            .git_ignore(true)     // Respect .gitignore files
            .git_global(true)     // Respect global gitignore
            .git_exclude(true)    // Respect .git/info/exclude
            .require_git(false)   // Work even if not in a git repo
            .follow_links(false)  // Don't follow symlinks
            .build()
        {
            match result {
                Ok(entry) => {
                    let path = entry.path();

                    // Skip .git directory and other common non-code directories
                    let path_str = path.to_string_lossy();
                    if path_str.contains("/.git/")
                        || path_str.contains("\\.git\\")
                        || path_str.ends_with("/.git")
                        || path_str.ends_with("\\.git")
                        || path_str.contains("/node_modules/")
                        || path_str.contains("\\node_modules\\")
                        || path_str.contains("/.venv/")
                        || path_str.contains("\\.venv\\")
                        || path_str.contains("/__pycache__/")
                        || path_str.contains("\\__pycache__\\")
                        || path_str.contains("/target/debug/")
                        || path_str.contains("/target/release/")
                        || path_str.contains("\\target\\debug\\")
                        || path_str.contains("\\target\\release\\")
                    {
                        continue;
                    }

                    if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        self.files.push(path.to_path_buf());
                    }
                }
                Err(_) => continue, // Skip errors (permission denied, etc.)
            }
        }

        // Sort for consistent ordering
        self.files.sort();

        Ok(())
    }

    /// Get all files in the tree
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Get files as relative paths from root
    pub fn relative_files(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter_map(|path| path.strip_prefix(&self.root).ok())
            .map(|p| p.to_path_buf())
            .collect()
    }

    /// Get the root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get file count
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Find files matching a pattern (for fuzzy search)
    pub fn find_files(&self, pattern: &str) -> Vec<PathBuf> {
        let pattern_lower = pattern.to_lowercase();

        self.files
            .iter()
            .filter(|path| {
                if let Some(path_str) = path.to_str() {
                    path_str.to_lowercase().contains(&pattern_lower)
                } else {
                    false
                }
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_tree_creation() {
        let tree = FileTree::new(PathBuf::from("/tmp"));
        assert_eq!(tree.root(), Path::new("/tmp"));
        assert_eq!(tree.file_count(), 0);
    }
}
