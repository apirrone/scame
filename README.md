# scame - Fast Terminal Text Editor

A super-fast terminal-based text editor/IDE written in Rust with minimal dependencies and AI-powered code completions.

## Current Status: Phase 6 Complete ✅

### Implemented Features (Phases 1, 2, 3, 4, 5 & 6)

**Core Text Editing:**
- ✅ Open and edit single files
- ✅ Insert and delete characters
- ✅ Line-based editing with rope data structure (O(log n) operations)
- ✅ Copy/Paste (Ctrl+C, Ctrl+V) with internal clipboard
- ✅ Kill line (Ctrl+K) - Emacs-style delete to end of line
- ✅ Line numbers display
- ✅ Status bar with file info and cursor position
- ✅ Undo/Redo (Ctrl+Z, Ctrl+Shift+Z)
- ✅ File save (Ctrl+S)

**Cursor Movement:**
- ✅ Arrow keys navigation
- ✅ Home/End keys (start/end of line)
- ✅ Ctrl+A (beginning of line, Emacs style)
- ✅ Ctrl+E (end of line, Emacs style)
- ✅ Ctrl+Left/Right - Word-based navigation (jump by words)
- ✅ Ctrl+Up/Down - Block-based navigation (jump by paragraphs/blank lines)
- ✅ Page Up/Page Down
- ✅ Shift + Arrow keys for text selection (visual highlighting)
- ✅ Ctrl+Shift+Left/Right - Select word by word
- ✅ Ctrl+Shift+A/E - Select to start/end of line

**Keyboard Shortcuts:**

*Navigation:*
- `Arrow Keys` - Move cursor one character/line
- `Ctrl+Left/Right` - Move by word
- `Ctrl+Up/Down` - Move by block (paragraphs)
- `Ctrl+A` - Beginning of line (Emacs style)
- `Ctrl+E` - End of line (Emacs style)
- `Home/End` - Line start/end
- `Page Up/Down` - Page scrolling

*Selection:*
- `Shift + Arrows` - Select character by character
- `Ctrl+Shift+Left/Right` - Select word by word
- `Ctrl+Shift+A` - Select to start of line
- `Ctrl+Shift+E` - Select to end of line

*Editing:*
- `Ctrl+C` - Copy selected text to clipboard
- `Ctrl+V` - Paste from clipboard
- `Ctrl+K` - Kill line (delete from cursor to end of line, copies to clipboard)
- `Ctrl+Z` - Undo
- `Ctrl+Shift+Z` - Redo
- `Backspace/Delete` - Delete characters
- `Enter` - New line
- `Tab` - Insert 4 spaces (soft tabs, useful for Python indentation)

*File Operations:*
- `Ctrl+X Ctrl+S` - Save file (Emacs style - press Ctrl+X, then Ctrl+S)
- `Ctrl+X Ctrl+C` - Exit editor (Emacs style - press Ctrl+X, then Ctrl+C)
- `Ctrl+Q` - Quit (with save prompt if modified)

*Search (Emacs-style):*
- `Ctrl+S` - Search forward (incremental, **regex by default**, case-insensitive)
  - Press `Ctrl+S` again to find next occurrence forward
  - Shows "Last occurrence" when no more matches found
- `Ctrl+R` - Search backward/reverse (incremental, **regex by default**, case-insensitive)
  - Press `Ctrl+R` again to find next occurrence backward
  - Shows "First occurrence" when no more matches found
- `Ctrl+T` - Toggle regex mode (while in search mode)
  - Status bar shows "[REGEX]" indicator when enabled
  - Press Ctrl+T to switch to plain string search
  - Supports full regex patterns (e.g., `\d+`, `[a-z]+`, `\w+`)
- `Enter`, `Esc`, `Ctrl+G`, or arrow keys - Exit search mode (clears selection)
- Type to search as you go, backspace to edit pattern
- Found text is highlighted while searching
- **Both modes are case-insensitive by default**

*Search and Replace:*
- `Ctrl+H` - Query-replace (search and replace interactively, **regex by default**, case-insensitive)
  - Enter search pattern (regex patterns supported, e.g., `\d+`, `[a-z]+`)
  - Press `Ctrl+T` to toggle between regex and plain string mode
  - Enter replacement string
  - For each match: `y` (replace), `n` (skip), `a` (replace all), `q` (quit)
  - Shows count of replacements made
  - Status bar shows "[REGEX]" indicator when in regex mode
  - **Both modes are case-insensitive by default**
  - Supports undo (Ctrl+Z after completing replacements)

