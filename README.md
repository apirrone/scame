# scame - Fast Terminal Text Editor

A super-fast terminal-based text editor/IDE written in Rust with AI-powered code completions, LSP support, and syntax highlighting.

## What is this ?

This editor is for me, it's not meant to be easily customizable or extensible, it just contains the features I need, bound to keybinings I'm used to (a weird mix of emacs and vscode).


## Quick Start

### Installation

Install with a single command:

```bash
curl -sSL https://raw.githubusercontent.com/apirrone/scame/main/install.sh | sh
```

**Supported Platforms:** Linux (x86_64, ARM64, ARMv7 / Raspberry Pi)

### Building from Source

```bash
# Clone and build
git clone https://github.com/apirrone/scame.git
cd scame
cargo build --release

# Run
./target/release/scame myfile.py
./target/release/scame .  # Open directory as project
```

### Basic Usage

```bash
# Open a file
scame myfile.py

# Open a project directory
scame /path/to/project

# Side-by-side diff viewer
scame --diff file1.py file2.py

# Show version
scame --version
```

**Essential Shortcuts:**
- `Ctrl+S` - Save file
- `Ctrl+Q` - Quit
- `Ctrl+P` - File picker (fuzzy search)
- `Ctrl+Space` - Auto-completion (LSP)
- `Tab` - Accept AI suggestion
- `Ctrl+Shift+P` - Command palette

---

## Features

<details>
<summary><b>📝 Core Text Editing</b></summary>

- ✅ Rope-based text buffer (O(log n) operations)
- ✅ Multiple buffers / workspace management
- ✅ Copy/Paste with system clipboard integration
- ✅ Undo/Redo (Ctrl+Z, Ctrl+Shift+Z)
- ✅ Line numbers and status bar
- ✅ Smart indentation (4 spaces, configurable)
- ✅ Python indentation guides (vertical lines)
- ✅ Terminal resize support

</details>

<details>
<summary><b>🎨 Syntax Highlighting</b></summary>

Tree-sitter powered syntax highlighting with VS Code Dark+ inspired theme:

**Supported Languages:**
- Python (`.py`)
- Rust (`.rs`)
- JSON (`.json`)
- Markdown (`.md`)
- JavaScript (`.js`)
- HTML/CSS

Features:
- Automatic language detection
- Smart caching for performance
- Graceful fallback if highlighting fails

</details>

<details>
<summary><b>🔍 Search & Replace</b></summary>

- **Incremental Search** (Ctrl+S forward, Ctrl+R reverse)
  - Search as you type
  - Regex support by default (toggle with Ctrl+T)
  - Case-insensitive
  - Press Ctrl+S/R again to find next/previous

- **Search & Replace** (Ctrl+H)
  - Interactive query-replace
  - Regex patterns supported
  - Options: replace (y), skip (n), replace all (a), quit (q)

- **Jump to Line** (Alt+G)

</details>

<details>
<summary><b>🤖 AI-Powered Completions</b></summary>

GitHub Copilot-style inline code suggestions:

- **Multi-line completions** - Full function implementations, not just single lines
- **Ghost text rendering** - Suggestions appear as gray text after cursor
- **Tab to accept** - Insert full completion with Tab key
- **Auto-trigger** - Suggestions appear as you type (150ms debounce)
- **Auto-dismiss** - Disappears on cursor movement or commands
- **Toggle on/off** - Via command palette (Ctrl+Shift+P)

**Supported AI Providers:**
- Claude (Anthropic) - Best quality, prompt caching for speed
- OpenAI (GPT-4, GPT-3.5)
- GitHub Copilot
- Local LLMs (Ollama, custom endpoints)

**Quick Setup:**
```bash
export ANTHROPIC_API_KEY="sk-ant-xxxxx"
export SCAME_AI_PROVIDER="claude"
./target/release/scame myfile.py
```

<details>
<summary>Detailed AI Configuration</summary>

Create `~/.scame/config.toml`:

```toml
[ai]
enabled = true
provider = "claude"  # Options: "claude", "openai", "copilot", "local"
debounce_ms = 150

[ai.claude]
api_key = "sk-ant-xxxxx"
model = "claude-3-5-sonnet-20241022"

[ai.openai]
api_key = "sk-xxxxx"
model = "gpt-4"

[ai.copilot]
api_token = "gho_xxxxx"

[ai.local]
endpoint = "http://localhost:11434/api/generate"  # Ollama
```

