use crate::buffer::TextBuffer;
use crate::editor::EditorState;
use crate::lsp::{Diagnostic, DiagnosticSeverity};
use crate::render::terminal::Terminal;
use crate::syntax::{HighlightSpan, Theme};
use anyhow::Result;
use crossterm::style::Color;

pub struct BufferView;

impl BufferView {
    /// Render the text buffer to the terminal
    pub fn render(
        terminal: &Terminal,
        buffer: &TextBuffer,
        state: &EditorState,
        show_line_numbers: bool,
        highlight_spans: Option<&[HighlightSpan]>,
        theme: &Theme,
        diagnostics: Option<&[Diagnostic]>,
        show_diagnostics: bool,
        ai_suggestion: Option<&String>,
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();
        let line_number_width = if show_line_numbers {
            Self::calculate_line_number_width(buffer) + 2 // +1 for diagnostic marker space, +1 for trailing space
        } else {
            0
        };

        // Tab bar and path bar take 2 lines at top, status bar takes 1 line at bottom
        let tab_bar_height = 1;
        let path_bar_height = 1;
        let status_bar_height = 1;
        let top_bars_height = tab_bar_height + path_bar_height;
        let content_height = term_height.saturating_sub(top_bars_height + status_bar_height);

        // Parse AI suggestion into lines if present
        let ai_suggestion_lines: Option<Vec<&str>> = ai_suggestion.map(|s| s.lines().collect());

        // Render each visible line
        for screen_row in 0..content_height {
            let buffer_line = state.viewport.top_line + screen_row as usize;

            // Offset by tab bar and path bar height
            terminal.move_cursor(0, screen_row + top_bars_height)?;
            terminal.clear_line()?;

            // Check if this is a ghost line (AI suggestion continuation beyond buffer)
            let is_ghost_line = buffer_line >= buffer.len_lines()
                && ai_suggestion_lines.is_some()
                && buffer_line >= state.cursor.line
                && buffer_line < state.cursor.line + ai_suggestion_lines.as_ref().unwrap().len();

            if buffer_line >= buffer.len_lines() && !is_ghost_line {
                // Empty line beyond buffer (and not an AI ghost line)
                terminal.reset_color()?;
                terminal.set_fg(Color::DarkGrey)?;
                terminal.print("~")?;
                terminal.reset_color()?;
                continue;
            }

            // Check if this line has diagnostics
            let line_diagnostic = if show_diagnostics && !is_ghost_line {
                diagnostics.and_then(|diags| {
                    diags.iter().find(|d| {
                        buffer_line >= d.range.0.line && buffer_line <= d.range.1.line
                    })
                })
            } else {
                None
            };

            // Render line number with diagnostic indicator
            if show_line_numbers {
                // Show diagnostic marker if present
                if let Some(diag) = line_diagnostic {
                    let (marker, color) = match diag.severity {
                        DiagnosticSeverity::Error => ("●", Color::Red),
                        DiagnosticSeverity::Warning => ("●", Color::Yellow),
                        DiagnosticSeverity::Information => ("●", Color::Blue),
                        DiagnosticSeverity::Hint => ("●", Color::Cyan),
                    };
                    terminal.set_fg(color)?;
                    terminal.print(marker)?;
                    terminal.reset_color()?;
                    terminal.set_fg(Color::DarkGrey)?;
                    let line_num = format!("{:>width$}", buffer_line + 1, width = (line_number_width - 2) as usize);
                    terminal.print(&line_num)?;
                    terminal.print(" ")?;
                    terminal.reset_color()?;
                } else {
                    // No diagnostic - print space for marker, then line number
                    terminal.print(" ")?; // Space where marker would be
                    terminal.set_fg(Color::DarkGrey)?;
                    let line_num = format!("{:>width$} ", buffer_line + 1, width = (line_number_width - 2) as usize);
                    terminal.print(&line_num)?;
                    terminal.reset_color()?;
                }
            }

            // Render ghost line (AI suggestion beyond buffer)
            if is_ghost_line {
                // Line index starts from 1 because line 0 is shown on cursor line
                let suggestion_line_idx = buffer_line - state.cursor.line;
                if let Some(ref lines) = ai_suggestion_lines {
                    if suggestion_line_idx < lines.len() {
                        terminal.set_fg(Color::DarkGrey)?;
                        terminal.print(lines[suggestion_line_idx])?;
                        terminal.reset_color()?;
                    }
                }
                continue;
            }

            // Get the line text
            if let Some(line) = buffer.get_line(buffer_line) {
                // Remove the trailing newline for display
                let line = line.trim_end_matches(&['\n', '\r'][..]);

                // Only show AI suggestion on the cursor line (first line of suggestion)
                // Additional lines are shown as ghost lines beyond the buffer
                let ai_line_to_show = if buffer_line == state.cursor.line && ai_suggestion_lines.is_some() {
                    Some(ai_suggestion_lines.as_ref().unwrap()[0])
                } else {
                    None
                };

                let show_ai_on_cursor_line = buffer_line == state.cursor.line;

                // Render the line with selection highlighting if applicable
                Self::render_line(terminal, line, buffer_line, state, line_number_width, buffer, highlight_spans, theme, ai_line_to_show, show_ai_on_cursor_line)?;
            }
        }

        Ok(())
    }

