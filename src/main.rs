mod app;
mod buffer;
mod editor;
mod logger;
mod render;

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
use render::Terminal;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Initialize logger
    logger::init();

    // Create Tokio runtime for LSP background tasks
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

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

    // Initialize terminal
    let terminal = Terminal::new()?;

    // Initial render
    app.render(&terminal)?;

    // Main event loop
    loop {
        // Poll for LSP messages (non-blocking)
        let had_lsp_updates = app.poll_lsp_messages();

        // Only render if LSP had updates
        if had_lsp_updates {
            app.render(&terminal)?;
        }

        // Handle input - wait for events (no timeout = blocks until event)
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match app.handle_event(event)? {
                ControlFlow::Continue => {
                    // Render after handling input
                    app.render(&terminal)?;
                }
                ControlFlow::Exit => break,
            }
        }
    }

    // Shutdown LSP
    app.shutdown_lsp()?;

    // Cleanup
    terminal.cleanup()?;
    logger::close();

    Ok(())
}
