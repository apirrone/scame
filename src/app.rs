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
    CommandPanel,       // Command palette (Ctrl+Shift+P)
    ConfirmExit,
    Search,
    JumpToLine,
    ReplacePrompt,      // Prompting for search pattern
    ReplaceEnterRepl,   // Prompting for replacement string
    ReplaceConfirm,     // Confirming each replacement
    Completion,         // Showing completion suggestions
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub keybinding: Option<String>,
    pub action: CommandAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    Search,
    SearchAndReplace,
    JumpToLine,
    OpenFile,
    SaveFile,
    ToggleSplit,
    SwitchPane,
    NextBuffer,
    PreviousBuffer,
    CloseBuffer,
}

pub struct App {
    workspace: Workspace,
    layout: crate::workspace::LayoutManager,
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
    // Per-buffer highlight cache for split mode
    buffer_highlight_cache: std::collections::HashMap<crate::workspace::BufferId, (u64, Vec<HighlightSpan>)>,
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
    navigation_history: crate::lsp::NavigationHistory,
    // Completion state
    completion_items: Vec<crate::lsp::CompletionItem>,
    completion_selected: usize,
    completion_scroll_offset: usize,
    // Command panel state
    command_panel_pattern: String,
    command_panel_results: Vec<Command>,
    command_panel_selected: usize,
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

        let mut layout = crate::workspace::LayoutManager::new();
        // Initialize layout with the first buffer
        if let Some(buffer_id) = workspace.active_buffer_id() {
            layout.set_buffer(crate::workspace::PaneId::Left, buffer_id);
        }

        Ok(Self {
            workspace,
            layout,
            backup_manager: BackupManager::new(),
            file_tree: None,
            file_search: FileSearch::new(),
            highlighter: Highlighter::new(),
            mode: AppMode::Normal,
            logged_highlighting: false,
            cached_highlights: None,
            cached_text_hash: 0,
            buffer_highlight_cache: std::collections::HashMap::new(),
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
            navigation_history: crate::lsp::NavigationHistory::new(),
            completion_items: Vec::new(),
            completion_selected: 0,
            completion_scroll_offset: 0,
            command_panel_pattern: String::new(),
            command_panel_results: Vec::new(),
            command_panel_selected: 0,
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

            // Initialize layout
            let mut layout = crate::workspace::LayoutManager::new();
            if let Some(buffer_id) = workspace.active_buffer_id() {
                layout.set_buffer(crate::workspace::PaneId::Left, buffer_id);
            }

            return Ok(Self {
                workspace,
                layout,
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
                buffer_highlight_cache: std::collections::HashMap::new(),
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
                navigation_history: crate::lsp::NavigationHistory::new(),
                completion_items: Vec::new(),
                completion_selected: 0,
                completion_scroll_offset: 0,
                command_panel_pattern: String::new(),
                command_panel_results: Vec::new(),
                command_panel_selected: 0,
            });
        }

        // Initialize layout
        let mut layout = crate::workspace::LayoutManager::new();
        if let Some(buffer_id) = workspace.active_buffer_id() {
            layout.set_buffer(crate::workspace::PaneId::Left, buffer_id);
        }

        Ok(Self {
            workspace,
            layout,
            backup_manager: BackupManager::new(),
            file_tree: None,
            file_search: FileSearch::new(),
            highlighter: Highlighter::new(),
            mode: AppMode::Normal,
            logged_highlighting: false,
            cached_highlights: None,
            cached_text_hash: 0,
            buffer_highlight_cache: std::collections::HashMap::new(),
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
            navigation_history: crate::lsp::NavigationHistory::new(),
            completion_items: Vec::new(),
            completion_selected: 0,
            completion_scroll_offset: 0,
            command_panel_pattern: String::new(),
            command_panel_results: Vec::new(),
            command_panel_selected: 0,
        })
    }

    /// Build list of all available commands
    fn build_all_commands() -> Vec<Command> {
        vec![
            Command {
                name: "Search".to_string(),
                description: "Search for text in the current buffer".to_string(),
                keybinding: Some("Ctrl+S".to_string()),
                action: CommandAction::Search,
            },
            Command {
                name: "Search and Replace".to_string(),
                description: "Search and replace text in the current buffer".to_string(),
                keybinding: Some("Ctrl+H".to_string()),
                action: CommandAction::SearchAndReplace,
            },
            Command {
                name: "Jump to Line".to_string(),
                description: "Jump to a specific line number".to_string(),
                keybinding: Some("Ctrl+G".to_string()),
                action: CommandAction::JumpToLine,
            },
            Command {
                name: "Open File".to_string(),
                description: "Open a file in the workspace".to_string(),
                keybinding: Some("Ctrl+P".to_string()),
                action: CommandAction::OpenFile,
            },
            Command {
                name: "Save File".to_string(),
                description: "Save the current buffer to disk".to_string(),
                keybinding: Some("Ctrl+X Ctrl+S".to_string()),
                action: CommandAction::SaveFile,
            },
            Command {
                name: "Toggle Split".to_string(),
                description: "Toggle vertical split view".to_string(),
                keybinding: Some("Ctrl+X 3".to_string()),
                action: CommandAction::ToggleSplit,
            },
            Command {
                name: "Switch Pane".to_string(),
                description: "Switch focus between split panes".to_string(),
                keybinding: Some("Ctrl+X O".to_string()),
                action: CommandAction::SwitchPane,
            },
            Command {
                name: "Next Buffer".to_string(),
                description: "Switch to the next buffer".to_string(),
                keybinding: Some("Ctrl+PageDown".to_string()),
                action: CommandAction::NextBuffer,
            },
            Command {
                name: "Previous Buffer".to_string(),
                description: "Switch to the previous buffer".to_string(),
                keybinding: Some("Ctrl+PageUp".to_string()),
                action: CommandAction::PreviousBuffer,
            },
            Command {
                name: "Close Buffer".to_string(),
                description: "Close the current buffer".to_string(),
                keybinding: Some("Ctrl+W".to_string()),
                action: CommandAction::CloseBuffer,
            },
        ]
    }

