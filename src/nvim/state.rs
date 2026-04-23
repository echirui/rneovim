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

    pub search_start_cursor: Option<Cursor>,
    pub last_change: Option<Request>,
    pub last_insert_cursor: Option<Cursor>,
    pub last_char_search: Option<Request>,
    pub last_search: Option<String>,
    pub last_search_query: Option<String>,
    pub pending_key: Option<char>,
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
    pub user_commands: HashMap<String, mlua::RegistryKey>,
    pub abbreviations: HashMap<String, String>,
    pub highlighter: SyntaxHighlighter,
    pub grid: Grid,
    pub cmdline: String,
    pub cmdline_cursor: usize,
    pub cmd_history: Vec<String>,
    pub cmd_history_idx: Option<usize>,
    pub keymap: crate::nvim::keymap::KeymapEngine,
    pub quickfix_list: Vec<(String, usize, String)>, // (file, lnum, text)
    pub globals: HashMap<String, String>, // Simple global variables for now
    pub quit: bool,
    pub vim_did_enter: bool,
    pub lua_env: Rc<RefCell<LuaEnv>>,
    pub eval_context: EvalContext,
}

#[derive(Clone)]
pub enum AutoCmdCallback {
    Command(String),
    Lua(Rc<mlua::RegistryKey>),
}

#[derive(Clone)]
pub struct AutoCmd {
    pub callback: AutoCmdCallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AutoCmdEvent {
    BufReadPost,
    BufWritePost,
    BufEnter,
    BufLeave,
    VimEnter,
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
        
        let mut rtp = Vec::new();
        if let Some(config) = dirs::config_dir() {
            rtp.push(config.join("rneovim").to_string_lossy().to_string());
        }
        if let Some(data) = dirs::data_dir() {
            rtp.push(data.join("rneovim/site").to_string_lossy().to_string());
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

        let buf = Rc::new(RefCell::new(Buffer::new()));
        let win = Window::new(buf.clone());
        let tab = TabPage::new(win);

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
            pending_register: None,
            pending_op: None,
            pending_count: None,
            pum_items: None,
            pum_index: None,
            alternate_file: None,
            sender: None,
            jobs: HashMap::new(),
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
            globals: HashMap::new(),
            quit: false,
            vim_did_enter: false,
            lua_env: Rc::new(RefCell::new(LuaEnv::new())),
            eval_context: EvalContext::new(),
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
        self.grid.force_redraw();
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
        self.grid.put_str(status_row, 0, &status_text, Color::Black, Color::White, true);

        let cmd_row = self.grid.height.saturating_sub(1);
        if self.mode == Mode::CommandLine || self.mode == Mode::Search {
            let prefix = if self.mode == Mode::CommandLine { ":" } else { "/" };
            self.grid.put_str(cmd_row, 0, prefix, Color::White, Color::Default, false);
            self.grid.put_str(cmd_row, 1, &self.cmdline, Color::White, Color::Default, false);
        } else if !self.messages.is_empty() {
            let msg = self.messages.last().unwrap();
            self.grid.put_str(cmd_row, 0, msg, Color::White, Color::Default, false);
        }
        self.grid.flush();
    }

    fn draw_window(&mut self, win: &Window) {
        let buf = win.buffer();
        let b = buf.borrow();
        let cursor = win.cursor();
        
        let (win_row, win_col, win_width, win_height) = if let Some(config) = &win.config {
            (config.row, config.col, config.width, config.height)
        } else {
            (0, 0, self.grid.width, self.grid.height.saturating_sub(2))
        };

        for i in 0..win_height {
            let lnum = i + 1; // 簡易的に1行目から表示
            if let Some(line) = b.get_line(lnum) {
                let mut col = 0;
                for c in line.chars() {
                    if col >= win_width { break; }
                    let is_cursor = (self.mode != Mode::CommandLine && self.mode != Mode::Search) 
                                    && lnum == cursor.row && col == cursor.col;
                    let (fg, bg) = if is_cursor { (Color::Black, Color::White) } else { (Color::White, Color::Default) };
                    self.grid.put_char(win_row + i, win_col + col, c, fg, bg, false);
                    col += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            } else {
                self.grid.put_char(win_row + i, win_col, '~', Color::Blue, Color::Default, false);
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

    pub fn mode(&self) -> Mode { self.mode }

    pub fn quit(&mut self) { self.quit = true; }

    pub fn should_quit(&self) -> bool { self.quit }

    pub fn init_plugins(&mut self) {
        let env = self.lua_env.clone();
        env.borrow().load_plugins(self, None);
    }

    pub fn get_option_bool(&self, name: &str) -> bool {
        match self.options.get(name) { Some(OptionValue::Bool(b)) => *b, _ => false }
    }

    pub fn log(&self, msg: &str) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("rneovim.log") 
        {
            let _ = writeln!(file, "[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), msg);
        }
    }
}
