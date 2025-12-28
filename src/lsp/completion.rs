use crate::lsp::protocol::{CompletionItem, CompletionItemKind};
use crate::render::terminal::Terminal;
use anyhow::Result;
use crossterm::style::Color;

/// Completion popup UI
pub struct CompletionPopup;

impl CompletionPopup {
    /// Render the completion popup near the cursor
    pub fn render(
        terminal: &Terminal,
        items: &[CompletionItem],
        selected: usize,
        cursor_screen_pos: (u16, u16),
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let (term_width, term_height) = terminal.size();
        let (cursor_x, cursor_y) = cursor_screen_pos;

        // Calculate popup dimensions
        let max_items = 10.min(items.len());
        let popup_height = max_items as u16;
        let popup_width = 40.min(term_width - 2);

        // Position popup below cursor, or above if not enough space
        let popup_y = if cursor_y + popup_height + 1 < term_height {
            cursor_y + 1
        } else {
            cursor_y.saturating_sub(popup_height)
        };

        let popup_x = cursor_x.min(term_width.saturating_sub(popup_width));

        // Render each item
        for (i, item) in items.iter().take(max_items).enumerate() {
            let y = popup_y + i as u16;
            terminal.move_cursor(popup_x, y)?;

            // Set background for selected item
            if i == selected {
                terminal.set_bg(Color::DarkBlue)?;
                terminal.set_fg(Color::White)?;
            } else {
                terminal.set_bg(Color::DarkGrey)?;
                terminal.set_fg(Color::White)?;
            }

            // Icon based on kind
            let icon = Self::icon_for_kind(item.kind);
            let mut display = format!("{} {}", icon, item.label);

            // Add detail if available
            if let Some(detail) = &item.detail {
                let detail_str = format!(" : {}", detail);
                if display.len() + detail_str.len() < popup_width as usize {
                    display.push_str(&detail_str);
                }
            }

            // Truncate and pad to popup width
            if display.len() > popup_width as usize {
                display.truncate(popup_width as usize - 3);
                display.push_str("...");
            } else {
                display.push_str(&" ".repeat(popup_width as usize - display.len()));
            }

            terminal.print(&display)?;
            terminal.reset_color()?;
        }

        Ok(())
    }

    /// Get icon character for completion item kind
    fn icon_for_kind(kind: Option<CompletionItemKind>) -> &'static str {
        match kind {
            Some(CompletionItemKind::Function) => "ƒ",
            Some(CompletionItemKind::Method) => "m",
            Some(CompletionItemKind::Variable) => "v",
            Some(CompletionItemKind::Field) => "f",
            Some(CompletionItemKind::Keyword) => "k",
            Some(CompletionItemKind::Module) => "M",
            Some(CompletionItemKind::Struct) => "S",
            Some(CompletionItemKind::Enum) => "E",
            Some(CompletionItemKind::Interface) => "I",
            Some(CompletionItemKind::Constant) => "C",
            Some(CompletionItemKind::Other) | None => "•",
        }
    }
}
