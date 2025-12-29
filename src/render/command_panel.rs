use crate::app::Command;
use crate::render::terminal::Terminal;
use anyhow::Result;
use crossterm::style::Color;

/// Render the command panel UI (Ctrl+Shift+P)
pub struct CommandPanel;

impl CommandPanel {
    /// Render the command panel overlay
    pub fn render(
        terminal: &Terminal,
        pattern: &str,
        commands: &[Command],
        selected: usize,
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();

        // Calculate dimensions (centered, 80% width, max 20 lines)
        let width = (term_width as f32 * 0.8) as u16;
        let height = 20.min(commands.len() as u16 + 2);
        let x = (term_width - width) / 2;
        let y = (term_height - height) / 2;

        // Draw header
        terminal.move_cursor(x, y)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::White)?;
        terminal.print(&format!(" Command Palette: {} ", pattern))?;
        terminal.print(&" ".repeat((width as usize).saturating_sub(pattern.len() + 19)))?;
        terminal.reset_color()?;

        // Draw commands
        for (i, command) in commands.iter().take((height - 2) as usize).enumerate() {
            let row = y + 1 + i as u16;
            terminal.move_cursor(x, row)?;

            if i == selected {
                terminal.set_bg(Color::Blue)?;
                terminal.set_fg(Color::White)?;
            } else {
                terminal.set_bg(Color::Black)?;
                terminal.set_fg(Color::White)?;
            }

            // Format: "Command Name    Keybinding"
            let keybinding = command.keybinding.as_deref().unwrap_or("");
            let keybinding_len = keybinding.len();

            // Reserve space for keybinding on the right
            let max_name_len = (width as usize).saturating_sub(keybinding_len + 4);
            let display_name = if command.name.len() > max_name_len {
                format!("{}...", &command.name[..max_name_len.saturating_sub(3)])
            } else {
                command.name.clone()
            };

            // Calculate padding to right-align keybinding
            let padding = (width as usize)
                .saturating_sub(display_name.len() + keybinding_len + 2);

            terminal.print(&format!(" {}{}{}", display_name, " ".repeat(padding), keybinding))?;
            terminal.reset_color()?;
        }

        // Draw footer with help text
        terminal.move_cursor(x, y + height - 1)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::Grey)?;
        let footer = format!(" {} commands | ↑↓: Navigate | Enter: Execute | Esc: Cancel ", commands.len());
        let footer_display = if footer.len() > width as usize {
            format!(" {} commands ", commands.len())
        } else {
            footer
        };
        terminal.print(&footer_display)?;
        terminal.print(&" ".repeat((width as usize).saturating_sub(footer_display.len())))?;
        terminal.reset_color()?;

        terminal.flush()?;
        Ok(())
    }
}