*Navigation:*
- `Alt+G` - Jump to line (enter line number, press Enter to jump)
- `Ctrl+J` or `Ctrl+L` - Center view on cursor (Emacs style - scrolls viewport to center cursor vertically)
- `F12` - Jump to definition (LSP - works with Rust/Python when language server installed)
- `Alt+F12` - Jump back to previous location (navigate back through jump history)

*Code Completion (LSP):*
- `Ctrl+Space` - Trigger auto-completion suggestions
  - `↑↓` arrows to navigate suggestions
  - `Enter` to insert selected completion
  - `Esc` or `Ctrl+G` to cancel

*AI Completions (GitHub Copilot-style):*
- **Automatic inline suggestions** - AI suggests code completions as you type (150ms debounce)
  - Appears as gray ghost text after cursor
  - Multi-line suggestions shown as ghost lines below cursor
- `Tab` - Accept AI suggestion (inserts full multi-line completion)
- `Esc` - Explicitly dismiss suggestion (with message)
- **Auto-dismiss** - Suggestion automatically disappears when you:
  - Move cursor (arrow keys, Page Up/Down, Home, End)
  - Execute any command (Ctrl+S, Ctrl+P, etc.)
  - Type new characters (starts new suggestion)
- `Ctrl+Shift+P` → "Toggle AI Completions [ON/OFF]" - Enable/disable AI completions

*Universal Cancel:*
- `Esc` or `Ctrl+G` - Cancel/exit any mode (search, file picker, prompts)
- Arrow keys in search mode - Exit search and move cursor

**Note:** Block cursor is always visible and indicates current position.

**Project Mode (NEW in Phase 2):**
- ✅ Multi-buffer workspace (switch between multiple open files)
- ✅ File picker with fuzzy search (`Ctrl+P`) - quickly open files in project
- ✅ **Smart .gitignore support** - automatically excludes ignored files/folders
- ✅ Emacs-style file backups (creates `~backup` files automatically)
- ✅ Buffer switching: `Ctrl+Tab` (next), `Ctrl+Shift+Tab` (previous)
- ✅ Open directory as project: `./scame .` or `./scame /path/to/project`

**New Keyboard Shortcuts:**
- `Ctrl+P` - Fuzzy file picker (type to search, Enter to open, Esc/Ctrl+G to cancel)
  - **Smart priority**: Prioritizes files with same extension as current file!
  - E.g., in a `.py` file, Python files rank higher in search results
  - **Respects .gitignore**: Won't show ignored files (node_modules, target, etc.)
- `Ctrl+Tab` - Switch to next buffer
- `Ctrl+Shift+Tab` - Switch to previous buffer
- Arrow keys in file picker to navigate results

**Syntax Highlighting (NEW in Phase 3):**
- ✅ Tree-sitter powered syntax highlighting
- ✅ **Python support** - Full syntax highlighting for `.py` files
- ✅ **Rust support** - Full syntax highlighting for `.rs` files
- ✅ VS Code Dark+ inspired color theme
- ✅ Automatic language detection from file extension
- ✅ Smart caching for performance
- ✅ Graceful fallback if highlighting fails

**Search Features (NEW in Phase 4):**
- ✅ **Incremental search** - Search as you type (Emacs-style)
- ✅ **Forward search** (Ctrl+S) - Find text ahead of cursor, press Ctrl+S again to iterate
- ✅ **Reverse search** (Ctrl+R) - Find text before cursor, press Ctrl+R again to iterate
- ✅ **Automatic selection** - Found text is highlighted
- ✅ **Bidirectional iteration** - Switch between forward/backward in same search

**LSP Integration (NEW in Phase 5):**
- ✅ **Real-time diagnostics** - Errors and warnings displayed inline with colored markers (●)
- ✅ **Jump to definition** (F12) - Navigate to symbol definitions across files
- ✅ **Jump back** (Alt+F12) - Return to previous locations (up to 50 levels)
- ✅ **Auto-completion** (Ctrl+Space) - Intelligent code suggestions with icons
- ✅ **Language support** - Rust (rust-analyzer) and Python (pyright/pylsp)
- ✅ **Non-blocking architecture** - Maintains 60 FPS while communicating with language servers

**AI-Powered Code Completions (NEW in Phase 6):**
- ✅ **GitHub Copilot-style inline suggestions** - AI completions appear as gray ghost text
- ✅ **Multi-line completions** - Full function implementations, not just single lines
- ✅ **Multiple AI providers** - Support for Claude, OpenAI, GitHub Copilot, and local LLMs
- ✅ **Automatic triggering** - Suggestions appear as you type (150ms debounce)
- ✅ **Smart overlap detection** - Prevents duplicate code when accepting suggestions
- ✅ **Ghost text rendering** - Multi-line suggestions shown as gray lines below cursor
- ✅ **Tab to accept** - Press Tab to insert full multi-line completion
- ✅ **Auto-dismiss** - Suggestions disappear on cursor movement or commands
- ✅ **Toggle command** - Enable/disable AI completions via command palette
- ✅ **Prompt caching** - Fast responses with Claude's ephemeral cache (90% latency reduction)
- ✅ **Non-blocking** - All API calls run asynchronously, maintains 60 FPS

