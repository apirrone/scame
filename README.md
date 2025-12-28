# scame - Fast Terminal Text Editor

A super-fast terminal-based text editor/IDE written in Rust with minimal dependencies.

## Current Status: Phase 3 Complete ✅

### Implemented Features (Phases 1, 2 & 3)

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

*File Operations:*
- `Ctrl+X Ctrl+S` - Save file (Emacs style - press Ctrl+X, then Ctrl+S)
- `Ctrl+X Ctrl+C` - Exit editor (Emacs style - press Ctrl+X, then Ctrl+C)
- `Ctrl+Q` - Quit (with save prompt if modified)

**Note:** `Ctrl+S` is now reserved for future search functionality

**Note:** Block cursor is always visible and indicates current position.

**Project Mode (NEW in Phase 2):**
- ✅ Multi-buffer workspace (switch between multiple open files)
- ✅ File picker with fuzzy search (`Ctrl+P`) - quickly open files in project
- ✅ **Smart .gitignore support** - automatically excludes ignored files/folders
- ✅ Emacs-style file backups (creates `~backup` files automatically)
- ✅ Buffer switching: `Ctrl+Tab` (next), `Ctrl+Shift+Tab` (previous)
- ✅ Open directory as project: `./scame .` or `./scame /path/to/project`

**New Keyboard Shortcuts:**
- `Ctrl+P` - Fuzzy file picker (type to search, Enter to open, Esc to cancel)
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

## Building and Running

```bash
# Build in release mode (optimized)
cargo build --release

# Run the editor
cargo run --release

# Open a specific file
cargo run --release -- test.txt
./target/release/scame test.txt
```

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
│   │   ├── buffer_view.rs  # Text buffer rendering
│   │   └── statusbar.rs    # Status bar
│   ├── app.rs           # Main application logic
│   └── main.rs          # Entry point and event loop
```

## Performance Characteristics

- **Startup:** ~10-20ms (minimal dependencies, lazy loading)
- **Edit latency:** < 1ms for character insertion (rope-based buffer)
- **Rendering:** 60 FPS target (16ms frame budget)
- **Memory:** ~5-10MB for typical editing session

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

### Phase 4: Search Features
- [ ] Global search (Ctrl+S/Ctrl+R)
- [ ] Regex search support
- [ ] Search and replace
- [ ] Jump to line (Alt+G)

### Phase 5: LSP Integration
- [ ] LSP client with tower-lsp
- [ ] Jump to definition (F12)
- [ ] Jump back (Shift+F12)
- [ ] Auto-completion (Tab)
- [ ] Diagnostics (errors/warnings)

### Phase 6: Polish & Extensions
- [ ] Command palette (Ctrl+Shift+P)
- [ ] Configuration system (TOML)
- [ ] Horizontal/vertical splits
- [ ] Multiple tabs
- [ ] Git integration

## Dependencies

**Core (minimal set):**
- `crossterm` - Terminal UI (cross-platform)
- `ropey` - Rope data structure for text
- `anyhow` - Error handling

**Future additions:**
- `tree-sitter` - Syntax parsing (Phase 3)
- `tower-lsp` - LSP client (Phase 5)
- `regex` - Search support (Phase 4)
- `fuzzy-matcher` - File search (Phase 2)

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
