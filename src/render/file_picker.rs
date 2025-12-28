use crate::render::terminal::Terminal;
use crate::search::FileSearchResult;
use anyhow::Result;
use crossterm::style::Color;

/// Render the file picker UI (Ctrl+P)
pub struct FilePicker;

impl FilePicker {
    /// Render the file picker overlay
    pub fn render(
        terminal: &Terminal,
        pattern: &str,
        results: &[FileSearchResult],
        selected: usize,
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();

        // Calculate dimensions (centered, 70% width, max 15 lines)
        let width = (term_width as f32 * 0.7) as u16;
        let height = 15.min(results.len() as u16 + 2);
        let x = (term_width - width) / 2;
        let y = (term_height - height) / 2;

        // Draw border
        terminal.move_cursor(x, y)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::White)?;
        terminal.print(&format!(" Find File: {} ", pattern))?;
        terminal.print(&" ".repeat((width as usize).saturating_sub(pattern.len() + 13)))?;
        terminal.reset_color()?;

        // Draw results
        for (i, result) in results.iter().take((height - 2) as usize).enumerate() {
            let row = y + 1 + i as u16;
            terminal.move_cursor(x, row)?;

            if i == selected {
                terminal.set_bg(Color::Blue)?;
                terminal.set_fg(Color::White)?;
            } else {
                terminal.set_bg(Color::Black)?;
                terminal.set_fg(Color::White)?;
            }

            // Truncate display path if too long
            let display = if result.display_path.len() > width as usize - 2 {
                format!("{}...", &result.display_path[..width as usize - 5])
            } else {
                result.display_path.clone()
            };

            terminal.print(&format!(" {}", display))?;
            terminal.print(&" ".repeat((width as usize).saturating_sub(display.len() + 1)))?;
            terminal.reset_color()?;
        }

        // Draw status line
        terminal.move_cursor(x, y + height - 1)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::Grey)?;
        let status = format!(" {} matches ", results.len());
        terminal.print(&status)?;
        terminal.print(&" ".repeat((width as usize).saturating_sub(status.len())))?;
        terminal.reset_color()?;

        terminal.flush()?;
        Ok(())
    }
}
