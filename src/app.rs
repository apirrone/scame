use crate::backup::BackupManager;
use crate::buffer::{Change, Position};
use crate::editor::movement::Movement;
use crate::logger;
use crate::render::{BufferView, FilePicker, StatusBar, Terminal};
use crate::search::{FileSearch, FileSearchResult};
use crate::syntax::{HighlightSpan, Highlighter, SupportedLanguage};
use crate::workspace::{FileTree, Workspace};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::Duration;

pub enum ControlFlow {
    Continue,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    FilePicker,
}

pub struct App {
    workspace: Workspace,
    backup_manager: BackupManager,
    file_tree: Option<FileTree>,
    file_search: FileSearch,
    highlighter: Highlighter,
    mode: AppMode,
    message: Option<String>,
    show_line_numbers: bool,
    quit_attempts: u8,
    clipboard: String,
    // Emacs-style key chord state
    waiting_for_second_key: bool,
    // File picker state
    file_picker_pattern: String,
    file_picker_results: Vec<FileSearchResult>,
    file_picker_selected: usize,
    // Track if we've logged highlighting info for this file
    logged_highlighting: bool,
    // Cache highlight spans to avoid re-parsing every frame
    cached_highlights: Option<Vec<HighlightSpan>>,
    cached_text_hash: u64,
}

impl App {
    /// Simple hash function for text
    fn simple_hash(text: &str) -> u64 {
        let mut hash: u64 = 5381;
        for byte in text.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }

    /// Create a new app instance
    pub fn new() -> Result<Self> {
        let (width, height) = crossterm::terminal::size()?;
        let mut workspace = Workspace::new(width, height.saturating_sub(1));

        // Create an empty buffer
        workspace.new_buffer();

        Ok(Self {
            workspace,
            backup_manager: BackupManager::new(),
            file_tree: None,
            file_search: FileSearch::new(),
            highlighter: Highlighter::new(),
            mode: AppMode::Normal,
            logged_highlighting: false,
            cached_highlights: None,
            cached_text_hash: 0,
            message: None,
            show_line_numbers: true,
            quit_attempts: 0,
            clipboard: String::new(),
            waiting_for_second_key: false,
            file_picker_pattern: String::new(),
            file_picker_results: Vec::new(),
            file_picker_selected: 0,
        })
    }

    /// Create app from a file or directory
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let (width, height) = crossterm::terminal::size()?;
        let mut workspace = Workspace::new(width, height.saturating_sub(1));

        // Determine if it's a file or directory
        if path.is_file() {
            workspace.open_file(path)?;
        } else if path.is_dir() {
            // Open directory as project
            let mut file_tree = FileTree::new(path.clone());
            file_tree.scan()?;

            // Create empty buffer
            workspace.new_buffer();

            return Ok(Self {
                workspace,
                backup_manager: BackupManager::new(),
                file_tree: Some(file_tree),
                file_search: FileSearch::new(),
                highlighter: Highlighter::new(),
                mode: AppMode::Normal,
                message: None,
                show_line_numbers: true,
                quit_attempts: 0,
                clipboard: String::new(),
                waiting_for_second_key: false,
                file_picker_pattern: String::new(),
                file_picker_results: Vec::new(),
                file_picker_selected: 0,
                logged_highlighting: false,
                cached_highlights: None,
                cached_text_hash: 0,
            });
        }

