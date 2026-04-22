use std::rc::Rc;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;
use serde::{Serialize, Deserialize};
use crate::nvim::buffer::Buffer;
use crate::nvim::window::{Window, Cursor};
use crate::nvim::ui::grid::{Grid, Color};
use crate::nvim::eval::EvalContext;
use crate::nvim::lua::LuaEnv;
use crate::nvim::request::{Request, Operator};
use crate::nvim::event::event_loop::EventCallback;
use crate::nvim::syntax::SyntaxHighlighter;

pub struct VimState {
    pub mode: Mode,
    pub buffers: Vec<Rc<RefCell<Buffer>>>,
    pub tabpages: Vec<TabPage>,
    pub current_tab_idx: usize,
    pub eval_context: EvalContext,
    pub lua_env: Rc<RefCell<LuaEnv>>,
    pub visual_anchor: Option<Cursor>,
    pub last_visual_anchor: Option<Cursor>,
    pub last_visual_cursor: Option<Cursor>,
    pub last_visual_mode: Option<VisualMode>,
    pub last_insert_cursor: Option<Cursor>,
    pub registers: HashMap<char, String>,
    pub macros: HashMap<char, Vec<Request>>,
    pub macro_recording: Option<(char, Vec<Request>)>,
    pub options: HashMap<String, OptionValue>,

