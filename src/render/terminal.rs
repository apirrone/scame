use anyhow::Result;
use crossterm::{
    cursor::{self, SetCursorStyle},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute, queue,
    style::{self, Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Write};

pub struct Terminal {
    width: u16,
    height: u16,
}

impl Terminal {
    /// Initialize the terminal
    pub fn new() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            Clear(ClearType::All),
            SetCursorStyle::SteadyBlock,
            cursor::Show
        )?;

        let (width, height) = terminal::size()?;

        Ok(Self { width, height })
    }

    /// Get terminal dimensions
    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Update terminal size (call on resize events)
    pub fn resize(&mut self) -> Result<()> {
        let (width, height) = terminal::size()?;
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Clear the entire screen
    pub fn clear(&self) -> Result<()> {
        let mut stdout = io::stdout();
        execute!(stdout, Clear(ClearType::All))?;
        Ok(())
    }

    /// Clear from cursor to end of line
    pub fn clear_line(&self) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, Clear(ClearType::UntilNewLine))?;
        Ok(())
    }

    /// Move cursor to position
    pub fn move_cursor(&self, x: u16, y: u16) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, cursor::MoveTo(x, y))?;
        Ok(())
    }

    /// Show cursor
    pub fn show_cursor(&self) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, cursor::Show)?;
        Ok(())
    }

    /// Hide cursor
    pub fn hide_cursor(&self) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, cursor::Hide)?;
        Ok(())
    }

    /// Set foreground color
    pub fn set_fg(&self, color: Color) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, SetForegroundColor(color))?;
        Ok(())
    }

    /// Set background color
    pub fn set_bg(&self, color: Color) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, SetBackgroundColor(color))?;
        Ok(())
    }

    /// Reset colors to default
    pub fn reset_color(&self) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, style::ResetColor)?;
        Ok(())
    }

    /// Print text at current cursor position
    pub fn print(&self, text: &str) -> Result<()> {
        let mut stdout = io::stdout();
        queue!(stdout, Print(text))?;
        Ok(())
    }

    /// Print text at specific position
    pub fn print_at(&self, x: u16, y: u16, text: &str) -> Result<()> {
        self.move_cursor(x, y)?;
        self.print(text)?;
        Ok(())
    }

    /// Flush the output
    pub fn flush(&self) -> Result<()> {
        io::stdout().flush()?;
        Ok(())
    }

    /// Cleanup terminal on exit
    pub fn cleanup(&self) -> Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            cursor::Show,
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