## Building and Running

```bash
# Build in release mode (optimized)
cargo build --release

# Run the editor
cargo run --release

# Open a specific file
cargo run --release -- test.txt
./target/release/scame test.txt

# Open a project directory
cargo run --release -- .
cargo run --release -- /path/to/project
```

## Quick Start with AI Completions

1. **Set up your API key:**
   ```bash
   export ANTHROPIC_API_KEY="sk-ant-xxxxx"
   export SCAME_AI_PROVIDER="claude"
   ```

2. **Start editing:**
   ```bash
   ./target/release/scame myfile.py
   ```

3. **Try it out:**
   - Type `def fibonacci` and wait 150ms
   - Gray ghost text will appear showing the suggested implementation
   - Press `Tab` to accept the full multi-line suggestion
   - Press `Esc` or move cursor to dismiss

4. **Toggle AI completions:**
   - Press `Ctrl+Shift+P` to open command palette
   - Type "toggle ai" and press Enter
   - Shows current status: `[ON]` or `[OFF]`

## Project Architecture

```
scame/
├── src/
│   ├── buffer/          # Text buffer with rope data structure
│   │   ├── rope_buffer.rs  # Core text operations (O(log n))
│   │   └── undo.rs         # Undo/redo manager
│   ├── editor/          # Editor state and operations
│   │   ├── state.rs        # Cursor, selection, viewport
│   │   └── movement.rs     # Cursor movement logic
│   ├── render/          # Terminal rendering
│   │   ├── terminal.rs     # Terminal control (crossterm)
│   │   ├── buffer_view.rs  # Text buffer rendering (includes ghost text)
│   │   └── statusbar.rs    # Status bar
│   ├── ai/              # AI completion system
│   │   ├── manager.rs      # Channel-based async AI manager
│   │   ├── provider.rs     # Provider trait and types
│   │   └── providers/      # AI provider implementations
│   │       ├── claude.rs   # Claude (Anthropic) API
│   │       ├── openai.rs   # OpenAI API
│   │       ├── copilot.rs  # GitHub Copilot API
│   │       └── local.rs    # Local LLM endpoint
│   ├── lsp/             # Language Server Protocol
│   │   └── manager.rs      # LSP client and manager
│   ├── config.rs        # Configuration (TOML + env vars)
│   ├── app.rs           # Main application logic
│   └── main.rs          # Entry point and event loop
```

## Performance Characteristics

- **Startup:** ~10-20ms (minimal dependencies, lazy loading)
- **Edit latency:** < 1ms for character insertion (rope-based buffer)
- **Rendering:** 60 FPS target (16ms frame budget)
- **Memory:** ~5-10MB for typical editing session
- **AI Completions:** 150ms debounce, non-blocking async (maintains 60 FPS)
  - Claude with prompt caching: ~100-200ms response time
  - Without caching: ~500-1000ms response time

## Roadmap

### Phase 2: Project & File Management ✅ COMPLETE
- [x] Multi-buffer workspace management
- [x] File search (Ctrl+P) with fuzzy matching
- [x] Emacs-style file backups (~backup files)
- [x] Buffer switching keybindings

### Phase 3: Syntax Highlighting ✅ COMPLETE
- [x] Tree-sitter integration
- [x] Python syntax highlighting
- [x] Rust syntax highlighting
- [x] Color themes

### Phase 4: Search Features ✅ COMPLETE
- [x] Global search (Ctrl+S forward, Ctrl+R reverse) - **Regex by default**
- [x] Incremental search (search as you type) - **Regex by default**
- [x] Jump to line (Alt+G)
- [x] Regex search support (Ctrl+T to toggle between regex/plain string)
- [x] Search and replace (Ctrl+H interactive query-replace) - **Regex by default**

### Phase 5: LSP Integration ✅ COMPLETE
- [x] LSP client with tower-lsp (background Tokio task)
- [x] Text synchronization (didOpen, didChange, didSave)
- [x] Diagnostics display (● markers in gutter, E:/W: counts in status bar)
- [x] Non-blocking channel architecture (maintains 60 FPS)
- [x] Jump to definition (F12) - works with rust-analyzer and pyright!
- [x] Jump back (Alt+F12) - navigate back through jump history
- [x] Auto-completion (Ctrl+Space) - intelligent code suggestions with popup UI

**Supported Languages:**
- Rust (rust-analyzer)
- Python (pyright, python-lsp-server)

