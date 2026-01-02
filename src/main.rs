mod ai;
mod app;
mod buffer;
mod diff;
mod editor;
mod logger;
mod render;
mod session;

// Stub modules for future implementation
mod backup;
mod commands;
mod config;
mod input;
mod lsp;
mod search;
mod syntax;
mod workspace;

use app::{poll_event, App, ControlFlow};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Color;
use diff::DiffView;
use render::Terminal;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Initialize logger
    logger::init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    // Check for --version flag
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-v") {
        println!("scame {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Check for --diff flag
    if args.len() > 1 && args[1] == "--diff" {
        if args.len() < 4 {
            eprintln!("Usage: scame --diff <file1> <file2>");
            std::process::exit(1);
        }
        let file1 = PathBuf::from(&args[2]);
        let file2 = PathBuf::from(&args[3]);

        if !file1.exists() {
            eprintln!("Error: File does not exist: {}", args[2]);
            std::process::exit(1);
        }
        if !file2.exists() {
            eprintln!("Error: File does not exist: {}", args[3]);
            std::process::exit(1);
        }

        return run_diff_mode(file1, file2);
    }

    // Create Tokio runtime for LSP background tasks
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    // Create app instance
    let mut app = if args.len() > 1 {
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            App::from_path(path)?
        } else {
            eprintln!("Error: Path does not exist: {}", args[1]);
            std::process::exit(1);
        }
    } else {
        App::new()?
    };

    // Initialize LSP
    app.initialize_lsp()?;

    // Initialize AI completion
    app.initialize_ai()?;

    // Initialize terminal
    let mut terminal = Terminal::new()?;

    // Initial render
    app.render(&terminal)?;

    // Main event loop
    loop {
        // Poll for LSP messages (non-blocking)
        let had_lsp_updates = app.poll_lsp_messages();

        // Poll for AI completion messages (non-blocking)
        let had_ai_updates = app.poll_ai_messages();

        // Check AI debounce timer and trigger completion if needed
        app.check_ai_debounce()?;

        // Only render if LSP or AI had updates
        if had_lsp_updates || had_ai_updates {
            app.render(&terminal)?;
        }

        // Handle input - wait for events (no timeout = blocks until event)
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            // Handle resize events specially to update terminal dimensions
            if let crossterm::event::Event::Resize(_width, _height) = event {
                terminal.resize()?;
            }

            match app.handle_event(event)? {
                ControlFlow::Continue => {
                    // Execute sudo save if needed
                    if app.needs_sudo_save() {
                        app.execute_sudo_save(&terminal)?;
                    }

                    // Render after handling input
                    app.render(&terminal)?;
                }
                ControlFlow::Exit => break,
            }
        }
    }

    // Save session state
    if let Err(e) = app.save_session_state() {
        eprintln!("Warning: Failed to save session state: {}", e);
    }

    // Shutdown LSP
    app.shutdown_lsp()?;

    // Shutdown AI
    app.shutdown_ai()?;

    // Cleanup
    terminal.cleanup()?;
    logger::close();

    Ok(())
}

/// Run the diff viewer mode
fn run_diff_mode(left_path: PathBuf, right_path: PathBuf) -> anyhow::Result<()> {
    use crate::syntax::{Highlighter, SupportedLanguage};

    // Load the diff
    let mut diff_view = DiffView::new(left_path, right_path)?;

    // Initialize syntax highlighter if supported
    let mut highlighter = Highlighter::new();
    let highlight_data = if diff_view.supports_syntax_highlighting() {
        let lang = SupportedLanguage::from_path(&diff_view.left_path);
        if let Some(language) = lang {
            highlighter.set_language(&language.language()).ok();
            let left_query = language.query().ok();
            let right_query = language.query().ok();
            let left_capture_names = language.capture_names().ok();
            let right_capture_names = language.capture_names().ok();

            // Get highlight spans for both files
            let left_spans = if let (Some(query), Some(capture_names)) = (&left_query, &left_capture_names) {
                highlighter.highlight(&diff_view.left_content, "left", query, capture_names).ok()
            } else {
                None
            };

            let right_spans = if let (Some(query), Some(capture_names)) = (&right_query, &right_capture_names) {
                highlighter.highlight(&diff_view.right_content, "right", query, capture_names).ok()
            } else {
                None
            };

            Some((left_spans, right_spans))
        } else {
            None
        }
    } else {
        None
    };

    // Initialize terminal
    let terminal = Terminal::new()?;

    // Initial render
    render_diff(&terminal, &diff_view, &highlighter, &highlight_data)?;

    // Event loop
    loop {
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match event {
                Event::Key(key_event) => {
                    if handle_diff_key(&terminal, &mut diff_view, key_event, &highlighter, &highlight_data)? {
                        break; // Exit requested
                    }
                }
                Event::Resize(_width, _height) => {
                    render_diff(&terminal, &diff_view, &highlighter, &highlight_data)?;
                }
                _ => {}
            }
        }
    }

    // Cleanup
    terminal.cleanup()?;
    logger::close();

    Ok(())
}

