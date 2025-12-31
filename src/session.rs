use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Session state for a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// List of open file paths
    pub open_files: Vec<PathBuf>,
    /// Index of the active buffer
    pub active_buffer_index: usize,
    /// Project root directory
    pub project_root: PathBuf,
}

impl SessionState {
    /// Create a new session state
    pub fn new(project_root: PathBuf, open_files: Vec<PathBuf>, active_buffer_index: usize) -> Self {
        Self {
            open_files,
            active_buffer_index,
            project_root,
        }
    }

    /// Generate a unique session file path for a project
    fn session_file_path(project_root: &PathBuf) -> Result<PathBuf> {
        let home = std::env::var("HOME")?;
        let cache_dir = PathBuf::from(home).join(".cache/scame");
        std::fs::create_dir_all(&cache_dir)?;

        // Canonicalize the path to avoid issues with relative paths, symlinks, etc.
        let canonical_root = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());

        // Generate hash from canonical project path
        let mut hasher = DefaultHasher::new();
        canonical_root.hash(&mut hasher);
        let hash = hasher.finish();

        let session_file = cache_dir.join(format!("session_{:x}.json", hash));
        Ok(session_file)
    }

    /// Save session state to disk
    pub fn save(&self) -> Result<()> {
        // Canonicalize the project root before saving to ensure consistency
        let canonical_root = self.project_root.canonicalize().unwrap_or_else(|_| self.project_root.clone());

        let mut state = self.clone();
        state.project_root = canonical_root;

        let session_file = Self::session_file_path(&state.project_root)?;
        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(session_file, json)?;
        Ok(())
    }

    /// Load session state from disk
    pub fn load(project_root: &PathBuf) -> Result<Option<Self>> {
        let session_file = Self::session_file_path(project_root)?;

        if !session_file.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(session_file)?;
        let state: SessionState = serde_json::from_str(&json)?;

        // Canonicalize both the requested and stored project roots
        let canonical_requested = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());
        let canonical_stored = state.project_root.canonicalize().unwrap_or_else(|_| state.project_root.clone());

        // Verify the session is for the correct project
        if canonical_requested != canonical_stored {
            // Session is for a different project, don't load it
            return Ok(None);
        }

        // Verify files still exist and are within the project directory
        let mut valid_files = Vec::new();
        for file in &state.open_files {
            if !file.exists() {
                continue;
            }

            // Canonicalize the file path to resolve symlinks, relative paths, etc.
            let canonical_file = match file.canonicalize() {
                Ok(p) => p,
                Err(_) => continue, // Skip files that can't be canonicalized
            };

            // Only include files that are within the project directory
            if canonical_file.starts_with(&canonical_requested) {
                valid_files.push(file.clone());
            }
        }

        if valid_files.is_empty() {
            return Ok(None);
        }

        // Adjust active buffer index if needed
        let active_index = state.active_buffer_index.min(valid_files.len().saturating_sub(1));

        Ok(Some(SessionState {
            open_files: valid_files,
            active_buffer_index: active_index,
            project_root: state.project_root,
        }))
    }

    /// Clear session state for a project
    pub fn clear(project_root: &PathBuf) -> Result<()> {
        let session_file = Self::session_file_path(project_root)?;
        if session_file.exists() {
            std::fs::remove_file(session_file)?;
        }
        Ok(())
    }
}
