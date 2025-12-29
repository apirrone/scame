use crate::app::ProjectSearchResult;
use crate::render::terminal::Terminal;
use anyhow::Result;
use crossterm::style::Color;

/// Render the project search UI (Ctrl+X Ctrl+F)
pub struct ProjectSearch;

impl ProjectSearch {
    /// Render the project search overlay
    pub fn render(
        terminal: &Terminal,
        pattern: &str,
        results: &[ProjectSearchResult],
        selected: usize,
    ) -> Result<()> {
        let (term_width, term_height) = terminal.size();

        // Calculate dimensions (centered, 90% width, 70% height)
        let width = (term_width as f32 * 0.9) as u16;
        let height = ((term_height as f32 * 0.7) as u16).min(results.len() as u16 + 2);
        let x = (term_width - width) / 2;
        let y = (term_height - height) / 2;

        // Draw header
        terminal.move_cursor(x, y)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::White)?;
        terminal.print(&format!(" Project Search: {} ", pattern))?;
        terminal.print(&" ".repeat((width as usize).saturating_sub(pattern.len() + 18)))?;
        terminal.reset_color()?;

        // Calculate scroll offset to keep selected item visible
        let visible_lines = (height - 2) as usize; // -2 for header and footer
        let scroll_offset = if selected >= visible_lines {
            selected - visible_lines + 1
        } else {
            0
        };

        // Draw results with scrolling
        for (i, result) in results.iter()
            .skip(scroll_offset)
            .take(visible_lines)
            .enumerate()
        {
            let result_index = scroll_offset + i;
            let row = y + 1 + i as u16;
            terminal.move_cursor(x, row)?;

            let bg_color = if result_index == selected { Color::Blue } else { Color::Black };
            let fg_color = if result_index == selected { Color::White } else { Color::White };

            terminal.set_bg(bg_color)?;
            terminal.set_fg(fg_color)?;

            // Format: "filename:line: content"
            let filename = result.file_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("?");
            let line_num = result.line_number + 1;
            let prefix = format!(" {}:{}:  ", filename, line_num);

            // Print prefix
            terminal.print(&prefix)?;

            // Calculate remaining space for content
            let max_content_len = (width as usize).saturating_sub(prefix.len());

            // Use original line content (don't trim to preserve match positions)
            let content = &result.line_content;

            // Split content into before match, match, and after match
            let match_start = result.match_start;
            let match_end = result.match_end;

            // Handle case where content needs truncation
            if content.len() > max_content_len {
                // Try to center the match in the visible area
                let context = 20;
                let start = match_start.saturating_sub(context).min(content.len().saturating_sub(max_content_len));
                let end = (start + max_content_len).min(content.len());

                // Safe slicing for visible content
                if let Some(visible_content) = content.get(start..end) {
                    let adjusted_match_start = match_start.saturating_sub(start);
                    let adjusted_match_end = match_end.saturating_sub(start).min(visible_content.len());

                    // Print before match (safe slicing)
                    if adjusted_match_start > 0 {
                        if let Some(before) = visible_content.get(..adjusted_match_start) {
                            terminal.print(before)?;
                        }
                    }

                    // Print match (highlighted in yellow, safe slicing)
                    if adjusted_match_start < visible_content.len() {
                        terminal.set_fg(Color::Yellow)?;
                        if let Some(matched) = visible_content.get(adjusted_match_start..adjusted_match_end) {
                            terminal.print(matched)?;
                        }
                        terminal.set_fg(fg_color)?;
                    }

                    // Print after match (safe slicing)
                    if adjusted_match_end < visible_content.len() {
                        if let Some(after) = visible_content.get(adjusted_match_end..) {
                            terminal.print(after)?;
                        }
                    }

                    terminal.print("...")?;
                } else {
                    // If slicing failed, just print the content without highlighting
                    terminal.print(content)?;
                }
            } else {
                // Full content fits - print with highlighting (safe slicing)
                // Print before match
                if match_start > 0 && match_start <= content.len() {
                    if let Some(before) = content.get(..match_start) {
                        terminal.print(before)?;
                    }
                }

                // Print match (highlighted in yellow)
                if match_start < content.len() && match_end <= content.len() {
                    terminal.set_fg(Color::Yellow)?;
                    if let Some(matched) = content.get(match_start..match_end) {
                        terminal.print(matched)?;
                    }
                    terminal.set_fg(fg_color)?;
                }

                // Print after match
                if match_end < content.len() {
                    if let Some(after) = content.get(match_end..) {
                        terminal.print(after)?;
                    }
                }

                // Fill remaining space
                let printed_len = prefix.len() + content.len();
                if printed_len < width as usize {
                    terminal.print(&" ".repeat((width as usize) - printed_len))?;
                }
            }

            terminal.reset_color()?;
        }

        // Fill empty lines if results don't fill the visible area
        let results_shown = results.len().saturating_sub(scroll_offset).min(visible_lines);
        for i in results_shown..visible_lines {
            let row = y + 1 + i as u16;
            terminal.move_cursor(x, row)?;
            terminal.set_bg(Color::Black)?;
            terminal.print(&" ".repeat(width as usize))?;
            terminal.reset_color()?;
        }

        // Draw footer with help text and position indicator
        terminal.move_cursor(x, y + height - 1)?;
        terminal.set_bg(Color::DarkGrey)?;
        terminal.set_fg(Color::Grey)?;

        let position_info = if !results.is_empty() {
            format!("{}/{}", selected + 1, results.len())
        } else {
            "0/0".to_string()
        };

        let footer = if results.len() > 1000 {
            format!(" 1000+ results ({}) | ↑↓: Navigate | Enter: Open | Esc: Cancel ", position_info)
        } else if results.is_empty() {
            " No results | Type to search | Esc: Cancel ".to_string()
        } else {
            format!(" {} results ({}) | ↑↓: Navigate | Enter: Open | Esc: Cancel ", results.len(), position_info)
        };

        let footer_display = if footer.len() > width as usize {
            if results.is_empty() {
                " No results ".to_string()
            } else {
                format!(" {} ({}) ", results.len(), position_info)
            }
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
