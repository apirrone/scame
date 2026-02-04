use crate::backup::BackupManager;
use crate::buffer::{Change, Position};
use crate::config::Config;
use crate::editor::movement::Movement;
use crate::logger;
use crate::ai::{AiManager, AiResponse};
use crate::lsp::{DiagnosticsStore, LspManager, LspResponse};
use crate::render::{BufferView, FilePicker, StatusBar, Terminal};
use crate::search::{FileSearch, FileSearchResult};
use crate::syntax::{HighlightSpan, Highlighter, SupportedLanguage};
use crate::workspace::{FileTree, Workspace};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};
use regex::RegexBuilder;
use std::path::PathBuf;
use std::time::{Duration, Instant};
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
    ProjectSearch,      // Project-wide search (Ctrl+X Ctrl+F)
    ConfirmExit,
    ConfirmCloseTab,    // Confirming close of modified buffer
    ConfirmSudoSave,    // Confirming sudo save operation
    Search,
    JumpToLine,
    ReplacePrompt,      // Prompting for search pattern
    ReplaceEnterRepl,   // Prompting for replacement string
    ReplaceConfirm,     // Confirming each replacement
    Completion,         // Showing completion suggestions
    SaveAsPrompt,       // Prompting for filename to save as
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub keybinding: Option<String>,
    pub action: CommandAction,
}

