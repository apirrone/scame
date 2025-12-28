use super::buffer::{Buffer, BufferId};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// Manages multiple buffers in the workspace
pub struct Workspace {
    buffers: HashMap<BufferId, Buffer>,
    active_buffer: Option<BufferId>,
    next_id: usize,
    buffer_history: Vec<BufferId>, // For navigation (jump back)
    viewport_size: (u16, u16), // width, height
}

impl Workspace {
    /// Create a new workspace
    pub fn new(viewport_width: u16, viewport_height: u16) -> Self {
        Self {
            buffers: HashMap::new(),
            active_buffer: None,
            next_id: 0,
            buffer_history: Vec::new(),
            viewport_size: (viewport_width, viewport_height),
        }
    }

    /// Create a new empty buffer and make it active
    pub fn new_buffer(&mut self) -> BufferId {
        let id = BufferId(self.next_id);
        self.next_id += 1;

        let (width, height) = self.viewport_size;
        let buffer = Buffer::new(id, width, height);

        self.buffers.insert(id, buffer);
        self.set_active_buffer(id);

        id
    }

    /// Open a file in a new buffer
    pub fn open_file(&mut self, path: PathBuf) -> Result<BufferId> {
        // Check if file is already open
        let existing_id = self.buffers
            .iter()
            .find(|(_, buffer)| buffer.file_path() == Some(&path))
            .map(|(id, _)| *id);

        if let Some(id) = existing_id {
            self.set_active_buffer(id);
            return Ok(id);
        }

        // Create new buffer
        let id = BufferId(self.next_id);
        self.next_id += 1;

        let (width, height) = self.viewport_size;
        let buffer = Buffer::from_file(id, path, width, height)?;

        self.buffers.insert(id, buffer);
        self.set_active_buffer(id);

        Ok(id)
    }

    /// Get the active buffer
    pub fn active_buffer(&self) -> Option<&Buffer> {
        self.active_buffer.and_then(|id| self.buffers.get(&id))
    }

    /// Get the active buffer mutably
    pub fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        self.active_buffer.and_then(|id| self.buffers.get_mut(&id))
    }

    /// Get a specific buffer
    pub fn get_buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    /// Get a specific buffer mutably
    pub fn get_buffer_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    /// Set the active buffer
    pub fn set_active_buffer(&mut self, id: BufferId) {
        if self.buffers.contains_key(&id) {
            // Add current buffer to history before switching
            if let Some(current_id) = self.active_buffer {
                if current_id != id {
                    self.buffer_history.push(current_id);
                }
            }
            self.active_buffer = Some(id);
        }
    }

    /// Switch to the next buffer
    pub fn next_buffer(&mut self) {
        if self.buffers.is_empty() {
            return;
        }

        let ids: Vec<BufferId> = self.buffers.keys().copied().collect();
        if let Some(current_id) = self.active_buffer {
            if let Some(pos) = ids.iter().position(|&id| id == current_id) {
                let next_pos = (pos + 1) % ids.len();
                self.set_active_buffer(ids[next_pos]);
            }
        } else if !ids.is_empty() {
            self.set_active_buffer(ids[0]);
        }
    }

    /// Switch to the previous buffer
    pub fn previous_buffer(&mut self) {
        if self.buffers.is_empty() {
            return;
        }

        let ids: Vec<BufferId> = self.buffers.keys().copied().collect();
        if let Some(current_id) = self.active_buffer {
            if let Some(pos) = ids.iter().position(|&id| id == current_id) {
                let prev_pos = if pos == 0 { ids.len() - 1 } else { pos - 1 };
                self.set_active_buffer(ids[prev_pos]);
            }
        } else if !ids.is_empty() {
            self.set_active_buffer(ids[0]);
        }
    }

    /// Jump back to previous buffer in history
    pub fn jump_back(&mut self) {
        if let Some(prev_id) = self.buffer_history.pop() {
            if self.buffers.contains_key(&prev_id) {
                self.active_buffer = Some(prev_id);
            }
        }
    }

    /// Close a buffer
    pub fn close_buffer(&mut self, id: BufferId) -> Result<()> {
        if let Some(buffer) = self.buffers.get(&id) {
            if buffer.is_modified() {
                anyhow::bail!("Buffer is modified, save or force close");
            }
        }

        self.buffers.remove(&id);

        // If we closed the active buffer, switch to another
        if self.active_buffer == Some(id) {
            self.active_buffer = self.buffers.keys().next().copied();
        }

        Ok(())
    }

    /// Close a buffer without checking if modified
    pub fn force_close_buffer(&mut self, id: BufferId) {
        self.buffers.remove(&id);

        if self.active_buffer == Some(id) {
            self.active_buffer = self.buffers.keys().next().copied();
        }
    }

    /// Get all buffer IDs
    pub fn buffer_ids(&self) -> Vec<BufferId> {
        self.buffers.keys().copied().collect()
    }

    /// Get the number of open buffers
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Check if any buffer is modified
    pub fn has_modified_buffers(&self) -> bool {
        self.buffers.values().any(|b| b.is_modified())
    }

    /// Resize all buffers
    pub fn resize(&mut self, width: u16, height: u16) {
        self.viewport_size = (width, height);
        for buffer in self.buffers.values_mut() {
            buffer.resize(width, height);
        }
    }

    /// Get list of modified buffers
    pub fn modified_buffers(&self) -> Vec<BufferId> {
        self.buffers
            .iter()
            .filter(|(_, b)| b.is_modified())
            .map(|(id, _)| *id)
            .collect()
    }
}
