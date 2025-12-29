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
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();
        let line_number_width = if show_line_numbers {
            Self::calculate_line_number_width(buffer) + 1 // +1 for space
        } else {
            0
        };

        // Tab bar and path bar take 2 lines at top, status bar takes 1 line at bottom
        let tab_bar_height = 1;
        let path_bar_height = 1;
        let status_bar_height = 1;
        let top_bars_height = tab_bar_height + path_bar_height;
        let content_height = term_height.saturating_sub(top_bars_height + status_bar_height);

        // Render each visible line
        for screen_row in 0..content_height {
            let buffer_line = state.viewport.top_line + screen_row as usize;

            // Offset by tab bar and path bar height
            terminal.move_cursor(0, screen_row + top_bars_height)?;
            terminal.clear_line()?;

            if buffer_line >= buffer.len_lines() {
                // Empty line beyond buffer
                terminal.reset_color()?;
                terminal.set_fg(Color::DarkGrey)?;
                terminal.print("~")?;
                terminal.reset_color()?;
                continue;
            }

            // Check if this line has diagnostics
            let line_diagnostic = diagnostics.and_then(|diags| {
                diags.iter().find(|d| {
                    buffer_line >= d.range.0.line && buffer_line <= d.range.1.line
                })
            });

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
                    terminal.set_fg(Color::DarkGrey)?;
                    let line_num = format!("{:>width$} ", buffer_line + 1, width = (line_number_width - 1) as usize);
                    terminal.print(&line_num)?;
                    terminal.reset_color()?;
                }
            }

            // Get the line text
            if let Some(line) = buffer.get_line(buffer_line) {
                // Remove the trailing newline for display
                let line = line.trim_end_matches(&['\n', '\r'][..]);

                // Render the line with selection highlighting if applicable
                Self::render_line(terminal, line, buffer_line, state, line_number_width, buffer, highlight_spans, theme)?;
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
            Self::calculate_line_number_width(buffer) + 1
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
