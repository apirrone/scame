use crate::buffer::TextBuffer;
use crate::editor::EditorState;
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
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();
        let line_number_width = if show_line_numbers {
            Self::calculate_line_number_width(buffer) + 1 // +1 for space
        } else {
            0
        };

        // Render each visible line
        for screen_row in 0..term_height.saturating_sub(1) {
            // -1 for status bar
            let buffer_line = state.viewport.top_line + screen_row as usize;

            terminal.move_cursor(0, screen_row)?;
            terminal.clear_line()?;

            if buffer_line >= buffer.len_lines() {
                // Empty line beyond buffer
                terminal.reset_color()?;
                terminal.set_fg(Color::DarkGrey)?;
                terminal.print("~")?;
                terminal.reset_color()?;
                continue;
            }

            // Render line number
            if show_line_numbers {
                terminal.set_fg(Color::DarkGrey)?;
                let line_num = format!("{:>width$} ", buffer_line + 1, width = (line_number_width - 1) as usize);
                terminal.print(&line_num)?;
                terminal.reset_color()?;
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

        // If no syntax highlighting, use simple rendering
        if highlight_spans.is_none() {
            if let Some((start_col, end_col)) = selection_range {
                // Render with selection only
                if start_col > 0 {
                    terminal.print(&line[..start_col.min(line.len())])?;
                }
                if end_col > start_col && start_col < line.len() {
                    terminal.set_bg(Color::Rgb { r: 51, g: 102, b: 153 })?; // Blue background
                    terminal.set_fg(Color::White)?;
                    terminal.print(&line[start_col..end_col.min(line.len())])?;
                    terminal.reset_color()?;
                }
                if end_col < line.len() {
                    terminal.print(&line[end_col..])?;
                }
            } else {
                terminal.print(line)?;
            }
            return Ok(());
        }

        // Render with syntax highlighting
        let spans = highlight_spans.unwrap();
        let mut current_pos = 0;
        let mut current_color: Option<Color> = None;

        // Filter spans that overlap with this line
        let line_spans: Vec<_> = spans
            .iter()
            .filter(|span| span.start_byte < line_end_byte && span.end_byte > line_start_byte)
            .collect();

        for byte_offset in 0..line.len() {
            let absolute_byte = line_start_byte + byte_offset;
            let is_selected = selection_range
                .map(|(start, end)| byte_offset >= start && byte_offset < end)
                .unwrap_or(false);

            // Find the span for this byte
            let token_color = if !is_selected {
                line_spans
                    .iter()
                    .find(|span| absolute_byte >= span.start_byte && absolute_byte < span.end_byte)
                    .map(|span| theme.color_for(span.token_type))
            } else {
                None
            };

            // If color changed, flush previous segment
            if token_color != current_color {
                if current_pos < byte_offset {
                    // Flush previous segment
                    if current_color.is_some() || selection_range.is_some() {
                        terminal.reset_color()?;
                    }
                    if let Some((start, end)) = selection_range {
                        if current_pos >= start && current_pos < end {
                            terminal.set_bg(Color::Rgb { r: 51, g: 102, b: 153 })?; // Blue background
                            terminal.set_fg(Color::White)?;
                        } else if let Some(color) = current_color {
                            terminal.set_fg(color)?;
                        }
                    } else if let Some(color) = current_color {
                        terminal.set_fg(color)?;
                    }
                    terminal.print(&line[current_pos..byte_offset])?;
                }
                current_color = token_color;
                current_pos = byte_offset;
            }
        }

        // Flush remaining segment
        if current_pos < line.len() {
            terminal.reset_color()?;
            if let Some((start, end)) = selection_range {
                if current_pos >= start && current_pos < end {
                    terminal.set_bg(Color::Rgb { r: 51, g: 102, b: 153 })?; // Blue background
                    terminal.set_fg(Color::White)?;
                } else if let Some(color) = current_color {
                    terminal.set_fg(color)?;
                }
            } else if let Some(color) = current_color {
                terminal.set_fg(color)?;
            }
            terminal.print(&line[current_pos..])?;
        }

        terminal.reset_color()?;
        Ok(())
    }

    fn render_cursor(
        terminal: &Terminal,
        state: &EditorState,
        line_number_width: u16,
    ) -> Result<()> {
        // Calculate screen position of cursor
        let screen_line = state.cursor.line.saturating_sub(state.viewport.top_line);
        let screen_col = state.cursor.column as u16 + line_number_width;

        // Just position the cursor, don't show it yet (done at app level)
        terminal.move_cursor(screen_col, screen_line as u16)?;

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
