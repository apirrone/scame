use crate::buffer::TextBuffer;
use crate::editor::EditorState;
use crate::render::terminal::Terminal;
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
                Self::render_line(terminal, line, buffer_line, state, line_number_width)?;
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
    ) -> Result<()> {
        // Check if this line has a selection
        if let Some(selection) = &state.selection {
            let (start, end) = selection.range();

            if line_num >= start.line && line_num <= end.line {
                // This line has selection
                let start_col = if line_num == start.line { start.column } else { 0 };
                let end_col = if line_num == end.line {
                    end.column
                } else {
                    line.len()
                };

                // Before selection
                if start_col > 0 {
                    terminal.print(&line[..start_col.min(line.len())])?;
                }

                // Selection
                if end_col > start_col && start_col < line.len() {
                    terminal.set_bg(Color::DarkGrey)?;
                    terminal.set_fg(Color::White)?;
                    let sel_end = end_col.min(line.len());
                    terminal.print(&line[start_col..sel_end])?;
                    terminal.reset_color()?;
                }

                // After selection
                if end_col < line.len() {
                    terminal.print(&line[end_col..])?;
                }

                return Ok(());
            }
        }

        // No selection, just print the line
        terminal.print(line)?;
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

        terminal.move_cursor(screen_col, screen_line as u16)?;
        terminal.show_cursor()?;

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
