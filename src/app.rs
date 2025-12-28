use crate::backup::BackupManager;
use crate::buffer::{Change, Position};
use crate::editor::movement::Movement;
use crate::logger;
use crate::lsp::{DiagnosticsStore, LspManager, LspResponse};
use crate::render::{BufferView, FilePicker, StatusBar, Terminal};
use crate::search::{FileSearch, FileSearchResult};
use crate::syntax::{HighlightSpan, Highlighter, SupportedLanguage};
use crate::workspace::{FileTree, Workspace};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use regex::RegexBuilder;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub enum ControlFlow {
    Continue,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    FilePicker,
    ConfirmExit,
    Search,
    JumpToLine,
    ReplacePrompt,      // Prompting for search pattern
    ReplaceEnterRepl,   // Prompting for replacement string
    ReplaceConfirm,     // Confirming each replacement
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
    // Search state
    search_pattern: String,
    search_start_pos: Option<Position>,
    search_is_reverse: bool,
    search_use_regex: bool,
    // Jump to line state
    jump_to_line_input: String,
    // Replace state
    replace_pattern: String,
    replace_with: String,
    replace_count: usize,
    // LSP state
    lsp_manager: Option<LspManager>,
    lsp_receiver: Option<mpsc::UnboundedReceiver<LspResponse>>,
    diagnostics_store: DiagnosticsStore,
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
            clipboard: String::new(),
            waiting_for_second_key: false,
            file_picker_pattern: String::new(),
            file_picker_results: Vec::new(),
            file_picker_selected: 0,
            search_pattern: String::new(),
            search_start_pos: None,
            search_is_reverse: false,
            search_use_regex: false,
            jump_to_line_input: String::new(),
            replace_pattern: String::new(),
            replace_with: String::new(),
            replace_count: 0,
            lsp_manager: None,
            lsp_receiver: None,
            diagnostics_store: DiagnosticsStore::new(),
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
                clipboard: String::new(),
                waiting_for_second_key: false,
                file_picker_pattern: String::new(),
                file_picker_results: Vec::new(),
                file_picker_selected: 0,
                logged_highlighting: false,
                cached_highlights: None,
                cached_text_hash: 0,
                search_pattern: String::new(),
                search_start_pos: None,
                search_is_reverse: false,
                search_use_regex: false,
                jump_to_line_input: String::new(),
                replace_pattern: String::new(),
                replace_with: String::new(),
                replace_count: 0,
                lsp_manager: None,
                lsp_receiver: None,
                diagnostics_store: DiagnosticsStore::new(),
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
            clipboard: String::new(),
            waiting_for_second_key: false,
            file_picker_pattern: String::new(),
            file_picker_results: Vec::new(),
            file_picker_selected: 0,
            search_pattern: String::new(),
            search_start_pos: None,
            search_is_reverse: false,
            search_use_regex: false,
            jump_to_line_input: String::new(),
            replace_pattern: String::new(),
            replace_with: String::new(),
            replace_count: 0,
            lsp_manager: None,
            lsp_receiver: None,
            diagnostics_store: DiagnosticsStore::new(),
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

            // Get diagnostics for current buffer
            let buffer_diagnostics = self.diagnostics_store.get(buffer.id().0);

            BufferView::render(
                terminal,
                buffer.text_buffer(),
                buffer.editor_state(),
                self.show_line_numbers,
                highlight_spans.as_deref(),
                self.highlighter.theme(),
                buffer_diagnostics,
            )?;
            StatusBar::render(
                terminal,
                buffer.text_buffer(),
                buffer.editor_state(),
                self.message.as_deref(),
                buffer_diagnostics,
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
            AppMode::ConfirmExit => self.handle_confirm_exit_mode(key),
            AppMode::Search => self.handle_search_mode(key),
            AppMode::JumpToLine => self.handle_jump_to_line_mode(key),
            AppMode::ReplacePrompt => self.handle_replace_prompt_mode(key),
            AppMode::ReplaceEnterRepl => self.handle_replace_enter_repl_mode(key),
            AppMode::ReplaceConfirm => self.handle_replace_confirm_mode(key),
        }
    }

    /// Handle key in file picker mode
    fn handle_file_picker_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc => {
                // Cancel file picker
                self.mode = AppMode::Normal;
                self.file_picker_pattern.clear();
                self.file_picker_results.clear();
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel file picker (Emacs style)
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
                    // Notify LSP about newly opened file
                    self.notify_lsp_did_open();
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

    /// Handle key in confirm exit mode
    fn handle_confirm_exit_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Save all modified buffers then exit
                self.workspace.save_all_modified_buffers(&self.backup_manager)?;
                self.message = Some("Saved all buffers".to_string());
                return Ok(ControlFlow::Exit);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Exit without saving
                return Ok(ControlFlow::Exit);
            }
            KeyCode::Esc => {
                // Cancel and return to normal mode
                self.mode = AppMode::Normal;
                self.message = None;
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel and return to normal mode (Emacs style)
                self.mode = AppMode::Normal;
                self.message = None;
            }
            _ => {
                // Any other key cancels
                self.mode = AppMode::Normal;
                self.message = None;
            }
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in jump to line mode
    fn handle_jump_to_line_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc => {
                // Cancel jump to line
                self.mode = AppMode::Normal;
                self.message = None;
                self.jump_to_line_input.clear();
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel jump to line (Emacs style)
                self.mode = AppMode::Normal;
                self.message = None;
                self.jump_to_line_input.clear();
            }
            KeyCode::Enter => {
                // Jump to the specified line
                if let Ok(line_num) = self.jump_to_line_input.parse::<usize>() {
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        let text_buffer = buffer.text_buffer();
                        let max_line = text_buffer.len_lines().saturating_sub(1);

                        // Line numbers are 1-indexed for users, but 0-indexed internally
                        let target_line = if line_num > 0 {
                            (line_num - 1).min(max_line)
                        } else {
                            0
                        };

                        // Jump to the line
                        let line_len = text_buffer.line_len(target_line);
                        buffer.editor_state_mut().cursor.move_to(target_line, 0);
                        buffer.editor_state_mut().ensure_cursor_visible();

                        self.message = Some(format!("Line {}", target_line + 1));
                    }
                } else {
                    self.message = Some("Invalid line number".to_string());
                }

                self.mode = AppMode::Normal;
                self.jump_to_line_input.clear();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                // Add digit to input
                self.jump_to_line_input.push(c);
                self.message = Some(format!("Go to line: {}", self.jump_to_line_input));
            }
            KeyCode::Backspace => {
                self.jump_to_line_input.pop();
                if self.jump_to_line_input.is_empty() {
                    self.message = Some("Go to line:".to_string());
                } else {
                    self.message = Some(format!("Go to line: {}", self.jump_to_line_input));
                }
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in replace prompt mode (entering search pattern)
    fn handle_replace_prompt_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('g') if key.code == KeyCode::Esc || key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Cancel replace
                self.mode = AppMode::Normal;
                self.message = None;
                self.replace_pattern.clear();
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+T: Toggle regex mode
                self.search_use_regex = !self.search_use_regex;
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("Replace{}: {}", regex_indicator, self.replace_pattern));
            }
            KeyCode::Enter => {
                if !self.replace_pattern.is_empty() {
                    // Move to replacement string prompt
                    self.mode = AppMode::ReplaceEnterRepl;
                    let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                    self.message = Some(format!("Replace{} \"{}\" with:", regex_indicator, self.replace_pattern));
                } else {
                    // Empty pattern, cancel
                    self.mode = AppMode::Normal;
                    self.message = None;
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Add character to pattern
                self.replace_pattern.push(c);
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("Replace{}: {}", regex_indicator, self.replace_pattern));
            }
            KeyCode::Backspace => {
                self.replace_pattern.pop();
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("Replace{}: {}", regex_indicator, self.replace_pattern));
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in replace enter replacement mode (entering replacement string)
    fn handle_replace_enter_repl_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('g') if key.code == KeyCode::Esc || key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Cancel replace
                self.mode = AppMode::Normal;
                self.message = None;
                self.replace_pattern.clear();
                self.replace_with.clear();
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+T: Toggle regex mode
                self.search_use_regex = !self.search_use_regex;
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("Replace{} \"{}\" with: {}", regex_indicator, self.replace_pattern, self.replace_with));
            }
            KeyCode::Enter => {
                // Start replacing
                self.replace_count = 0;
                // Find first occurrence
                if self.find_next_replace_match()? {
                    self.mode = AppMode::ReplaceConfirm;
                    let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                    self.message = Some(format!(
                        "Replace{} \"{}\" with \"{}\"? (y)es, (n)o, (a)ll, (q)uit",
                        regex_indicator, self.replace_pattern, self.replace_with
                    ));
                } else {
                    // No matches found
                    self.mode = AppMode::Normal;
                    self.message = Some("No matches found".to_string());
                    self.replace_pattern.clear();
                    self.replace_with.clear();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Add character to replacement
                self.replace_with.push(c);
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("Replace{} \"{}\" with: {}", regex_indicator, self.replace_pattern, self.replace_with));
            }
            KeyCode::Backspace => {
                self.replace_with.pop();
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("Replace{} \"{}\" with: {}", regex_indicator, self.replace_pattern, self.replace_with));
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in replace confirm mode (confirming each replacement)
    fn handle_replace_confirm_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Yes, replace current match
                self.perform_replace()?;
                self.replace_count += 1;
                // Find next match
                if self.find_next_replace_match()? {
                    let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                    self.message = Some(format!(
                        "Replace{} \"{}\" with \"{}\"? (y)es, (n)o, (a)ll, (q)uit [{} replaced]",
                        regex_indicator, self.replace_pattern, self.replace_with, self.replace_count
                    ));
                } else {
                    // No more matches
                    self.mode = AppMode::Normal;
                    self.message = Some(format!("Replaced {} occurrence(s)", self.replace_count));
                    self.replace_pattern.clear();
                    self.replace_with.clear();
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        buffer.editor_state_mut().clear_selection();
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // No, skip this match
                // Find next match
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    let text_buffer = buffer.text_buffer();
                    let current_pos = buffer.editor_state().cursor.position();
                    let current_char = text_buffer.pos_to_char(current_pos)?;
                    let new_pos = text_buffer.char_to_pos(current_char + 1);
                    buffer.editor_state_mut().cursor.set_position(new_pos);
                }

                if self.find_next_replace_match()? {
                    let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                    self.message = Some(format!(
                        "Replace{} \"{}\" with \"{}\"? (y)es, (n)o, (a)ll, (q)uit [{} replaced]",
                        regex_indicator, self.replace_pattern, self.replace_with, self.replace_count
                    ));
                } else {
                    // No more matches
                    self.mode = AppMode::Normal;
                    self.message = Some(format!("Replaced {} occurrence(s)", self.replace_count));
                    self.replace_pattern.clear();
                    self.replace_with.clear();
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        buffer.editor_state_mut().clear_selection();
                    }
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Replace all remaining matches
                loop {
                    self.perform_replace()?;
                    self.replace_count += 1;

                    // Move cursor forward
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        let text_buffer = buffer.text_buffer();
                        let current_pos = buffer.editor_state().cursor.position();
                        let current_char = text_buffer.pos_to_char(current_pos)?;
                        let new_pos = text_buffer.char_to_pos(current_char + self.replace_with.chars().count());
                        buffer.editor_state_mut().cursor.set_position(new_pos);
                    }

                    if !self.find_next_replace_match()? {
                        break;
                    }
                }

                // Done replacing all
                self.mode = AppMode::Normal;
                self.message = Some(format!("Replaced {} occurrence(s)", self.replace_count));
                self.replace_pattern.clear();
                self.replace_with.clear();
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                // Quit replacing
                self.mode = AppMode::Normal;
                self.message = Some(format!("Replaced {} occurrence(s)", self.replace_count));
                self.replace_pattern.clear();
                self.replace_with.clear();
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Quit (Emacs style)
                self.mode = AppMode::Normal;
                self.message = Some(format!("Replaced {} occurrence(s)", self.replace_count));
                self.replace_pattern.clear();
                self.replace_with.clear();
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in search mode
    fn handle_search_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc => {
                // Cancel search
                self.mode = AppMode::Normal;
                self.message = None;
                self.search_pattern.clear();
                // Clear selection when exiting search
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel search (Emacs style)
                self.mode = AppMode::Normal;
                self.message = None;
                self.search_pattern.clear();
                // Clear selection when exiting search
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+S in search mode: search forward for next occurrence
                if !self.search_pattern.is_empty() {
                    self.search_is_reverse = false;
                    // Save current position in case search fails
                    let saved_pos = if let Some(buffer) = self.workspace.active_buffer() {
                        Some(buffer.editor_state().cursor.position())
                    } else {
                        None
                    };

                    // Move cursor forward by one character to find next match
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        let current_pos = buffer.editor_state().cursor.position();
                        let text_buffer = buffer.text_buffer();
                        let current_char = text_buffer.pos_to_char(current_pos)?;
                        let new_pos = text_buffer.char_to_pos(current_char + 1);
                        buffer.editor_state_mut().cursor.set_position(new_pos);
                    }

                    // Try to search
                    if !self.perform_search()? {
                        // No match found, restore position and show message
                        if let Some(pos) = saved_pos {
                            self.workspace.active_buffer_mut().unwrap().editor_state_mut().cursor.set_position(pos);
                        }
                        self.message = Some("Last occurrence".to_string());
                    }
                }
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+R in search mode: search backward for previous occurrence
                if !self.search_pattern.is_empty() {
                    self.search_is_reverse = true;
                    // Save current position in case search fails
                    let saved_pos = if let Some(buffer) = self.workspace.active_buffer() {
                        Some(buffer.editor_state().cursor.position())
                    } else {
                        None
                    };

                    // Move cursor backward by one character to find previous match
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        let current_pos = buffer.editor_state().cursor.position();
                        let text_buffer = buffer.text_buffer();
                        let current_char = text_buffer.pos_to_char(current_pos)?;
                        if current_char > 0 {
                            let new_pos = text_buffer.char_to_pos(current_char - 1);
                            buffer.editor_state_mut().cursor.set_position(new_pos);
                        }
                    }

                    // Try to search
                    if !self.perform_search()? {
                        // No match found, restore position and show message
                        if let Some(pos) = saved_pos {
                            self.workspace.active_buffer_mut().unwrap().editor_state_mut().cursor.set_position(pos);
                        }
                        self.message = Some("First occurrence".to_string());
                    }
                }
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+T: Toggle regex mode
                self.search_use_regex = !self.search_use_regex;
                let search_type = if self.search_is_reverse { "Reverse search" } else { "Search" };
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("{}{}: {}", search_type, regex_indicator, self.search_pattern));
                // Re-search with new mode if pattern exists
                if !self.search_pattern.is_empty() {
                    if let Some(start_pos) = self.search_start_pos {
                        self.workspace.active_buffer_mut().unwrap().editor_state_mut().cursor.set_position(start_pos);
                    }
                    let _ = self.perform_search();
                }
            }
            KeyCode::Enter => {
                // Enter: exit search mode
                self.mode = AppMode::Normal;
                self.message = None;
                self.search_pattern.clear();
                // Clear selection when exiting search
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                // Arrow keys: exit search mode and handle the movement in normal mode
                self.mode = AppMode::Normal;
                self.message = None;
                self.search_pattern.clear();
                // Clear selection when exiting search
                if let Some(buffer) = self.workspace.active_buffer_mut() {
                    buffer.editor_state_mut().clear_selection();
                }
                // Re-handle the key event in normal mode
                return self.handle_normal_mode(key);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Add character to search pattern
                self.search_pattern.push(c);
                // Update message to show current pattern
                let search_type = if self.search_is_reverse { "Reverse search" } else { "Search" };
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("{}{}: {}", search_type, regex_indicator, self.search_pattern));
                // Incremental search - search as we type
                if !self.search_pattern.is_empty() {
                    let _ = self.perform_search(); // Ignore errors for incremental search
                }
            }
            KeyCode::Backspace => {
                self.search_pattern.pop();
                // Update message to show current pattern
                let search_type = if self.search_is_reverse { "Reverse search" } else { "Search" };
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("{}{}: {}", search_type, regex_indicator, self.search_pattern));
                // Reset to start position when pattern changes
                if let Some(buffer) = self.workspace.active_buffer() {
                    if let Some(start_pos) = self.search_start_pos {
                        self.workspace.active_buffer_mut().unwrap().editor_state_mut().cursor.set_position(start_pos);
                    }
                }
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Perform search from current cursor position
    /// Returns true if a match was found, false otherwise
    fn perform_search(&mut self) -> Result<bool> {
        let Some(buffer) = self.workspace.active_buffer_mut() else {
            return Ok(false);
        };

        let text = buffer.text_buffer().to_string();
        let current_pos = buffer.editor_state().cursor.position();
        let current_char_idx = buffer.text_buffer().pos_to_char(current_pos)?;

        // Search using regex or plain string
        let found_match: Option<(usize, usize)> = if self.search_use_regex {
            // Regex search (case-insensitive by default)
            match RegexBuilder::new(&self.search_pattern).case_insensitive(true).build() {
                Ok(re) => {
                    if self.search_is_reverse {
                        // Reverse regex search - find all matches before cursor
                        let before_chars: String = text.chars().take(current_char_idx).collect();
                        let mut last_match: Option<(usize, usize)> = None;
                        for m in re.find_iter(&before_chars) {
                            let char_start = before_chars[..m.start()].chars().count();
                            let char_len = before_chars[m.start()..m.end()].chars().count();
                            last_match = Some((char_start, char_len));
                        }
                        last_match
                    } else {
                        // Forward regex search
                        let after_chars: String = text.chars().skip(current_char_idx).collect();
                        re.find(&after_chars).map(|m| {
                            let char_start = current_char_idx + after_chars[..m.start()].chars().count();
                            let char_len = after_chars[m.start()..m.end()].chars().count();
                            (char_start, char_len)
                        })
                    }
                }
                Err(_) => {
                    // Invalid regex, show error
                    self.message = Some(format!("Invalid regex: {}", self.search_pattern));
                    return Ok(false);
                }
            }
        } else {
            // Plain string search (case-insensitive)
            let text_lower = text.to_lowercase();
            let pattern_lower = self.search_pattern.to_lowercase();

            if self.search_is_reverse {
                // Reverse search - search before current position
                let before_chars: String = text_lower.chars().take(current_char_idx).collect();
                before_chars.rfind(&pattern_lower).map(|byte_offset| {
                    let char_idx = before_chars[..byte_offset].chars().count();
                    let match_len = self.search_pattern.chars().count();
                    (char_idx, match_len)
                })
            } else {
                // Forward search (start from next character)
                let after_chars: String = text_lower.chars().skip(current_char_idx).collect();
                after_chars.find(&pattern_lower).map(|byte_offset| {
                    let char_idx = current_char_idx + after_chars[..byte_offset].chars().count();
                    let match_len = self.search_pattern.chars().count();
                    (char_idx, match_len)
                })
            }
        };

        if let Some((char_idx, match_len)) = found_match {
            let pos = buffer.text_buffer().char_to_pos(char_idx);

            // Move cursor to found position
            buffer.editor_state_mut().cursor.set_position(pos);
            buffer.editor_state_mut().ensure_cursor_visible();

            // Select the found text
            let end_char_idx = char_idx + match_len;
            let end_pos = buffer.text_buffer().char_to_pos(end_char_idx);
            buffer.editor_state_mut().selection = Some(crate::editor::state::Selection::new(pos, end_pos));

            self.message = Some(format!("Found: {}", self.search_pattern));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Find the next match for replacement (forward search only)
    /// Returns true if a match was found, false otherwise
    fn find_next_replace_match(&mut self) -> Result<bool> {
        let Some(buffer) = self.workspace.active_buffer_mut() else {
            return Ok(false);
        };

        let text = buffer.text_buffer().to_string();
        let current_pos = buffer.editor_state().cursor.position();
        let current_char_idx = buffer.text_buffer().pos_to_char(current_pos)?;

        // Search using regex or plain string
        let found_match: Option<(usize, usize)> = if self.search_use_regex {
            // Regex search (case-insensitive by default)
            match RegexBuilder::new(&self.replace_pattern).case_insensitive(true).build() {
                Ok(re) => {
                    // Forward search only
                    let after_chars: String = text.chars().skip(current_char_idx).collect();
                    re.find(&after_chars).map(|m| {
                        let char_start = current_char_idx + after_chars[..m.start()].chars().count();
                        let char_len = after_chars[m.start()..m.end()].chars().count();
                        (char_start, char_len)
                    })
                }
                Err(_) => {
                    // Invalid regex, return no match
                    return Ok(false);
                }
            }
        } else {
            // Plain string search (case-insensitive)
            let text_lower = text.to_lowercase();
            let pattern_lower = self.replace_pattern.to_lowercase();

            // Forward search only
            let after_chars: String = text_lower.chars().skip(current_char_idx).collect();
            after_chars.find(&pattern_lower).map(|byte_offset| {
                let char_idx = current_char_idx + after_chars[..byte_offset].chars().count();
                let match_len = self.replace_pattern.chars().count();
                (char_idx, match_len)
            })
        };

        if let Some((char_idx, match_len)) = found_match {
            let pos = buffer.text_buffer().char_to_pos(char_idx);

            // Move cursor to found position
            buffer.editor_state_mut().cursor.set_position(pos);
            buffer.editor_state_mut().ensure_cursor_visible();

            // Select the found text
            let end_char_idx = char_idx + match_len;
            let end_pos = buffer.text_buffer().char_to_pos(end_char_idx);
            buffer.editor_state_mut().selection = Some(crate::editor::state::Selection::new(pos, end_pos));

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Perform the replacement of currently selected text
    fn perform_replace(&mut self) -> Result<()> {
        let Some(buffer) = self.workspace.active_buffer_mut() else {
            return Ok(());
        };

        // Delete the selected text and insert replacement
        if let Some(selection) = buffer.editor_state().selection {
            let (start, end) = selection.range();
            let (text_buffer, editor_state, undo_manager) = buffer.split_mut();

            // Delete the matched text
            if let Ok(deleted) = text_buffer.delete_range(start, end) {
                undo_manager.record(Change::Delete {
                    pos: start,
                    text: deleted,
                });

                // Insert the replacement
                text_buffer.insert(start, &self.replace_with)?;
                undo_manager.record(Change::Insert {
                    pos: start,
                    text: self.replace_with.clone(),
                });

                // Move cursor to end of replacement
                editor_state.cursor.set_position(start);
                editor_state.ensure_cursor_visible();
            }
        }

        Ok(())
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
                    // Notify LSP about save
                    self.notify_lsp_did_save();
                } else {
                    self.message = Some("No file path set".to_string());
                }
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+C - Exit
                if self.workspace.has_modified_buffers() {
                    self.mode = AppMode::ConfirmExit;
                    self.message = Some("Save modified buffers? (y/n)".to_string());
                    return Ok(ControlFlow::Continue);
                }
                return Ok(ControlFlow::Exit);
            }
            // If not Ctrl+S or Ctrl+C, fall through to handle the key normally
        }

        match (key.code, key.modifiers) {
            // Ctrl+Q - Quit
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                if self.workspace.has_modified_buffers() {
                    self.mode = AppMode::ConfirmExit;
                    self.message = Some("Save modified buffers? (y/n)".to_string());
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

            // Ctrl+S - Search forward
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                self.mode = AppMode::Search;
                self.search_pattern.clear();
                self.search_start_pos = Some(buffer.editor_state().cursor.position());
                self.search_is_reverse = false;
                self.search_use_regex = true;  // Default to regex mode
                self.message = Some("Search [REGEX]:".to_string());
            }

            // Ctrl+R - Search reverse
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.mode = AppMode::Search;
                self.search_pattern.clear();
                self.search_start_pos = Some(buffer.editor_state().cursor.position());
                self.search_is_reverse = true;
                self.search_use_regex = true;  // Default to regex mode
                self.message = Some("Reverse search [REGEX]:".to_string());
            }

            // Alt+G - Jump to line
            (KeyCode::Char('g'), KeyModifiers::ALT) => {
                self.mode = AppMode::JumpToLine;
                self.jump_to_line_input.clear();
                self.message = Some("Go to line:".to_string());
            }

            // Ctrl+H - Replace (query-replace)
            (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
                self.mode = AppMode::ReplacePrompt;
                self.replace_pattern.clear();
                self.replace_with.clear();
                self.replace_count = 0;
                self.search_start_pos = Some(buffer.editor_state().cursor.position());
                self.search_use_regex = true;  // Default to regex mode
                self.message = Some("Replace [REGEX]:".to_string());
            }

            // Ctrl+J - Center view on cursor (Emacs style)
            (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                let current_line = buffer.editor_state().cursor.line;
                buffer.editor_state_mut().viewport.center_on_line(current_line);
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

            // Ctrl+Shift+A - Select to beginning of line
            (KeyCode::Char(c), mods) if (c == 'a' || c == 'A') &&
                mods.contains(KeyModifiers::CONTROL) &&
                mods.contains(KeyModifiers::SHIFT) => {
                let (_text_buffer, editor_state, _) = buffer.split_mut();
                editor_state.start_selection();
                Movement::move_to_line_start(editor_state);
                editor_state.update_selection();
            }

            // Ctrl+A - Beginning of line
            (KeyCode::Char(c), mods) if (c == 'a' || c == 'A') &&
                mods.contains(KeyModifiers::CONTROL) => {
                let (_text_buffer, editor_state, _) = buffer.split_mut();
                Movement::move_to_line_start(editor_state);
                editor_state.clear_selection();
            }

            // Ctrl+Shift+E - Select to end of line
            (KeyCode::Char(c), mods) if (c == 'e' || c == 'E') &&
                mods.contains(KeyModifiers::CONTROL) &&
                mods.contains(KeyModifiers::SHIFT) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                editor_state.start_selection();
                Movement::move_to_line_end(editor_state, text_buffer);
                editor_state.update_selection();
            }

            // Ctrl+E - End of line
            (KeyCode::Char(c), mods) if (c == 'e' || c == 'E') &&
                mods.contains(KeyModifiers::CONTROL) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                Movement::move_to_line_end(editor_state, text_buffer);
                editor_state.clear_selection();
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

            // Arrow keys with optional Shift (selection) and Ctrl (word/block movement)
            (KeyCode::Left, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }

                if mods.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Left or Ctrl+Shift+Left: move by word
                    Movement::move_word_left(editor_state, text_buffer);
                } else {
                    // Regular left movement
                    Movement::move_left(editor_state, text_buffer);
                }

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

                if mods.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Right or Ctrl+Shift+Right: move by word
                    Movement::move_word_right(editor_state, text_buffer);
                } else {
                    // Regular right movement
                    Movement::move_right(editor_state, text_buffer);
                }

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

                if mods.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Up: move by block
                    Movement::move_block_up(editor_state, text_buffer);
                } else {
                    // Regular up movement
                    Movement::move_up(editor_state, text_buffer);
                }

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

                if mods.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Down: move by block
                    Movement::move_block_down(editor_state, text_buffer);
                } else {
                    // Regular down movement
                    Movement::move_down(editor_state, text_buffer);
                }

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

            // F12 - Jump to definition
            (KeyCode::F(12), _) => {
                if let Some(lsp) = &mut self.lsp_manager {
                    if let Some(path) = buffer.file_path() {
                        if crate::lsp::Language::from_path(path).is_some() {
                            let pos = buffer.editor_state().cursor.position();
                            let buffer_id = buffer.id().0;
                            let lsp_pos = crate::lsp::Position::new(pos.line, pos.column);
                            if lsp.goto_definition(buffer_id, path.clone(), lsp_pos).is_ok() {
                                self.message = Some("Finding definition...".to_string());
                            }
                        }
                    }
                }
            }

            // Backspace
            (KeyCode::Backspace, _) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();

                // If there's a selection, delete it
                if let Some(selection) = editor_state.selection {
                    let (start, end) = selection.range();
                    if let Ok(deleted) = text_buffer.delete_range(start, end) {
                        undo_manager.record(Change::Delete {
                            pos: start,
                            text: deleted,
                        });
                        editor_state.cursor.set_position(start);
                        editor_state.clear_selection();
                        editor_state.ensure_cursor_visible();
                    }
                } else if editor_state.cursor.column > 0 {
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

                // If there's a selection, delete it
                if let Some(selection) = editor_state.selection {
                    let (start, end) = selection.range();
                    if let Ok(deleted) = text_buffer.delete_range(start, end) {
                        undo_manager.record(Change::Delete {
                            pos: start,
                            text: deleted,
                        });
                        editor_state.cursor.set_position(start);
                        editor_state.clear_selection();
                        editor_state.ensure_cursor_visible();
                    }
                } else {
                    let pos = editor_state.cursor.position();
                    if let Ok(Some(ch)) = text_buffer.delete_char(pos) {
                        undo_manager.record(Change::Delete {
                            pos,
                            text: ch.to_string(),
                        });
                    }
                }
            }

            // Enter
            (KeyCode::Enter, _) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();

                // If there's a selection, delete it first
                if let Some(selection) = editor_state.selection {
                    let (start, end) = selection.range();
                    if let Ok(deleted) = text_buffer.delete_range(start, end) {
                        undo_manager.record(Change::Delete {
                            pos: start,
                            text: deleted,
                        });
                        editor_state.cursor.set_position(start);
                        editor_state.clear_selection();
                    }
                }

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

                // If there's a selection, delete it first
                if let Some(selection) = editor_state.selection {
                    let (start, end) = selection.range();
                    if let Ok(deleted) = text_buffer.delete_range(start, end) {
                        undo_manager.record(Change::Delete {
                            pos: start,
                            text: deleted,
                        });
                        editor_state.cursor.set_position(start);
                        editor_state.clear_selection();
                    }
                }

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

        // Notify LSP about text changes (if buffer was modified)
        if let Some(buffer) = self.workspace.active_buffer() {
            if buffer.text_buffer().is_modified() {
                self.notify_lsp_did_change();
            }
        }

        Ok(ControlFlow::Continue)
    }

    /// Initialize the LSP manager
    pub fn initialize_lsp(&mut self) -> Result<()> {
        let (manager, receiver) = LspManager::new();
        self.lsp_manager = Some(manager);
        self.lsp_receiver = Some(receiver);

        // Notify LSP about any currently open file
        self.notify_lsp_did_open();

        Ok(())
    }

    /// Poll for LSP messages (non-blocking)
    pub fn poll_lsp_messages(&mut self) {
        // Collect responses first to avoid borrow checker issues
        let mut responses = Vec::new();
        if let Some(receiver) = &mut self.lsp_receiver {
            while let Ok(response) = receiver.try_recv() {
                responses.push(response);
            }
        }

        // Handle all collected responses
        for response in responses {
            self.handle_lsp_response(response);
        }
    }

    /// Handle an LSP response
    fn handle_lsp_response(&mut self, response: LspResponse) {
        match response {
            LspResponse::Diagnostics {
                buffer_id,
                diagnostics,
            } => {
                self.diagnostics_store.update(buffer_id, diagnostics);
            }
            LspResponse::GotoDefinition { location } => {
                // Open file and jump to position
                match self.workspace.open_file(location.path.clone()) {
                    Ok(_) => {
                        if let Some(buffer) = self.workspace.active_buffer_mut() {
                            let editor_state = buffer.editor_state_mut();
                            editor_state.cursor.set_position(crate::buffer::Position {
                                line: location.position.line,
                                column: location.position.column,
                            });
                            editor_state.ensure_cursor_visible();
                            self.message = Some(format!("Jumped to {}", location.path.display()));
                        }
                    }
                    Err(e) => {
                        self.message = Some(format!("Error: {}", e));
                    }
                }
            }
            LspResponse::Completion { items: _ } => {
                // TODO: Show completion popup
            }
            LspResponse::Error { message } => {
                self.message = Some(format!("LSP Error: {}", message));
            }
        }
    }

    /// Shutdown the LSP manager
    pub fn shutdown_lsp(&mut self) -> Result<()> {
        if let Some(manager) = &mut self.lsp_manager {
            manager.shutdown()?;
        }
        Ok(())
    }

    /// Notify LSP that the active buffer was opened
    fn notify_lsp_did_open(&mut self) {
        if let Some(lsp) = &mut self.lsp_manager {
            if let Some(buffer) = self.workspace.active_buffer() {
                if let Some(path) = buffer.file_path() {
                    if let Some(language) = crate::lsp::Language::from_path(path) {
                        let content = buffer.text_buffer().to_string();
                        let buffer_id = buffer.id().0; // Extract usize from BufferId
                        let _ = lsp.did_open(buffer_id, path.clone(), content, language);
                    }
                }
            }
        }
    }

    /// Notify LSP that the active buffer was changed
    fn notify_lsp_did_change(&mut self) {
        if let Some(lsp) = &mut self.lsp_manager {
            if let Some(buffer) = self.workspace.active_buffer() {
                if let Some(path) = buffer.file_path() {
                    if crate::lsp::Language::from_path(path).is_some() {
                        let content = buffer.text_buffer().to_string();
                        let buffer_id = buffer.id().0; // Extract usize from BufferId
                        let _ = lsp.did_change(buffer_id, path.clone(), content);
                    }
                }
            }
        }
    }

    /// Notify LSP that the active buffer was saved
    fn notify_lsp_did_save(&mut self) {
        if let Some(lsp) = &mut self.lsp_manager {
            if let Some(buffer) = self.workspace.active_buffer() {
                if let Some(path) = buffer.file_path() {
                    if crate::lsp::Language::from_path(path).is_some() {
                        let buffer_id = buffer.id().0; // Extract usize from BufferId
                        let _ = lsp.did_save(buffer_id, path.clone());
                    }
                }
            }
        }
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