    pub autocmds: HashMap<AutoCmdEvent, Vec<String>>,
    pub user_commands: HashMap<String, mlua::RegistryKey>,
    pub abbreviations: HashMap<String, String>,
    pub highlighter: SyntaxHighlighter,
    pub grid: Grid,
    pub cmdline: String,
    pub cmdline_cursor: usize,
    pub messages: Vec<String>,
    pub last_search: Option<String>,
    pub last_substitute: Option<(String, String, String)>,
    pub last_char_search: Option<crate::nvim::state::LastCharSearch>,
    pub alternate_file: Option<String>,
    pub search_start_cursor: Option<Cursor>,
    pub last_change: Option<Request>,
    pub pending_key: Option<char>,
    pub pending_register: Option<char>,
    pub pending_op: Option<Operator>,
    pub pending_count: Option<u32>,
    pub pum_items: Option<Vec<String>>,
    pub pum_index: Option<usize>,
    pub jobs: HashMap<i32, std::process::Child>,
    pub job_outputs: HashMap<i32, Vec<String>>,
    pub sender: Option<Sender<EventCallback<VimState>>>,
    pub lsp_client: Option<crate::nvim::lsp::LspClient>,
    pub ts_manager: Option<crate::nvim::treesitter::TreesitterManager>,
    pub profiling_log: Option<String>,
    pub breakpoints: HashSet<usize>,
    pub jump_list: Vec<(String, Cursor)>,
    pub jump_idx: usize,
    pub cmd_history: Vec<String>,
    pub cmd_history_idx: Option<usize>,
    pub keymap: crate::nvim::keymap::KeymapEngine,
    pub quickfix_list: Vec<(String, usize, String)>, // (file, lnum, text)
    pub globals: HashMap<String, String>, // Simple global variables for now
    pub quit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AutoCmdEvent {
    BufReadPost,
    BufWritePost,
    BufEnter,
    FileType,
    InsertEnter,
    InsertLeave,
    VimEnter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    Insert,
    InsertOnce,
    InsertLiteral,
    InsertDigraph1(char),
    InsertWaitRegister,
    Visual(VisualMode),
    CommandLine,
    Search,
    CommandLineWaitRegister,
    Replace,
    VirtualReplace,
    WaitMacroReg,
    WaitReplaceChar,
    BlockInsert { append: bool, anchor: Cursor, cursor: Cursor },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualMode {
    Char,
    Line,
    Block,
}

#[derive(Debug, Clone)]
pub enum OptionValue {
    Bool(bool),
    Int(i64),
    String(String),
}

#[derive(Debug, Clone)]
pub struct LastCharSearch {
    pub target: char,
    pub forward: bool,
    pub till: bool,
}

pub struct TabPage {
    pub windows: Vec<Window>,
    pub current_win_idx: usize,
}

impl TabPage {
    pub fn new(window: Window) -> Self {
        Self {
            windows: vec![window],
            current_win_idx: 0,
        }
    }
}

impl VimState {
    pub fn new() -> Self {
        let buf = Rc::new(RefCell::new(Buffer::new()));
        let mut win = Window::new(Rc::clone(&buf));
        win.set_height(20);
        let tabpage = TabPage::new(win);
        let mut registers = HashMap::new();
        registers.insert('"', String::new());
        let mut options = HashMap::new();
        options.insert("number".to_string(), OptionValue::Bool(false));
        options.insert("tabstop".to_string(), OptionValue::Int(8));
        options.insert("statusline".to_string(), OptionValue::String("%f %m %y [%L lines]".to_string()));
        options.insert("signcolumn".to_string(), OptionValue::String("auto".to_string()));
        
        let mut rtp = Vec::new();
        if let Some(config) = dirs::config_dir() {
            rtp.push(config.join("rneovim").to_string_lossy().to_string());
        }
        if let Some(data) = dirs::data_dir() {
            rtp.push(data.join("rneovim/site").to_string_lossy().to_string());
        }
        options.insert("runtimepath".to_string(), OptionValue::String(rtp.join(",")));
        
        Self {
            mode: Mode::Normal,
            buffers: vec![Rc::clone(&buf)],
            tabpages: vec![tabpage],
            current_tab_idx: 0,
            eval_context: EvalContext::new(),
            lua_env: Rc::new(RefCell::new(LuaEnv::new())),
            visual_anchor: None,
            last_visual_anchor: None,
            last_visual_cursor: None,
            last_visual_mode: None,
            last_insert_cursor: None,
            registers,
            macros: HashMap::new(),
            macro_recording: None,
            options,
            autocmds: HashMap::new(),
            user_commands: HashMap::new(),
            abbreviations: HashMap::new(),
            highlighter: SyntaxHighlighter::new_rust(),
            grid: Grid::new(80, 24),
            cmdline: String::new(),
            cmdline_cursor: 0,
            messages: Vec::new(),
            last_search: None,
            last_substitute: None,
            last_char_search: None,
            alternate_file: None,
            search_start_cursor: None,
            last_change: None,
            pending_key: None,
            pending_register: None,
            pending_op: None,
            pending_count: None,
            pum_items: None,
            pum_index: None,
            jobs: HashMap::new(),
            job_outputs: HashMap::new(),
            sender: None,
            lsp_client: None,
            ts_manager: None,
            profiling_log: None,
            breakpoints: HashSet::new(),
            jump_list: Vec::new(),
            jump_idx: 0,
            cmd_history: Vec::new(),
            cmd_history_idx: None,
            keymap: crate::nvim::keymap::KeymapEngine::new(),
            quickfix_list: Vec::new(),
            globals: HashMap::new(),
            quit: false,
        }
    }

    pub fn current_tabpage(&self) -> &TabPage { &self.tabpages[self.current_tab_idx] }
    pub fn current_tabpage_mut(&mut self) -> &mut TabPage { &mut self.tabpages[self.current_tab_idx] }

    pub fn current_window(&self) -> &Window {
        &self.current_tabpage().windows[self.current_tabpage().current_win_idx]
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

        // 2. フローティングウィンドウの描画 (上に重ねる)
        for win in &windows {
            if win.is_floating() {
                self.draw_window(win);
            }
        }

        // ステータスラインとコマンドライン
        let mode_label = match self.mode {
            Mode::Normal => "-- NORMAL --",
            Mode::Insert | Mode::BlockInsert {..} => "-- INSERT --",
            Mode::Visual(VisualMode::Char) => "-- VISUAL --",
            Mode::Visual(VisualMode::Line) => "-- VISUAL LINE --",
            Mode::Visual(VisualMode::Block) => "-- VISUAL BLOCK --",
            Mode::CommandLine => ":",
            Mode::Search => "/",
            _ => "-- UNKNOWN --",
        };
        let status_bg = match self.mode {
            Mode::Insert | Mode::BlockInsert {..} => Color::Green,
            Mode::Visual(_) => Color::Magenta,
            Mode::CommandLine | Mode::Search => Color::Yellow,
            _ => Color::White,
        };

        let status_row = self.grid.height.saturating_sub(2);
        self.grid.put_str(status_row, 0, mode_label, Color::Black, status_bg, true);
        let status = self.format_statusline();
        self.grid.put_str(status_row, mode_label.len() + 1, &status, Color::White, Color::Black, false);
        
        let cmd_row = self.grid.height.saturating_sub(1);
        if self.mode == Mode::CommandLine || self.mode == Mode::Search {
            let prefix = if self.mode == Mode::CommandLine { ":" } else { "/" };
            self.grid.put_str(cmd_row, 0, prefix, Color::White, Color::Default, false);
            self.grid.put_str(cmd_row, 1, &self.cmdline, Color::White, Color::Default, false);
        } else if !self.messages.is_empty() {
            let msg = self.messages.last().unwrap();
            // 長いメッセージがステータスバーを壊さないように、cmd_row に描画
            self.grid.put_str(cmd_row, 0, msg, Color::White, Color::Default, false);
        }
        self.grid.flush();
    }

    fn draw_window(&mut self, win: &Window) {
        let buf = win.buffer();
        let b = buf.borrow();
        let win_cursor = win.cursor();
        let (win_row, win_col, win_w, win_h) = if let Some(config) = win.config() {
            (config.row, config.col, config.width, config.height)
        } else {
            (0, 0, self.grid.width, self.grid.height.saturating_sub(2))
        };

        let show_number = match self.options.get("number") { Some(OptionValue::Bool(b)) => *b, _ => false };
        let show_signcolumn = match self.options.get("signcolumn") { Some(OptionValue::String(s)) => s != "no", _ => false };
        let margin = (if show_number { 4 } else { 0 }) + (if show_signcolumn { 2 } else { 0 }) + 2;

        let mut row = 0;
        let mut lnum = win.topline();

        while row < win_h {
            if let Some(line) = b.get_line(lnum) {
                let is_current_line = lnum == win_cursor.row;
                let mut current_offset = 0;
                
                // サインカラムと行番号
                if show_signcolumn {
                    if let Some(sign) = b.get_sign(lnum) { self.grid.put_str(win_row + row, win_col + current_offset, sign, Color::Red, Color::Default, true); }
                    else { self.grid.put_str(win_row + row, win_col + current_offset, "  ", Color::White, Color::Default, false); }
                    current_offset += 2;
                }
                if is_current_line && !win.is_floating() {
                    self.grid.put_str(win_row + row, win_col + current_offset, "> ", Color::Blue, Color::Default, true);
                } else {
                    self.grid.put_str(win_row + row, win_col + current_offset, "  ", Color::White, Color::Default, false);
                }
                current_offset += 2;
                if show_number {
                    let num_str = format!("{:>3} ", lnum);
                    self.grid.put_str(win_row + row, win_col + current_offset, &num_str, Color::Yellow, Color::Default, false);
                }

                let highlights = self.highlighter.highlight_line(line);
                let mut current_vcol = 0;
                for (c_idx, c) in line.chars().enumerate() {
                    if current_vcol >= win_w.saturating_sub(margin) { break; }
                    let mut fg = Color::Default; let mut bg = if win.is_floating() { Color::Black } else { Color::Default };
                    for (start, end, group) in &highlights { if c_idx >= *start && c_idx < *end { fg = group.color(); break; } }
                    
                    if is_current_line && c_idx == win_cursor.col && self.current_window().id() == win.id() {
                        fg = Color::Black; bg = Color::Cyan;
                    }
                    self.grid.put_char(win_row + row, win_col + margin + current_vcol, c, fg, bg, false);
                    current_vcol += 1;
                }
                // 空白埋め (フローティングウィンドウ用)
                if win.is_floating() {
                    for vcol in current_vcol..win_w.saturating_sub(margin) {
                        self.grid.put_char(win_row + row, win_col + margin + vcol, ' ', Color::Default, Color::Black, false);
                    }
                }
            } else {
                self.grid.put_str(win_row + row, win_col, "~", Color::Blue, Color::Default, false);
            }
            row += 1; lnum += 1;
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if let Mode::Visual(vmode) = self.mode {
            self.last_visual_anchor = self.visual_anchor;
            self.last_visual_cursor = Some(self.current_window().cursor());
            self.last_visual_mode = Some(vmode);
        }
        let is_insert_like = |m: &Mode| matches!(m, Mode::Insert | Mode::Replace | Mode::VirtualReplace | Mode::BlockInsert {..} | Mode::InsertOnce | Mode::InsertLiteral | Mode::InsertDigraph1(_));
        let new_is_insert = is_insert_like(&mode);
        let old_is_insert = is_insert_like(&self.mode);
        if new_is_insert && !old_is_insert {
            self.current_window().buffer().borrow_mut().start_undo_group();
        } else if old_is_insert && !new_is_insert {
            self.current_window().buffer().borrow_mut().end_undo_group();
        }
        self.mode = mode;
        if matches!(self.mode, Mode::Normal) {
            self.pending_key = None;
            self.pending_op = None;
            self.pum_items = None;
        }
    }

    pub fn mode(&self) -> Mode { self.mode.clone() }
    pub fn cmdline(&self) -> &str { &self.cmdline }
    pub fn set_cmdline(&mut self, s: String) { self.cmdline = s; }
    pub fn push_cmdline(&mut self, c: char) { self.cmdline.push(c); }
    pub fn quit(&mut self) { self.quit = true; }
    pub fn should_quit(&self) -> bool { self.quit }

    pub fn init_plugins(&mut self) {
        let env = self.lua_env.clone();
        if let Err(e) = env.borrow().register_api(self.buffers.clone(), self.sender.clone()) {
            eprintln!("Failed to register Lua API: {}", e);
        }
        env.borrow().load_plugins(self, None);
    }

    pub fn format_statusline(&self) -> String {
        let b = self.current_window().buffer();
        let b = b.borrow();
        let cur = self.current_window().cursor();
        let mut res = match self.options.get("statusline") {
            Some(OptionValue::String(s)) => s.clone(),
            _ => "%f %m %y [%L lines]".to_string(),
        };
        let name = b.name().unwrap_or("[No Name]");
        res = res.replace("%f", name);
        res = res.replace("%m", if b.is_modified() { "[+]" } else { "" });
        res = res.replace("%y", "[rust]");
        res = res.replace("%L", &b.line_count().to_string());
        res = res.replace("%l", &cur.row.to_string());
        res = res.replace("%c", &cur.col.to_string());
        res
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_vim_state_initialization() { let state = VimState::new(); assert_eq!(state.mode, Mode::Normal); }
}