    /// Position and show the cursor (call this AFTER status bar rendering)
    pub fn position_cursor(
        terminal: &Terminal,
        state: &EditorState,
        show_line_numbers: bool,
        buffer: &TextBuffer,
    ) -> Result<()> {
        let line_number_width = if show_line_numbers {
            Self::calculate_line_number_width(buffer) + 2 // +1 for diagnostic marker space, +1 for trailing space
        } else {
            0
        };
        Self::render_cursor(terminal, state, line_number_width)?;
        Ok(())
    }

    fn render_line(
        terminal: &Terminal,
        line: &str,
        line_num: usize,
        state: &EditorState,
        _line_number_width: u16,
        buffer: &TextBuffer,
        highlight_spans: Option<&[HighlightSpan]>,
        theme: &Theme,
        ai_line_to_show: Option<&str>,
        show_ai_on_cursor_line: bool,
    ) -> Result<()> {
        // Calculate byte offset for this line in the buffer
        let line_start_byte = buffer.line_to_byte(line_num);
        let line_end_byte = line_start_byte + line.len();

        // Get selection range for this line
        let selection_range = if let Some(selection) = &state.selection {
            let (start, end) = selection.range();
            if line_num >= start.line && line_num <= end.line {
                let start_col = if line_num == start.line { start.column } else { 0 };
                let end_col = if line_num == end.line { end.column } else { line.len() };
                Some((start_col, end_col))
            } else {
                None
            }
        } else {
            None
        };

        // Selection colors - use bright cyan background with black text for maximum contrast
        let selection_bg = Color::Rgb { r: 100, g: 180, b: 255 }; // Bright blue
        let selection_fg = Color::Black;

        // If no syntax highlighting, use simple rendering
        if highlight_spans.is_none() {
            if let Some((start_col, end_col)) = selection_range {
                // Render with selection - need to work character-by-character
                // because start_col/end_col are character positions, not byte offsets
                let chars: Vec<char> = line.chars().collect();

                for (col_idx, &ch) in chars.iter().enumerate() {
                    if col_idx >= start_col && col_idx < end_col {
                        terminal.set_bg(selection_bg)?;
                        terminal.set_fg(selection_fg)?;
                        terminal.print(&ch.to_string())?;
                        terminal.reset_color()?;
                    } else {
                        terminal.print(&ch.to_string())?;
                    }
                }
            } else {
                terminal.print(line)?;
            }
            return Ok(());
        }

        // Render with syntax highlighting
        let spans = highlight_spans.unwrap();

        // Filter spans that overlap with this line
        let line_spans: Vec<_> = spans
            .iter()
            .filter(|span| span.start_byte < line_end_byte && span.end_byte > line_start_byte)
            .collect();

        // Simple batching: group consecutive selected/non-selected characters
        let chars: Vec<char> = line.chars().collect();

        if chars.is_empty() {
            terminal.reset_color()?;
            return Ok(());
        }

        let mut char_idx = 0;
        let mut col_idx = 0;

        while col_idx < chars.len() {
            let absolute_byte = line_start_byte + char_idx;

            // Check if current character is selected
            let is_selected = selection_range
                .map(|(start, end)| col_idx >= start && col_idx < end)
                .unwrap_or(false);

            if is_selected {
                // Find the end of the selected region
                let mut end_col = col_idx;
                while end_col < chars.len() {
                    let is_still_selected = selection_range
                        .map(|(start, end)| end_col >= start && end_col < end)
                        .unwrap_or(false);
                    if !is_still_selected {
                        break;
                    }
                    end_col += 1;
                }

                // Render the entire selected region at once
                let selected_text: String = chars[col_idx..end_col].iter().collect();
                terminal.set_bg(selection_bg)?;
                terminal.set_fg(selection_fg)?;
                terminal.print(&selected_text)?;
                terminal.reset_color()?;

                // Advance
                for i in col_idx..end_col {
                    char_idx += chars[i].len_utf8();
                }
                col_idx = end_col;
            } else {
                // Non-selected character - just print with syntax color if available
                let ch = chars[col_idx];
                let token_color = line_spans
                    .iter()
                    .find(|span| absolute_byte >= span.start_byte && absolute_byte < span.end_byte)
                    .map(|span| theme.color_for(span.token_type));

                if let Some(color) = token_color {
                    terminal.set_fg(color)?;
                    terminal.print(&ch.to_string())?;
                    terminal.reset_color()?;
                } else {
                    terminal.print(&ch.to_string())?;
                }

                char_idx += ch.len_utf8();
                col_idx += 1;
            }
        }

        // Render AI suggestion as ghost text (gray text after cursor)
        // This only happens on the cursor line, and only if cursor is at the end
        if let Some(suggestion_line) = ai_line_to_show {
            let cursor_col = state.cursor.column;
            if show_ai_on_cursor_line && cursor_col >= line.chars().count() {
                terminal.set_fg(Color::DarkGrey)?;
                terminal.print(suggestion_line)?;
                terminal.reset_color()?;
            }
        }

        terminal.reset_color()?;
        Ok(())
    }

    fn render_cursor(
        terminal: &Terminal,
        state: &EditorState,
        line_number_width: u16,
    ) -> Result<()> {
        let (_, term_height) = terminal.size();

        // Calculate screen position of cursor
        let screen_line = state.cursor.line.saturating_sub(state.viewport.top_line);
        let screen_col = state.cursor.column as u16 + line_number_width;

        // Account for tab bar and path bar at top (2 lines offset)
        let tab_bar_height = 1;
        let path_bar_height = 1;
        let status_bar_height = 1;
        let top_bars_height = tab_bar_height + path_bar_height;
        let content_height = term_height.saturating_sub(top_bars_height + status_bar_height);
        let screen_y = screen_line as u16 + top_bars_height;

        // Only position cursor if it's within the content area (not in status bar)
        if screen_line < content_height as usize {
            terminal.move_cursor(screen_col, screen_y)?;
        }

        Ok(())
    }

    fn calculate_line_number_width(buffer: &TextBuffer) -> u16 {
        let line_count = buffer.len_lines();
        let digits = if line_count == 0 {
            1
        } else {
            (line_count as f64).log10().floor() as u16 + 1
        };
        digits.max(3) // Minimum width of 3
    }
}