    /// Filter commands by pattern (fuzzy search)
    fn filter_commands(pattern: &str) -> Vec<Command> {
        let all_commands = Self::build_all_commands();

        if pattern.is_empty() {
            return all_commands;
        }

        let pattern_lower = pattern.to_lowercase();
        let mut scored_commands: Vec<(Command, i32)> = all_commands
            .into_iter()
            .filter_map(|cmd| {
                let name_lower = cmd.name.to_lowercase();
                let desc_lower = cmd.description.to_lowercase();

                // Simple fuzzy matching: check if pattern chars appear in order
                let mut pattern_chars = pattern_lower.chars();
                let mut current_pattern_char = pattern_chars.next();
                let mut score = 0;
                let mut last_match_pos = 0;

                for (pos, c) in name_lower.chars().enumerate() {
                    if let Some(pc) = current_pattern_char {
                        if c == pc {
                            score += 100 - (pos - last_match_pos) as i32;
                            last_match_pos = pos;
                            current_pattern_char = pattern_chars.next();
                        }
                    }
                }

                // If all pattern chars matched, include this command
                if current_pattern_char.is_none() {
                    Some((cmd, score))
                } else if desc_lower.contains(&pattern_lower) {
                    // Fallback: substring match in description
                    Some((cmd, 10))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (highest first)
        scored_commands.sort_by(|a, b| b.1.cmp(&a.1));

        scored_commands.into_iter().map(|(cmd, _)| cmd).collect()
    }

    /// Render the application
    pub fn render(&mut self, terminal: &Terminal) -> Result<()> {
        // Hide cursor during rendering to prevent flickering
        terminal.hide_cursor()?;

        // Render tab bar at top
        let buffer_list = self.workspace.buffer_list();
        let active_buffer_id = self.layout.active_buffer().unwrap_or(crate::workspace::BufferId(0));
        crate::render::TabBar::render(terminal, &buffer_list, active_buffer_id)?;

        // Check if we're in split mode
        let (term_width, term_height) = terminal.size();

        if self.layout.mode() == crate::workspace::LayoutMode::VerticalSplit {
            // Render split panes
            let pane_dims = self.layout.pane_dimensions(term_width, term_height);

            // Compute highlights for left pane
            let left_highlights = if let Some(left_id) = self.layout.left_buffer() {
                self.get_cached_highlights(left_id)
            } else {
                None
            };

            // Compute highlights for right pane
            let right_highlights = if let Some(right_id) = self.layout.right_buffer() {
                self.get_cached_highlights(right_id)
            } else {
                None
            };

            // Render left pane
            if let Some(left_id) = self.layout.left_buffer() {
                if let Some(buffer) = self.workspace.get_buffer(left_id) {
                    self.render_buffer_in_pane(terminal, buffer, &pane_dims.left, left_id == active_buffer_id, left_highlights.as_deref())?;
                }
            }

            // Render right pane
            if let Some(ref right_rect) = pane_dims.right {
                if let Some(right_id) = self.layout.right_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer(right_id) {
                        self.render_buffer_in_pane(terminal, buffer, right_rect, right_id == active_buffer_id, right_highlights.as_deref())?;
                    }
                }
            }

            // Render status bar for split mode
            if let Some(active_id) = self.layout.active_buffer() {
                if let Some(buffer) = self.workspace.get_buffer(active_id) {
                    let buffer_diagnostics = self.diagnostics_store.get(buffer.id().0);
                    StatusBar::render(
                        terminal,
                        buffer.text_buffer(),
                        buffer.editor_state(),
                        self.message.as_deref(),
                        buffer_diagnostics,
                    )?;

                    // Position cursor in the active pane (simplified for now)
                    let pane_rect = if self.layout.active_pane() == crate::workspace::PaneId::Left {
                        &pane_dims.left
                    } else {
                        pane_dims.right.as_ref().unwrap_or(&pane_dims.left)
                    };

                    let editor_state = buffer.editor_state();
                    let screen_line = editor_state.cursor.line.saturating_sub(editor_state.viewport.top_line);
                    let line_num_width = if self.show_line_numbers {
                        let max_line = buffer.text_buffer().len_lines();
                        (if max_line == 0 { 1u16 } else { (max_line as f64).log10().floor() as u16 + 1 }) + 1
                    } else {
                        0
                    };
                    let screen_col = editor_state.cursor.column as u16 + line_num_width;
                    terminal.move_cursor(pane_rect.x + screen_col, pane_rect.y + screen_line as u16)?;
                }
            }
        } else {
            // Single pane mode - render just the active buffer
            let buffer_id = self.layout.active_buffer();
            if let Some(id) = buffer_id {
                let buffer = self.workspace.get_buffer(id);
                if let Some(buffer) = buffer {
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
            }
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

        // Render command panel overlay if active
        if self.mode == AppMode::CommandPanel {
            crate::render::CommandPanel::render(
                terminal,
                &self.command_panel_pattern,
                &self.command_panel_results,
                self.command_panel_selected,
            )?;
        }

        // Render completion popup if active
        if self.mode == AppMode::Completion {
            let buffer_id = self.layout.active_buffer();
            if let Some(id) = buffer_id {
                if let Some(buffer) = self.workspace.get_buffer(id) {
                // Calculate screen position of cursor
                let editor_state = buffer.editor_state();
                let viewport = &editor_state.viewport;
                let cursor = &editor_state.cursor;

                // Calculate cursor screen position
                let show_line_numbers = self.show_line_numbers;
                let gutter_width = if show_line_numbers {
                    // Line numbers + space + diagnostic marker
                    format!("{}", buffer.text_buffer().len_lines()).len() + 3
                } else {
                    0
                };

                let screen_x = (gutter_width + cursor.column).saturating_sub(viewport.left_column);
                let screen_y = cursor.line.saturating_sub(viewport.top_line) + 1; // +1 for tab bar offset

                crate::lsp::CompletionPopup::render(
                    terminal,
                    &self.completion_items,
                    self.completion_selected,
                    self.completion_scroll_offset,
                    (screen_x as u16, screen_y as u16),
                )?;
                }
            }
        }

        // Flush all buffered commands
        terminal.flush()?;

        // Show cursor only after everything is flushed
        terminal.show_cursor()?;
        terminal.flush()?;

        Ok(())
    }

    /// Get cached syntax highlights for a buffer (with caching to avoid recomputing every frame)
    fn get_cached_highlights(&mut self, buffer_id: crate::workspace::BufferId) -> Option<Vec<crate::syntax::HighlightSpan>> {
        let buffer = self.workspace.get_buffer(buffer_id)?;
        let path = buffer.file_path()?;
        let lang = crate::syntax::SupportedLanguage::from_path(path)?;

        let text = buffer.text_buffer().to_string();
        let text_hash = Self::simple_hash(&text);

        // Check if we have cached highlights for this buffer
        if let Some((cached_hash, cached_highlights)) = self.buffer_highlight_cache.get(&buffer_id) {
            if *cached_hash == text_hash {
                return Some(cached_highlights.clone());
            }
        }

        // Compute new highlights
        let file_id = path.to_string_lossy().to_string();
        let query = lang.query().ok()?;
        let capture_names = lang.capture_names().ok()?;

        self.highlighter.set_language(&lang.language()).ok()?;
        let highlights = self.highlighter.highlight(&text, &file_id, &query, &capture_names).ok()?;

        // Cache the results
        self.buffer_highlight_cache.insert(buffer_id, (text_hash, highlights.clone()));

        Some(highlights)
    }

    /// Render a buffer in a specific pane
    fn render_buffer_in_pane(
        &self,
        terminal: &Terminal,
        buffer: &crate::workspace::Buffer,
        pane_rect: &crate::workspace::PaneRect,
        is_active: bool,
        highlights: Option<&[crate::syntax::HighlightSpan]>,
    ) -> Result<()> {
        let text_buffer = buffer.text_buffer();
        let editor_state = buffer.editor_state();

        // Render each line in the pane
        for screen_row in 0..pane_rect.height {
            let buffer_line = editor_state.viewport.top_line + screen_row as usize;
            let screen_y = pane_rect.y + screen_row;

            terminal.move_cursor(pane_rect.x, screen_y)?;

            // Clear the line in this pane
            terminal.print(&" ".repeat(pane_rect.width as usize))?;
            terminal.move_cursor(pane_rect.x, screen_y)?;

            if buffer_line >= text_buffer.len_lines() {
                // Empty line beyond buffer
                if is_active {
                    terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                } else {
                    terminal.set_fg(crossterm::style::Color::Grey)?;
                }
                terminal.print("~")?;
                terminal.reset_color()?;
                continue;
            }

            // Get the line content
            if let Some(line) = text_buffer.get_line(buffer_line) {
                let line_num_width = if self.show_line_numbers {
                    let max_line = text_buffer.len_lines();
                    let digits = if max_line == 0 { 1 } else { (max_line as f64).log10().floor() as usize + 1 };
                    let line_num_str = format!("{:>width$} ", buffer_line + 1, width = digits);
                    terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                    terminal.print(&line_num_str)?;
                    terminal.reset_color()?;
                    digits + 1
                } else {
                    0
                };

                // Render the line with syntax highlighting if available
                let available_width = (pane_rect.width as usize).saturating_sub(line_num_width);
                let line_start_byte = text_buffer.line_to_byte(buffer_line);

                if let Some(highlight_spans) = highlights {
                    // Render with syntax highlighting
                    let chars: Vec<char> = line.chars().collect();
                    let mut current_col = 0;

                    for col_idx in editor_state.viewport.left_column..(editor_state.viewport.left_column + available_width) {
                        if col_idx >= chars.len() {
                            break;
                        }

                        let ch = chars[col_idx];
                        let byte_offset = line_start_byte + line.chars().take(col_idx).map(|c| c.len_utf8()).sum::<usize>();

                        // Find highlight color for this position
                        let color = highlight_spans
                            .iter()
                            .find(|span| byte_offset >= span.start_byte && byte_offset < span.end_byte)
                            .map(|span| self.highlighter.theme().color_for(span.token_type))
                            .unwrap_or(crossterm::style::Color::Reset);

                        if !is_active {
                            terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                        } else {
                            terminal.set_fg(color)?;
                        }
                        terminal.print(&ch.to_string())?;
                        current_col += 1;
                    }
                    terminal.reset_color()?;
                } else {
                    // Render without syntax highlighting
                    let chars: Vec<char> = line.chars().skip(editor_state.viewport.left_column).take(available_width).collect();
                    let line_str: String = chars.into_iter().collect();

                    if !is_active {
                        terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                    }
                    terminal.print(&line_str)?;
                    terminal.reset_color()?;
                }
            }
        }

        // Draw a vertical separator if this is the left pane
        if pane_rect.x == 0 && pane_rect.width < terminal.size().0 {
            let separator_x = pane_rect.x + pane_rect.width;
            for y in pane_rect.y..(pane_rect.y + pane_rect.height) {
                terminal.move_cursor(separator_x.saturating_sub(1), y)?;
                terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                terminal.print("│")?;
                terminal.reset_color()?;
            }
        }

        Ok(())
    }

    /// Handle terminal resize
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.workspace.resize(width, height.saturating_sub(1));

        // Recalculate split position if in split mode
        if self.layout.mode() == crate::workspace::LayoutMode::VerticalSplit {
            self.layout.recalculate_split(width);
        }
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
            AppMode::CommandPanel => self.handle_command_panel_mode(key),
            AppMode::ConfirmExit => self.handle_confirm_exit_mode(key),
            AppMode::Search => self.handle_search_mode(key),
            AppMode::JumpToLine => self.handle_jump_to_line_mode(key),
            AppMode::ReplacePrompt => self.handle_replace_prompt_mode(key),
            AppMode::ReplaceEnterRepl => self.handle_replace_enter_repl_mode(key),
            AppMode::ReplaceConfirm => self.handle_replace_confirm_mode(key),
            AppMode::Completion => self.handle_completion_mode(key),
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
                // Open selected file in the active pane
                if let Some(result) = self.file_picker_results.get(self.file_picker_selected) {
                    match self.workspace.open_file(result.path.clone()) {
                        Ok(buffer_id) => {
                            // Set the opened file to the active pane
                            let pane = self.layout.active_pane();
                            self.layout.set_buffer(pane, buffer_id);

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
                        Err(e) => {
                            self.message = Some(format!("Failed to open file: {}", e));
                        }
                    }
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

    /// Handle key in command panel mode
    fn handle_command_panel_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc => {
                // Cancel command panel
                self.mode = AppMode::Normal;
                self.command_panel_pattern.clear();
                self.command_panel_results.clear();
                self.message = None;
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel command panel (Emacs style)
                self.mode = AppMode::Normal;
                self.command_panel_pattern.clear();
                self.command_panel_results.clear();
                self.message = None;
            }
            KeyCode::Enter => {
                // Execute selected command
                if let Some(command) = self.command_panel_results.get(self.command_panel_selected) {
                    let action = command.action;
                    self.mode = AppMode::Normal;
                    self.command_panel_pattern.clear();
                    self.command_panel_results.clear();

                    // Execute the command
                    return self.execute_command(action);
                }
            }
            KeyCode::Up => {
                if self.command_panel_selected > 0 {
                    self.command_panel_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.command_panel_selected + 1 < self.command_panel_results.len() {
                    self.command_panel_selected += 1;
                }
            }
            KeyCode::Char(c) => {
                self.command_panel_pattern.push(c);
                self.command_panel_results = Self::filter_commands(&self.command_panel_pattern);
                self.command_panel_selected = 0;
            }
            KeyCode::Backspace => {
                self.command_panel_pattern.pop();
                self.command_panel_results = Self::filter_commands(&self.command_panel_pattern);
                self.command_panel_selected = 0;
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Execute a command from the command panel
    fn execute_command(&mut self, action: CommandAction) -> Result<ControlFlow> {
        match action {
            CommandAction::Search => {
                // Enter search mode
                let buffer = self.workspace.active_buffer();
                if let Some(buffer) = buffer {
                    self.mode = AppMode::Search;
                    self.search_pattern.clear();
                    self.search_start_pos = Some(buffer.editor_state().cursor.position());
                    self.search_is_reverse = false;
                    self.search_use_regex = true;
                    self.message = Some("Search [REGEX]:".to_string());
                }
            }
            CommandAction::SearchAndReplace => {
                // Enter replace mode
                self.mode = AppMode::ReplacePrompt;
                self.replace_pattern.clear();
                self.replace_with.clear();
                self.replace_count = 0;
                self.message = Some("Search pattern:".to_string());
            }
            CommandAction::JumpToLine => {
                // Enter jump to line mode
                self.mode = AppMode::JumpToLine;
                self.jump_to_line_input.clear();
                self.message = Some("Jump to line:".to_string());
            }
            CommandAction::OpenFile => {
                // Enter file picker mode
                if self.file_tree.is_some() {
                    self.mode = AppMode::FilePicker;
                    self.file_picker_pattern.clear();
                    self.file_picker_results.clear();
                    self.file_picker_selected = 0;
                } else {
                    self.message = Some("No project directory open".to_string());
                }
            }
            CommandAction::SaveFile => {
                // Save current buffer
                if let Some(buffer_id) = self.layout.active_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        if let Some(path) = buffer.file_path() {
                            self.backup_manager.create_backup(path)?;
                            buffer.text_buffer_mut().save()?;
                            self.message = Some("Saved".to_string());
                            self.notify_lsp_did_save();
                        } else {
                            self.message = Some("Buffer has no file path".to_string());
                        }
                    }
                } else {
                    self.message = Some("No active buffer".to_string());
                }
            }
            CommandAction::ToggleSplit => {
                // Toggle split
                let (term_width, _) = crossterm::terminal::size()?;
                self.layout.toggle_split(term_width);

                if self.layout.mode() == crate::workspace::LayoutMode::VerticalSplit {
                    // Opening split: assign current buffer to left, find another for right
                    if let Some(active_id) = self.workspace.active_buffer_id() {
                        self.layout.set_buffer(crate::workspace::PaneId::Left, active_id);

                        let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                        let other_id = buffer_ids.iter()
                            .find(|&&id| id != active_id)
                            .or_else(|| buffer_ids.first())
                            .copied();

                        if let Some(other_id) = other_id {
                            self.layout.set_buffer(crate::workspace::PaneId::Right, other_id);
                        }
                    }
                    self.message = Some("Split opened".to_string());
                } else {
                    self.message = Some("Split closed".to_string());
                }
            }
            CommandAction::SwitchPane => {
                // Switch pane
                self.layout.switch_pane();
                self.message = Some(format!("Switched to {:?} pane", self.layout.active_pane()));
            }
            CommandAction::NextBuffer => {
                // Switch to next buffer in active pane
                let current_id = self.layout.active_buffer();
                let buffer_ids: Vec<_> = self.workspace.buffer_ids();

                if let Some(current) = current_id {
                    let current_idx = buffer_ids.iter().position(|&id| id == current).unwrap_or(0);
                    let next_idx = (current_idx + 1) % buffer_ids.len();

                    if let Some(&next_id) = buffer_ids.get(next_idx) {
                        let pane = self.layout.active_pane();
                        self.layout.set_buffer(pane, next_id);
                        self.message = Some("Next buffer".to_string());
                    }
                }
            }
            CommandAction::PreviousBuffer => {
                // Switch to previous buffer in active pane
                let current_id = self.layout.active_buffer();
                let buffer_ids: Vec<_> = self.workspace.buffer_ids();

                if let Some(current) = current_id {
                    let current_idx = buffer_ids.iter().position(|&id| id == current).unwrap_or(0);
                    let prev_idx = if current_idx == 0 {
                        buffer_ids.len() - 1
                    } else {
                        current_idx - 1
                    };

                    if let Some(&prev_id) = buffer_ids.get(prev_idx) {
                        let pane = self.layout.active_pane();
                        self.layout.set_buffer(pane, prev_id);
                        self.message = Some("Previous buffer".to_string());
                    }
                }
            }
            CommandAction::CloseBuffer => {
                // Close current buffer
                if let Some(buffer_id) = self.layout.active_buffer() {
                    // Check if buffer is modified
                    if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                        if buffer.text_buffer().is_modified() {
                            self.message = Some("Buffer has unsaved changes (save first)".to_string());
                        } else {
                            // Close the buffer
                            if let Err(e) = self.workspace.close_buffer(buffer_id) {
                                self.message = Some(format!("Error closing buffer: {}", e));
                            } else {
                                // If there are remaining buffers, switch to another one
                                let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                                if let Some(&next_id) = buffer_ids.first() {
                                    let pane = self.layout.active_pane();
                                    self.layout.set_buffer(pane, next_id);
                                    self.message = Some("Buffer closed".to_string());
                                } else {
                                    self.message = Some("All buffers closed".to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(ControlFlow::Continue)
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
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+A: exit search mode and move to beginning of line
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
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+E: exit search mode and move to end of line
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
                // Reset to start position before searching to stay on current match
                if !self.search_pattern.is_empty() {
                    if let Some(start_pos) = self.search_start_pos {
                        self.workspace.active_buffer_mut().unwrap().editor_state_mut().cursor.set_position(start_pos);
                    }
                    let _ = self.perform_search(); // Ignore errors for incremental search
                }
            }
            KeyCode::Backspace => {
                self.search_pattern.pop();
                // Update message to show current pattern
                let search_type = if self.search_is_reverse { "Reverse search" } else { "Search" };
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("{}{}: {}", search_type, regex_indicator, self.search_pattern));
                // Reset to start position and re-search with shorter pattern
                if let Some(start_pos) = self.search_start_pos {
                    self.workspace.active_buffer_mut().unwrap().editor_state_mut().cursor.set_position(start_pos);
                }
                if !self.search_pattern.is_empty() {
                    let _ = self.perform_search();
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
                        // Forward regex search - start from current cursor position
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
                // Forward search - start from current cursor position
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
            let end_char_idx = char_idx + match_len;
            let end_pos = buffer.text_buffer().char_to_pos(end_char_idx);

            // Move cursor to END of match (not start) to avoid interfering with first character highlight
            buffer.editor_state_mut().cursor.set_position(end_pos);
            buffer.editor_state_mut().ensure_cursor_visible();

            // Select the found text (anchor at start, head at end)
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
            let end_char_idx = char_idx + match_len;
            let end_pos = buffer.text_buffer().char_to_pos(end_char_idx);

            // Move cursor to END of match (not start) to avoid interfering with first character highlight
            buffer.editor_state_mut().cursor.set_position(end_pos);
            buffer.editor_state_mut().ensure_cursor_visible();

            // Select the found text (anchor at start, head at end)
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

    /// Handle key in completion mode
    fn handle_completion_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        const MAX_VISIBLE: usize = 10;

        match key.code {
            KeyCode::Esc | KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Cancel completion
                self.mode = AppMode::Normal;
                self.completion_items.clear();
                self.completion_selected = 0;
                self.completion_scroll_offset = 0;
                self.message = None;
            }
            KeyCode::Up => {
                // Move selection up
                if self.completion_selected > 0 {
                    self.completion_selected -= 1;
                    // Scroll up if needed
                    if self.completion_selected < self.completion_scroll_offset {
                        self.completion_scroll_offset = self.completion_selected;
                    }
                }
            }
            KeyCode::Down => {
                // Move selection down
                if self.completion_selected < self.completion_items.len().saturating_sub(1) {
                    self.completion_selected += 1;
                    // Scroll down if needed
                    if self.completion_selected >= self.completion_scroll_offset + MAX_VISIBLE {
                        self.completion_scroll_offset = self.completion_selected - MAX_VISIBLE + 1;
                    }
                }
            }
            KeyCode::Enter => {
                // Insert selected completion
                if let Some(item) = self.completion_items.get(self.completion_selected) {
                    if let Some(buffer) = self.workspace.active_buffer_mut() {
                        let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                        let pos = editor_state.cursor.position();

                        // Find the start of the current word (go back while alphanumeric)
                        let line = text_buffer.get_line(pos.line).unwrap_or_default();
                        let mut word_start_col = pos.column;
                        let chars: Vec<char> = line.chars().collect();

                        while word_start_col > 0 {
                            let idx = word_start_col - 1;
                            if idx < chars.len() {
                                let ch = chars[idx];
                                if ch.is_alphanumeric() {
                                    word_start_col -= 1;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }

                        // Delete the partial word
                        if word_start_col < pos.column {
                            let delete_start = crate::buffer::Position::new(pos.line, word_start_col);
                            let delete_end = pos;
                            if let Ok(deleted) = text_buffer.delete_range(delete_start, delete_end) {
                                undo_manager.record(Change::Delete {
                                    pos: delete_start,
                                    text: deleted,
                                });
                                editor_state.cursor.set_position(delete_start);
                            }
                        }

                        // Insert the completion text (use insert_text if available, otherwise label)
                        let completion_text = item.insert_text.as_ref()
                            .unwrap_or(&item.label);
                        let insert_pos = editor_state.cursor.position();
                        text_buffer.insert(insert_pos, completion_text)?;
                        undo_manager.record(Change::Insert {
                            pos: insert_pos,
                            text: completion_text.to_string(),
                        });

                        // Move cursor to end of inserted text
                        editor_state.cursor.column += completion_text.chars().count();
                        editor_state.ensure_cursor_visible();
                    }

                    self.message = Some(format!("Inserted: {}", item.label));
                }

                // Exit completion mode
                self.mode = AppMode::Normal;
                self.completion_items.clear();
                self.completion_selected = 0;
                self.completion_scroll_offset = 0;
            }
            _ => {
                // Any other key cancels completion and processes normally
                self.mode = AppMode::Normal;
                self.completion_items.clear();
                self.message = None;
                return self.handle_normal_mode(key);
            }
        }

        Ok(ControlFlow::Continue)
    }

    /// Handle key in normal mode
    fn handle_normal_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        // Handle buffer switching first (before checking for active buffer)
        // because these commands don't need an active buffer to work
        match (key.code, key.modifiers) {
            // Ctrl+Tab - Next buffer (if terminal sends it properly)
            (KeyCode::Tab, mods) if mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::SHIFT) => {
                let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                if buffer_ids.is_empty() {
                    return Ok(ControlFlow::Continue);
                }

                let current_id = self.layout.active_buffer();
                if let Some(current) = current_id {
                    let current_idx = buffer_ids.iter().position(|&id| id == current).unwrap_or(0);
                    let next_idx = (current_idx + 1) % buffer_ids.len();
                    let next_id = buffer_ids[next_idx];

                    let pane = self.layout.active_pane();
                    self.layout.set_buffer(pane, next_id);

                    self.cached_highlights = None;
                    self.cached_text_hash = 0;
                    self.logged_highlighting = false;

                    if let Some(buf) = self.workspace.get_buffer(next_id) {
                        self.message = Some(format!("Switched to {}", buf.display_name()));
                    }
                } else if let Some(&first_id) = buffer_ids.first() {
                    let pane = self.layout.active_pane();
                    self.layout.set_buffer(pane, first_id);
                }
                return Ok(ControlFlow::Continue);
            }

            // Ctrl+PageDown - Next buffer (reliable fallback)
            (KeyCode::PageDown, KeyModifiers::CONTROL) => {
                let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                if buffer_ids.is_empty() {
                    return Ok(ControlFlow::Continue);
                }

                let current_id = self.layout.active_buffer();
                if let Some(current) = current_id {
                    let current_idx = buffer_ids.iter().position(|&id| id == current).unwrap_or(0);
                    let next_idx = (current_idx + 1) % buffer_ids.len();
                    let next_id = buffer_ids[next_idx];

                    // Update layout to point to new buffer
                    let pane = self.layout.active_pane();
                    self.layout.set_buffer(pane, next_id);

                    // Clear highlight cache when switching buffers
                    self.cached_highlights = None;
                    self.cached_text_hash = 0;
                    self.logged_highlighting = false;

                    // Get buffer name for message
                    if let Some(buf) = self.workspace.get_buffer(next_id) {
                        self.message = Some(format!("Switched to {}", buf.display_name()));
                    }
                } else if let Some(&first_id) = buffer_ids.first() {
                    let pane = self.layout.active_pane();
                    self.layout.set_buffer(pane, first_id);
                }
                return Ok(ControlFlow::Continue);
            }

            // Ctrl+PageUp or Ctrl+Shift+Tab - Previous buffer
            (KeyCode::PageUp, KeyModifiers::CONTROL) | (KeyCode::BackTab, _) => {
                let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                if buffer_ids.is_empty() {
                    return Ok(ControlFlow::Continue);
                }

                let current_id = self.layout.active_buffer();
                if let Some(current) = current_id {
                    let current_idx = buffer_ids.iter().position(|&id| id == current).unwrap_or(0);
                    let prev_idx = if current_idx == 0 { buffer_ids.len() - 1 } else { current_idx - 1 };
                    let prev_id = buffer_ids[prev_idx];

                    // Update layout to point to new buffer
                    let pane = self.layout.active_pane();
                    self.layout.set_buffer(pane, prev_id);

                    // Clear highlight cache when switching buffers
                    self.cached_highlights = None;
                    self.cached_text_hash = 0;
                    self.logged_highlighting = false;

                    // Get buffer name for message
                    if let Some(buf) = self.workspace.get_buffer(prev_id) {
                        self.message = Some(format!("Switched to {}", buf.display_name()));
                    }
                } else if let Some(&first_id) = buffer_ids.first() {
                    let pane = self.layout.active_pane();
                    self.layout.set_buffer(pane, first_id);
                }
                return Ok(ControlFlow::Continue);
            }
            _ => {}
        }

        // Get the active buffer from the layout manager (not workspace's internal state)
        let active_buffer_id = self.layout.active_buffer();
        let Some(buffer_id) = active_buffer_id else {
            return Ok(ControlFlow::Continue);
        };
        let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
            return Ok(ControlFlow::Continue);
        };

        // Handle Ctrl+X Ctrl+S (Emacs-style save) and Ctrl+X Ctrl+C (Emacs-style exit)
        if self.waiting_for_second_key {
            self.waiting_for_second_key = false;
            if matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+P - Command panel
                self.mode = AppMode::CommandPanel;
                self.command_panel_pattern.clear();
                self.command_panel_results = Self::filter_commands("");
                self.command_panel_selected = 0;
                self.message = Some("Command Palette (Ctrl+X Ctrl+P)".to_string());
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('s')) && key.modifiers.contains(KeyModifiers::CONTROL) {
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
            } else if matches!(key.code, KeyCode::Char('3')) && !key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X 3 - Toggle vertical split (emacs-style)
                let (term_width, _) = crossterm::terminal::size()?;
                self.layout.toggle_split(term_width);

                if self.layout.mode() == crate::workspace::LayoutMode::VerticalSplit {
                    // If opening split, assign current buffer to left pane
                    if let Some(active_id) = self.layout.active_buffer() {
                        self.layout.set_buffer(crate::workspace::PaneId::Left, active_id);

                        // Find another buffer for right pane, or use the same one
                        let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                        let other_id = buffer_ids.iter()
                            .find(|&&id| id != active_id)
                            .or_else(|| buffer_ids.first())
                            .copied();

                        if let Some(other_id) = other_id {
                            self.layout.set_buffer(crate::workspace::PaneId::Right, other_id);
                        }
                    }
                    self.message = Some("Split opened".to_string());
                } else {
                    self.message = Some("Split closed".to_string());
                }
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O')) && !key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X O - Switch pane (emacs-style)
                if self.layout.mode() == crate::workspace::LayoutMode::VerticalSplit {
                    self.layout.switch_pane();
                    self.message = Some(format!("Switched to {:?} pane", self.layout.active_pane()));
                } else {
                    self.message = Some("Not in split mode".to_string());
                }
                return Ok(ControlFlow::Continue);
            }
            // If not a recognized chord, fall through to handle the key normally
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

            // Ctrl+L - Center view on cursor (traditional Emacs binding)
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                let current_line = buffer.editor_state().cursor.line;
                buffer.editor_state_mut().viewport.center_on_line(current_line);
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
            (KeyCode::F(12), KeyModifiers::NONE) => {
                if let Some(lsp) = &mut self.lsp_manager {
                    if let Some(path) = buffer.file_path() {
                        if crate::lsp::Language::from_path(path).is_some() {
                            let pos = buffer.editor_state().cursor.position();
                            let buffer_id = buffer.id().0;

                            // Push current location to navigation history before jumping
                            let current_location = crate::lsp::Location {
                                path: path.clone(),
                                position: crate::lsp::Position::new(pos.line, pos.column),
                            };
                            self.navigation_history.push(current_location);

                            let lsp_pos = crate::lsp::Position::new(pos.line, pos.column);
                            if lsp.goto_definition(buffer_id, path.clone(), lsp_pos).is_ok() {
                                self.message = Some("Finding definition...".to_string());
                            }
                        }
                    }
                }
            }

            // Alt+F12 - Jump back to previous location
            (KeyCode::F(12), KeyModifiers::ALT) => {
                if let Some(location) = self.navigation_history.pop() {
                    // Open the file and jump to the position
                    match self.workspace.open_file(location.path.clone()) {
                        Ok(_) => {
                            if let Some(buffer) = self.workspace.active_buffer_mut() {
                                let editor_state = buffer.editor_state_mut();
                                editor_state.cursor.set_position(crate::buffer::Position {
                                    line: location.position.line,
                                    column: location.position.column,
                                });
                                editor_state.ensure_cursor_visible();
                                self.message = Some(format!(
                                    "Jumped back to {} ({}:{})",
                                    location.path.display(),
                                    location.position.line + 1,
                                    location.position.column + 1
                                ));
                            }
                        }
                        Err(e) => {
                            self.message = Some(format!("Error opening file: {}", e));
                        }
                    }
                } else {
                    self.message = Some("No more locations in navigation history".to_string());
                }
            }

            // Ctrl+Space - Trigger auto-completion
            (KeyCode::Char(' '), KeyModifiers::CONTROL) => {
                // Debug: Check if we have LSP manager
                if self.lsp_manager.is_none() {
                    self.message = Some("DEBUG: No LSP manager".to_string());
                    return Ok(ControlFlow::Continue);
                }

                // Debug: Check if we have a file path
                let path = buffer.file_path();
                if path.is_none() {
                    self.message = Some("DEBUG: No file path".to_string());
                    return Ok(ControlFlow::Continue);
                }
                let path = path.unwrap();

                // Debug: Check if language is detected
                let language = crate::lsp::Language::from_path(path);
                if language.is_none() {
                    self.message = Some(format!("DEBUG: No language detected for {:?}", path));
                    return Ok(ControlFlow::Continue);
                }

                // Send completion request
                if let Some(lsp) = &mut self.lsp_manager {
                    let pos = buffer.editor_state().cursor.position();
                    let buffer_id = buffer.id().0;
                    let lsp_pos = crate::lsp::Position::new(pos.line, pos.column);
                    match lsp.completion(buffer_id, path.clone(), lsp_pos) {
                        Ok(_) => {
                            self.message = Some(format!("Requesting completions at {}:{}...", pos.line, pos.column));
                        }
                        Err(e) => {
                            self.message = Some(format!("DEBUG: Completion error: {}", e));
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

            // Tab - Smart completion or indentation (but not Ctrl+Tab)
            (KeyCode::Tab, mods) if !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::SHIFT) => {
                // Determine if we should trigger completion or indent
                let should_complete = if let Some(path) = buffer.file_path() {
                    if crate::lsp::Language::from_path(path).is_some() {
                        // Check the character before the cursor
                        let pos = buffer.editor_state().cursor.position();
                        if let Some(line) = buffer.text_buffer().get_line(pos.line) {
                            let chars: Vec<char> = line.chars().collect();
                            if pos.column > 0 && pos.column <= chars.len() {
                                let prev_char = chars[pos.column - 1];
                                // Complete after dot or if we're in the middle of a word
                                prev_char == '.' || prev_char.is_alphanumeric()
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if should_complete && self.lsp_manager.is_some() {
                    // Trigger completion (same logic as Ctrl+Space)
                    if let Some(path) = buffer.file_path() {
                        if let Some(lsp) = &mut self.lsp_manager {
                            let pos = buffer.editor_state().cursor.position();
                            let buffer_id = buffer.id().0;
                            let lsp_pos = crate::lsp::Position::new(pos.line, pos.column);
                            match lsp.completion(buffer_id, path.clone(), lsp_pos) {
                                Ok(_) => {
                                    self.message = Some(format!("Requesting completions at {}:{}...", pos.line, pos.column));
                                }
                                Err(e) => {
                                    self.message = Some(format!("Completion error: {}", e));
                                }
                            }
                        }
                    }
                } else {
                    // Insert 4 spaces for indentation
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
                    // Insert 4 spaces
                    text_buffer.insert(pos, "    ")?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: "    ".to_string(),
                    });
                    // Move cursor 4 positions right
                    editor_state.cursor.column += 4;
                    editor_state.ensure_cursor_visible();
                }
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
            LspResponse::Completion { mut items } => {
                if items.is_empty() {
                    self.message = Some("No completions available".to_string());
                } else {
                    // Get the partial word the user has typed
                    let partial_word = if let Some(buffer) = self.workspace.active_buffer() {
                        let pos = buffer.editor_state().cursor.position();
                        if let Some(line) = buffer.text_buffer().get_line(pos.line) {
                            let chars: Vec<char> = line.chars().collect();
                            let mut word_start = pos.column;
                            while word_start > 0 && word_start - 1 < chars.len() {
                                let ch = chars[word_start - 1];
                                if ch.is_alphanumeric() {
                                    word_start -= 1;
                                } else {
                                    break;
                                }
                            }
                            if word_start < pos.column && word_start < chars.len() {
                                chars[word_start..pos.column].iter().collect::<String>().to_lowercase()
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };

                    // Filter and sort completions
                    // 1. Filter out dunder methods unless user typed "__"
                    let show_dunder = partial_word.starts_with("__");
                    if !show_dunder {
                        items.retain(|item| !item.label.starts_with("__"));
                    }

                    // 2. Sort by relevance
                    items.sort_by(|a, b| {
                        // Prioritize items that start with the partial word
                        let a_starts = a.label.to_lowercase().starts_with(&partial_word);
                        let b_starts = b.label.to_lowercase().starts_with(&partial_word);

                        if a_starts != b_starts {
                            return b_starts.cmp(&a_starts);
                        }

                        // Then prioritize methods/fields over other types
                        let a_is_member = matches!(a.kind, Some(crate::lsp::CompletionItemKind::Method) | Some(crate::lsp::CompletionItemKind::Field));
                        let b_is_member = matches!(b.kind, Some(crate::lsp::CompletionItemKind::Method) | Some(crate::lsp::CompletionItemKind::Field));

                        if a_is_member != b_is_member {
                            return b_is_member.cmp(&a_is_member);
                        }

                        // Finally sort alphabetically
                        a.label.cmp(&b.label)
                    });

                    self.completion_items = items;
                    self.completion_selected = 0;
                    self.completion_scroll_offset = 0;
                    self.mode = AppMode::Completion;
                    self.message = Some(format!(
                        "{} completions (↑↓ to navigate, Enter to select, Esc to cancel)",
                        self.completion_items.len()
                    ));
                }
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
