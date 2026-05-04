# rneovim Architecture & Specification Document

## 1. Overview and Objectives

This document serves as a comprehensive architectural specification of `rneovim`, an experimental project aiming to rebuild the core engine of Neovim using modern Rust paradigms. The objective is to achieve maximum compatibility with the Neovim Lua ecosystem (such as `lazy.nvim`, `md-render.nvim`) while maintaining a highly performant, safe, and maintainable codebase.

This specification details both the current state of the Rust codebase (`rneovim`) and the target architecture of the original C codebase (`neovim-src`) to guide future porting efforts.

---

## 2. Current Rust Architecture (rneovim)

The `rneovim` core is encapsulated within the `src/nvim/` directory, minimizing external dependencies where possible while leveraging robust tools like `mlua` for LuaJIT integration.

### 2.1 Core State Management (`src/nvim/state.rs`)
- **`VimState`**: The centralized, monolithic state container of the editor. It holds:
  - `buffers`: A `Vec<Rc<RefCell<Buffer>>>` storing all open files.
  - `tabpages`: A list of `TabPage` structs, which in turn hold `Window` views.
  - `grid`: The `Grid` structure responsible for UI state.
  - `lua_env`: An `Rc<LuaEnv>` wrapping the Lua virtual machine.
  - `active_events`: A registry of delayed functions and callbacks used to emulate `libuv` timers and asynchronous execution.
  - `options` / `highlight_groups`: Maps maintaining global options and UI highlight definitions.
- **Control Flow (`main.rs`)**: 
  - Initializes the OS terminal into Raw Mode via FFI (`os_setup_terminal`).
  - Sets up the `EventLoop` and `KeyProcessor`.
  - Continuously polls for events (`eloop.poll_events`) and executes pending callbacks via `state.step()`.
  - Periodically checks terminal size constraints (e.g., `os_get_terminal_size`) and automatically resizes the internal `Grid`, triggering `VimResized` autocmds.

### 2.2 Text Storage & Buffers (`src/nvim/buffer.rs`, `src/nvim/memline.rs`)
- **`Buffer`**: Represents a file in memory. 
- **`MemLine` (B-Tree)**: The text storage engine. Instead of a flat array of strings, `MemLine` implements a custom B-Tree (`Node::Leaf`, `Node::Internal`). Leaves split when they exceed `MAX_CAPACITY`, ensuring that insertions and deletions in massive files operate in $O(\log N)$ or $O(\sqrt{N})$ time.
- **`ExtmarkManager` (`src/nvim/extmark.rs`)**: Tracks virtual text, highlights, and signs.
  - Supports multiple namespaces.
  - Extmarks automatically shift their row/col positions when text is inserted or deleted before them.
  - Supports `virt_text_pos` configurations (`eol`, `overlay`) for rendering inline UI components.

### 2.3 Window & UI System (`src/nvim/window.rs`, `src/nvim/ui/grid.rs`)
- **`Window`**: Acts as a viewport mapping a portion of a `Buffer` onto the `Grid`.
  - **Floating Windows**: Managed via `WinConfig`. Supports relative positioning (`editor`, `cursor`, `win`), Z-indexing, and borders (`rounded`, `single`, `double`).
- **`Grid`**: A 2D buffer of `Cell` objects.
  - Each `Cell` tracks a character, foreground/background color (supporting full TrueColor `Color::Rgb`), and styles (bold, italic, underline).
  - The `flush()` method calculates the delta between the current frame and the next, outputting optimized ANSI escape sequences directly to stdout.
- **Notification Popup**: Leveraging floating windows, Lua `vim.notify` calls are intercepted and rendered as auto-closing, rounded-border floating windows in the top-right corner via `VimState::show_notification`.

### 2.4 Lua API Integration (`src/nvim/lua/`)
- **`LuaEnv`**: The bridge between Rust and LuaJIT via `mlua`.
- **API Surface (`api.rs`)**: Exposes native Rust functions to Lua under the `vim.api.*` namespace (e.g., `nvim_create_buf`, `nvim_set_hl`, `nvim_open_win`). State is safely accessed by extracting a raw pointer from the Lua registry (`StateWrapper`).
- **vim.uv (libuv stub)**: Replicates the asynchronous event loop API expected by plugins (timers, filesystem stats) by deferring closures into `VimState::active_events`.
- **Tree-sitter Stub (`treesitter.rs`)**: Implements dummy parsers (`get_parser`, `query.parse`, `get_string_parser`) to prevent plugins like `md-render.nvim` from crashing during initialization, buying time until the real Rust `tree-sitter` crate is fully wired into the rendering pipeline.