/// Handle keyboard input in diff mode
fn handle_diff_key(
    terminal: &Terminal,
    diff_view: &mut DiffView,
    key: KeyEvent,
    highlighter: &syntax::Highlighter,
    highlight_data: &Option<(Option<Vec<syntax::HighlightSpan>>, Option<Vec<syntax::HighlightSpan>>)>,
) -> anyhow::Result<bool> {
    let (_, term_height) = terminal.size();
    let visible_height = term_height.saturating_sub(3) as usize; // Leave space for header/footer

    match (key.code, key.modifiers) {
        // Quit
        (KeyCode::Char('q'), KeyModifiers::NONE)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL)
        | (KeyCode::Esc, KeyModifiers::NONE) => {
            return Ok(true);
        }

        // Scroll up
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
            diff_view.scroll_up(1);
            render_diff(terminal, diff_view, highlighter, highlight_data)?;
        }

        // Scroll down
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
            diff_view.scroll_down(1, visible_height);
            render_diff(terminal, diff_view, highlighter, highlight_data)?;
        }

        // Page up
        (KeyCode::PageUp, _) => {
            diff_view.scroll_up(visible_height);
            render_diff(terminal, diff_view, highlighter, highlight_data)?;
        }

        // Page down
        (KeyCode::PageDown, _) => {
            diff_view.scroll_down(visible_height, visible_height);
            render_diff(terminal, diff_view, highlighter, highlight_data)?;
        }

        _ => {}
    }

    Ok(false)
}

/// Render the diff view
fn render_diff(
    terminal: &Terminal,
    diff_view: &DiffView,
    highlighter: &syntax::Highlighter,
    highlight_data: &Option<(Option<Vec<syntax::HighlightSpan>>, Option<Vec<syntax::HighlightSpan>>)>,
) -> anyhow::Result<()> {
    use crossterm::execute;
    use crossterm::terminal::{Clear, ClearType};
    use std::io::stdout;

    let (term_width, term_height) = terminal.size();

    // Clear screen
    execute!(stdout(), Clear(ClearType::All))?;

    // Calculate pane widths (50/50 split with a divider)
    let pane_width = (term_width / 2).saturating_sub(1);

    // Render header (file names)
    terminal.move_cursor(0, 0)?;
    terminal.set_bg(Color::DarkGrey)?;
    terminal.set_fg(Color::White)?;

    // Left file name
    let left_name = diff_view
        .left_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("(unknown)");
    let left_header = format!("{:width$}", left_name, width = pane_width as usize);
    terminal.print(&left_header)?;

    // Divider
    terminal.print("│")?;

    // Right file name
    let right_name = diff_view
        .right_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("(unknown)");
    let right_header = format!("{:width$}", right_name, width = pane_width as usize);
    terminal.print(&right_header)?;
    terminal.reset_color()?;

    // Calculate visible content area
    let content_height = term_height.saturating_sub(2) as usize; // Header + status bar
    let (left_lines, right_lines) = diff_view.visible_lines(content_height);

    // Extract highlight spans if available
    let (left_spans, right_spans) = match highlight_data {
        Some((left, right)) => (left.as_deref(), right.as_deref()),
        None => (None, None),
    };

    // Render diff lines
    for (idx, (left_line, right_line)) in left_lines.iter().zip(right_lines.iter()).enumerate() {
        let screen_row = idx as u16 + 1;

        // Render left pane
        terminal.move_cursor(0, screen_row)?;
        render_diff_line(terminal, left_line, pane_width, left_spans, highlighter.theme())?;

        // Render divider
        terminal.move_cursor(pane_width, screen_row)?;
        terminal.set_fg(Color::DarkGrey)?;
        terminal.print("│")?;
        terminal.reset_color()?;

        // Render right pane
        terminal.move_cursor(pane_width + 1, screen_row)?;
        render_diff_line(terminal, right_line, pane_width, right_spans, highlighter.theme())?;
    }

    // Render status bar
    let status_row = term_height - 1;
    terminal.move_cursor(0, status_row)?;
    terminal.set_bg(Color::DarkGrey)?;
    terminal.set_fg(Color::White)?;
    let status = format!(
        " Line {}/{} | q: quit | ↑↓/jk: scroll | PgUp/PgDn: page ",
        diff_view.scroll_offset + 1,
        diff_view.left_lines.len()
    );
    terminal.print(&format!("{:width$}", status, width = term_width as usize))?;
    terminal.reset_color()?;

    terminal.flush()?;

    Ok(())
}

