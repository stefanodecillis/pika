# Pika

A fast, keyboard-driven terminal IDE built with Rust and [Ratatui](https://ratatui.rs). Designed for developers who live in the terminal — quick file navigation, code editing with syntax highlighting, and LSP support.

## Features

- **Code editor** with syntax highlighting (powered by [syntect](https://github.com/trishume/syntect))
- **Collapsible file tree** sidebar with keyboard navigation
- **Tab management** — open multiple files side by side
- **LSP integration** — auto-discovers installed language servers for autocompletion, go-to-definition, diagnostics, and more
- **Drag and drop** — drop files from your OS file manager into the terminal
- **File operations** — copy, cut, paste, rename, delete files from the sidebar
- **Undo/redo** with full edit history
- **Text selection** with Shift+Arrow keys
- **System clipboard** integration (copy/paste between Pika and other apps)
- **Fuzzy file finder** (Ctrl+P)
- **Save-on-close prompts** — never lose unsaved changes
- **Configurable** via `~/.config/pika/config.toml`

## Installation

### From crates.io

```sh
cargo install pika-ide
```

### From source

```sh
git clone https://github.com/stefanodecillis/pika.git
cd pika
cargo install --path .
```

## Usage

```sh
# Open the current directory
pika

# Open a specific directory
pika /path/to/project
```

## Keyboard Shortcuts

Press `Ctrl+H` inside Pika to see the full shortcut reference.

### Global

| Key | Action |
|-----|--------|
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+P` | Open file finder |
| `Ctrl+S` | Save file |
| `Ctrl+W` | Close tab |
| `Ctrl+Q` | Quit |
| `Ctrl+Tab` | Next tab |
| `Ctrl+H` | Show keyboard shortcuts |
| `Esc` | Switch focus (sidebar / editor) |

### Editor

| Key | Action |
|-----|--------|
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / Cut / Paste |
| `Ctrl+A` | Select all |
| `Shift+Arrows` | Extend selection |
| `Ctrl+Space` | Trigger autocomplete |
| `F12` | Go to definition |
| `Shift+F12` | Find references |
| `F2` | Rename symbol |
| `Ctrl+F` | Find in file |

### Sidebar (File Tree)

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate |
| `Enter` | Open file / toggle directory |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / Cut / Paste file |
| `Delete` / `Backspace` | Delete file (to trash) |
| `F2` | Rename |
| `N` / `Shift+N` | New file / New directory |

## LSP Support

Pika auto-discovers language servers installed on your system. Supported servers include:

| Language | Server |
|----------|--------|
| Rust | `rust-analyzer` |
| TypeScript/JS | `typescript-language-server` |
| Python | `pyright-langserver` |
| Go | `gopls` |
| C/C++ | `clangd` |
| Lua | `lua-language-server` |
| Zig | `zls` |
| Java | `jdtls` |

Add custom servers in `~/.config/pika/lsp.toml`:

```toml
[servers.my-language]
command = "/path/to/server"
args = ["--stdio"]
extensions = ["mylang"]
```

## Configuration

Pika reads its configuration from `~/.config/pika/config.toml`. Example:

```toml
sidebar_width = 30
tab_size = 4
show_line_numbers = true
word_wrap = false

[lsp]
auto_discover = true
```

## License

MIT
