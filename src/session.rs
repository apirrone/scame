use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Session state for a project
#[derive(Debug, Serialize, Deserialize)]
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

        // Generate hash from project path
        let mut hasher = DefaultHasher::new();
        project_root.hash(&mut hasher);
        let hash = hasher.finish();

        let session_file = cache_dir.join(format!("session_{:x}.json", hash));
        Ok(session_file)
    }

    /// Save session state to disk
    pub fn save(&self) -> Result<()> {
        let session_file = Self::session_file_path(&self.project_root)?;
        let json = serde_json::to_string_pretty(self)?;
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

        // Verify files still exist
        let mut valid_files = Vec::new();
        for file in &state.open_files {
            if file.exists() {
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