/// Render a single diff line with syntax highlighting
fn render_diff_line(
    terminal: &Terminal,
    line: &diff::DiffLine,
    width: u16,
    highlight_spans: Option<&[syntax::HighlightSpan]>,
    theme: &syntax::Theme,
) -> anyhow::Result<()> {
    use diff::ChangeType;

    // Set background color based on change type
    let bg_color = line.change_type.bg_color();
    terminal.set_bg(bg_color)?;

    // Render line number (without syntax highlighting)
    if let Some(line_num) = line.old_line_num.or(line.new_line_num) {
        terminal.set_fg(Color::DarkGrey)?;
        terminal.print(&format!("{:>4} ", line_num))?;
    } else {
        terminal.print("     ")?; // Empty lines
    }

    // Render content with syntax highlighting if available
    let content_width = (width as usize).saturating_sub(5); // 5 for line number + space

    if line.content.is_empty() {
        // Empty line - just fill with background
        for _ in 0..content_width {
            terminal.print(" ")?;
        }
    } else if let Some(spans) = highlight_spans {
        // Render with syntax highlighting
        // We need to find which spans correspond to this line
        // For simplicity, we'll render character by character and check spans
        let displayed_content = if line.content.len() > content_width {
            &line.content[..content_width]
        } else {
            &line.content
        };

        let chars: Vec<char> = displayed_content.chars().collect();
        let mut byte_offset = 0;

        for ch in &chars {
            // Find if this character is within a syntax span
            let syntax_color = spans
                .iter()
                .find(|span| byte_offset >= span.start_byte && byte_offset < span.end_byte)
                .map(|span| theme.color_for(span.token_type));

            // Apply syntax color or default color
            if let Some(color) = syntax_color {
                terminal.set_fg(color)?;
            } else {
                // Default text color for diff type
                terminal.set_fg(match line.change_type {
                    ChangeType::Unchanged => Color::White,
                    ChangeType::Added => Color::Rgb { r: 144, g: 238, b: 144 }, // Light green
                    ChangeType::Deleted => Color::Rgb { r: 255, g: 160, b: 122 }, // Light red
                    ChangeType::Modified => Color::Yellow,
                })?;
            }

            terminal.print(&ch.to_string())?;
            byte_offset += ch.len_utf8();
        }

        // Fill remaining space
        let padding = content_width.saturating_sub(displayed_content.len());
        for _ in 0..padding {
            terminal.print(" ")?;
        }
    } else {
        // No syntax highlighting - use plain colors
        let displayed_content = if line.content.len() > content_width {
            &line.content[..content_width]
        } else {
            &line.content
        };

        terminal.set_fg(match line.change_type {
            ChangeType::Unchanged => Color::White,
            ChangeType::Added => Color::Rgb { r: 144, g: 238, b: 144 }, // Light green
            ChangeType::Deleted => Color::Rgb { r: 255, g: 160, b: 122 }, // Light red
            ChangeType::Modified => Color::Yellow,
        })?;

        terminal.print(displayed_content)?;

        // Fill remaining space
        let padding = content_width.saturating_sub(line.content.len());
        for _ in 0..padding {
            terminal.print(" ")?;
        }
    }

    terminal.reset_color()?;

    Ok(())
}