        Ok(Self {
            workspace,
            backup_manager: BackupManager::new(),
            file_tree: None,
            file_search: FileSearch::new(),
            highlighter: Highlighter::new(),
            mode: AppMode::Normal,
            logged_highlighting: false,
            cached_highlights: None,
            cached_text_hash: 0,
            message: None,
            show_line_numbers: true,
            quit_attempts: 0,
            clipboard: String::new(),
            waiting_for_second_key: false,
            file_picker_pattern: String::new(),
            file_picker_results: Vec::new(),
            file_picker_selected: 0,
        })
    }

    /// Render the application
    pub fn render(&mut self, terminal: &Terminal) -> Result<()> {
        // Hide cursor during rendering to prevent flickering
        terminal.hide_cursor()?;

        if let Some(buffer) = self.workspace.active_buffer() {
            // Get syntax highlighting if supported (with caching)
            let highlight_spans = if let Some(path) = buffer.file_path() {
                if let Some(lang) = SupportedLanguage::from_path(path) {
                    // Compute hash of current text
                    let text = buffer.text_buffer().to_string();
                    let text_hash = Self::simple_hash(&text);

                    // Check if we can use cached highlights
                    if self.cached_text_hash == text_hash && self.cached_highlights.is_some() {
                        // Use cached highlights
                        self.cached_highlights.clone()
                    } else {
                        // Need to recompute
                        if !self.logged_highlighting {
                            logger::log(&format!("File path detected: {:?}", path));
                            logger::log(&format!("Language detected: {:?}", lang));
                        }

                        match (|| -> anyhow::Result<Vec<HighlightSpan>> {
                            if !self.logged_highlighting {
                                logger::log("Computing syntax highlighting...");
                            }
                            self.highlighter.set_language(&lang.language())?;
                            let file_id = path.to_string_lossy().to_string();
                            let query = lang.query()?;
                            let capture_names = lang.capture_names()?;
                            let result = self.highlighter.highlight(&text, &file_id, &query, &capture_names)?;
                            if !self.logged_highlighting {
                                logger::log(&format!("Got {} highlight spans", result.len()));
                            }
                            Ok(result)
                        })() {
                            Ok(spans) => {
                                if !self.logged_highlighting {
                                    logger::log("Highlighting successful!");
                                    self.logged_highlighting = true;
                                }
                                // Cache the results
                                self.cached_highlights = Some(spans.clone());
                                self.cached_text_hash = text_hash;
                                Some(spans)
                            },
                            Err(e) => {
                                if !self.logged_highlighting {
                                    logger::log(&format!("ERROR: Syntax highlighting failed: {}", e));
                                    self.logged_highlighting = true;
                                }
                                None
                            }
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            BufferView::render(
                terminal,
                buffer.text_buffer(),
                buffer.editor_state(),
                self.show_line_numbers,
                highlight_spans.as_deref(),
                self.highlighter.theme(),
            )?;
            StatusBar::render(
                terminal,
                buffer.text_buffer(),
                buffer.editor_state(),
                self.message.as_deref(),
            )?;
            // Position cursor (but don't show yet)
            BufferView::position_cursor(
                terminal,
                buffer.editor_state(),
                self.show_line_numbers,
                buffer.text_buffer(),
            )?;
        }

        // Render file picker overlay if active
        if self.mode == AppMode::FilePicker {
            FilePicker::render(
                terminal,
                &self.file_picker_pattern,
                &self.file_picker_results,
                self.file_picker_selected,
            )?;
        }

        // Flush all buffered commands
        terminal.flush()?;

        // Show cursor only after everything is flushed
        terminal.show_cursor()?;
        terminal.flush()?;

        Ok(())
    }

    /// Handle terminal resize
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.workspace.resize(width, height.saturating_sub(1));
    }

    /// Handle an input event
    pub fn handle_event(&mut self, event: Event) -> Result<ControlFlow> {
        match event {
            Event::Key(key_event) => {
                // Reset quit attempts if not Ctrl+Q
                if !(key_event.code == KeyCode::Char('q')
                    && key_event.modifiers.contains(KeyModifiers::CONTROL))
                {
                    self.quit_attempts = 0;
                    if self.mode == AppMode::Normal {
                        self.message = None;
                    }
                }
                self.handle_key(key_event)
            }
            Event::Resize(width, height) => {
                self.handle_resize(width, height);
                Ok(ControlFlow::Continue)
            }
            _ => Ok(ControlFlow::Continue),
        }
    }

    /// Handle a key press
    fn handle_key(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match self.mode {
            AppMode::Normal => self.handle_normal_mode(key),
            AppMode::FilePicker => self.handle_file_picker_mode(key),
        }
    }

    /// Handle key in file picker mode
    fn handle_file_picker_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Cancel file picker
                self.mode = AppMode::Normal;
                self.file_picker_pattern.clear();
                self.file_picker_results.clear();
            }
            KeyCode::Enter => {
                // Open selected file
                if let Some(result) = self.file_picker_results.get(self.file_picker_selected) {
                    self.workspace.open_file(result.path.clone())?;
                    self.mode = AppMode::Normal;
                    self.file_picker_pattern.clear();
                    self.file_picker_results.clear();
                    // Invalidate highlight cache when opening a new file
                    self.cached_highlights = None;
                    self.cached_text_hash = 0;
                    self.logged_highlighting = false;
                }
            }
            KeyCode::Up => {
                if self.file_picker_selected > 0 {
                    self.file_picker_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.file_picker_selected + 1 < self.file_picker_results.len() {
                    self.file_picker_selected += 1;
                }
            }
            KeyCode::Char(c) => {
                self.file_picker_pattern.push(c);
                self.update_file_picker_results();
            }
            KeyCode::Backspace => {
                self.file_picker_pattern.pop();
                self.update_file_picker_results();
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    fn update_file_picker_results(&mut self) {
        if let Some(file_tree) = &self.file_tree {
            // Get current file extension for priority matching
            let priority_ext = self.workspace.active_buffer()
                .and_then(|b| b.file_path())
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str());

            self.file_picker_results = self.file_search.search(file_tree, &self.file_picker_pattern, priority_ext);
            self.file_picker_selected = 0;
        }
    }

    /// Handle key in normal mode
    fn handle_normal_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        let Some(buffer) = self.workspace.active_buffer_mut() else {
            return Ok(ControlFlow::Continue);
        };

        // Handle Ctrl+X Ctrl+S (Emacs-style save) and Ctrl+X Ctrl+C (Emacs-style exit)
        if self.waiting_for_second_key {
            self.waiting_for_second_key = false;
            if matches!(key.code, KeyCode::Char('s')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+S - Save
                if let Some(path) = buffer.file_path() {
                    self.backup_manager.create_backup(path)?;
                    buffer.text_buffer_mut().save()?;
                    self.message = Some("Saved".to_string());
                } else {
                    self.message = Some("No file path set".to_string());
                }
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+C - Exit
                if self.workspace.has_modified_buffers() && self.quit_attempts == 0 {
                    self.quit_attempts += 1;
                    self.message = Some("Buffers modified! Press Ctrl+X Ctrl+C again to quit without saving".to_string());
                    self.waiting_for_second_key = true; // Keep waiting for second key
                    return Ok(ControlFlow::Continue);
                }
                return Ok(ControlFlow::Exit);
            }
            // If not Ctrl+S or Ctrl+C, fall through to handle the key normally
        }

        match (key.code, key.modifiers) {
            // Ctrl+Q - Quit
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                if self.workspace.has_modified_buffers() && self.quit_attempts == 0 {
                    self.quit_attempts += 1;
                    self.message = Some("Buffers modified! Press Ctrl+Q again to quit without saving".to_string());
                    return Ok(ControlFlow::Continue);
                }
                return Ok(ControlFlow::Exit);
            }

            // Ctrl+P - File picker
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                if self.file_tree.is_some() {
                    self.mode = AppMode::FilePicker;
                    self.file_picker_pattern.clear();
                    self.file_picker_results.clear();
                    self.file_picker_selected = 0;

                    // Show what extension is being prioritized
                    if let Some(ext) = buffer.file_path()
                        .and_then(|p| p.extension())
                        .and_then(|e| e.to_str()) {
                        self.message = Some(format!("Prioritizing .{} files", ext));
                    }
                } else {
                    self.message = Some("No project directory open".to_string());
                }
            }

            // Ctrl+Tab - Next buffer
            (KeyCode::Tab, KeyModifiers::CONTROL) => {
                self.workspace.next_buffer();
                // Invalidate highlight cache when switching buffers
                self.cached_highlights = None;
                self.cached_text_hash = 0;
                self.logged_highlighting = false;
            }

            // Ctrl+Shift+Tab - Previous buffer
            (KeyCode::BackTab, _) => {
                self.workspace.previous_buffer();
                // Invalidate highlight cache when switching buffers
                self.cached_highlights = None;
                self.cached_text_hash = 0;
                self.logged_highlighting = false;
            }

            // Ctrl+X - Start Emacs-style chord (Ctrl+X Ctrl+S for save, Ctrl+X Ctrl+C to exit)
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                self.waiting_for_second_key = true;
                self.message = Some("Ctrl+X-".to_string());
            }

            // Ctrl+Z - Undo
            (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
                if let Some(change) = buffer.undo_manager_mut().undo() {
                    buffer.apply_change(&change)?;
                    buffer.undo_manager_mut().finish_undo_redo();
                    self.message = Some("Undo".to_string());
                }
            }

            // Ctrl+Shift+Z - Redo
            (KeyCode::Char('z'), mods)
                if mods.contains(KeyModifiers::CONTROL) && mods.contains(KeyModifiers::SHIFT) =>
            {
                if let Some(change) = buffer.undo_manager_mut().redo() {
                    buffer.apply_change(&change)?;
                    buffer.undo_manager_mut().finish_undo_redo();
                    self.message = Some("Redo".to_string());
                }
            }

            // Ctrl+A - Beginning of line
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                Movement::move_to_line_start(buffer.editor_state_mut());
            }

            // Ctrl+E - End of line
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                Movement::move_to_line_end(editor_state, text_buffer);
            }

            // Ctrl+K - Kill line (delete from cursor to end of line)
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                let line = editor_state.cursor.line;
                let col = editor_state.cursor.column;
                let line_len = text_buffer.line_len(line);

                if col < line_len {
                    let start = Position::new(line, col);
                    let end = Position::new(line, line_len);
                    if let Ok(deleted) = text_buffer.delete_range(start, end) {
                        self.clipboard = deleted.clone();
                        undo_manager.record(Change::Delete {
                            pos: start,
                            text: deleted,
                        });
                        self.message = Some("Killed to clipboard".to_string());
                    }
                } else if line + 1 < text_buffer.len_lines() {
                    // At end of line, delete the newline
                    let pos = Position::new(line, line_len);
                    if let Ok(Some(ch)) = text_buffer.delete_char(pos) {
                        self.clipboard = ch.to_string();
                        undo_manager.record(Change::Delete {
                            pos,
                            text: ch.to_string(),
                        });
                    }
                }
            }

            // Ctrl+C - Copy selection to clipboard
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                if let Some(selection) = buffer.editor_state().selection {
                    let (start, end) = selection.range();
                    let start_idx = buffer.text_buffer().pos_to_char(start)?;
                    let end_idx = buffer.text_buffer().pos_to_char(end)?;

                    if start_idx < end_idx {
                        let text = buffer.text_buffer().to_string();
                        let selected = text.chars().skip(start_idx).take(end_idx - start_idx).collect();
                        self.clipboard = selected;
                        self.message = Some("Copied to clipboard".to_string());
                    }
                } else {
                    self.message = Some("No selection".to_string());
                }
            }

            // Ctrl+V - Paste from clipboard
            (KeyCode::Char('v'), KeyModifiers::CONTROL) => {
                if !self.clipboard.is_empty() {
                    let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                    let pos = editor_state.cursor.position();
                    text_buffer.insert(pos, &self.clipboard)?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: self.clipboard.clone(),
                    });

                    // Move cursor to end of pasted text
                    let char_idx = text_buffer.pos_to_char(pos)? + self.clipboard.len();
                    editor_state.cursor.set_position(text_buffer.char_to_pos(char_idx));
                    editor_state.ensure_cursor_visible();
                    self.message = Some("Pasted from clipboard".to_string());
                } else {
                    self.message = Some("Clipboard empty".to_string());
                }
            }

            // Arrow keys with optional Shift (selection)
            (KeyCode::Left, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::move_left(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                } else {
                    editor_state.clear_selection();
                }
            }
            (KeyCode::Right, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::move_right(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                } else {
                    editor_state.clear_selection();
                }
            }
            (KeyCode::Up, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::move_up(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                } else {
                    editor_state.clear_selection();
                }
            }
            (KeyCode::Down, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::move_down(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                } else {
                    editor_state.clear_selection();
                }
            }

            // Home/End
            (KeyCode::Home, _) => {
                let (_, editor_state, _) = buffer.split_mut();
                Movement::move_to_line_start(editor_state);
                editor_state.clear_selection();
            }
            (KeyCode::End, _) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                Movement::move_to_line_end(editor_state, text_buffer);
                editor_state.clear_selection();
            }

            // Page Up/Down
            (KeyCode::PageUp, _) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                Movement::page_up(editor_state, text_buffer);
                editor_state.clear_selection();
            }
            (KeyCode::PageDown, _) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                Movement::page_down(editor_state, text_buffer);
                editor_state.clear_selection();
            }

            // Backspace
            (KeyCode::Backspace, _) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                if editor_state.cursor.column > 0 {
                    let pos = Position::new(
                        editor_state.cursor.line,
                        editor_state.cursor.column - 1,
                    );
                    if let Ok(Some(ch)) = text_buffer.delete_char(pos) {
                        undo_manager.record(Change::Delete {
                            pos,
                            text: ch.to_string(),
                        });
                        Movement::move_left(editor_state, text_buffer);
                    }
                } else if editor_state.cursor.line > 0 {
                    let prev_line_len = text_buffer.line_len(editor_state.cursor.line - 1);
                    let pos = Position::new(editor_state.cursor.line - 1, prev_line_len);
                    if let Ok(Some(deleted)) = text_buffer.delete_char(pos) {
                        undo_manager.record(Change::Delete {
                            pos,
                            text: deleted.to_string(),
                        });
                        editor_state.cursor.move_to(editor_state.cursor.line - 1, prev_line_len);
                        editor_state.ensure_cursor_visible();
                    }
                }
            }

            // Delete
            (KeyCode::Delete, _) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                let pos = editor_state.cursor.position();
                if let Ok(Some(ch)) = text_buffer.delete_char(pos) {
                    undo_manager.record(Change::Delete {
                        pos,
                        text: ch.to_string(),
                    });
                }
            }

            // Enter
            (KeyCode::Enter, _) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                let pos = editor_state.cursor.position();
                text_buffer.insert_char(pos, '\n')?;
                undo_manager.record(Change::Insert {
                    pos,
                    text: "\n".to_string(),
                });
                editor_state.cursor.line += 1;
                editor_state.cursor.move_horizontal(0);
                editor_state.ensure_cursor_visible();
            }

            // Regular character input
            (KeyCode::Char(c), mods) if !mods.contains(KeyModifiers::CONTROL) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                let pos = editor_state.cursor.position();
                text_buffer.insert_char(pos, c)?;
                undo_manager.record(Change::Insert {
                    pos,
                    text: c.to_string(),
                });
                Movement::move_right(editor_state, text_buffer);
            }

            _ => {}
        }

        Ok(ControlFlow::Continue)
    }
}

/// Poll for an event with a timeout
pub fn poll_event(timeout: Duration) -> Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}
