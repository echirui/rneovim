use std::collections::HashMap;
use crate::nvim::buffer::Buffer;
use crate::nvim::window::{Window, Cursor, TabPage};
use crate::nvim::ui::grid::{Grid, Color};
use crate::nvim::eval::EvalContext;
use crate::nvim::lua::LuaEnv;
use crate::nvim::request::{Request, Operator};
use crate::nvim::event::event_loop::EventCallback;
use crate::nvim::syntax::SyntaxHighlighter;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::mpsc::Sender;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    Insert,
    Visual(VisualMode),
    CommandLine,
    Search,
    OperatorPending,
    Terminal,
    Replace,
    VirtualReplace,
    WaitMacroReg,
    WaitReplaceChar,
    BlockInsert { append: bool, anchor: Cursor, cursor: Cursor },
    InsertWaitRegister,
    InsertLiteral,
    InsertDigraph1(char),
    InsertOnce,
    CommandLineWaitRegister,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualMode {
    Char,
    Line,
    Block,
}

#[derive(Debug, Clone, Default)]
pub struct HlAttrs {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub reverse: bool,
    pub underline: bool,
}

pub struct UvEvent {
    pub callback_id: i32,
    pub next_run: std::time::Instant,
    pub repeat: Option<u64>, // ms
}

pub struct VimState {
    pub mode: Mode,
    pub visual_mode: Option<VisualMode>,
    pub visual_anchor: Option<Cursor>,
    pub buffers: Vec<Rc<RefCell<Buffer>>>,
    pub tabpages: Vec<TabPage>,
    pub current_tab_idx: usize,
    pub messages: Vec<String>,
    pub registers: HashMap<char, String>,
    pub macros: HashMap<char, Vec<Request>>,
    pub macro_recording: Option<(char, Vec<Request>)>,
    pub options: HashMap<String, OptionValue>,
    pub highlight_groups: HashMap<String, HlAttrs>,

    pub active_events: HashMap<i32, UvEvent>,
    pub next_event_id: i32,

    pub search_start_cursor: Option<Cursor>,
    pub last_change: Option<Request>,
    pub last_insert_cursor: Option<Cursor>,
    pub last_char_search: Option<Request>,
    pub last_search: Option<String>,
    pub last_search_query: Option<String>,
    pub pending_key: Option<char>,
    pub input_buffer: String,
    pub pending_register: Option<char>,
    pub pending_op: Option<Operator>,
    pub pending_count: Option<u32>,
    pub pum_items: Option<Vec<String>>,
    pub pum_index: Option<usize>,
    pub alternate_file: Option<String>,
    pub sender: Option<Sender<EventCallback<VimState>>>,
    pub jobs: HashMap<i32, std::process::Child>,
    pub job_outputs: HashMap<i32, Vec<String>>,
    pub lsp_client: Option<crate::nvim::lsp::LspClient>,
    pub ts_manager: Option<crate::nvim::treesitter::TreesitterManager>,
    pub profiling_log: Option<String>,

    pub autocmds: HashMap<AutoCmdEvent, Vec<AutoCmd>>,
    pub user_commands: HashMap<String, i32>, // callback_id
    pub abbreviations: HashMap<String, String>,
    pub highlighter: SyntaxHighlighter,
    pub grid: Grid,
    pub cmdline: String,
    pub cmdline_cursor: usize,
    pub cmd_history: Vec<String>,
    pub cmd_history_idx: Option<usize>,
    pub keymap: crate::nvim::keymap::KeymapEngine,
    pub quickfix_list: Vec<(String, usize, String)>, // (file, lnum, text)
    pub globals: HashMap<String, crate::nvim::eval::TypVal>,
    pub quit: bool,
    pub vim_did_enter: bool,
    pub lua_env: Rc<LuaEnv>,
    pub eval_context: EvalContext,
    pub open_files: HashMap<i32, std::fs::File>,
    pub active_scandirs: HashMap<i32, std::fs::ReadDir>,
    pub next_scandir_id: i32,
    pub next_fd: i32,
    pub next_win_id: i32,
    pub pending_callbacks: Vec<i32>, // callback_ids
    pub check_callbacks: Vec<i32>, // callback_ids
    pub config_dir: Option<std::path::PathBuf>,
}

#[derive(Clone)]
pub enum AutoCmdCallback {
    Command(String),
    Lua(i32), // callback_id
}