#[derive(Debug, Clone)]
pub struct ProjectSearchResult {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionMark {
    pub file_path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    Search,
    SearchAndReplace,
    ProjectSearch,
    JumpToLine,
    OpenFile,
    SaveFile,
    ToggleSplit,
    SwitchPane,
    NextBuffer,
    PreviousBuffer,
    CloseBuffer,
    FormatDocument,
    OrganizeImports,
    ToggleSyntaxHighlighting,
    ToggleSmartIndentation,
    ToggleDiagnostics,
    ToggleAiCompletions,
    ToggleIndentGuides,
    ToggleBackups,
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
    enable_syntax_highlighting: bool,
    smart_indentation: bool,
    show_diagnostics: bool,
    show_indent_guides: bool,
    clipboard: String,
    // Emacs-style key chord state
    waiting_for_second_key: bool,
    // File picker state
    file_picker_pattern: String,
    file_picker_results: Vec<FileSearchResult>,
    file_picker_selected: usize,
    file_picker_scroll_offset: usize,
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
    // Save as state
    save_as_filename: String,
    // LSP state
    lsp_manager: Option<LspManager>,
    lsp_receiver: Option<mpsc::UnboundedReceiver<LspResponse>>,
    diagnostics_store: DiagnosticsStore,
    navigation_history: crate::lsp::NavigationHistory,
    // AI completion state
    ai_manager: Option<AiManager>,
    ai_receiver: Option<mpsc::UnboundedReceiver<AiResponse>>,
    ai_suggestion: Option<String>,
    ai_last_keystroke: Option<Instant>,
    ai_pending_request: bool,
    ai_completions_enabled: bool,
    // Completion state
    completion_items: Vec<crate::lsp::CompletionItem>,
    completion_selected: usize,
    completion_scroll_offset: usize,
    // Command panel state
    command_panel_pattern: String,
    command_panel_results: Vec<Command>,
    command_panel_selected: usize,
    command_panel_scroll_offset: usize,
    // Project search state
    project_search_pattern: String,
    project_search_results: Vec<ProjectSearchResult>,
    project_search_selected: usize,
    project_search_scroll_offset: usize,
    // Position marks state
    position_marks: Vec<PositionMark>,
    current_mark_index: usize,
    // Pending close buffer (for ConfirmCloseTab mode)
    pending_close_buffer_id: Option<crate::workspace::BufferId>,
    // Sudo save state
    pending_sudo_save_path: Option<PathBuf>,
    pending_sudo_save_content: Option<String>,
    execute_sudo_save_on_render: bool,
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
        // Tab bar (1) + Path bar (1) + Status bar (1) = 3 lines to subtract
        let content_height = height.saturating_sub(3);
        let mut workspace = Workspace::new(width, content_height);

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
            enable_syntax_highlighting: true,
            smart_indentation: true,
            show_diagnostics: true,
            show_indent_guides: true,
            clipboard: String::new(),
            waiting_for_second_key: false,
            file_picker_pattern: String::new(),
            file_picker_results: Vec::new(),
            file_picker_selected: 0,
            file_picker_scroll_offset: 0,
            search_pattern: String::new(),
            search_start_pos: None,
            search_is_reverse: false,
            search_use_regex: false,
            jump_to_line_input: String::new(),
            replace_pattern: String::new(),
            replace_with: String::new(),
            replace_count: 0,
            save_as_filename: String::new(),
            lsp_manager: None,
            lsp_receiver: None,
            diagnostics_store: DiagnosticsStore::new(),
            navigation_history: crate::lsp::NavigationHistory::new(),
            ai_manager: None,
            ai_receiver: None,
            ai_suggestion: None,
            ai_last_keystroke: None,
            ai_pending_request: false,
            ai_completions_enabled: false,
            completion_items: Vec::new(),
            completion_selected: 0,
            completion_scroll_offset: 0,
            command_panel_pattern: String::new(),
            command_panel_results: Vec::new(),
            command_panel_selected: 0,
            command_panel_scroll_offset: 0,
            project_search_pattern: String::new(),
            project_search_results: Vec::new(),
            project_search_selected: 0,
            project_search_scroll_offset: 0,
            position_marks: Vec::new(),
            current_mark_index: 0,
            pending_close_buffer_id: None,
            pending_sudo_save_path: None,
            pending_sudo_save_content: None,
            execute_sudo_save_on_render: false,
        })
    }

    /// Create app from a file or directory
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let (width, height) = crossterm::terminal::size()?;
        // Tab bar (1) + Path bar (1) + Status bar (1) = 3 lines to subtract
        let content_height = height.saturating_sub(3);
        let mut workspace = Workspace::new(width, content_height);

        // Determine if it's a file or directory
        if path.is_file() {
            workspace.open_file(path)?; // Result ignored in initialization
        } else if path.is_dir() {
            // Open directory as project
            let mut file_tree = FileTree::new(path.clone());
            file_tree.scan()?;

            // Try to restore session state
            if let Ok(Some(session)) = crate::session::SessionState::load(&path) {
                // Restore open files
                for file_path in &session.open_files {
                    if let Err(e) = workspace.open_file(file_path.clone()) {
                        eprintln!("Failed to restore file {:?}: {}", file_path, e);
                    }
                }

                // Set active buffer if we have any
                let buffer_ids: Vec<_> = workspace.buffer_ids();
                if let Some(&active_id) = buffer_ids.get(session.active_buffer_index) {
                    workspace.set_active_buffer(active_id);
                }
            } else {
                // No session to restore, create empty buffer
                workspace.new_buffer();
            }

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
                enable_syntax_highlighting: true,
                smart_indentation: true,
                show_diagnostics: true,
                show_indent_guides: true,
                clipboard: String::new(),
                waiting_for_second_key: false,
                file_picker_pattern: String::new(),
                file_picker_results: Vec::new(),
                file_picker_selected: 0,
                file_picker_scroll_offset: 0,
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
                save_as_filename: String::new(),
                lsp_manager: None,
                lsp_receiver: None,
                diagnostics_store: DiagnosticsStore::new(),
                navigation_history: crate::lsp::NavigationHistory::new(),
                ai_manager: None,
                ai_receiver: None,
                ai_suggestion: None,
                ai_last_keystroke: None,
                ai_pending_request: false,
                ai_completions_enabled: false,
                completion_items: Vec::new(),
                completion_selected: 0,
                completion_scroll_offset: 0,
                command_panel_pattern: String::new(),
                command_panel_results: Vec::new(),
                command_panel_selected: 0,
                command_panel_scroll_offset: 0,
                project_search_pattern: String::new(),
                project_search_results: Vec::new(),
                project_search_selected: 0,
                project_search_scroll_offset: 0,
                position_marks: Vec::new(),
                current_mark_index: 0,
                pending_close_buffer_id: None,
                pending_sudo_save_path: None,
                pending_sudo_save_content: None,
                execute_sudo_save_on_render: false,
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
            enable_syntax_highlighting: true,
            smart_indentation: true,
            show_diagnostics: true,
            show_indent_guides: true,
            clipboard: String::new(),
            waiting_for_second_key: false,
            file_picker_pattern: String::new(),
            file_picker_results: Vec::new(),
            file_picker_selected: 0,
            file_picker_scroll_offset: 0,
            search_pattern: String::new(),
            search_start_pos: None,
            search_is_reverse: false,
            search_use_regex: false,
            jump_to_line_input: String::new(),
            replace_pattern: String::new(),
            replace_with: String::new(),
            replace_count: 0,
            save_as_filename: String::new(),
            lsp_manager: None,
            lsp_receiver: None,
            diagnostics_store: DiagnosticsStore::new(),
            navigation_history: crate::lsp::NavigationHistory::new(),
            ai_manager: None,
            ai_receiver: None,
            ai_suggestion: None,
            ai_last_keystroke: None,
            ai_pending_request: false,
            ai_completions_enabled: false,
            completion_items: Vec::new(),
            completion_selected: 0,
            completion_scroll_offset: 0,
            command_panel_pattern: String::new(),
            command_panel_results: Vec::new(),
            command_panel_selected: 0,
            command_panel_scroll_offset: 0,
            project_search_pattern: String::new(),
            project_search_results: Vec::new(),
            project_search_selected: 0,
            project_search_scroll_offset: 0,
            position_marks: Vec::new(),
            current_mark_index: 0,
            pending_close_buffer_id: None,
            pending_sudo_save_path: None,
            pending_sudo_save_content: None,
            execute_sudo_save_on_render: false,
        })
    }

    /// Build list of all available commands
    fn build_all_commands(&self) -> Vec<Command> {
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
                keybinding: Some("Ctrl+X Ctrl+H".to_string()),
                action: CommandAction::SearchAndReplace,
            },
            Command {
                name: "Search in Project".to_string(),
                description: "Search for text across all files in the project".to_string(),
                keybinding: Some("Ctrl+X Ctrl+F".to_string()),
                action: CommandAction::ProjectSearch,
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
            Command {
                name: "Format Document".to_string(),
                description: "Format Python file with black".to_string(),
                keybinding: None,
                action: CommandAction::FormatDocument,
            },
            Command {
                name: "Organize Imports".to_string(),
                description: "Organize Python imports with isort".to_string(),
                keybinding: None,
                action: CommandAction::OrganizeImports,
            },
            Command {
                name: format!("Toggle Syntax Highlighting [{}]",
                    if self.enable_syntax_highlighting { "ON" } else { "OFF" }),
                description: "Enable/disable syntax highlighting for better performance".to_string(),
                keybinding: None,
                action: CommandAction::ToggleSyntaxHighlighting,
            },
            Command {
                name: format!("Toggle Smart Indentation [{}]",
                    if self.smart_indentation { "ON" } else { "OFF" }),
                description: "Enable/disable smart Python indentation".to_string(),
                keybinding: Some("Ctrl+X I".to_string()),
                action: CommandAction::ToggleSmartIndentation,
            },
            Command {
                name: format!("Toggle Diagnostics [{}]",
                    if self.show_diagnostics { "ON" } else { "OFF" }),
                description: "Enable/disable diagnostic dots/markers".to_string(),
                keybinding: Some("Ctrl+X D".to_string()),
                action: CommandAction::ToggleDiagnostics,
            },
            Command {
                name: format!("Toggle AI Completions [{}]",
                    if self.ai_completions_enabled { "ON" } else { "OFF" }),
                description: "Enable/disable AI-powered code completions".to_string(),
                keybinding: None,
                action: CommandAction::ToggleAiCompletions,
            },
            Command {
                name: format!("Toggle Indent Guides [{}]",
                    if self.show_indent_guides { "ON" } else { "OFF" }),
                description: "Enable/disable indentation guide lines (Python)".to_string(),
                keybinding: None,
                action: CommandAction::ToggleIndentGuides,
            },
            Command {
                name: format!("Toggle Backups [{}]",
                    if self.backup_manager.is_enabled() { "ON" } else { "OFF" }),
                description: "Enable/disable automatic file backups (~ files)".to_string(),
                keybinding: None,
                action: CommandAction::ToggleBackups,
            },
        ]
    }

    /// Filter commands by pattern (fuzzy search)
    fn filter_commands(&self, pattern: &str) -> Vec<Command> {
        let all_commands = self.build_all_commands();

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

        // Render path bar (line 1)
        let (term_width, term_height) = terminal.size();
        self.render_path_bar(terminal, term_width)?;

        // Check if we're in split mode

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

                    // Only position cursor if it's within visible area
                    if screen_line < pane_rect.height as usize {
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
            }
        } else {
            // Single pane mode - render just the active buffer
            let buffer_id = self.layout.active_buffer();
            if let Some(id) = buffer_id {
                let buffer = self.workspace.get_buffer(id);
                if let Some(buffer) = buffer {
            // Get syntax highlighting if supported (with caching)
            let highlight_spans = if self.enable_syntax_highlighting {
                if let Some(path) = buffer.file_path() {
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
                }
            } else {
                None
            };

            // Get diagnostics for current buffer
            let buffer_diagnostics = self.diagnostics_store.get(buffer.id().0);

            // Get position marks for current buffer
            let position_marks_positions: Vec<(usize, usize, usize)> = if let Some(file_path) = buffer.file_path() {
                self.position_marks
                    .iter()
                    .filter(|mark| mark.file_path == *file_path)
                    .map(|mark| (mark.line, mark.column, 1)) // Single character
                    .collect()
            } else {
                Vec::new()
            };

            BufferView::render(
                terminal,
                buffer.text_buffer(),
                buffer.editor_state(),
                self.show_line_numbers,
                highlight_spans.as_deref(),
                self.highlighter.theme(),
                buffer_diagnostics,
                self.show_diagnostics,
                self.ai_suggestion.as_ref(),
                buffer.file_path().map(|p| p.as_path()),
                self.show_indent_guides,
                &position_marks_positions,
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
                self.file_picker_scroll_offset,
            )?;
        }

        // Render command panel overlay if active
        if self.mode == AppMode::CommandPanel {
            crate::render::CommandPanel::render(
                terminal,
                &self.command_panel_pattern,
                &self.command_panel_results,
                self.command_panel_selected,
                self.command_panel_scroll_offset,
            )?;
        }

        // Render project search overlay if active
        if self.mode == AppMode::ProjectSearch {
            crate::render::ProjectSearch::render(
                terminal,
                &self.project_search_pattern,
                &self.project_search_results,
                self.project_search_selected,
                self.project_search_scroll_offset,
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
                let screen_y = cursor.line.saturating_sub(viewport.top_line) + 2; // +2 for tab bar and path bar offset

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

    /// Render the path bar showing the current file path relative to project root
    fn render_path_bar(&self, terminal: &Terminal, term_width: u16) -> Result<()> {
        terminal.move_cursor(0, 1)?;
        terminal.set_bg(crossterm::style::Color::DarkGrey)?;
        terminal.set_fg(crossterm::style::Color::White)?;

        // Get the active buffer and its path
        let path_text = if let Some(buffer_id) = self.layout.active_buffer() {
            if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                if let Some(file_path) = buffer.file_path() {
                    // Try to get relative path from project root
                    if let Some(file_tree) = &self.file_tree {
                        let project_root = file_tree.root();
                        if let Ok(relative) = file_path.strip_prefix(project_root) {
                            format!(" {}", relative.display())
                        } else {
                            format!(" {}", file_path.display())
                        }
                    } else {
                        format!(" {}", file_path.display())
                    }
                } else {
                    " [No file]".to_string()
                }
            } else {
                " [No buffer]".to_string()
            }
        } else {
            " [No buffer]".to_string()
        };

        // Print the path, truncate if too long
        let max_width = term_width as usize;
        let display_text = if path_text.len() > max_width {
            format!("...{}", &path_text[path_text.len() - max_width + 3..])
        } else {
            path_text
        };

        terminal.print(&display_text)?;

        // Fill remaining space
        let remaining = max_width.saturating_sub(display_text.len());
        if remaining > 0 {
            terminal.print(&" ".repeat(remaining))?;
        }

        terminal.reset_color()?;
        Ok(())
    }

    /// Get cached syntax highlights for a buffer (with caching to avoid recomputing every frame)
    fn get_cached_highlights(&mut self, buffer_id: crate::workspace::BufferId) -> Option<Vec<crate::syntax::HighlightSpan>> {
        if !self.enable_syntax_highlighting {
            return None;
        }
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

        // Get position marks for current buffer
        let mark_positions: std::collections::HashSet<(usize, usize)> = if let Some(file_path) = buffer.file_path() {
            self.position_marks
                .iter()
                .filter(|mark| mark.file_path == *file_path)
                .map(|mark| (mark.line, mark.column))
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        // Mark colors
        let mark_bg = crossterm::style::Color::Rgb { r: 255, g: 100, b: 255 }; // Magenta
        let mark_fg = crossterm::style::Color::White;

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

                // Get selection range for this line
                let selection_range = if let Some(selection) = &editor_state.selection {
                    let (start, end) = selection.range();
                    if buffer_line >= start.line && buffer_line <= end.line {
                        let start_col = if buffer_line == start.line { start.column } else { 0 };
                        let end_col = if buffer_line == end.line { end.column } else { line.chars().count() };
                        Some((start_col, end_col))
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Selection colors
                let selection_bg = crossterm::style::Color::Rgb { r: 100, g: 180, b: 255 };
                let selection_fg = crossterm::style::Color::Black;

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

                        // Check if this position is marked (highest priority)
                        let is_marked = mark_positions.contains(&(buffer_line, col_idx));

                        // Check if this character is selected
                        let is_selected = selection_range
                            .map(|(start, end)| col_idx >= start && col_idx < end)
                            .unwrap_or(false);

                        if is_marked {
                            // Render with mark highlighting (highest priority)
                            terminal.set_bg(mark_bg)?;
                            terminal.set_fg(mark_fg)?;
                        } else if is_selected {
                            // Render with selection highlighting
                            terminal.set_bg(selection_bg)?;
                            terminal.set_fg(selection_fg)?;
                        } else {
                            // Find highlight color for this position
                            let color = highlight_spans
                                .iter()
                                .find(|span| byte_offset >= span.start_byte && byte_offset < span.end_byte)
                                .map(|span| self.highlighter.theme().color_for(span.token_type))
                                .unwrap_or(crossterm::style::Color::Reset);

                            // Use syntax highlighting color for both active and inactive panes
                            terminal.set_fg(color)?;
                        }
                        terminal.print(&ch.to_string())?;
                        terminal.reset_color()?;
                        current_col += 1;
                    }
                    terminal.reset_color()?;
                } else {
                    // Render without syntax highlighting but with selection
                    let chars: Vec<char> = line.chars().collect();

                    if let Some((start_col, end_col)) = selection_range {
                        // Render with selection highlighting
                        for (col_idx, &ch) in chars.iter().enumerate() {
                            if col_idx < editor_state.viewport.left_column {
                                continue;
                            }
                            if col_idx >= editor_state.viewport.left_column + available_width {
                                break;
                            }

                            // Check if this position is marked (highest priority)
                            let is_marked = mark_positions.contains(&(buffer_line, col_idx));

                            if is_marked {
                                terminal.set_bg(mark_bg)?;
                                terminal.set_fg(mark_fg)?;
                            } else if col_idx >= start_col && col_idx < end_col {
                                terminal.set_bg(selection_bg)?;
                                terminal.set_fg(selection_fg)?;
                            } else if !is_active {
                                terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                            }
                            terminal.print(&ch.to_string())?;
                            terminal.reset_color()?;
                        }
                    } else {
                        // No selection, render with mark highlighting
                        let chars: Vec<char> = line.chars().collect();

                        for (col_idx, &ch) in chars.iter().enumerate() {
                            if col_idx < editor_state.viewport.left_column {
                                continue;
                            }
                            if col_idx >= editor_state.viewport.left_column + available_width {
                                break;
                            }

                            // Check if this position is marked
                            let is_marked = mark_positions.contains(&(buffer_line, col_idx));

                            if is_marked {
                                terminal.set_bg(mark_bg)?;
                                terminal.set_fg(mark_fg)?;
                            } else if !is_active {
                                terminal.set_fg(crossterm::style::Color::DarkGrey)?;
                            }
                            terminal.print(&ch.to_string())?;
                            terminal.reset_color()?;
                        }
                    }
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
        // Tab bar (1) + Path bar (1) + Status bar (1) = 3 lines to subtract
        let content_height = height.saturating_sub(3);
        self.workspace.resize(width, content_height);

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
            Event::Mouse(mouse_event) => {
                self.handle_mouse(mouse_event)
            }
            Event::Paste(text) => {
                self.handle_paste(text)
            }
            _ => Ok(ControlFlow::Continue),
        }
    }

    /// Handle a mouse event
    fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Result<ControlFlow> {
        // Only handle middle-click in normal mode
        if self.mode != AppMode::Normal {
            return Ok(ControlFlow::Continue);
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Left-click to position cursor
                let Some(buffer_id) = self.layout.active_buffer() else {
                    return Ok(ControlFlow::Continue);
                };

                let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
                    return Ok(ControlFlow::Continue);
                };

                // Convert mouse coordinates to buffer position
                let mouse_col = mouse_event.column as usize;
                let mouse_row = mouse_event.row as usize;

                // Account for UI elements: tab bar (1 line), path bar (1 line)
                let top_offset = 2;
                if mouse_row < top_offset {
                    return Ok(ControlFlow::Continue);
                }
                let content_row = mouse_row - top_offset;

                // Calculate line number width
                let line_number_width = if self.show_line_numbers {
                    let line_count = buffer.text_buffer().len_lines();
                    let digits = if line_count == 0 {
                        1
                    } else {
                        (line_count as f64).log10().floor() as usize + 1
                    };
                    digits.max(3) + 2 // +1 for diagnostic marker, +1 for trailing space
                } else {
                    0
                };

                // Check if click is in the line number area
                if mouse_col < line_number_width {
                    return Ok(ControlFlow::Continue);
                }

                let content_col = mouse_col - line_number_width;

                // Calculate buffer line and column
                let buffer_line = buffer.editor_state().viewport.top_line + content_row;
                let buffer_col = content_col;

                // Make sure the line exists
                if buffer_line >= buffer.text_buffer().len_lines() {
                    return Ok(ControlFlow::Continue);
                }

                // Clamp column to line length
                let line_len = buffer.text_buffer().line_len(buffer_line);
                let clamped_col = buffer_col.min(line_len);

                // Position cursor at click location
                buffer.editor_state_mut().cursor.set_position(Position::new(buffer_line, clamped_col));
                buffer.editor_state_mut().clear_selection();

                Ok(ControlFlow::Continue)
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                // Middle-click paste (Linux X11 style)
                let Some(buffer_id) = self.layout.active_buffer() else {
                    return Ok(ControlFlow::Continue);
                };

                let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
                    return Ok(ControlFlow::Continue);
                };

                // Convert mouse coordinates to buffer position
                let mouse_col = mouse_event.column as usize;
                let mouse_row = mouse_event.row as usize;

                // Account for UI elements: tab bar (1 line), path bar (1 line)
                let top_offset = 2;
                if mouse_row < top_offset {
                    return Ok(ControlFlow::Continue);
                }
                let content_row = mouse_row - top_offset;

                // Calculate line number width
                let line_number_width = if self.show_line_numbers {
                    let line_count = buffer.text_buffer().len_lines();
                    let digits = if line_count == 0 {
                        1
                    } else {
                        (line_count as f64).log10().floor() as usize + 1
                    };
                    digits.max(3) + 2 // +1 for diagnostic marker, +1 for trailing space
                } else {
                    0
                };

                // Check if click is in the line number area
                if mouse_col < line_number_width {
                    return Ok(ControlFlow::Continue);
                }

                let content_col = mouse_col - line_number_width;

                // Calculate buffer line and column
                let buffer_line = buffer.editor_state().viewport.top_line + content_row;
                let buffer_col = content_col;

                // Make sure the line exists
                if buffer_line >= buffer.text_buffer().len_lines() {
                    return Ok(ControlFlow::Continue);
                }

                // Clamp column to line length
                let line_len = buffer.text_buffer().line_len(buffer_line);
                let clamped_col = buffer_col.min(line_len);

                // Get text from primary selection (X11 selection, what's currently highlighted)
                // On Linux, this gets whatever is selected anywhere in the OS
                let clipboard_text = {
                    #[cfg(target_os = "linux")]
                    {
                        // Try to get primary selection first (X11 selection buffer)
                        use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind};
                        match Clipboard::new() {
                            Ok(mut clipboard) => {
                                clipboard
                                    .get()
                                    .clipboard(LinuxClipboardKind::Primary)
                                    .text()
                                    .ok()
                            }
                            Err(_) => None
                        }
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        // On non-Linux systems, fall back to regular clipboard
                        match arboard::Clipboard::new() {
                            Ok(mut clipboard) => clipboard.get_text().ok(),
                            Err(_) => None
                        }
                    }
                };

                if let Some(text) = clipboard_text {
                    let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                    let pos = Position::new(buffer_line, clamped_col);
                    text_buffer.insert(pos, &text)?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: text.clone(),
                    });

                    // Move cursor to end of pasted text
                    let char_idx = text_buffer.pos_to_char(pos)? + text.len();
                    editor_state.cursor.set_position(text_buffer.char_to_pos(char_idx));
                    editor_state.ensure_cursor_visible();
                    self.message = Some("Pasted".to_string());
                }

                Ok(ControlFlow::Continue)
            }
            MouseEventKind::ScrollUp => {
                // Scroll up with mouse wheel
                let Some(buffer_id) = self.layout.active_buffer() else {
                    return Ok(ControlFlow::Continue);
                };

                let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
                    return Ok(ControlFlow::Continue);
                };

                // Scroll up by 3 lines
                let editor_state = buffer.editor_state_mut();
                if editor_state.viewport.top_line >= 3 {
                    editor_state.viewport.top_line -= 3;
                } else {
                    editor_state.viewport.top_line = 0;
                }

                Ok(ControlFlow::Continue)
            }
            MouseEventKind::ScrollDown => {
                // Scroll down with mouse wheel
                let Some(buffer_id) = self.layout.active_buffer() else {
                    return Ok(ControlFlow::Continue);
                };

                let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
                    return Ok(ControlFlow::Continue);
                };

                // Get max lines before borrowing editor state mutably
                let max_lines = buffer.text_buffer().len_lines();
                let max_top_line = max_lines.saturating_sub(1);

                // Scroll down by 3 lines
                let editor_state = buffer.editor_state_mut();
                editor_state.viewport.top_line = (editor_state.viewport.top_line + 3).min(max_top_line);

                Ok(ControlFlow::Continue)
            }
            _ => Ok(ControlFlow::Continue),
        }
    }

    /// Handle paste event (bracketed paste)
    fn handle_paste(&mut self, text: String) -> Result<ControlFlow> {
        // Only handle paste in normal mode
        if self.mode != AppMode::Normal {
            return Ok(ControlFlow::Continue);
        }

        let Some(buffer_id) = self.layout.active_buffer() else {
            return Ok(ControlFlow::Continue);
        };

        let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
            return Ok(ControlFlow::Continue);
        };

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
        text_buffer.insert(pos, &text)?;
        undo_manager.record(Change::Insert {
            pos,
            text: text.clone(),
        });

        // Move cursor to end of pasted text
        let char_idx = text_buffer.pos_to_char(pos)? + text.len();
        editor_state.cursor.set_position(text_buffer.char_to_pos(char_idx));
        editor_state.ensure_cursor_visible();

        // Invalidate syntax highlighting caches to force re-parse
        self.cached_highlights = None;
        self.cached_text_hash = 0;
        self.buffer_highlight_cache.clear();

        // Completely recreate the highlighter to ensure no stale state
        self.highlighter = crate::syntax::Highlighter::new();

        // Trigger AI completion debouncing (only if enabled)
        if self.ai_completions_enabled {
            // Clear any existing suggestion and reset timer
            self.ai_suggestion = None;
            self.ai_last_keystroke = Some(std::time::Instant::now());
            // Cancel any pending AI request
            if self.ai_pending_request {
                if let Some(manager) = &self.ai_manager {
                    let _ = manager.cancel_pending();
                }
                self.ai_pending_request = false;
            }
        }

        // Notify LSP about the change
        self.notify_lsp_did_change();

        Ok(ControlFlow::Continue)
    }

    /// Copy current selection to primary selection (X11)
    fn copy_selection_to_primary(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if let Some(buffer_id) = self.layout.active_buffer() {
                if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                    if let Some(selection) = buffer.editor_state().selection {
                        let (start, end) = selection.range();
                        if let Ok(start_idx) = buffer.text_buffer().pos_to_char(start) {
                            if let Ok(end_idx) = buffer.text_buffer().pos_to_char(end) {
                                if start_idx < end_idx {
                                    let text = buffer.text_buffer().to_string();
                                    let selected: String = text.chars().skip(start_idx).take(end_idx - start_idx).collect();

                                    // Copy to primary selection
                                    use arboard::{Clipboard, SetExtLinux, LinuxClipboardKind};
                                    if let Ok(mut clipboard) = Clipboard::new() {
                                        let _ = clipboard
                                            .set()
                                            .clipboard(LinuxClipboardKind::Primary)
                                            .text(selected);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Handle a key press
    fn handle_key(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match self.mode {
            AppMode::Normal => self.handle_normal_mode(key),
            AppMode::FilePicker => self.handle_file_picker_mode(key),
            AppMode::CommandPanel => self.handle_command_panel_mode(key),
            AppMode::ProjectSearch => self.handle_project_search_mode(key),
            AppMode::ConfirmExit => self.handle_confirm_exit_mode(key),
            AppMode::ConfirmCloseTab => self.handle_confirm_close_tab_mode(key),
            AppMode::ConfirmSudoSave => self.handle_confirm_sudo_save_mode(key),
            AppMode::Search => self.handle_search_mode(key),
            AppMode::JumpToLine => self.handle_jump_to_line_mode(key),
            AppMode::ReplacePrompt => self.handle_replace_prompt_mode(key),
            AppMode::ReplaceEnterRepl => self.handle_replace_enter_repl_mode(key),
            AppMode::ReplaceConfirm => self.handle_replace_confirm_mode(key),
            AppMode::Completion => self.handle_completion_mode(key),
            AppMode::SaveAsPrompt => self.handle_save_as_prompt_mode(key),
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
                        Ok(open_result) => {
                            let buffer_id = open_result.buffer_id();
                            // Set the opened file to the active pane
                            let pane = self.layout.active_pane();
                            self.layout.set_buffer(pane, buffer_id);

                            self.mode = AppMode::Normal;
                            self.file_picker_pattern.clear();
                            self.file_picker_results.clear();

                            // Show message based on whether it's new or existing
                            if open_result.is_new() {
                                // Invalidate highlight cache when opening a new file
                                self.cached_highlights = None;
                                self.cached_text_hash = 0;
                                self.logged_highlighting = false;
                                // Notify LSP about newly opened file
                                self.notify_lsp_did_open();
                            } else {
                                self.message = Some("Switched to existing buffer".to_string());
                            }
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
                    // Scroll up if selector moved above visible area
                    if self.file_picker_selected < self.file_picker_scroll_offset {
                        self.file_picker_scroll_offset = self.file_picker_selected;
                    }
                }
            }
            KeyCode::Down => {
                if self.file_picker_selected + 1 < self.file_picker_results.len() {
                    self.file_picker_selected += 1;
                    // Scroll down if selector moved below visible area (13 visible items)
                    let visible_count = 13;
                    if self.file_picker_selected >= self.file_picker_scroll_offset + visible_count {
                        self.file_picker_scroll_offset = self.file_picker_selected - visible_count + 1;
                    }
                }
            }
            KeyCode::Char(c) => {
                self.file_picker_pattern.push(c);
                self.update_file_picker_results();
            }
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
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
            self.file_picker_scroll_offset = 0;
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
                    // Scroll up if selector moved above visible area
                    if self.command_panel_selected < self.command_panel_scroll_offset {
                        self.command_panel_scroll_offset = self.command_panel_selected;
                    }
                }
            }
            KeyCode::Down => {
                if self.command_panel_selected + 1 < self.command_panel_results.len() {
                    self.command_panel_selected += 1;
                    // Scroll down if selector moved below visible area (18 visible items)
                    let visible_count = 18;
                    if self.command_panel_selected >= self.command_panel_scroll_offset + visible_count {
                        self.command_panel_scroll_offset = self.command_panel_selected - visible_count + 1;
                    }
                }
            }
            KeyCode::Char(c) => {
                self.command_panel_pattern.push(c);
                self.command_panel_results = self.filter_commands(&self.command_panel_pattern);
                self.command_panel_selected = 0;
                self.command_panel_scroll_offset = 0;
            }
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_panel_pattern.pop();
                self.command_panel_results = self.filter_commands(&self.command_panel_pattern);
                self.command_panel_selected = 0;
                self.command_panel_scroll_offset = 0;
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in project search mode
    fn handle_project_search_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc => {
                // Cancel project search
                self.mode = AppMode::Normal;
                self.project_search_pattern.clear();
                self.project_search_results.clear();
                self.message = None;
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel project search (Emacs style)
                self.mode = AppMode::Normal;
                self.project_search_pattern.clear();
                self.project_search_results.clear();
                self.message = None;
            }
            KeyCode::Enter => {
                // Jump to selected result
                if let Some(result) = self.project_search_results.get(self.project_search_selected) {
                    let file_path = result.file_path.clone();
                    let line_number = result.line_number;

                    // Open the file
                    match self.workspace.open_file(file_path.clone()) {
                        Ok(open_result) => {
                            let buffer_id = open_result.buffer_id();
                            // Set buffer to active pane
                            let pane = self.layout.active_pane();
                            self.layout.set_buffer(pane, buffer_id);

                            // Jump to line
                            if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                                buffer.editor_state_mut().cursor.line = line_number;
                                buffer.editor_state_mut().cursor.column = 0;
                                // Center the view on the search result line
                                buffer.editor_state_mut().viewport.center_on_line(line_number);
                            }

                            self.mode = AppMode::Normal;
                            self.project_search_pattern.clear();
                            self.project_search_results.clear();
                            self.message = Some(format!("Jumped to {}:{}", file_path.display(), line_number + 1));
                        }
                        Err(e) => {
                            self.message = Some(format!("Failed to open file: {}", e));
                        }
                    }
                }
            }
            KeyCode::Up => {
                if self.project_search_selected > 0 {
                    self.project_search_selected -= 1;
                    // Scroll up if selector moved above visible area
                    if self.project_search_selected < self.project_search_scroll_offset {
                        self.project_search_scroll_offset = self.project_search_selected;
                    }
                }
            }
            KeyCode::Down => {
                if self.project_search_selected + 1 < self.project_search_results.len() {
                    self.project_search_selected += 1;
                    // Scroll down if selector moved below visible area (calculate visible count dynamically)
                    let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 24));
                    let visible_lines = ((term_height as f32 * 0.7) as usize).saturating_sub(2);
                    if self.project_search_selected >= self.project_search_scroll_offset + visible_lines {
                        self.project_search_scroll_offset = self.project_search_selected - visible_lines + 1;
                    }
                }
            }
            KeyCode::Char(c) => {
                self.project_search_pattern.push(c);
                self.update_project_search_results();
            }
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.project_search_pattern.pop();
                self.update_project_search_results();
            }
            _ => {}
        }
        Ok(ControlFlow::Continue)
    }

    fn update_project_search_results(&mut self) {
        if self.project_search_pattern.is_empty() {
            self.project_search_results.clear();
            self.project_search_selected = 0;
            return;
        }

        // Build regex pattern (case insensitive)
        let pattern = match RegexBuilder::new(&self.project_search_pattern)
            .case_insensitive(true)
            .build()
        {
            Ok(p) => p,
            Err(_) => {
                self.project_search_results.clear();
                self.project_search_selected = 0;
                return;
            }
        };

        let mut results = Vec::new();

        // Search through all files in the project
        if let Some(file_tree) = &self.file_tree {
            for file_path in file_tree.files() {
                // Skip binary and large files
                if let Ok(metadata) = std::fs::metadata(file_path) {
                    // Skip files larger than 10MB to avoid slowdown
                    if metadata.len() > 10_000_000 {
                        continue;
                    }
                }

                // Skip common binary file extensions
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    match ext {
                        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "svg" |
                        "pdf" | "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" |
                        "exe" | "dll" | "so" | "dylib" | "a" | "o" |
                        "woff" | "woff2" | "ttf" | "eot" |
                        "mp3" | "mp4" | "avi" | "mov" | "wav" |
                        "pyc" | "pyo" | "class" | "jar" => continue,
                        _ => {}
                    }
                }

                // Read file contents
                if let Ok(contents) = std::fs::read_to_string(file_path) {
                    // Search each line
                    for (line_idx, line) in contents.lines().enumerate() {
                        if let Some(mat) = pattern.find(line) {
                            results.push(ProjectSearchResult {
                                file_path: file_path.clone(),
                                line_number: line_idx,
                                line_content: line.to_string(),
                                match_start: mat.start(),
                                match_end: mat.end(),
                            });

                            // Limit results to prevent UI slowdown
                            if results.len() >= 1000 {
                                break;
                            }
                        }
                    }
                }

                // Limit results
                if results.len() >= 1000 {
                    break;
                }
            }
        }

        self.project_search_results = results;
        self.project_search_selected = 0;
        self.project_search_scroll_offset = 0;
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
            CommandAction::ProjectSearch => {
                // Enter project search mode
                if self.file_tree.is_some() {
                    self.mode = AppMode::ProjectSearch;
                    self.project_search_pattern.clear();
                    self.project_search_results.clear();
                    self.project_search_selected = 0;
                    self.project_search_scroll_offset = 0;
                    self.message = Some("Project Search".to_string());
                } else {
                    self.message = Some("No project directory open".to_string());
                }
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
                    self.file_picker_scroll_offset = 0;
                } else {
                    self.message = Some("No project directory open".to_string());
                }
            }
            CommandAction::SaveFile => {
                // Save current buffer
                if let Some(buffer_id) = self.layout.active_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        if let Some(path) = buffer.file_path().cloned() {
                            // Try to create backup (ignore errors - backup is optional)
                            let _ = self.backup_manager.create_backup(&path);

                            // Store content before attempting save (for sudo retry)
                            let content = buffer.text_buffer().to_string();

                            match buffer.text_buffer_mut().save() {
                                Ok(_) => {
                                    self.message = Some("Saved".to_string());
                                    self.notify_lsp_did_save();
                                }
                                Err(e) => {
                                    if self.is_permission_denied(&e) {
                                        self.pending_sudo_save_path = Some(path);
                                        self.pending_sudo_save_content = Some(content);
                                        self.mode = AppMode::ConfirmSudoSave;
                                        self.message = Some(
                                            "Permission denied. Save with sudo? (y/n)".to_string()
                                        );
                                    } else {
                                        self.message = Some(format!("Save failed: {}", e));
                                    }
                                }
                            }
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
                    // Check if buffer is actually modified (compares with file on disk)
                    if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                        if buffer.is_actually_modified() {
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
            CommandAction::FormatDocument => {
                // Format Python file with black
                if let Some(buffer_id) = self.layout.active_buffer() {
                    // Get file path first to avoid borrowing issues
                    let path_opt = self.workspace.get_buffer(buffer_id)
                        .and_then(|b| b.file_path().cloned());

                    if let Some(path) = path_opt {
                        // Check if it's a Python file
                        if path.extension().and_then(|s| s.to_str()) == Some("py") {
                            // Save file first if modified
                            if let Some(buffer_mut) = self.workspace.get_buffer_mut(buffer_id) {
                                if buffer_mut.text_buffer().is_modified() {
                                    let _ = self.backup_manager.create_backup(&path);

                                    if let Err(e) = buffer_mut.text_buffer_mut().save() {
                                        self.message = Some(format!("Failed to save: {}", e));
                                        return Ok(ControlFlow::Continue);
                                    }
                                }
                            }

                            // Run black
                            let output = std::process::Command::new("black")
                                .arg(&path)
                                .output();

                            match output {
                                Ok(result) if result.status.success() => {
                                    // Reload the file
                                    if let Some(buffer_mut) = self.workspace.get_buffer_mut(buffer_id) {
                                        let reloaded = crate::buffer::TextBuffer::from_file(path)?;
                                        *buffer_mut.text_buffer_mut() = reloaded;
                                        self.message = Some("Formatted with black".to_string());
                                    }
                                }
                                Ok(result) => {
                                    let stderr = String::from_utf8_lossy(&result.stderr);
                                    self.message = Some(format!("black error: {}", stderr));
                                }
                                Err(e) => {
                                    self.message = Some(format!("Failed to run black: {}", e));
                                }
                            }
                        } else {
                            self.message = Some("Not a Python file".to_string());
                        }
                    } else {
                        self.message = Some("Buffer has no file path".to_string());
                    }
                }
            }
            CommandAction::OrganizeImports => {
                // Organize imports with isort
                if let Some(buffer_id) = self.layout.active_buffer() {
                    // Get file path first to avoid borrowing issues
                    let path_opt = self.workspace.get_buffer(buffer_id)
                        .and_then(|b| b.file_path().cloned());

                    if let Some(path) = path_opt {
                        // Check if it's a Python file
                        if path.extension().and_then(|s| s.to_str()) == Some("py") {
                            // Save file first if modified
                            if let Some(buffer_mut) = self.workspace.get_buffer_mut(buffer_id) {
                                if buffer_mut.text_buffer().is_modified() {
                                    let _ = self.backup_manager.create_backup(&path);

                                    if let Err(e) = buffer_mut.text_buffer_mut().save() {
                                        self.message = Some(format!("Failed to save: {}", e));
                                        return Ok(ControlFlow::Continue);
                                    }
                                }
                            }

                            // Run isort
                            let output = std::process::Command::new("isort")
                                .arg(&path)
                                .output();

                            match output {
                                Ok(result) if result.status.success() => {
                                    // Reload the file
                                    if let Some(buffer_mut) = self.workspace.get_buffer_mut(buffer_id) {
                                        let reloaded = crate::buffer::TextBuffer::from_file(path)?;
                                        *buffer_mut.text_buffer_mut() = reloaded;
                                        self.message = Some("Organized imports with isort".to_string());
                                    }
                                }
                                Ok(result) => {
                                    let stderr = String::from_utf8_lossy(&result.stderr);
                                    self.message = Some(format!("isort error: {}", stderr));
                                }
                                Err(e) => {
                                    self.message = Some(format!("Failed to run isort: {}", e));
                                }
                            }
                        } else {
                            self.message = Some("Not a Python file".to_string());
                        }
                    } else {
                        self.message = Some("Buffer has no file path".to_string());
                    }
                }
            }
            CommandAction::ToggleSyntaxHighlighting => {
                self.enable_syntax_highlighting = !self.enable_syntax_highlighting;
                let status = if self.enable_syntax_highlighting { "enabled" } else { "disabled" };
                self.message = Some(format!("Syntax highlighting {}", status));
            }
            CommandAction::ToggleSmartIndentation => {
                self.smart_indentation = !self.smart_indentation;
                self.message = Some(format!("Smart indentation: {}", if self.smart_indentation { "ON" } else { "OFF" }));
            }
            CommandAction::ToggleDiagnostics => {
                self.show_diagnostics = !self.show_diagnostics;
                self.message = Some(format!("Diagnostic dots: {}", if self.show_diagnostics { "ON" } else { "OFF" }));
            }
            CommandAction::ToggleAiCompletions => {
                self.ai_completions_enabled = !self.ai_completions_enabled;
                // Clear any existing suggestion when disabling
                if !self.ai_completions_enabled {
                    self.ai_suggestion = None;
                    self.ai_last_keystroke = None;
                    self.ai_pending_request = false;
                }
                self.message = Some(format!("AI completions: {}", if self.ai_completions_enabled { "ON" } else { "OFF" }));
            }
            CommandAction::ToggleIndentGuides => {
                self.show_indent_guides = !self.show_indent_guides;
                self.message = Some(format!("Indent guides: {}", if self.show_indent_guides { "ON" } else { "OFF" }));
            }
            CommandAction::ToggleBackups => {
                let new_state = !self.backup_manager.is_enabled();
                self.backup_manager.set_enabled(new_state);
                self.message = Some(format!("Backups: {}", if new_state { "ON" } else { "OFF" }));
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

    /// Handle key in confirm close tab mode
    fn handle_confirm_close_tab_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Save buffer then close
                if let Some(buffer_id) = self.pending_close_buffer_id {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        if let Some(path) = buffer.file_path().cloned() {
                            // Try to create backup (ignore errors - backup is optional)
                            let _ = self.backup_manager.create_backup(&path);

                            // Store content before attempting save (for sudo retry)
                            let content = buffer.text_buffer().to_string();

                            match buffer.text_buffer_mut().save() {
                                Ok(_) => {
                                    self.message = Some("Saved".to_string());
                                    self.notify_lsp_did_save();
                                }
                                Err(e) => {
                                    if self.is_permission_denied(&e) {
                                        // Set sudo save state
                                        self.pending_sudo_save_path = Some(path);
                                        self.pending_sudo_save_content = Some(content);
                                        self.mode = AppMode::ConfirmSudoSave;
                                        self.message = Some(
                                            "Permission denied. Save with sudo? (y/n)".to_string()
                                        );
                                        // Keep the close pending
                                        return Ok(ControlFlow::Continue);
                                    } else {
                                        self.message = Some(format!("Save failed: {}", e));
                                        self.mode = AppMode::Normal;
                                        self.pending_close_buffer_id = None;
                                        return Ok(ControlFlow::Continue);
                                    }
                                }
                            }
                        } else {
                            self.message = Some("No file path set".to_string());
                            self.mode = AppMode::Normal;
                            self.pending_close_buffer_id = None;
                            return Ok(ControlFlow::Continue);
                        }
                    }

                    // Now close the buffer
                    if let Err(e) = self.workspace.close_buffer(buffer_id) {
                        self.message = Some(format!("Error closing buffer: {}", e));
                    } else {
                        // If there are remaining buffers, switch to another one
                        let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                        if let Some(&next_id) = buffer_ids.first() {
                            let pane = self.layout.active_pane();
                            self.layout.set_buffer(pane, next_id);
                            self.message = Some("Buffer saved and closed".to_string());
                        } else {
                            // No more buffers, exit
                            self.message = Some("All buffers closed".to_string());
                            self.mode = AppMode::Normal;
                            self.pending_close_buffer_id = None;
                            return Ok(ControlFlow::Exit);
                        }
                    }
                }
                self.mode = AppMode::Normal;
                self.pending_close_buffer_id = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Close without saving (force close)
                if let Some(buffer_id) = self.pending_close_buffer_id {
                    self.workspace.force_close_buffer(buffer_id);

                    // If there are remaining buffers, switch to another one
                    let buffer_ids: Vec<_> = self.workspace.buffer_ids();
                    if let Some(&next_id) = buffer_ids.first() {
                        let pane = self.layout.active_pane();
                        self.layout.set_buffer(pane, next_id);
                        self.message = Some("Buffer closed without saving".to_string());
                    } else {
                        // No more buffers, exit
                        self.message = Some("All buffers closed".to_string());
                        self.mode = AppMode::Normal;
                        self.pending_close_buffer_id = None;
                        return Ok(ControlFlow::Exit);
                    }
                }
                self.mode = AppMode::Normal;
                self.pending_close_buffer_id = None;
            }
            KeyCode::Esc => {
                // Cancel and return to normal mode
                self.mode = AppMode::Normal;
                self.message = Some("Close cancelled".to_string());
                self.pending_close_buffer_id = None;
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel and return to normal mode (Emacs style)
                self.mode = AppMode::Normal;
                self.message = Some("Close cancelled".to_string());
                self.pending_close_buffer_id = None;
            }
            _ => {
                // Any other key cancels
                self.mode = AppMode::Normal;
                self.message = Some("Close cancelled".to_string());
                self.pending_close_buffer_id = None;
            }
        }
        Ok(ControlFlow::Continue)
    }

    /// Handle key in confirm sudo save mode
    fn handle_confirm_sudo_save_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // User confirmed - set flag to execute on next cycle
                self.execute_sudo_save_on_render = true;
                self.mode = AppMode::Normal;
                self.message = Some("Executing sudo save...".to_string());
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // Cancel sudo save
                self.mode = AppMode::Normal;
                self.message = Some("Save cancelled".to_string());
                self.pending_sudo_save_path = None;
                self.pending_sudo_save_content = None;
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel (Emacs style)
                self.mode = AppMode::Normal;
                self.message = None;
                self.pending_sudo_save_path = None;
                self.pending_sudo_save_content = None;
            }
            _ => {}
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
                        // Center the view on the target line after jumping
                        buffer.editor_state_mut().viewport.center_on_line(target_line);

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
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
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

    /// Handle key in save-as prompt mode
    fn handle_save_as_prompt_mode(&mut self, key: KeyEvent) -> Result<ControlFlow> {
        match key.code {
            KeyCode::Esc => {
                // Cancel save-as
                self.mode = AppMode::Normal;
                self.message = Some("Save cancelled".to_string());
                self.save_as_filename.clear();
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel save-as (Emacs style)
                self.mode = AppMode::Normal;
                self.message = Some("Save cancelled".to_string());
                self.save_as_filename.clear();
            }
            KeyCode::Enter => {
                // Save with the specified filename
                if self.save_as_filename.is_empty() {
                    self.message = Some("Filename cannot be empty".to_string());
                    return Ok(ControlFlow::Continue);
                }

                // Get current working directory
                let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let file_path = current_dir.join(&self.save_as_filename);

                // Get the active buffer and save it
                if let Some(buffer_id) = self.layout.active_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        // Get the content to save
                        let content = buffer.text_buffer().to_string();

                        // Try to write the file
                        match std::fs::write(&file_path, &content) {
                            Ok(_) => {
                                // Update the buffer with the new file path
                                buffer.text_buffer_mut().set_file_path(file_path.clone());
                                buffer.text_buffer_mut().set_modified(false);

                                self.message = Some(format!("Saved as {}", file_path.display()));
                                self.notify_lsp_did_save();
                            }
                            Err(e) => {
                                self.message = Some(format!("Save failed: {}", e));
                            }
                        }
                    }
                }

                self.mode = AppMode::Normal;
                self.save_as_filename.clear();
            }
            KeyCode::Char(c) => {
                // Add character to filename
                self.save_as_filename.push(c);
                self.message = Some(format!("Save as: {}", self.save_as_filename));
            }
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_as_filename.pop();
                if self.save_as_filename.is_empty() {
                    self.message = Some("Save as:".to_string());
                } else {
                    self.message = Some(format!("Save as: {}", self.save_as_filename));
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
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
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
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
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
                if let Some(buffer_id) = self.layout.active_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        buffer.editor_state_mut().clear_selection();
                    }
                }
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+G: Cancel search (Emacs style)
                self.mode = AppMode::Normal;
                self.message = None;
                self.search_pattern.clear();
                // Clear selection when exiting search
                if let Some(buffer_id) = self.layout.active_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        buffer.editor_state_mut().clear_selection();
                    }
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+S in search mode: search forward for next occurrence
                if !self.search_pattern.is_empty() {
                    self.search_is_reverse = false;

                    // Move cursor forward by one character to find next match
                    if let Some(buffer_id) = self.layout.active_buffer() {
                        if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                            let current_pos = buffer.editor_state().cursor.position();
                            let text_buffer = buffer.text_buffer();
                            let current_char = text_buffer.pos_to_char(current_pos)?;
                            let new_pos = text_buffer.char_to_pos(current_char + 1);
                            buffer.editor_state_mut().cursor.set_position(new_pos);
                        }
                    }

                    // Search (will wrap automatically if needed)
                    self.perform_search()?;
                }
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+R in search mode: search backward for previous occurrence
                if !self.search_pattern.is_empty() {
                    self.search_is_reverse = true;

                    // Move cursor backward by one character to find previous match
                    if let Some(buffer_id) = self.layout.active_buffer() {
                        if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                            let current_pos = buffer.editor_state().cursor.position();
                            let text_buffer = buffer.text_buffer();
                            let current_char = text_buffer.pos_to_char(current_pos)?;
                            if current_char > 0 {
                                let new_pos = text_buffer.char_to_pos(current_char - 1);
                                buffer.editor_state_mut().cursor.set_position(new_pos);
                            }
                        }
                    }

                    // Search (will wrap automatically if needed)
                    self.perform_search()?;
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
                        if let Some(buffer_id) = self.layout.active_buffer() {
                            if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                                buffer.editor_state_mut().cursor.set_position(start_pos);
                            }
                        }
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
                        if let Some(buffer_id) = self.layout.active_buffer() {
                            if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                                buffer.editor_state_mut().cursor.set_position(start_pos);
                            }
                        }
                    }
                    let _ = self.perform_search(); // Ignore errors for incremental search
                }
            }
            KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_pattern.pop();
                // Update message to show current pattern
                let search_type = if self.search_is_reverse { "Reverse search" } else { "Search" };
                let regex_indicator = if self.search_use_regex { " [REGEX]" } else { "" };
                self.message = Some(format!("{}{}: {}", search_type, regex_indicator, self.search_pattern));
                // Reset to start position and re-search with shorter pattern
                if let Some(start_pos) = self.search_start_pos {
                    if let Some(buffer_id) = self.layout.active_buffer() {
                        if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                            buffer.editor_state_mut().cursor.set_position(start_pos);
                        }
                    }
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
        let Some(buffer_id) = self.layout.active_buffer() else {
            return Ok(false);
        };
        let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
            return Ok(false);
        };

        let text = buffer.text_buffer().to_string();
        let current_pos = buffer.editor_state().cursor.position();
        let current_char_idx = buffer.text_buffer().pos_to_char(current_pos)?;

        // Find ALL matches in the buffer
        let all_matches: Vec<(usize, usize)> = if self.search_use_regex {
            // Regex search (case-insensitive by default)
            match RegexBuilder::new(&self.search_pattern).case_insensitive(true).build() {
                Ok(re) => {
                    re.find_iter(&text)
                        .map(|m| {
                            let char_start = text[..m.start()].chars().count();
                            let char_len = text[m.start()..m.end()].chars().count();
                            (char_start, char_len)
                        })
                        .collect()
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
            let mut matches = Vec::new();
            let mut search_pos = 0;

            while let Some(byte_offset) = text_lower[search_pos..].find(&pattern_lower) {
                let absolute_byte_pos = search_pos + byte_offset;
                let char_idx = text_lower[..absolute_byte_pos].chars().count();
                let match_len = self.search_pattern.chars().count();
                matches.push((char_idx, match_len));
                search_pos = absolute_byte_pos + pattern_lower.len();
            }
            matches
        };

        if all_matches.is_empty() {
            return Ok(false);
        }

        // Find the next match based on direction
        let found_match = if self.search_is_reverse {
            // Reverse search - find the last match that ends at or before cursor (or wrap to end)
            all_matches.iter()
                .filter(|(start, len)| *start + *len <= current_char_idx)
                .last()
                .or_else(|| all_matches.last()) // Wrap to last match if nothing before
        } else {
            // Forward search - find the first match at or after cursor (or wrap to beginning)
            all_matches.iter()
                .find(|(start, _)| *start >= current_char_idx)
                .or_else(|| all_matches.first()) // Wrap to first match if nothing after
        };

        if let Some(&(char_idx, match_len)) = found_match {
            let pos = buffer.text_buffer().char_to_pos(char_idx);
            let end_char_idx = char_idx + match_len;
            let end_pos = buffer.text_buffer().char_to_pos(end_char_idx);

            // Move cursor to END of match (not start) to avoid interfering with first character highlight
            buffer.editor_state_mut().cursor.set_position(end_pos);
            // Center the view on the search result
            buffer.editor_state_mut().viewport.center_on_line(end_pos.line);

            // Select the found text (anchor at start, head at end)
            buffer.editor_state_mut().selection = Some(crate::editor::state::Selection::new(pos, end_pos));
            self.copy_selection_to_primary();

            // Calculate current match index (1-based)
            let current_index = all_matches.iter().position(|(start, _)| *start == char_idx).unwrap() + 1;
            let total_count = all_matches.len();

            self.message = Some(format!("Found: {} ({}/{})", self.search_pattern, current_index, total_count));
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
            self.copy_selection_to_primary();

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
            KeyCode::Enter | KeyCode::Tab => {
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
                                if ch.is_alphanumeric() || ch == '_' {
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

        // Dismiss AI suggestion on any key except Tab (for accepting) or regular character input
        // Character input is handled separately to start new debounce
        if self.ai_suggestion.is_some() {
            match (key.code, key.modifiers) {
                // Tab without modifiers - will be handled later to accept suggestion
                (KeyCode::Tab, mods) if !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::SHIFT) => {
                    // Don't dismiss, let the Tab handler accept it
                }
                // Regular character without control modifier - will be handled to start new debounce
                (KeyCode::Char(_), mods) if !mods.contains(KeyModifiers::CONTROL) => {
                    // Don't dismiss here, character input handler will clear and restart debounce
                }
                // Esc - handled separately with message
                (KeyCode::Esc, KeyModifiers::NONE) => {
                    // Will be handled below with message
                }
                // Any other key - dismiss the suggestion silently
                _ => {
                    self.ai_suggestion = None;
                }
            }
        }

        // Handle Ctrl+X Ctrl+S (Emacs-style save) and Ctrl+X Ctrl+C (Emacs-style exit)
        if self.waiting_for_second_key {
            self.waiting_for_second_key = false;
            if matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+P - Command panel
                self.mode = AppMode::CommandPanel;
                self.command_panel_pattern.clear();
                self.command_panel_results = self.filter_commands("");
                self.command_panel_selected = 0;
                self.command_panel_scroll_offset = 0;
                self.message = Some("Command Palette (Ctrl+X Ctrl+P)".to_string());
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+F - Project search
                if self.file_tree.is_some() {
                    self.mode = AppMode::ProjectSearch;
                    self.project_search_pattern.clear();
                    self.project_search_results.clear();
                    self.project_search_selected = 0;
                    self.project_search_scroll_offset = 0;
                    self.message = Some("Project Search (Ctrl+X Ctrl+F)".to_string());
                } else {
                    self.message = Some("No project directory open".to_string());
                }
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('s')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+S - Save
                if let Some(path) = buffer.file_path().cloned() {
                    // Try to create backup (ignore errors - backup is optional)
                    let _ = self.backup_manager.create_backup(&path);

                    // Store content before attempting save (for sudo retry)
                    let content = buffer.text_buffer().to_string();

                    match buffer.text_buffer_mut().save() {
                        Ok(_) => {
                            self.message = Some("Saved".to_string());
                            self.notify_lsp_did_save();
                        }
                        Err(e) => {
                            if self.is_permission_denied(&e) {
                                self.pending_sudo_save_path = Some(path);
                                self.pending_sudo_save_content = Some(content);
                                self.mode = AppMode::ConfirmSudoSave;
                                self.message = Some(
                                    "Permission denied. Save with sudo? (y/n)".to_string()
                                );
                            } else {
                                self.message = Some(format!("Save failed: {}", e));
                            }
                        }
                    }
                } else {
                    // No file path - prompt for filename
                    self.mode = AppMode::SaveAsPrompt;
                    self.save_as_filename.clear();
                    self.message = Some("Save as:".to_string());
                }
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('h')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+H - Replace (query-replace)
                self.mode = AppMode::ReplacePrompt;
                self.replace_pattern.clear();
                self.replace_with.clear();
                self.replace_count = 0;
                self.search_start_pos = Some(buffer.editor_state().cursor.position());
                self.search_use_regex = true;  // Default to regex mode
                self.message = Some("Replace [REGEX]:".to_string());
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('u') | KeyCode::Char('U')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X Ctrl+U - Toggle mark word at cursor
                if let Err(e) = self.toggle_word_mark_at_cursor() {
                    self.message = Some(format!("Toggle mark failed: {}", e));
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
            } else if matches!(key.code, KeyCode::Char('1')) && !key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X 1 - Close split and go to single pane mode (emacs-style)
                if self.layout.mode() == crate::workspace::LayoutMode::VerticalSplit {
                    self.layout.close_split();
                    self.message = Some("Split closed".to_string());
                } else {
                    self.message = Some("Already in single pane mode".to_string());
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
            } else if matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I')) && !key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X I - Toggle smart indentation
                self.smart_indentation = !self.smart_indentation;
                self.message = Some(format!("Smart indentation: {}", if self.smart_indentation { "ON" } else { "OFF" }));
                return Ok(ControlFlow::Continue);
            } else if matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D')) && !key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+X D - Toggle diagnostic dots
                self.show_diagnostics = !self.show_diagnostics;
                self.message = Some(format!("Diagnostic dots: {}", if self.show_diagnostics { "ON" } else { "OFF" }));
                return Ok(ControlFlow::Continue);
            }
            // If not a recognized chord, fall through to handle the key normally
        }

        match (key.code, key.modifiers) {
            // Esc - Dismiss AI suggestion
            (KeyCode::Esc, KeyModifiers::NONE) if self.ai_suggestion.is_some() => {
                self.ai_suggestion = None;
                self.message = Some("AI suggestion dismissed".to_string());
                return Ok(ControlFlow::Continue);
            }

            // Ctrl+Q - Quit
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                if self.workspace.has_modified_buffers() {
                    self.mode = AppMode::ConfirmExit;
                    self.message = Some("Save modified buffers? (y/n)".to_string());
                    return Ok(ControlFlow::Continue);
                }
                return Ok(ControlFlow::Exit);
            }

            // Ctrl+U - Cycle through marks
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                if let Err(e) = self.cycle_to_next_mark() {
                    self.message = Some(format!("Cycle marks failed: {}", e));
                }
                return Ok(ControlFlow::Continue);
            }

            // Ctrl+W - Close current buffer/tab
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                if let Some(buffer_id) = self.layout.active_buffer() {
                    // Check if buffer is actually modified (compares with file on disk)
                    if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                        if buffer.is_actually_modified() {
                            // Enter confirmation mode
                            self.mode = AppMode::ConfirmCloseTab;
                            self.pending_close_buffer_id = Some(buffer_id);
                            self.message = Some("Save buffer before closing? (y/n)".to_string());
                        } else {
                            // Close the buffer directly
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
                                    // No more buffers, exit
                                    self.message = Some("All buffers closed".to_string());
                                    return Ok(ControlFlow::Exit);
                                }
                            }
                        }
                    }
                }
                return Ok(ControlFlow::Continue);
            }

            // Ctrl+P - File picker
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                if self.file_tree.is_some() {
                    // Get extension before entering file picker mode (to avoid borrow conflicts)
                    let priority_ext = buffer.file_path()
                        .and_then(|p| p.extension())
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_string());

                    self.mode = AppMode::FilePicker;
                    self.file_picker_pattern.clear();
                    self.file_picker_selected = 0;
                    self.file_picker_scroll_offset = 0;

                    // Populate initial list with all files (alphabetically sorted)
                    self.update_file_picker_results();

                    // Show what extension is being prioritized
                    if let Some(ext) = priority_ext {
                        self.message = Some(format!("Prioritizing .{} files", ext));
                    }
                } else {
                    self.message = Some("No project directory open".to_string());
                }
            }

            // Ctrl+N - New buffer
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                // Create a new empty buffer
                let new_buffer_id = self.workspace.new_buffer();

                // Set it as active in the current pane
                let active_pane = self.layout.active_pane();
                self.layout.set_buffer(active_pane, new_buffer_id);

                self.message = Some("New buffer created".to_string());
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

            // Alt+Up - Move line up
            (KeyCode::Up, KeyModifiers::ALT) => {
                let current_line = buffer.editor_state().cursor.line;
                let cursor_col = buffer.editor_state().cursor.column;

                // Can't move first line up
                if current_line == 0 {
                    self.message = Some("Already at first line".to_string());
                } else {
                    let (text_buffer, editor_state, undo_manager) = buffer.split_mut();

                    // Get the current line and the line above (includes newlines)
                    if let (Some(current_content), Some(prev_content)) =
                        (text_buffer.get_line(current_line), text_buffer.get_line(current_line - 1)) {

                        // Remove newlines to get actual content length
                        let current_trimmed = current_content.trim_end_matches(&['\n', '\r'][..]);
                        let current_line_len = current_trimmed.len();

                        // Calculate positions for deletion
                        let start_pos = Position::new(current_line - 1, 0);
                        let end_pos = Position::new(current_line + 1, 0);

                        // Delete both lines (returns deleted content)
                        if let Ok(deleted) = text_buffer.delete_range(start_pos, end_pos) {
                            // Swap: current line + prev line (both already have newlines from get_line)
                            let swapped = format!("{}{}", current_content, prev_content);

                            // Insert swapped content
                            if text_buffer.insert(start_pos, &swapped).is_ok() {
                                // Record for undo as a single compound change
                                undo_manager.record(Change::Compound(vec![
                                    Change::Delete {
                                        pos: start_pos,
                                        text: deleted.clone(),
                                    },
                                    Change::Insert {
                                        pos: start_pos,
                                        text: swapped,
                                    }
                                ]));

                                // Move cursor up and preserve column
                                let new_col = cursor_col.min(current_line_len);
                                editor_state.cursor.line = current_line - 1;
                                editor_state.cursor.column = new_col;
                                self.message = Some("Line moved up".to_string());
                            }
                        }
                    }
                }
            }

            // Alt+Down - Move line down
            (KeyCode::Down, KeyModifiers::ALT) => {
                let current_line = buffer.editor_state().cursor.line;
                let cursor_col = buffer.editor_state().cursor.column;

                // Check if we can move down
                if current_line >= buffer.text_buffer().len_lines() - 1 {
                    self.message = Some("Already at last line".to_string());
                } else {
                    let (text_buffer, editor_state, undo_manager) = buffer.split_mut();

                    // Get the current line and the line below (includes newlines)
                    if let (Some(current_content), Some(next_content)) =
                        (text_buffer.get_line(current_line), text_buffer.get_line(current_line + 1)) {

                        // Remove newlines to get actual content length
                        let current_trimmed = current_content.trim_end_matches(&['\n', '\r'][..]);
                        let current_line_len = current_trimmed.len();

                        // Calculate positions for deletion
                        let start_pos = Position::new(current_line, 0);
                        let end_pos = Position::new(current_line + 2, 0);

                        // Delete both lines (returns deleted content)
                        if let Ok(deleted) = text_buffer.delete_range(start_pos, end_pos) {
                            // Swap: next line + current line (both already have newlines from get_line)
                            let swapped = format!("{}{}", next_content, current_content);

                            // Insert swapped content
                            if text_buffer.insert(start_pos, &swapped).is_ok() {
                                // Record for undo as a single compound change
                                undo_manager.record(Change::Compound(vec![
                                    Change::Delete {
                                        pos: start_pos,
                                        text: deleted.clone(),
                                    },
                                    Change::Insert {
                                        pos: start_pos,
                                        text: swapped,
                                    }
                                ]));

                                // Move cursor down and preserve column
                                let new_col = cursor_col.min(current_line_len);
                                editor_state.cursor.line = current_line + 1;
                                editor_state.cursor.column = new_col;
                                self.message = Some("Line moved down".to_string());
                            }
                        }
                    }
                }
            }

            // Alt+G - Jump to line
            (KeyCode::Char('g'), KeyModifiers::ALT) => {
                self.mode = AppMode::JumpToLine;
                self.jump_to_line_input.clear();
                self.message = Some("Go to line:".to_string());
            }

            // Alt+; - Toggle comment on line or selection
            (KeyCode::Char(';'), KeyModifiers::ALT) => {
                let active_buffer_id = self.layout.active_buffer();
                let Some(buffer_id) = active_buffer_id else {
                    return Ok(ControlFlow::Continue);
                };
                let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) else {
                    return Ok(ControlFlow::Continue);
                };

                // Determine comment syntax based on file extension (prefix, suffix)
                let (comment_prefix, comment_suffix) = if let Some(path) = buffer.file_path() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        match ext {
                            "py" | "pyi" | "pyw" => ("# ", ""),
                            "rs" => ("// ", ""),
                            "js" | "jsx" | "ts" | "tsx" | "c" | "cpp" | "h" | "hpp" | "java" | "go" => ("// ", ""),
                            "sh" | "bash" | "zsh" => ("# ", ""),
                            "html" | "xml" | "svg" | "xhtml" => ("<!-- ", " -->"),
                            "css" | "scss" => ("/* ", " */"),
                            _ => ("# ", ""), // Default to # for unknown types
                        }
                    } else {
                        ("# ", "")
                    }
                } else {
                    ("# ", "")
                };

                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();

                // Determine which lines to comment
                let (start_line, end_line) = if let Some(selection) = editor_state.selection {
                    let (start_pos, end_pos) = selection.range();
                    (start_pos.line, end_pos.line)
                } else {
                    // No selection, just comment current line
                    (editor_state.cursor.line, editor_state.cursor.line)
                };

                // Check if lines are already commented
                let mut all_commented = true;
                for line_idx in start_line..=end_line {
                    if let Some(line) = text_buffer.get_line(line_idx) {
                        let trimmed = line.trim();
                        let trimmed_start = line.trim_start();
                        // Check both prefix and suffix
                        if !trimmed_start.starts_with(comment_prefix.trim()) ||
                           (!comment_suffix.is_empty() && !trimmed.ends_with(comment_suffix.trim())) {
                            all_commented = false;
                            break;
                        }
                    }
                }

                // Toggle comments (in reverse order to maintain positions)
                let mut changes = Vec::new();
                for line_idx in (start_line..=end_line).rev() {
                    if let Some(line) = text_buffer.get_line(line_idx) {
                        if all_commented {
                            // Remove comment - need to remove both prefix and suffix
                            let trimmed_start = line.trim_start();
                            let trimmed = line.trim();

                            if trimmed_start.starts_with(comment_prefix.trim()) {
                                // Remove prefix
                                let indent_len = line.len() - trimmed_start.len();
                                let comment_start_pos = crate::buffer::Position::new(line_idx, indent_len);
                                let comment_end_pos = crate::buffer::Position::new(line_idx, indent_len + comment_prefix.len());

                                if let Ok(deleted) = text_buffer.delete_range(comment_start_pos, comment_end_pos) {
                                    changes.push(crate::buffer::Change::Delete {
                                        pos: comment_start_pos,
                                        text: deleted,
                                    });
                                }

                                // Remove suffix if it exists
                                if !comment_suffix.is_empty() {
                                    // After removing prefix, recalculate line content
                                    if let Some(line) = text_buffer.get_line(line_idx) {
                                        let trimmed = line.trim_end();
                                        if trimmed.ends_with(comment_suffix.trim()) {
                                            let suffix_start = trimmed.len() - comment_suffix.trim().len();
                                            let line_start = line.len() - line.trim_end().len();
                                            let suffix_start_pos = crate::buffer::Position::new(line_idx, suffix_start);
                                            let suffix_end_pos = crate::buffer::Position::new(line_idx, suffix_start + comment_suffix.len());

                                            if let Ok(deleted) = text_buffer.delete_range(suffix_start_pos, suffix_end_pos) {
                                                changes.push(crate::buffer::Change::Delete {
                                                    pos: suffix_start_pos,
                                                    text: deleted,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Add comment - need to add both prefix and suffix
                            let indent_len = line.len() - line.trim_start().len();
                            let insert_pos = crate::buffer::Position::new(line_idx, indent_len);

                            // Insert prefix
                            text_buffer.insert(insert_pos, comment_prefix)?;
                            changes.push(crate::buffer::Change::Insert {
                                pos: insert_pos,
                                text: comment_prefix.to_string(),
                            });

                            // Insert suffix if it exists
                            if !comment_suffix.is_empty() {
                                // After inserting prefix, get updated line length
                                if let Some(line) = text_buffer.get_line(line_idx) {
                                    let line_end = line.trim_end().len();
                                    let suffix_pos = crate::buffer::Position::new(line_idx, line_end);

                                    text_buffer.insert(suffix_pos, comment_suffix)?;
                                    changes.push(crate::buffer::Change::Insert {
                                        pos: suffix_pos,
                                        text: comment_suffix.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }

                // Reverse changes back to forward order for undo
                changes.reverse();

                // Record all changes as a compound change for undo
                if !changes.is_empty() {
                    undo_manager.record(crate::buffer::Change::Compound(changes));
                    let action = if all_commented { "Uncommented" } else { "Commented" };
                    let line_count = end_line - start_line + 1;
                    self.message = Some(format!("{} {} line(s)", action, line_count));
                }

                // Clear selection after commenting
                editor_state.clear_selection();
            }

            // Ctrl+Backspace - Delete word before cursor
            // Also handle Ctrl+H since many terminals send this for Ctrl+Backspace
            (KeyCode::Backspace, KeyModifiers::CONTROL) | (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                let pos = editor_state.cursor.position();

                if pos.column > 0 {
                    if let Some(line) = text_buffer.get_line(pos.line) {
                        let chars: Vec<char> = line.chars().collect();
                        let mut word_start = pos.column;

                        // Skip trailing whitespace
                        while word_start > 0 && word_start <= chars.len() {
                            let idx = word_start - 1;
                            if idx < chars.len() && chars[idx].is_whitespace() {
                                word_start -= 1;
                            } else {
                                break;
                            }
                        }

                        // Check what character we're at now
                        if word_start > 0 {
                            let idx = word_start - 1;
                            if idx < chars.len() {
                                let ch = chars[idx];

                                if ch.is_alphanumeric() {
                                    // Delete the whole word (alphanumeric only)
                                    while word_start > 0 {
                                        let idx = word_start - 1;
                                        if idx < chars.len() {
                                            let ch = chars[idx];
                                            if ch.is_alphanumeric() {
                                                word_start -= 1;
                                            } else {
                                                break;
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                } else {
                                    // It's punctuation or underscore - delete just one character
                                    word_start -= 1;
                                }
                            }
                        }

                        // Delete from word_start to current cursor position
                        if word_start < pos.column {
                            let delete_start = Position::new(pos.line, word_start);
                            let delete_end = pos;
                            if let Ok(deleted) = text_buffer.delete_range(delete_start, delete_end) {
                                undo_manager.record(Change::Delete {
                                    pos: delete_start,
                                    text: deleted,
                                });
                                editor_state.cursor.set_position(delete_start);
                                editor_state.ensure_cursor_visible();
                            }
                        }
                    }
                }
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
                    // Save cursor position before undo
                    let saved_line = buffer.editor_state().cursor.line;
                    let saved_col = buffer.editor_state().cursor.column;

                    buffer.apply_change(&change)?;
                    buffer.undo_manager_mut().finish_undo_redo();

                    // For compound changes (like line movements), try to restore cursor position
                    if let Change::Compound(changes) = &change {
                        if let Some(Change::Delete { text, .. }) = changes.first() {
                            // Count lines in deleted text to determine offset
                            let line_count = text.matches('\n').count();
                            if line_count > 0 {
                                // Cursor was on line N, moved to line N-1 or N+1
                                // After undo, adjust back
                                let target_line = if saved_line == buffer.editor_state().cursor.line {
                                    // Line didn't change, might need adjustment
                                    saved_line + 1
                                } else {
                                    saved_line
                                };

                                let total_lines = buffer.text_buffer().len_lines();
                                if target_line < total_lines {
                                    let line_len = buffer.text_buffer().line_len(target_line);
                                    buffer.editor_state_mut().cursor.line = target_line;
                                    buffer.editor_state_mut().cursor.column = saved_col.min(line_len);
                                }
                            }
                        }
                    }

                    self.message = Some("Undo".to_string());
                }
            }

            // Ctrl+Y - Redo
            (KeyCode::Char('y'), KeyModifiers::CONTROL) => {
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
                self.copy_selection_to_primary();
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
                self.copy_selection_to_primary();
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

                        // Also copy to system clipboard
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&deleted);
                        }

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
                        let ch_str = ch.to_string();
                        self.clipboard = ch_str.clone();

                        // Also copy to system clipboard
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&ch_str);
                        }

                        undo_manager.record(Change::Delete {
                            pos,
                            text: ch_str,
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
                        let selected: String = text.chars().skip(start_idx).take(end_idx - start_idx).collect();

                        // Copy to both internal and system clipboard
                        self.clipboard = selected.clone();

                        // Try to copy to system clipboard
                        match arboard::Clipboard::new() {
                            Ok(mut clipboard) => {
                                if let Err(e) = clipboard.set_text(&selected) {
                                    self.message = Some(format!("Copied (system clipboard failed: {})", e));
                                } else {
                                    self.message = Some("Copied to clipboard".to_string());
                                }
                            }
                            Err(e) => {
                                self.message = Some(format!("Copied (system clipboard unavailable: {})", e));
                            }
                        }
                    }
                } else {
                    self.message = Some("No selection".to_string());
                }
            }

            // Ctrl+V - Paste from clipboard
            (KeyCode::Char('v'), KeyModifiers::CONTROL) => {
                // Try to get text from system clipboard first, fall back to internal
                let clipboard_text = match arboard::Clipboard::new() {
                    Ok(mut clipboard) => {
                        clipboard.get_text().ok()
                    }
                    Err(_) => None
                }.or_else(|| {
                    if !self.clipboard.is_empty() {
                        Some(self.clipboard.clone())
                    } else {
                        None
                    }
                });

                if let Some(text) = clipboard_text {
                    let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                    let pos = editor_state.cursor.position();
                    text_buffer.insert(pos, &text)?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: text.clone(),
                    });

                    // Move cursor to end of pasted text
                    let char_idx = text_buffer.pos_to_char(pos)? + text.len();
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
                    self.copy_selection_to_primary();
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
                    self.copy_selection_to_primary();
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
                    self.copy_selection_to_primary();
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
                    self.copy_selection_to_primary();
                } else {
                    editor_state.clear_selection();
                }
            }

            // Home/End (with optional Shift for selection)
            (KeyCode::Home, mods) => {
                let (_, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::move_to_line_start(editor_state);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                    self.copy_selection_to_primary();
                } else {
                    editor_state.clear_selection();
                }
            }
            (KeyCode::End, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::move_to_line_end(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                    self.copy_selection_to_primary();
                } else {
                    editor_state.clear_selection();
                }
            }

            // Page Up/Down (with optional Shift for selection)
            (KeyCode::PageUp, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::page_up(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                    self.copy_selection_to_primary();
                } else {
                    editor_state.clear_selection();
                }
            }
            (KeyCode::PageDown, mods) => {
                let (text_buffer, editor_state, _) = buffer.split_mut();
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.start_selection();
                }
                Movement::page_down(editor_state, text_buffer);
                if mods.contains(KeyModifiers::SHIFT) {
                    editor_state.update_selection();
                    self.copy_selection_to_primary();
                } else {
                    editor_state.clear_selection();
                }
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
                    // Check if we're jumping within the same file
                    let current_buffer_id = self.layout.active_buffer();
                    let is_same_file = if let Some(buffer_id) = current_buffer_id {
                        if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                            buffer.file_path() == Some(&location.path)
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_same_file {
                        // Just move cursor in current buffer
                        if let Some(buffer_id) = current_buffer_id {
                            if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                                let editor_state = buffer.editor_state_mut();
                                editor_state.cursor.set_position(crate::buffer::Position {
                                    line: location.position.line,
                                    column: location.position.column,
                                });
                                // Center the view on the cursor after jumping back
                                editor_state.viewport.center_on_line(location.position.line);
                                self.message = Some(format!(
                                    "Jumped back to line {}:{}",
                                    location.position.line + 1,
                                    location.position.column + 1
                                ));
                            }
                        }
                    } else {
                        // Open the file and jump to the position
                        match self.workspace.open_file(location.path.clone()) {
                            Ok(open_result) => {
                                let buffer_id = open_result.buffer_id();
                                // Set the buffer in the active pane
                                let pane = self.layout.active_pane();
                                self.layout.set_buffer(pane, buffer_id);

                                // Now get the buffer and set cursor position
                                if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                                    let editor_state = buffer.editor_state_mut();
                                    editor_state.cursor.set_position(crate::buffer::Position {
                                        line: location.position.line,
                                        column: location.position.column,
                                    });
                                    // Center the view on the cursor after jumping back
                                    editor_state.viewport.center_on_line(location.position.line);
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
            (KeyCode::Backspace, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
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

            // Ctrl+Delete - Delete word after cursor
            (KeyCode::Delete, KeyModifiers::CONTROL) => {
                let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                let pos = editor_state.cursor.position();

                // Get the current line
                if let Some(line) = text_buffer.get_line(pos.line) {
                    let chars: Vec<char> = line.chars().collect();
                    let mut word_end = pos.column;

                    // Skip leading whitespace
                    while word_end < chars.len() && chars[word_end].is_whitespace() {
                        word_end += 1;
                    }

                    // Delete the word (alphanumeric/underscore)
                    while word_end < chars.len() {
                        let ch = chars[word_end];
                        if ch.is_alphanumeric() || ch == '_' {
                            word_end += 1;
                        } else {
                            break;
                        }
                    }

                    // Delete from cursor to word_end
                    if word_end > pos.column {
                        let delete_start = pos;
                        let delete_end = Position::new(pos.line, word_end);
                        if let Ok(deleted) = text_buffer.delete_range(delete_start, delete_end) {
                            undo_manager.record(Change::Delete {
                                pos: delete_start,
                                text: deleted,
                            });
                        }
                    }
                }
            }

            // Delete
            (KeyCode::Delete, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
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

            // Enter - Smart indentation
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

                // Smart indentation (if enabled)
                if self.smart_indentation {
                    // Get current line to calculate indentation
                    let current_line = text_buffer.get_line(pos.line).unwrap_or_default();

                    // Calculate base indentation (leading spaces/tabs)
                    let base_indent = current_line.chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect::<String>();

                    // Check the part of the line before cursor to see if it ends with ':'
                    let line_chars: Vec<char> = current_line.chars().collect();
                    let line_before_cursor = if pos.column <= line_chars.len() {
                        line_chars.iter().take(pos.column).collect::<String>()
                    } else {
                        current_line.clone()
                    };
                    let line_before_cursor_trimmed = line_before_cursor.trim_end();
                    let needs_extra_indent = line_before_cursor_trimmed.ends_with(':');

                    // Build the newline + indentation string
                    let mut indent_str = String::from("\n");
                    indent_str.push_str(&base_indent);
                    if needs_extra_indent {
                        indent_str.push_str("    "); // Add 4 spaces for Python
                    }

                    // Insert newline with indentation
                    text_buffer.insert(pos, &indent_str)?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: indent_str.clone(),
                    });

                    // Move cursor to end of inserted text
                    let indent_len = base_indent.chars().count() + if needs_extra_indent { 4 } else { 0 };
                    editor_state.cursor.line += 1;
                    editor_state.cursor.move_horizontal(indent_len);
                } else {
                    // Simple newline without smart indentation
                    text_buffer.insert(pos, "\n")?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: String::from("\n"),
                    });
                    editor_state.cursor.line += 1;
                    editor_state.cursor.move_horizontal(0);
                }
                editor_state.ensure_cursor_visible();
            }

            // Tab - Accept AI suggestion, smart completion, or indentation (but not Ctrl+Tab)
            (KeyCode::Tab, mods) if !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::SHIFT) => {
                // Check if there's an AI suggestion to accept
                if let Some(suggestion) = self.ai_suggestion.take() {
                    // Accept the AI suggestion by inserting it at cursor
                    let (text_buffer, editor_state, undo_manager) = buffer.split_mut();
                    let pos = editor_state.cursor.position();

                    // Insert the full suggestion (can be multi-line)
                    text_buffer.insert(pos, &suggestion)?;
                    undo_manager.record(Change::Insert {
                        pos,
                        text: suggestion.clone(),
                    });

                    // Move cursor to end of inserted text
                    // Count newlines to update line number
                    let newline_count = suggestion.matches('\n').count();
                    if newline_count > 0 {
                        editor_state.cursor.line += newline_count;
                        // Get the last line to determine column position
                        let last_line = suggestion.lines().last().unwrap_or("");
                        editor_state.cursor.column = last_line.chars().count();
                    } else {
                        editor_state.cursor.column += suggestion.chars().count();
                    }
                    editor_state.ensure_cursor_visible();

                    self.message = Some("AI suggestion accepted".to_string());
                    return Ok(ControlFlow::Continue);
                }

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

                // Trigger AI completion debouncing (only if enabled)
                if self.ai_completions_enabled {
                    // Clear any existing suggestion and reset timer
                    self.ai_suggestion = None;
                    self.ai_last_keystroke = Some(Instant::now());
                    // Cancel any pending AI request
                    if self.ai_pending_request {
                        if let Some(manager) = &self.ai_manager {
                            let _ = manager.cancel_pending();
                        }
                        self.ai_pending_request = false;
                    }
                }
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

    /// Initialize the AI completion manager
    pub fn initialize_ai(&mut self) -> Result<()> {
        use crate::ai::provider::CompletionProvider;
        use crate::ai::providers::{ClaudeProvider, CopilotProvider, LocalLlmProvider, OpenAiProvider};
        use std::sync::Arc;

        // Load configuration
        let config = Config::load()?;

        // Check if AI is enabled
        if !config.ai.enabled {
            return Ok(());
        }

        // Create provider based on configuration
        let provider: Arc<dyn CompletionProvider> = match config.ai.provider.as_str() {
            "copilot" => {
                let api_token = match config.ai.copilot.api_token {
                    Some(token) if !token.is_empty() => token,
                    _ => {
                        logger::log("AI completion disabled: No Copilot API token configured");
                        return Ok(());
                    }
                };
                Arc::new(CopilotProvider::new(api_token))
            }
            "openai" => {
                let api_key = match config.ai.openai.api_key {
                    Some(key) if !key.is_empty() => key,
                    _ => {
                        logger::log("AI completion disabled: No OpenAI API key configured");
                        return Ok(());
                    }
                };
                Arc::new(OpenAiProvider::new(
                    api_key,
                    config.ai.openai.model,
                ))
            }
            "claude" => {
                let api_key = match config.ai.claude.api_key {
                    Some(key) if !key.is_empty() => key,
                    _ => {
                        logger::log("AI completion disabled: No Claude API key configured");
                        return Ok(());
                    }
                };
                Arc::new(ClaudeProvider::new(
                    api_key,
                    config.ai.claude.model,
                ))
            }
            "local" => {
                if config.ai.local.endpoint.is_empty() {
                    logger::log("AI completion disabled: No local LLM endpoint configured");
                    return Ok(());
                }
                Arc::new(LocalLlmProvider::new(config.ai.local.endpoint))
            }
            _ => {
                logger::log(&format!(
                    "AI completion disabled: Unknown provider '{}'",
                    config.ai.provider
                ));
                return Ok(());
            }
        };

        // Create AI manager
        let (manager, receiver) = AiManager::new(provider);
        self.ai_manager = Some(manager);
        self.ai_receiver = Some(receiver);

        logger::log(&format!(
            "AI completion enabled with provider: {}",
            config.ai.provider
        ));

        Ok(())
    }

    /// Poll for LSP messages (non-blocking)
    pub fn poll_lsp_messages(&mut self) -> bool {
        // Collect responses first to avoid borrow checker issues
        let mut responses = Vec::new();
        if let Some(receiver) = &mut self.lsp_receiver {
            while let Ok(response) = receiver.try_recv() {
                responses.push(response);
            }
        }

        let had_updates = !responses.is_empty();

        // Handle all collected responses
        for response in responses {
            self.handle_lsp_response(response);
        }

        had_updates
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
                // Check if we're jumping within the same file
                let current_buffer_id = self.layout.active_buffer();
                let is_same_file = if let Some(buffer_id) = current_buffer_id {
                    if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                        buffer.file_path() == Some(&location.path)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_same_file {
                    // Just move cursor in current buffer
                    if let Some(buffer_id) = current_buffer_id {
                        if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                            let editor_state = buffer.editor_state_mut();
                            editor_state.cursor.set_position(crate::buffer::Position {
                                line: location.position.line,
                                column: location.position.column,
                            });
                            // Center the view on the cursor after jumping
                            editor_state.viewport.center_on_line(location.position.line);
                            self.message = Some(format!("Jumped to line {}:{}",
                                location.position.line + 1,
                                location.position.column + 1));
                        }
                    }
                } else {
                    // Open file and jump to position
                    match self.workspace.open_file(location.path.clone()) {
                        Ok(open_result) => {
                            let buffer_id = open_result.buffer_id();
                            // Set the buffer in the active pane
                            let pane = self.layout.active_pane();
                            self.layout.set_buffer(pane, buffer_id);

                            // Now get the buffer and set cursor position
                            if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                                let editor_state = buffer.editor_state_mut();
                                editor_state.cursor.set_position(crate::buffer::Position {
                                    line: location.position.line,
                                    column: location.position.column,
                                });
                                // Center the view on the cursor after jumping
                                editor_state.viewport.center_on_line(location.position.line);
                                self.message = Some(format!("Jumped to {}:{}:{}",
                                    location.path.display(),
                                    location.position.line + 1,
                                    location.position.column + 1));
                            }
                        }
                        Err(e) => {
                            self.message = Some(format!("Error: {}", e));
                        }
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
                        "{} completions (↑↓ to navigate, Enter/Tab to select, Esc to cancel)",
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

    /// Poll for AI completion messages (non-blocking)
    pub fn poll_ai_messages(&mut self) -> bool {
        // Collect responses first to avoid borrow checker issues
        let mut responses = Vec::new();
        if let Some(receiver) = &mut self.ai_receiver {
            while let Ok(response) = receiver.try_recv() {
                responses.push(response);
            }
        }

        let had_updates = !responses.is_empty();

        // Handle all collected responses
        for response in responses {
            self.handle_ai_response(response);
        }

        had_updates
    }

    /// Handle an AI completion response
    fn handle_ai_response(&mut self, response: AiResponse) {
        match response {
            AiResponse::Completion {
                buffer_id,
                text,
                provider: _,
            } => {
                // Only show suggestion if this is still the active buffer
                if let Some(active_buffer_id) = self.layout.active_buffer() {
                    if active_buffer_id == buffer_id {
                        // Get the current line to detect overlap
                        let cleaned_text = if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                            let cursor_pos = buffer.editor_state().cursor.position();
                            if let Some(current_line) = buffer.text_buffer().get_line(cursor_pos.line) {
                                // Get text from line start to cursor
                                let line_chars: Vec<char> = current_line.chars().collect();
                                let text_before_cursor: String = line_chars
                                    .iter()
                                    .take(cursor_pos.column)
                                    .collect();

                                // Strip overlapping prefix from suggestion
                                Self::strip_overlap(&text_before_cursor, &text)
                            } else {
                                text
                            }
                        } else {
                            text
                        };

                        // Store the cleaned suggestion
                        self.ai_suggestion = Some(cleaned_text);
                        self.ai_pending_request = false;
                    }
                }
            }
            AiResponse::Error(error) => {
                logger::log(&format!("AI completion error: {}", error));
                self.ai_pending_request = false;
            }
        }
    }

    /// Shutdown the AI manager
    pub fn shutdown_ai(&mut self) -> Result<()> {
        if let Some(manager) = &mut self.ai_manager {
            manager.shutdown()?;
        }
        Ok(())
    }

    /// Strip overlapping prefix from AI suggestion
    /// If suggestion starts with text already on the line, remove that prefix
    fn strip_overlap(text_before_cursor: &str, suggestion: &str) -> String {
        // Try to find the longest suffix of text_before_cursor that matches
        // a prefix of the suggestion
        let text_before = text_before_cursor.trim_end();

        // Try progressively shorter suffixes of the current text
        for start_idx in 0..text_before.len() {
            let suffix = &text_before[start_idx..];
            if suggestion.starts_with(suffix) {
                // Found overlap - strip it
                return suggestion[suffix.len()..].to_string();
            }
        }

        // No overlap found, return suggestion as-is
        suggestion.to_string()
    }

    /// Trigger an AI completion request for the current cursor position
    fn trigger_ai_completion(&mut self) -> Result<()> {
        use crate::ai::provider::CompletionRequest;
        use crate::ai::AiRequest;

        // Check if AI manager is available
        if self.ai_manager.is_none() {
            return Ok(());
        }

        // Get active buffer
        let buffer_id = match self.layout.active_buffer() {
            Some(id) => id,
            None => return Ok(()),
        };

        let buffer = match self.workspace.get_buffer(buffer_id) {
            Some(b) => b,
            None => return Ok(()),
        };

        // Get file path and language
        let file_path = buffer.file_path().cloned().unwrap_or_else(|| {
            std::path::PathBuf::from("untitled")
        });

        let language = if let Some(path) = buffer.file_path() {
            path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("txt")
                .to_string()
        } else {
            "txt".to_string()
        };

        // Get cursor position
        let cursor_pos = buffer.editor_state().cursor.position();

        // Get text before and after cursor
        let text_buffer = buffer.text_buffer();
        let full_text = text_buffer.to_string();

        // Calculate byte offset of cursor position
        let mut byte_offset = 0;
        for (line_idx, line) in full_text.lines().enumerate() {
            if line_idx < cursor_pos.line {
                byte_offset += line.len() + 1; // +1 for newline
            } else if line_idx == cursor_pos.line {
                byte_offset += cursor_pos.column.min(line.len());
                break;
            }
        }

        // Split text at cursor
        let (before, after) = full_text.split_at(byte_offset);
        let code_before_cursor = before.chars().rev().take(2000).collect::<String>()
            .chars().rev().collect::<String>(); // Take last 2000 chars
        let code_after_cursor = after.chars().take(500).collect::<String>(); // Take next 500 chars

        // Create completion request
        let request = CompletionRequest {
            file_path,
            language,
            code_before_cursor,
            code_after_cursor,
            cursor_position: cursor_pos,
        };

        // Send request to AI manager
        if let Some(manager) = &self.ai_manager {
            manager.request_completion(request, buffer_id)?;
        }

        Ok(())
    }

    /// Check debounce timer and trigger AI completion if needed
    pub fn check_ai_debounce(&mut self) -> Result<()> {
        // Skip if AI completions are disabled
        if !self.ai_completions_enabled {
            return Ok(());
        }

        if let Some(last_keystroke) = self.ai_last_keystroke {
            // Check if 150ms has elapsed since last keystroke
            if last_keystroke.elapsed() >= Duration::from_millis(150) && !self.ai_pending_request {
                // Trigger completion request
                self.trigger_ai_completion()?;
                self.ai_last_keystroke = None;
                self.ai_pending_request = true;
            }
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

    /// Check if an error is a permission denied error
    fn is_permission_denied(&self, error: &anyhow::Error) -> bool {
        if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
            return io_error.kind() == std::io::ErrorKind::PermissionDenied;
        }
        false
    }

    /// Get word at cursor position with its boundaries
    fn get_word_at_cursor(&self, buffer: &crate::workspace::Buffer) -> Option<(String, usize, usize)> {
        let cursor_pos = buffer.editor_state().cursor.position();
        let line_text = buffer.text_buffer().get_line(cursor_pos.line)?;
        let col = cursor_pos.column;

        if line_text.is_empty() || col >= line_text.chars().count() {
            return None;
        }

        let chars: Vec<char> = line_text.chars().collect();
        let cursor_char = chars.get(col)?;

        // Check if we're on a word character (alphanumeric or underscore)
        if !cursor_char.is_alphanumeric() && *cursor_char != '_' {
            return None;
        }

        // Find word start
        let mut start = col;
        while start > 0 {
            let ch = chars[start - 1];
            if !ch.is_alphanumeric() && ch != '_' {
                break;
            }
            start -= 1;
        }

        // Find word end
        let mut end = col + 1;
        while end < chars.len() {
            let ch = chars[end];
            if !ch.is_alphanumeric() && ch != '_' {
                break;
            }
            end += 1;
        }

        let word: String = chars[start..end].iter().collect();
        Some((word, start, end))
    }

    /// Toggle mark at cursor position - marks if not marked, unmarks if already marked
    fn toggle_word_mark_at_cursor(&mut self) -> Result<()> {
        let buffer_id = self.layout.active_buffer()
            .ok_or_else(|| anyhow::anyhow!("No active buffer"))?;
        let buffer = self.workspace.get_buffer(buffer_id)
            .ok_or_else(|| anyhow::anyhow!("Buffer not found"))?;

        let file_path = buffer.file_path()
            .ok_or_else(|| anyhow::anyhow!("Buffer has no file path"))?
            .clone();

        let cursor_pos = buffer.editor_state().cursor.position();

        // Check if this position is already marked
        if let Some(pos) = self.position_marks.iter().position(|mark| {
            mark.file_path == file_path &&
            mark.line == cursor_pos.line &&
            mark.column == cursor_pos.column
        }) {
            // Already marked - unmark it
            self.position_marks.remove(pos);
            // Adjust current_mark_index if needed
            if self.current_mark_index >= self.position_marks.len() && !self.position_marks.is_empty() {
                self.current_mark_index = self.position_marks.len() - 1;
            }
            self.message = Some(format!("Unmarked position ({} marks remaining)", self.position_marks.len()));
        } else {
            // Not marked - mark it
            self.position_marks.push(PositionMark {
                file_path,
                line: cursor_pos.line,
                column: cursor_pos.column,
            });
            self.message = Some(format!("Marked position ({} marks total)", self.position_marks.len()));
        }

        Ok(())
    }

    /// Mark word at cursor
    fn mark_word_at_cursor(&mut self) -> Result<()> {
        let buffer_id = self.layout.active_buffer()
            .ok_or_else(|| anyhow::anyhow!("No active buffer"))?;
        let buffer = self.workspace.get_buffer(buffer_id)
            .ok_or_else(|| anyhow::anyhow!("Buffer not found"))?;

        let file_path = buffer.file_path()
            .ok_or_else(|| anyhow::anyhow!("Buffer has no file path"))?
            .clone();

        let cursor_pos = buffer.editor_state().cursor.position();

        if let Some((word, start_col, _)) = self.get_word_at_cursor(buffer) {
            // Check if this word is already marked at this location
            let already_marked = self.position_marks.iter().any(|mark| {
                mark.file_path == file_path &&
                mark.line == cursor_pos.line &&
                mark.column == start_col
            });

            if already_marked {
                self.message = Some(format!("'{}' already marked", word));
            } else {
                self.position_marks.push(PositionMark {
                    file_path,
                    line: cursor_pos.line,
                    column: start_col,
                });
                self.message = Some(format!("Marked '{}' ({} marks total)", word, self.position_marks.len()));
            }
        } else {
            self.message = Some("No word at cursor".to_string());
        }

        Ok(())
    }

    /// Unmark word at cursor
    fn unmark_word_at_cursor(&mut self) -> Result<()> {
        let buffer_id = self.layout.active_buffer()
            .ok_or_else(|| anyhow::anyhow!("No active buffer"))?;
        let buffer = self.workspace.get_buffer(buffer_id)
            .ok_or_else(|| anyhow::anyhow!("Buffer not found"))?;

        let file_path = buffer.file_path()
            .ok_or_else(|| anyhow::anyhow!("Buffer has no file path"))?;

        let cursor_pos = buffer.editor_state().cursor.position();

        if let Some((word, start_col, _)) = self.get_word_at_cursor(buffer) {
            // Find and remove the mark at this location
            if let Some(pos) = self.position_marks.iter().position(|mark| {
                mark.file_path == *file_path &&
                mark.line == cursor_pos.line &&
                mark.column == start_col
            }) {
                self.position_marks.remove(pos);
                // Adjust current_mark_index if needed
                if self.current_mark_index >= self.position_marks.len() && !self.position_marks.is_empty() {
                    self.current_mark_index = self.position_marks.len() - 1;
                }
                self.message = Some(format!("Unmarked '{}' ({} marks remaining)", word, self.position_marks.len()));
            } else {
                self.message = Some(format!("'{}' is not marked at this location", word));
            }
        } else {
            self.message = Some("No word at cursor".to_string());
        }

        Ok(())
    }

    /// Cycle to next mark
    fn cycle_to_next_mark(&mut self) -> Result<()> {
        if self.position_marks.is_empty() {
            self.message = Some("No marks set".to_string());
            return Ok(());
        }

        // Move to next mark
        self.current_mark_index = (self.current_mark_index + 1) % self.position_marks.len();
        let mark = &self.position_marks[self.current_mark_index].clone();

        // Find the buffer with this file path, or open it
        let buffer_id = {
            let mut found_id = None;
            for id in self.workspace.buffer_ids() {
                if let Some(buffer) = self.workspace.get_buffer(id) {
                    if let Some(path) = buffer.file_path() {
                        if path == &mark.file_path {
                            found_id = Some(id);
                            break;
                        }
                    }
                }
            }

            if let Some(id) = found_id {
                id
            } else {
                // Open the file
                let result = self.workspace.open_file(mark.file_path.clone())?;
                result.buffer_id()
            }
        };

        // Switch to the buffer
        let pane = self.layout.active_pane();
        self.layout.set_buffer(pane, buffer_id);

        // Move cursor to the marked position
        if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
            let editor_state = buffer.editor_state_mut();
            editor_state.cursor.set_position(Position::new(mark.line, mark.column));
            editor_state.clear_selection();

            // Center viewport on cursor
            let (_, term_height) = crossterm::terminal::size()?;
            let visible_lines = term_height.saturating_sub(4) as usize; // Account for status bars
            let target_top = mark.line.saturating_sub(visible_lines / 2);
            editor_state.viewport.top_line = target_top;
        }

        self.message = Some(format!("Mark {}/{} in {}:{}:{}",
            self.current_mark_index + 1,
            self.position_marks.len(),
            mark.file_path.display(),
            mark.line + 1,
            mark.column + 1));

        Ok(())
    }

    /// Check if sudo save needs to be executed
    pub fn needs_sudo_save(&self) -> bool {
        self.execute_sudo_save_on_render
    }

    /// Execute sudo save operation
    pub fn execute_sudo_save(&mut self, terminal: &crate::render::Terminal) -> Result<()> {
        self.execute_sudo_save_on_render = false;

        let path = self.pending_sudo_save_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pending sudo save path"))?;
        let content = self.pending_sudo_save_content.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pending sudo save content"))?;

        // Suspend TUI
        terminal.cleanup()?;

        // Execute: echo "content" | sudo tee filepath > /dev/null
        let result = std::process::Command::new("sudo")
            .arg("tee")
            .arg(path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(content.as_bytes())?;
                }
                child.wait()
            });

        // Resume TUI
        let _ = crossterm::terminal::enable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::SetCursorStyle::SteadyBlock,
            crossterm::cursor::Show
        );

        // Handle result
        match result {
            Ok(status) if status.success() => {
                if let Some(buffer_id) = self.layout.active_buffer() {
                    if let Some(buffer) = self.workspace.get_buffer_mut(buffer_id) {
                        buffer.text_buffer_mut().set_modified(false);
                    }
                }
                self.message = Some("Saved with sudo".to_string());
                self.notify_lsp_did_save();
            }
            Ok(status) => {
                self.message = Some(format!("Sudo save failed: exit code {}",
                    status.code().unwrap_or(-1)));
            }
            Err(e) => {
                self.message = Some(format!("Sudo error: {}", e));
            }
        }

        // Clear pending state
        self.pending_sudo_save_path = None;
        self.pending_sudo_save_content = None;

        Ok(())
    }

    /// Save session state for the current project
    pub fn save_session_state(&self) -> Result<()> {
        // Only save if we have a project directory open
        if let Some(file_tree) = &self.file_tree {
            let project_root = file_tree.root().to_path_buf();

            // Collect open file paths
            let mut open_files = Vec::new();
            let buffer_ids = self.workspace.buffer_ids();

            for &buffer_id in &buffer_ids {
                if let Some(buffer) = self.workspace.get_buffer(buffer_id) {
                    if let Some(file_path) = buffer.file_path() {
                        open_files.push(file_path.clone());
                    }
                }
            }

            // Find the active buffer index
            let active_buffer_index = if let Some(active_id) = self.workspace.active_buffer_id() {
                buffer_ids.iter().position(|&id| id == active_id).unwrap_or(0)
            } else {
                0
            };

            // Create and save session state
            let session = crate::session::SessionState::new(
                project_root,
                open_files,
                active_buffer_index,
            );

            session.save()?;
        }

        Ok(())
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