### 2.5 Input & Event Loop (`src/nvim/event/`)
- **`KeyProcessor`**: Maps raw terminal bytes into semantic `Request` enumerations. It integrates with `KeymapEngine` to process multi-key combinations (e.g., `jj`, `<C-w>j`).
- **Escape Sequences**: Robustly parses terminal escape sequences in `main.rs`, resolving arrow keys and Kitty-style terminal mouse tracking protocols (`<Esc>[<...m`).

---

## 3. Target C Neovim Architecture (neovim-src)

To achieve parity and continue porting features correctly, `rneovim` must understand and emulate (or consciously diverge from) the original C Neovim architecture.

### 3.1 Buffer Management and `memfile` (`src/nvim/memline.c`)
- **Architecture**: C Neovim uses a virtual memory approach. The `buf_T` structure delegates text storage to `memline.c`, which manages text via `memfile.c`.
- **Swap Files**: Text is stored in discrete memory blocks (`mf_block_T`). If memory runs low or for crash recovery, these blocks are flushed to a `.swp` file on disk. 
- **Porting Implication**: Rust currently uses a purely in-memory B-Tree. For massive files or exact crash-recovery parity, `rneovim` would need to implement an asynchronous swap-file flusher.

### 3.2 UI, Screen, and Msgpack-RPC (`src/nvim/ui.c`, `src/nvim/api/`)
- **Decoupling**: Unlike `rneovim` which directly writes ANSI codes, C Neovim completely decouples the core from the UI. The core calculates screen updates and broadcasts them as UI events (`grid_line`, `grid_resize`, `win_pos`) via Msgpack-RPC.
- **Internal TUI (`src/nvim/tui/`)**: The built-in terminal UI is just an internal RPC client that consumes these events and generates ANSI codes.
- **Porting Implication**: To support GUI clients (like Neovide) and true Neovim architecture, `rneovim`'s `Grid` module must be refactored to emit a stream of abstract UI events instead of raw terminal strings.

### 3.3 Extmark Tree (`src/nvim/decoration.c`, `src/nvim/marktree.c`)
- **Performance**: C Neovim uses a highly specialized `marktree` (a k-D tree variant) for storing extmarks. This allows for $O(\log N)$ lookup of highlights intersecting a specific rendering line.
- **Porting Implication**: While the Rust `ExtmarkManager` correctly handles logic and shifting, its lookup during `draw_window` may become a bottleneck on huge files with thousands of marks. The `marktree` algorithms should be ported for rendering optimization.

### 3.4 Event Loop (`src/nvim/event/loop.c`)
- **libuv Core**: C Neovim uses `libuv` as its absolute backbone. All I/O, timers, socket communication (RPC), and signals revolve around `uv_run`.
- **Multithreading & Callbacks**: Background worker threads push operations into a thread-safe `multiqueue`. The main loop drains this queue, safely executing Lua callbacks without race conditions.
- **Porting Implication**: `rneovim` currently uses a simple `std::sync::mpsc` channel and custom time checks in `step()`. While functional, completely mapping `vim.uv` to a Rust-native async runtime (like `tokio` or `mio`) would provide vastly better performance and compatibility with complex plugins.

---

## 4. Gap Analysis & Future Tasks

Based on the cross-review, here are the strategic next steps for the `rneovim` project:

1. **Tree-sitter Integration**:
   - Replace the Lua stubs in `src/nvim/lua/treesitter.rs` with the `tree-sitter` Rust crate.
   - Inject the parsed AST highlights dynamically into the `ExtmarkManager` so that `Grid::draw_window` automatically renders semantic highlighting.
2. **Msgpack RPC**:
   - Implement a TCP/Unix Domain Socket server using `rmp-serde`.
   - Route `Request` objects from the socket into `handle_request`, allowing external GUI clients to connect to the Rust core.
3. **Window Layout Tree**:
   - Transition `tabpage.windows` from a flat `Vec` to a tree structure (like C Neovim's `frame_T`). This is essential for proper `<C-w>` split management (vsplit/split) which is currently hardcoded or missing.
4. **Vimscript Evaluator**:
   - Currently, Lua takes precedence. However, many plugins still rely on `vim.fn` wrappers for legacy Vimscript. A rudimentary expression evaluator in `src/nvim/eval.rs` needs expansion to parse lists, dictionaries, and simple function calls.