#[derive(Clone)]
pub struct AutoCmd {
    pub callback: AutoCmdCallback,
    pub pattern: Option<String>,
    pub once: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AutoCmdEvent {
    BufReadPre,
    BufReadPost,
    BufWritePre,
    BufWritePost,
    BufEnter,
    BufLeave,
    BufDelete,
    BufHidden,
    BufWinEnter,
    BufWinLeave,
    VimEnter,
    VimLeave,
    VimLeavePre,
    UIEnter,
    CursorMoved,
    CursorMovedI,
    TextChanged,
    TextChangedI,
    WinEnter,
    WinLeave,
    TabEnter,
    TabLeave,
    FileType,
    Syntax,
    InsertEnter,
    InsertLeave,
    User,
    ColorScheme,
    ColorSchemePre,
    SafeState,
    CursorHold,
    CursorHoldI,
    WinClosed,
    WinResized,
    VimResized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptionValue {
    Bool(bool),
    Int(i64),
    String(String),
}

impl VimState {
    pub fn new() -> Self {
        let mut options = HashMap::new();
        options.insert("number".to_string(), OptionValue::Bool(false));
        options.insert("relativenumber".to_string(), OptionValue::Bool(false));
        options.insert("expandtab".to_string(), OptionValue::Bool(false));
        options.insert("tabstop".to_string(), OptionValue::Int(8));
        options.insert("shiftwidth".to_string(), OptionValue::Int(8));
        options.insert("autoindent".to_string(), OptionValue::Bool(false));
        options.insert("incsearch".to_string(), OptionValue::Bool(true));
        options.insert("hlsearch".to_string(), OptionValue::Bool(true));
        options.insert("ignorecase".to_string(), OptionValue::Bool(false));
        options.insert("smartcase".to_string(), OptionValue::Bool(false));
        options.insert("wrap".to_string(), OptionValue::Bool(true));
        options.insert("scrolloff".to_string(), OptionValue::Int(0));
        options.insert("sidescrolloff".to_string(), OptionValue::Int(0));
        options.insert("laststatus".to_string(), OptionValue::Int(2));
        options.insert("showmode".to_string(), OptionValue::Bool(true));
        options.insert("cursorline".to_string(), OptionValue::Bool(false));
        options.insert("signcolumn".to_string(), OptionValue::String("auto".to_string()));
        options.insert("loadplugins".to_string(), OptionValue::Bool(true));
        options.insert("headless".to_string(), OptionValue::Bool(false));
        options.insert("termguicolors".to_string(), OptionValue::Bool(true));
        
        let mut rtp = Vec::new();
        if let Some(config) = dirs::config_dir() {
            rtp.push(config.join("rneovim").to_string_lossy().to_string());
        }
        if let Some(data) = dirs::data_dir() {
            rtp.push(data.join("rneovim/site").to_string_lossy().to_string());
        }
        if let Ok(cwd) = std::env::current_dir() {
            rtp.push(cwd.join("neovim-src/runtime").to_string_lossy().to_string());
        }
        rtp.push("/usr/local/share/rneovim/site".to_string());
        rtp.push("/usr/share/rneovim/site".to_string());
        rtp.push("/usr/local/share/rneovim/runtime".to_string());
        rtp.push("/usr/share/rneovim/runtime".to_string());
        if let Some(home) = dirs::home_dir() {
            rtp.push(home.join(".local/share/rneovim/site").to_string_lossy().to_string());
        }
        if let Some(config) = dirs::config_dir() {
            rtp.push(config.join("rneovim/after").to_string_lossy().to_string());
        }
        options.insert("runtimepath".to_string(), OptionValue::String(rtp.join(",")));
        options.insert("columns".to_string(), OptionValue::Int(80));
        options.insert("lines".to_string(), OptionValue::Int(24));

        let buf = Rc::new(RefCell::new(Buffer::new()));
        let win = Window::new(buf.clone());
        let tab = TabPage::new(win);

        let mut globals = HashMap::new();
        globals.insert("did_load_filetypes".to_string(), crate::nvim::eval::TypVal::Number(1));
        globals.insert("mapleader".to_string(), crate::nvim::eval::TypVal::String(" ".to_string()));

        Self {
            mode: Mode::Normal,
            visual_mode: None,
            visual_anchor: None,
            buffers: vec![buf],
            tabpages: vec![tab],
            current_tab_idx: 0,
            messages: Vec::new(),
            registers: HashMap::new(),
            macros: HashMap::new(),
            macro_recording: None,
            options,

            search_start_cursor: None,
            last_change: None,
            last_insert_cursor: None,
            last_char_search: None,
            last_search: None,
            last_search_query: None,
            pending_key: None,
            input_buffer: String::new(),
            pending_register: None,
            pending_op: None,
            pending_count: None,
            pum_items: None,
            pum_index: None,
            alternate_file: None,
            sender: None,
            jobs: HashMap::new(),
            highlight_groups: HashMap::new(),
            active_events: HashMap::new(),
            next_event_id: 1,
            job_outputs: HashMap::new(),
            lsp_client: None,
            ts_manager: None,
            profiling_log: None,

            autocmds: HashMap::new(),
            user_commands: HashMap::new(),
            abbreviations: HashMap::new(),
            highlighter: SyntaxHighlighter::new_rust(),
            grid: Grid::new(80, 24),
            cmdline: String::new(),
            cmdline_cursor: 0,
            cmd_history: Vec::new(),
            cmd_history_idx: None,
            keymap: crate::nvim::keymap::KeymapEngine::new(),
            quickfix_list: Vec::new(),
            globals,
            quit: false,
            vim_did_enter: false,
            lua_env: Rc::new(LuaEnv::new()),
            eval_context: EvalContext::new(),
            open_files: HashMap::new(),
            active_scandirs: HashMap::new(),
            next_scandir_id: 1,
            next_fd: 10, // 0,1,2などは避ける
            next_win_id: 1000,
            pending_callbacks: Vec::new(),
            check_callbacks: Vec::new(),
            config_dir: None,
        }
    }

    pub fn current_tabpage(&self) -> &TabPage { &self.tabpages[self.current_tab_idx] }
    pub fn current_tabpage_mut(&mut self) -> &mut TabPage { &mut self.tabpages[self.current_tab_idx] }
    pub fn current_window(&self) -> &Window {
        let tab = &self.tabpages[self.current_tab_idx];
        &tab.windows[tab.current_win_idx]
    }
    pub fn current_window_mut(&mut self) -> &mut Window {
        let tab = &mut self.tabpages[self.current_tab_idx];
        &mut tab.windows[tab.current_win_idx]
    }

    pub fn redraw(&mut self) {
        self.grid.clear();
        
        // 1. 通常ウィンドウの描画
        let windows = self.current_tabpage().windows.clone();
        for win in &windows {
            if !win.is_floating() {
                self.draw_window(win);
            }
        }
        
        // 2. フローティングウィンドウの描画 (オーバーレイ)
        for win in &windows {
            if win.is_floating() {
                self.draw_window(win);
            }
        }

        // ステータスラインとコマンドラインの描画
        let status_row = self.grid.height.saturating_sub(2);
        let status_text = self.build_statusline();
        let (st_fg, st_bg) = (self.get_color_from_hl("StatusLine", true), self.get_color_from_hl("StatusLine", false));
        self.grid.put_str(status_row, 0, &status_text, st_fg, st_bg, true);

        let cmd_row = self.grid.height.saturating_sub(1);
        if self.mode == Mode::CommandLine || self.mode == Mode::Search {
            let prefix = if self.mode == Mode::CommandLine { ":" } else { "/" };
            self.grid.put_str(cmd_row, 0, prefix, Color::White, Color::Default, false);
            self.grid.put_str(cmd_row, 1, &self.cmdline, Color::White, Color::Default, false);
        } else if !self.messages.is_empty() {
            let msg = self.messages.last().unwrap();
            self.grid.put_str(cmd_row, 0, msg, Color::White, Color::Default, false);
        }
    }

    fn get_color_from_hl(&self, name: &str, is_fg: bool) -> Color {
        if let Some(attrs) = self.highlight_groups.get(name) {
            let val = if is_fg { attrs.fg } else { attrs.bg };
            if let Some(c) = val {
                return Color::from_u32(c);
            }
        }
        if is_fg { Color::White } else { Color::Default }
    }

    fn draw_border(&mut self, row: usize, col: usize, width: usize, height: usize, style: &str) {
        let (tl, tr, bl, br, v, h) = match style {
            "single" => ('┌', '┐', '└', '┘', '│', '─'),
            "double" => ('╔', '╗', '╚', '╝', '║', '═'),
            "rounded" => ('╭', '╮', '╰', '╯', '│', '─'),
            _ => ('+', '+', '+', '+', '|', '-'),
        };
        
        let fg = Color::White;
        let bg = Color::Default;

        if row < self.grid.height && col < self.grid.width { self.grid.put_char(row, col, tl, fg, bg, false); }
        if row < self.grid.height && col + width + 1 < self.grid.width { self.grid.put_char(row, col + width + 1, tr, fg, bg, false); }
        if row + height + 1 < self.grid.height && col < self.grid.width { self.grid.put_char(row + height + 1, col, bl, fg, bg, false); }
        if row + height + 1 < self.grid.height && col + width + 1 < self.grid.width { self.grid.put_char(row + height + 1, col + width + 1, br, fg, bg, false); }
        
        for i in 1..=width {
            if row < self.grid.height && col + i < self.grid.width { self.grid.put_char(row, col + i, h, fg, bg, false); }
            if row + height + 1 < self.grid.height && col + i < self.grid.width { self.grid.put_char(row + height + 1, col + i, h, fg, bg, false); }
        }
        for i in 1..=height {
            if row + i < self.grid.height && col < self.grid.width { self.grid.put_char(row + i, col, v, fg, bg, false); }
            if row + i < self.grid.height && col + width + 1 < self.grid.width { self.grid.put_char(row + i, col + width + 1, v, fg, bg, false); }
        }
    }

    fn draw_window(&mut self, win: &Window) {
        let buf = win.buffer();
        let b = buf.borrow();
        let cursor = win.cursor();
        self.log(&format!("DRAW_WINDOW: id={}, buf={}, lines={}, namespaces={}, marks={}", 
            win.id(), b.id(), b.line_count(), b.extmark_manager.ns_count(), b.extmark_manager.total_mark_count()));
        
        let (mut win_row, mut win_col, win_width, win_height) = if let Some(config) = &win.config {
            let (r, c) = match config.relative.as_str() {
                "editor" => (config.row as usize, config.col as usize),
                "win" => {
                    // TODO: Find window position. For now, assume editor relative.
                    (config.row as usize, config.col as usize)
                },
                "cursor" => {
                    let cur = self.current_window().cursor();
                    (cur.row + config.row as usize, cur.col + config.col as usize)
                },
                _ => (config.row as usize, config.col as usize),
            };
            (r, c, config.width, config.height)
        } else {
            (0, 0, self.grid.width, self.grid.height.saturating_sub(2))
        };

        if let Some(config) = &win.config {
            if let Some(border) = &config.border {
                if border != "none" {
                    self.draw_border(win_row, win_col, win_width, win_height, border);
                    win_row += 1;
                    win_col += 1;
                }
            }
        }

        for i in 0..win_height {
            let lnum = win.topline() + i;
            let mut col = 0;

            // 1. バッファのテキスト描画
            if let Some(line) = b.get_line(lnum) {
                for c in line.chars() {
                    if col >= win_width { break; }
                    let is_cursor = (self.mode != Mode::CommandLine && self.mode != Mode::Search) 
                                    && lnum == cursor.row && col == cursor.col;
                    let (fg, bg) = if is_cursor { (Color::Black, Color::White) } else { (Color::White, Color::Default) };
                    self.grid.put_char(win_row + i, win_col + col, c, fg, bg, false);
                    col += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            } else if win_row + i < self.grid.height && win_col < self.grid.width && !win.is_floating() {
                self.grid.put_char(win_row + i, win_col, '~', Color::Blue, Color::Default, false);
            }

            // 2. Extmark (Virtual Text) の描画 - テキストがなくても描画する
            let marks = b.extmark_manager.get_extmarks_in_range(-1, lnum, 0, lnum, 9999);
            for m in marks {
                // virt_text_pos が eol, overlay, delay のいずれかの場合に描画
                for (vt_text, vt_hl) in m.virt_text {
                    let vt_col = if m.virt_text_pos == "overlay" { m.col } else { col };
                    let mut current_vt_col = vt_col;
                    
                    let fg = self.get_color_from_hl(&vt_hl, true);
                    let bg = self.get_color_from_hl(&vt_hl, false);
                    
                    for vc in vt_text.chars() {
                        if current_vt_col >= win_width { break; }
                        self.grid.put_char(win_row + i, win_col + current_vt_col, vc, fg, bg, false);
                        current_vt_col += unicode_width::UnicodeWidthChar::width(vc).unwrap_or(1);
                    }
                    if m.virt_text_pos != "overlay" { col = current_vt_col; }
                }
            }
        }
    }

    fn build_statusline(&self) -> String {
        let b = self.current_window().buffer();
        let b = b.borrow();
        let cur = self.current_window().cursor();
        let mut res = match self.options.get("statusline") {
            Some(OptionValue::String(s)) => s.clone(),
            _ => format!(" {} [%L] %y %r %m  %p%%  %l:%c ", b.name().unwrap_or("[No Name]")),
        };
        res = res.replace("%L", &b.line_count().to_string());
        res = res.replace("%l", &cur.row.to_string());
        res = res.replace("%c", &cur.col.to_string());
        res = res.replace("%m", if b.is_modified() { "[+]" } else { "" });

        // %y: Filetype
        let ft = if let Some(OptionValue::String(s)) = b.get_option("filetype") {
            if s.is_empty() { "".to_string() } else { format!("[{}]", s) }
        } else {
            // Fallback: check file extension from name
            b.name().and_then(|name| {
                std::path::Path::new(name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!("[{}]", ext))
            }).unwrap_or_else(|| "".to_string())
        };
        res = res.replace("%y", &ft);

        // %r: Readonly / Writable (for debugging)
        let ro = if b.is_readonly() {
            "[RO]"
        } else {
            "[RW]"
        };
        res = res.replace("%r", ro);

        // %p: Percentage through file
        let pct = if b.line_count() > 1 {
            ((cur.row - 1) * 100) / (b.line_count() - 1)
        } else if b.line_count() == 1 {
            100
        } else {
            0
        };
        res = res.replace("%p", &pct.to_string());
        res = res.replace("%%", "%");

        res
    }

    pub fn set_cmdline(&mut self, text: String) {
        self.cmdline = text;
    }

    pub fn cmdline(&self) -> &str { &self.cmdline }

    pub fn set_mode(&mut self, mode: Mode) {
        let old_mode = self.mode;
        self.mode = mode;
        
        // モード遷移時のUndoグループ管理
        match (old_mode, mode) {
            (Mode::Normal, Mode::Insert) | (Mode::Normal, Mode::Visual(_)) => {
                let buf = self.current_window().buffer();
                buf.borrow_mut().start_undo_group();
            }
            (Mode::Insert, Mode::Normal) | (Mode::Visual(_), Mode::Normal) | (Mode::BlockInsert { .. }, Mode::Normal) => {
                let buf = self.current_window().buffer();
                buf.borrow_mut().end_undo_group();
            }
            // BlockInsert開始時は Visual からの移行なので、既にグループは開始されているはず
            _ => {}
        }

        if mode == Mode::CommandLine || mode == Mode::Search {
            self.cmdline.clear();
            self.cmdline_cursor = 0;
        } else {
            // 他のモードに移行した際もコマンドライン状態をリセット
            self.cmdline.clear();
            self.cmdline_cursor = 0;
        }
    }

    pub fn process_callbacks(&mut self) {
        let callbacks = std::mem::take(&mut self.pending_callbacks);
        if callbacks.is_empty() { return; }
        for id in callbacks {
            let env = self.lua_env.clone();
            self.log(&format!("PROCESS_CALLBACK: id={}", id));
            if let Err(e) = env.execute_callback(id) {
                self.log(&format!("Callback error (id={}): {}", id, e));
            }
            let _ = env.lua.gc_collect();
        }
    }

    pub fn step(&mut self) {
        let now = std::time::Instant::now();
        let mut to_run = Vec::new();

        let mut to_remove = Vec::new();
        for (id, ev) in &mut self.active_events {
            if now >= ev.next_run {
                to_run.push(ev.callback_id);
                if let Some(rep) = ev.repeat {
                    ev.next_run = now + std::time::Duration::from_millis(rep);
                } else {
                    to_remove.push(*id);
                }
            }
        }
        
        for &id in &self.check_callbacks {
            to_run.push(id);
        }

        for id in to_remove { self.active_events.remove(&id); }

        for cb in to_run {
            self.pending_callbacks.push(cb);
        }

        self.process_callbacks();
    }

    pub fn mode(&self) -> Mode { self.mode }

    pub fn quit(&mut self) { self.quit = true; }

    pub fn should_quit(&self) -> bool { self.quit }

    pub fn init_plugins(&mut self) {
        let env = self.lua_env.clone();
        env.load_plugins(self, None);
    }

    pub fn get_option_bool(&self, name: &str) -> bool {
        match self.options.get(name) { Some(OptionValue::Bool(b)) => *b, _ => false }
    }

    pub fn log(&self, msg: &str) {
        let entry = format!("[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), msg);
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("rneovim.log")
        {
            let _ = writeln!(file, "{}", entry);
        }
    }
}
