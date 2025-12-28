use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Manages Emacs-style file backups
pub struct BackupManager {
    enabled: bool,
}

impl BackupManager {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    /// Create a backup of a file before saving
    pub fn create_backup(&self, file_path: &Path) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if !file_path.exists() {
            // No need to backup if file doesn't exist yet
            return Ok(());
        }

        let backup_path = self.backup_path(file_path);

        // Copy the current file to backup
        fs::copy(file_path, &backup_path)?;

        Ok(())
    }

    /// Get the backup path for a file (adds ~ suffix)
    fn backup_path(&self, file_path: &Path) -> PathBuf {
        let file_name = file_path.file_name().unwrap_or_default();
        let mut backup_name = file_name.to_os_string();
        backup_name.push("~");

        if let Some(parent) = file_path.parent() {
            parent.join(backup_name)
        } else {
            PathBuf::from(backup_name)
        }
    }

    /// Enable or disable backups
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if backups are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for BackupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_path() {
        let manager = BackupManager::new();
        let path = Path::new("/home/user/test.txt");
        let backup = manager.backup_path(path);
        assert_eq!(backup, Path::new("/home/user/test.txt~"));
    }

    #[test]
    fn test_backup_path_no_dir() {
        let manager = BackupManager::new();
        let path = Path::new("test.txt");
        let backup = manager.backup_path(path);
        assert_eq!(backup, Path::new("test.txt~"));
    }
}