**Get API Keys:**
- Claude: https://console.anthropic.com/
- OpenAI: https://platform.openai.com/api-keys
- GitHub: https://github.com/settings/tokens

</details>

</details>

<details>
<summary><b>🔧 LSP Integration</b></summary>

Language Server Protocol support for intelligent code features:

- **Real-time diagnostics** - Errors and warnings displayed inline (● markers)
- **Jump to definition** (F12) - Navigate to symbol definitions across files
- **Jump back** (Alt+F12) - Return to previous location
- **Auto-completion** (Ctrl+Space) - Context-aware code suggestions with icons
- **Non-blocking** - Maintains 60 FPS while communicating with language servers

**Supported Languages:**
- Rust (rust-analyzer)
- Python (pyright, python-lsp-server)

**Install Language Servers:**
```bash
# Rust
rustup component add rust-analyzer

# Python (choose one)
npm install -g pyright            # Recommended
pip install python-lsp-server     # Alternative
```

</details>

<details>
<summary><b>🔀 Diff Viewer</b></summary>

GitHub PR-style side-by-side diff viewer:

```bash
scame --diff file1.py file2.py
```

Features:
- Side-by-side comparison with color-coded changes
- Syntax highlighting (Python, Rust)
- Navigation: Arrow keys, j/k, PgUp/PgDn
- Quit: q, Esc, or Ctrl+C

</details>

<details>
<summary><b>📂 Project Management</b></summary>

- Multi-buffer workspace (switch between files)
- File picker with fuzzy search (Ctrl+P)
- Smart .gitignore support
- Extension-aware file prioritization
- Emacs-style file backups (~backup files)
- Buffer switching: Ctrl+Tab (next), Ctrl+Shift+Tab (previous)

</details>

---

## Keyboard Shortcuts

<details>
<summary><b>View All Shortcuts</b></summary>

### Navigation
- `Arrow Keys` - Move cursor
- `Ctrl+Left/Right` - Move by word
- `Ctrl+Up/Down` - Move by block (paragraphs)
- `Ctrl+A` - Beginning of line (Emacs style)
- `Ctrl+E` - End of line (Emacs style)
- `Home/End` - Line start/end
- `Page Up/Down` - Page scrolling
- `Ctrl+J` or `Ctrl+L` - Center view on cursor

### Selection
- `Shift + Arrows` - Select character by character
- `Ctrl+Shift+Left/Right` - Select word by word
- `Ctrl+Shift+A` - Select to start of line
- `Ctrl+Shift+E` - Select to end of line

### Editing
- `Ctrl+C` - Copy selected text (to system clipboard)
- `Ctrl+V` - Paste (from system clipboard)
- `Ctrl+K` - Kill line (delete to end of line, copies to clipboard)
- `Ctrl+Z` - Undo
- `Ctrl+Shift+Z` - Redo
- `Tab` - Insert 4 spaces (or accept AI suggestion)
- `Backspace/Delete` - Delete characters

### File Operations
- `Ctrl+X Ctrl+S` - Save file (Emacs style)
- `Ctrl+X Ctrl+C` - Exit editor (Emacs style)
- `Ctrl+Q` - Quit (with save prompt if modified)

### Search
- `Ctrl+S` - Search forward (incremental, regex by default)
  - Press `Ctrl+S` again to find next
  - `Ctrl+T` to toggle regex/plain string mode
- `Ctrl+R` - Search backward/reverse
  - Press `Ctrl+R` again to find previous
- `Ctrl+H` - Search and replace (interactive query-replace)
  - `y` - replace, `n` - skip, `a` - replace all, `q` - quit
- `Enter/Esc/Ctrl+G` - Exit search mode

### Navigation
- `Alt+G` - Jump to line
- `F12` - Jump to definition (LSP)
- `Alt+F12` - Jump back to previous location

### Code Completion
- `Ctrl+Space` - Trigger LSP auto-completion
  - `↑↓` arrows to navigate suggestions
  - `Enter` to insert, `Esc` to cancel
- `Tab` - Accept AI suggestion
- `Esc` - Dismiss AI suggestion

### Project
- `Ctrl+P` - File picker (fuzzy search)
  - Type to search, `Enter` to open, `Esc` to cancel
  - Smart extension prioritization