**Installing Language Servers:**

For **Rust** (rust-analyzer):
```bash
rustup component add rust-analyzer
```

For **Python** (choose one):
```bash
# Option 1: Pyright (recommended, faster)
npm install -g pyright

# Option 2: Python LSP Server (fallback)
pip install python-lsp-server
```

The editor will automatically try alternatives if the primary server isn't found.

**Configuring AI Completions:**

AI completions are configured via `~/.scame/config.toml` or environment variables.

**Option 1: Quick Setup (Environment Variables)**

The easiest way to get started:

```bash
# For Claude (recommended for speed with prompt caching)
export ANTHROPIC_API_KEY="sk-ant-xxxxx"
export SCAME_AI_PROVIDER="claude"

# For OpenAI
export OPENAI_API_KEY="sk-xxxxx"
export SCAME_AI_PROVIDER="openai"

# For GitHub Copilot
export GITHUB_TOKEN="gho_xxxxx"
export SCAME_AI_PROVIDER="copilot"
```

Add these to your `~/.bashrc` or `~/.zshrc` to make them permanent.

**Option 2: Configuration File (Recommended for permanent setup)**

Create the configuration file:

```bash
# Create config directory
mkdir -p ~/.scame

# Create config file
cat > ~/.scame/config.toml << 'EOF'
[ai]
enabled = true
provider = "claude"
debounce_ms = 150

[ai.claude]
api_key = "sk-ant-xxxxx"
model = "claude-3-5-sonnet-20241022"
EOF

# Or edit manually
vim ~/.scame/config.toml
```

**Full Configuration File Format (~/.scame/config.toml):**
```toml
[ai]
enabled = true
provider = "claude"  # Options: "claude", "openai", "copilot", "local"
debounce_ms = 150    # Delay before triggering completion (milliseconds)

[ai.claude]
api_key = "sk-ant-xxxxx"
model = "claude-3-5-sonnet-20241022"  # Or "claude-3-5-haiku-20241022" for speed

[ai.openai]
api_key = "sk-xxxxx"
model = "gpt-4"  # Or "gpt-3.5-turbo"

[ai.copilot]
api_token = "gho_xxxxx"

[ai.local]
endpoint = "http://localhost:11434/api/generate"  # Ollama default endpoint
```

**Supported AI Providers:**
- **Claude (Anthropic)** - Best quality, prompt caching for speed, models: claude-3-5-sonnet, claude-3-5-haiku
- **OpenAI** - Reliable, models: gpt-4, gpt-3.5-turbo
- **GitHub Copilot** - Native GitHub integration (requires token)
- **Local LLMs** - Use Ollama or any local HTTP endpoint for privacy/offline use

**Getting API Keys:**
- Claude: https://console.anthropic.com/
- OpenAI: https://platform.openai.com/api-keys
- GitHub: https://github.com/settings/tokens

### Phase 6: AI Completions & Polish ✅ COMPLETE
- [x] AI-powered code completions (GitHub Copilot-style)
- [x] Multiple provider support (Claude, OpenAI, Copilot, Local)
- [x] Configuration system (TOML + environment variables)
- [x] Command palette toggle for AI completions
- [ ] Horizontal/vertical splits
- [ ] Multiple tabs
- [ ] Git integration

## Dependencies

**Core:**
- `crossterm` - Terminal UI (cross-platform)
- `ropey` - Rope data structure for text
- `anyhow` - Error handling

**Features:**
- `tree-sitter` - Syntax parsing (Phase 3) ✅
- `tower-lsp` - LSP client (Phase 5) ✅
- `tokio` - Async runtime for LSP and AI (Phase 5 & 6) ✅
- `lsp-types` - LSP protocol types (Phase 5) ✅
- `regex` - Search support (Phase 4) ✅
- `fuzzy-matcher` - File search (Phase 2) ✅
- `ignore` - .gitignore support (Phase 2) ✅
- `reqwest` - HTTP client for AI providers (Phase 6) ✅
- `serde` / `serde_json` - Configuration and API serialization (Phase 6) ✅
- `toml` - Configuration file parsing (Phase 6) ✅

## Design Principles

1. **Fast startup** - Lazy loading, minimal initialization
2. **Responsive editing** - O(log n) operations, 60 FPS rendering
3. **Minimal dependencies** - Only essential crates
4. **Clean architecture** - Modular, testable components
5. **MVP-first** - Ship working features incrementally

## Testing

Run unit tests:
```bash
cargo test
```

Run with optimizations:
```bash
cargo run --release
```

## License

MIT License

## Contributing

This is a learning project demonstrating how to build a fast text editor in Rust.
Feel free to explore the code and suggest improvements!
