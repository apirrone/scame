use crate::buffer::TextBuffer;
use crate::editor::EditorState;
use crate::render::terminal::Terminal;
use anyhow::Result;
use crossterm::style::Color;

pub struct StatusBar;

impl StatusBar {
    /// Render the status bar at the bottom of the terminal
    pub fn render(
        terminal: &Terminal,
        buffer: &TextBuffer,
        state: &EditorState,
        message: Option<&str>,
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();
        let status_y = term_height.saturating_sub(1);

        terminal.move_cursor(0, status_y)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::White)?;

        // Build status line content
        let mut status = String::new();

        // File info
        if let Some(path) = buffer.file_path() {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unnamed");
            status.push_str(filename);
        } else {
            status.push_str("[No Name]");
        }

        if buffer.is_modified() {
            status.push_str(" [+]");
        }

        // Cursor position
        let position_info = format!(
            " {}:{} ",
            state.cursor.line + 1,
            state.cursor.column + 1
        );

        // Line count
        let line_info = format!("{} lines ", buffer.len_lines());

        // Calculate spacing
        let right_side = format!("{}{}", position_info, line_info);
        let spaces_needed = term_width as usize - status.len() - right_side.len();

        status.push_str(&" ".repeat(spaces_needed));
        status.push_str(&right_side);

        // Truncate if too long
        if status.len() > term_width as usize {
            status.truncate(term_width as usize);
        } else {
            // Pad to full width
            status.push_str(&" ".repeat(term_width as usize - status.len()));
        }

        terminal.print(&status)?;
        terminal.reset_color()?;

        // Show message below status bar if present
        if let Some(msg) = message {
            if term_height > status_y + 1 {
                terminal.move_cursor(0, status_y + 1)?;
                terminal.set_fg(Color::Yellow)?;
                let truncated_msg = if msg.len() > term_width as usize {
                    &msg[..term_width as usize]
                } else {
                    msg
                };
                terminal.print(truncated_msg)?;
                terminal.reset_color()?;
            }
        }

        terminal.flush()?;
        Ok(())
    }
}
