use crate::render::terminal::Terminal;
use crossterm::style::Color;
use crate::workspace::BufferId;
use anyhow::Result;

pub struct TabBar;

impl TabBar {
    /// Render tab bar at top of screen
    pub fn render(
        terminal: &Terminal,
        buffers: &[(BufferId, String, bool)], // (id, name, modified)
        active_buffer: BufferId,
    ) -> Result<()> {
        terminal.move_cursor(0, 0)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::White)?;

        let (term_width, _) = terminal.size();
        let mut x_pos = 0;

        for (id, name, modified) in buffers {
            // Highlight active tab
            if *id == active_buffer {
                terminal.set_bg(Color::Blue)?;
                terminal.set_fg(Color::White)?;
            } else {
                terminal.set_bg(Color::DarkGrey)?;
                terminal.set_fg(Color::Grey)?;
            }

            let modified_marker = if *modified { "*" } else { "" };
            let tab_text = format!(" {}{} ", name, modified_marker);

            if x_pos + tab_text.len() as u16 > term_width {
                break; // No more space for tabs
            }

            terminal.move_cursor(x_pos, 0)?;
            terminal.print(&tab_text)?;
            x_pos += tab_text.len() as u16;
        }

        // Fill remaining space
        if x_pos < term_width {
            terminal.move_cursor(x_pos, 0)?;
            terminal.print(&" ".repeat((term_width - x_pos) as usize))?;
        }

        terminal.reset_color()?;
        Ok(())
    }
}