- `Ctrl+Tab` - Switch to next buffer
- `Ctrl+Shift+Tab` - Switch to previous buffer
- `Ctrl+Shift+P` - Command palette
  - Toggle AI completions [ON/OFF]
  - Toggle indentation guides [ON/OFF]

### Universal Cancel
- `Esc` or `Ctrl+G` - Cancel/exit any mode

</details>

---

## Performance

- **Startup:** ~10-20ms (minimal dependencies, lazy loading)
- **Edit latency:** < 1ms for character insertion (rope-based buffer)
- **Rendering:** 60 FPS target (16ms frame budget)
- **Memory:** ~5-10MB for typical editing session
- **AI Completions:** Non-blocking async, maintains 60 FPS
  - Claude with prompt caching: ~100-200ms response time
  - Without caching: ~500-1000ms response time

---

## Architecture

<details>
<summary><b>Project Structure</b></summary>

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
│   │   ├── buffer_view.rs  # Text buffer rendering with ghost text
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
│   ├── syntax/          # Syntax highlighting (Tree-sitter)
│   ├── diff.rs          # Side-by-side diff viewer
│   ├── workspace/       # Multi-buffer workspace
│   ├── config.rs        # Configuration (TOML + env vars)
│   ├── app.rs           # Main application logic
│   └── main.rs          # Entry point and event loop
```

</details>

<details>
<summary><b>Dependencies</b></summary>

**Core:**
- `crossterm` - Terminal UI (cross-platform)
- `ropey` - Rope data structure for text
- `anyhow` - Error handling

**Features:**
- `tree-sitter` - Syntax parsing
- `tower-lsp` - LSP client
- `tokio` - Async runtime for LSP and AI
- `lsp-types` - LSP protocol types
- `regex` - Search support
- `fuzzy-matcher` - File search
- `ignore` - .gitignore support
- `similar` - Diff computation
- `arboard` - System clipboard
- `reqwest` - HTTP client for AI providers
- `serde` / `serde_json` / `toml` - Configuration

</details>

---

## Development Roadmap

<details>
<summary><b>Completed Phases</b></summary>

### Phase 1: Basic Editor ✅
- [x] Core text editing (insert, delete, navigate)
- [x] Rope data structure
- [x] Copy/paste, undo/redo
- [x] Line numbers and status bar

### Phase 2: Project Management ✅
- [x] Multi-buffer workspace
- [x] File picker with fuzzy search (Ctrl+P)
- [x] .gitignore support
- [x] Buffer switching

### Phase 3: Syntax Highlighting ✅
- [x] Tree-sitter integration
- [x] Python, Rust, JSON, Markdown, JavaScript, HTML/CSS
- [x] VS Code Dark+ inspired theme

### Phase 4: Search Features ✅
- [x] Incremental search (forward/reverse)
- [x] Regex support with toggle
- [x] Search and replace
- [x] Jump to line

### Phase 5: LSP Integration ✅
- [x] Real-time diagnostics
- [x] Jump to definition / jump back
- [x] Auto-completion (Ctrl+Space)
- [x] Rust and Python support
- [x] Non-blocking architecture

### Phase 6: AI & Polish ✅
- [x] AI-powered code completions
- [x] Multiple provider support (Claude, OpenAI, Copilot, Local)
- [x] Configuration system (TOML + env vars)
- [x] Command palette toggles
- [x] System clipboard integration
- [x] Python indentation guides
- [x] Terminal resize support
- [x] Side-by-side diff viewer

</details>

<details>
<summary><b>Future Plans</b></summary>

### Phase 7: Advanced Features
- [ ] Horizontal/vertical splits
- [ ] Multiple tabs
- [ ] Git integration (status, diff, blame)
- [ ] File tree sidebar
- [ ] More language servers
- [ ] Snippets support
- [ ] Macro recording

</details>

---

## Design Principles

1. **Fast startup** - Lazy loading, minimal initialization
2. **Responsive editing** - O(log n) operations, 60 FPS rendering
3. **Minimal dependencies** - Only essential crates
4. **Clean architecture** - Modular, testable components
5. **MVP-first** - Ship working features incrementally

---

## Testing

```bash
# Run unit tests
cargo test

# Run with optimizations
cargo run --release

# Build for production
cargo build --release
```

---

## License

MIT License

## Contributing

This is a learning project demonstrating how to build a fast text editor in Rust.
Feel free to explore the code and suggest improvements!
