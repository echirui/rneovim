use crate::nvim::buffer::Buffer;
use crate::nvim::window::{Window, Cursor};
use crate::nvim::ui::grid::{Grid, Color};
use crate::nvim::eval::{EvalContext, TypVal};
use crate::nvim::lua::LuaEnv;
use crate::nvim::request::{Request, Operator};
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use crate::nvim::syntax::SyntaxHighlighter;
use crate::nvim::event::event_loop::EventCallback;
use std::sync::mpsc::Sender;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    Insert,
    InsertLiteral,
    InsertDigraph1(char),
    InsertWaitRegister,
    Replace,
    VirtualReplace,
    WaitReplaceChar,
    Visual(VisualMode),
    CommandLine,
    CommandLineWaitRegister,
    WaitMacroReg,
    Search,
    BlockInsert { append: bool, anchor: Cursor, cursor: Cursor },
    InsertOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VisualMode {
    Char,
    Line,
    Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Bool(bool),
    Int(i32),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutoCmdEvent {
    BufWritePost,
    BufReadPost,
    CursorMoved,
    InsertEnter,
    InsertLeave,
    VimEnter,
}

pub struct TabPage {
    pub windows: Vec<Window>,
    pub current_win_idx: usize,
}

impl TabPage {
    pub fn new(win: Window) -> Self {
        Self {
            windows: vec![win],
            current_win_idx: 0,
        }
    }

    pub fn current_window(&self) -> &Window {
        &self.windows[self.current_win_idx]
    }

    pub fn current_window_mut(&mut self) -> &mut Window {
        &mut self.windows[self.current_win_idx]
    }
}

pub struct VimState {
    pub mode: Mode,
    pub buffers: Vec<Rc<RefCell<Buffer>>>,
    pub tabpages: Vec<TabPage>,
    pub current_tab_idx: usize,
    pub eval_context: EvalContext,
    pub lua_env: LuaEnv,
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
    pub user_commands: HashMap<String, String>,
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
    pub quit: bool,
}

#[derive(Debug, Clone)]
pub struct LastCharSearch {
    pub target: char,
    pub forward: bool,
    pub till: bool,
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
        options.insert("incsearch".to_string(), OptionValue::Bool(true));

        let mut eval_context = EvalContext::new();
        eval_context.register_builtin("getline", |state, args| {
            if let Some(TypVal::String(s)) = args.first() {
                let lnum = match s.as_str() {
                    "." => state.current_window().cursor().row,
                    "$" => state.current_window().buffer().borrow().line_count(),
                    _ => s.parse::<usize>().unwrap_or(0),
                };
                if lnum > 0 {
                    if let Some(line) = state.current_window().buffer().borrow().get_line(lnum) {
                        return Ok(TypVal::String(line.to_string()));
                    }
                }
            } else if let Some(TypVal::Number(n)) = args.first() {
                let lnum = *n as usize;
                if lnum > 0 {
                    if let Some(line) = state.current_window().buffer().borrow().get_line(lnum) {
                        return Ok(TypVal::String(line.to_string()));
                    }
                }
            }
            Ok(TypVal::String("".to_string()))
        });
        eval_context.register_builtin("setline", |state, args| {
            if let (Some(lnum_arg), Some(TypVal::String(new_line))) = (args.get(0), args.get(1)) {
                let lnum = match lnum_arg {
                    TypVal::String(s) if s == "." => state.current_window().cursor().row,
                    TypVal::String(s) if s == "$" => state.current_window().buffer().borrow().line_count(),
                    TypVal::String(s) => s.parse::<usize>().unwrap_or(0),
                    TypVal::Number(n) => *n as usize,
                    _ => 0,
                };
                if lnum > 0 {
                    let _ = state.current_window().buffer().borrow_mut().set_line(lnum, 0, new_line);
                }
            }
            Ok(TypVal::Number(0))
        });

        Self {
            mode: Mode::Normal,
            buffers: vec![buf],
            tabpages: vec![tabpage],
            current_tab_idx: 0,
            eval_context,
            lua_env: LuaEnv::new(),
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
            quit: false,
        }
    }

    pub fn current_tabpage(&self) -> &TabPage { &self.tabpages[self.current_tab_idx] }
    pub fn current_tabpage_mut(&mut self) -> &mut TabPage { &mut self.tabpages[self.current_tab_idx] }
    pub fn current_window(&self) -> &Window { self.current_tabpage().current_window() }
    pub fn current_window_mut(&mut self) -> &mut Window { self.current_tabpage_mut().current_window_mut() }
    pub fn grid_mut(&mut self) -> &mut Grid { &mut self.grid }

    pub fn redraw(&mut self) {
        self.grid.clear();
        let (win_cursor, win_topline, win_height, buf_ref) = {
            let win = self.current_window();
            (win.cursor(), win.topline(), win.height(), win.buffer())
        };
        let b = buf_ref.borrow();
        
        let (mode_label, status_bg) = match self.mode {
            Mode::Normal => (" NORMAL ", Color::White),
            Mode::Insert | Mode::InsertWaitRegister => (" INSERT ", Color::Green),
            Mode::Replace | Mode::VirtualReplace => (" REPLACE ", Color::Green),
            Mode::WaitReplaceChar => (" REPLACE ", Color::Green),
            Mode::InsertLiteral => (" LITERAL ", Color::Green),
            Mode::InsertDigraph1(_) => (" DIGRAPH ", Color::Green),
            Mode::Visual(_) => (" VISUAL ", Color::Magenta),
            Mode::CommandLine | Mode::CommandLineWaitRegister | Mode::WaitMacroReg => (" COMMAND ", Color::Yellow),
            Mode::Search => (" SEARCH ", Color::Yellow),
            Mode::BlockInsert { .. } => (" (insert) BLOCK ", Color::Magenta),
            Mode::InsertOnce => (" (once) ", Color::Cyan),
        };
        let tab_info = format!(" Tab {}/{} ", self.current_tab_idx + 1, self.tabpages.len());
        let file_name = b.name().unwrap_or("[No Name]");
        let recording_status = if let Some((reg, _)) = self.macro_recording { format!(" recording @{} ", reg) } else { "".to_string() };
        let stats = format!(" {:>3}:{:>2} | Total: {:>4} lines ", win_cursor.row, win_cursor.col, b.line_count());
        let status = format!("{tab_info}| {file_name}{recording_status}{stats}");
        
        let show_number = match self.options.get("number") { Some(crate::nvim::state::OptionValue::Bool(b)) => *b, _ => false };
        let show_signcolumn = match self.options.get("signcolumn") { Some(crate::nvim::state::OptionValue::Bool(b)) => *b, _ => false };
        let sign_margin = if show_signcolumn { 2 } else { 0 };
        let num_margin = if show_number { 4 } else { 0 };
        let base_margin = 2; 
        let margin = sign_margin + num_margin + base_margin;

        let folds = { let win = self.current_window(); win.folds.clone() };
        let mut row = 0;
        let mut lnum = win_topline;

        while row < win_height {
            if lnum <= b.line_count() {
                let mut folded_range = None;
                for &(s, e) in &folds { if lnum >= s && lnum <= e { folded_range = Some((s, e)); break; } }
                if let Some((s, e)) = folded_range {
                    let fold_text = format!("+-- {:>3} lines: ", e - s + 1);
                    self.grid.put_str(row, 0, &fold_text, Color::Blue, Color::Default, false);
                    lnum = e + 1; row += 1; continue;
                }
                if let Some(line) = b.get_line(lnum) {
                    let is_current_line = lnum == win_cursor.row;
                    self.lua_env.trigger_on_line();
                    let mut current_offset = 0;
                    if show_signcolumn {
                        if let Some(sign) = b.get_sign(lnum) { self.grid.put_str(row, current_offset, sign, Color::Red, Color::Default, true); }
                        else { self.grid.put_str(row, current_offset, "  ", Color::White, Color::Default, false); }
                        current_offset += 2;
                    }
                    if is_current_line && self.mode != Mode::CommandLine && self.mode != Mode::Search {
                        self.grid.put_str(row, current_offset, "> ", Color::Blue, Color::Default, true);
                    } else { self.grid.put_str(row, current_offset, "  ", Color::White, Color::Default, false); }
                    current_offset += 2;
                    if show_number {
                        let num_str = format!("{:>3} ", lnum);
                        self.grid.put_str(row, current_offset, &num_str, Color::Yellow, Color::Default, false);
                    }
                    let highlights = self.highlighter.highlight_line(line);
                    let mut current_vcol = 0;
                    
                    // RTL判定
                    let has_rtl = line.chars().any(|c| {
                        let class = unicode_bidi::bidi_class(c);
                        matches!(class, unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL | unicode_bidi::BidiClass::AN)
                    });

                    if has_rtl {
                        self.grid.put_str_bidi(row, margin, line, Color::Default, Color::Default, false);
                        // カーソル表示などの最小限の処理
                        if is_current_line {
                            // RTLの場合は本来の位置計算が複雑だが、一旦簡易的に
                            // ...
                        }
                    } else {
                        for (c_idx, c) in line.chars().enumerate() {
                            let c_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                            let mut fg = Color::Default; let mut bg = Color::Default; let mut bold = false;
                            for (start, end, group) in &highlights { if c_idx >= *start && c_idx < *end { fg = group.color(); break; } }
                            if let Mode::Visual(ref vmode) = self.mode {
                                if let Some(anchor) = self.visual_anchor {
                                    match vmode {
                                        VisualMode::Char => {
                                            let (v_start, v_end) = if anchor.row < win_cursor.row || (anchor.row == win_cursor.row && anchor.col <= win_cursor.col) {
                                                (anchor, win_cursor)
                                            } else {
                                                (win_cursor, anchor)
                                            };

                                            if lnum > v_start.row && lnum < v_end.row {
                                                // 行間全体
                                                bg = Color::Blue; fg = Color::White;
                                            } else if lnum == v_start.row && lnum == v_end.row {
                                                // 同一行内
                                                if c_idx >= v_start.col && c_idx <= v_end.col { bg = Color::Blue; fg = Color::White; }
                                            } else if lnum == v_start.row {
                                                // 開始行
                                                if c_idx >= v_start.col { bg = Color::Blue; fg = Color::White; }
                                            } else if lnum == v_end.row {
                                                // 終了行
                                                if c_idx <= v_end.col { bg = Color::Blue; fg = Color::White; }
                                            }
                                        }
                                        VisualMode::Line => {
                                            let min_row = anchor.row.min(win_cursor.row);
                                            let max_row = anchor.row.max(win_cursor.row);
                                            if lnum >= min_row && lnum <= max_row { bg = Color::Blue; fg = Color::White; }
                                        }
                                        VisualMode::Block => {
                                            let min_row = anchor.row.min(win_cursor.row);
                                            let max_row = anchor.row.max(win_cursor.row);
                                            let min_col = anchor.col.min(win_cursor.col);
                                            let max_col = anchor.col.max(win_cursor.col);
                                            if lnum >= min_row && lnum <= max_row && c_idx >= min_col && c_idx <= max_col { bg = Color::Blue; fg = Color::White; }
                                        }
                                    }
                                }
                            }
                            if is_current_line && c_idx == win_cursor.col && self.mode != Mode::CommandLine && self.mode != Mode::Search {
                                fg = Color::Black; bg = Color::Cyan; bold = true;
                            }
                            self.grid.put_char(row, margin + current_vcol, c, fg, bg, bold);
                            current_vcol += c_width;
                        }
                    }
                    if is_current_line && win_cursor.col >= line.chars().count() && self.mode != Mode::CommandLine && self.mode != Mode::Search {
                        // RTLでない場合のみ末尾カーソルを表示（RTLは位置が逆転するため）
                        if !has_rtl {
                            self.grid.put_char(row, margin + current_vcol, ' ', Color::Black, Color::Cyan, true);
                        }
                    }
                    if let Some(vt) = b.get_virtual_text(lnum) {
                        let vt_start_col = margin + line.chars().count().max(win_cursor.col + 1);
                        self.grid.put_str(row, vt_start_col, vt, Color::Blue, Color::Default, false);
                    }
                }
            } else { self.grid.put_str(row, 0, "~", Color::Blue, Color::Default, false); }
            row += 1; lnum += 1;
        }

        let status_row = self.grid.height.saturating_sub(2);
        self.grid.put_str(status_row, 0, mode_label, Color::Black, status_bg, true);
        let mode_len = mode_label.len();
        self.grid.put_str(status_row, mode_len, &status, Color::Black, Color::White, false);
        for c in (mode_len + status.len())..self.grid.width { self.grid.put_char(status_row, c, ' ', Color::Black, Color::White, false); }

        let cmd_row = self.grid.height.saturating_sub(1);
        if self.mode == Mode::CommandLine || self.mode == Mode::Search {
            let prefix = if self.mode == Mode::CommandLine { ":" } else { "/" };
            self.grid.put_str(cmd_row, 0, prefix, Color::White, Color::Default, true);
            let mut current_vcol = 1;
            let cmd_chars: Vec<char> = self.cmdline.chars().collect();
            for (i, &c) in cmd_chars.iter().enumerate() {
                let c_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                let (fg, bg, bold) = if i == self.cmdline_cursor {
                    (Color::Black, Color::Cyan, true)
                } else {
                    (Color::White, Color::Default, false)
                };
                self.grid.put_char(cmd_row, current_vcol, c, fg, bg, bold);
                current_vcol += c_width;
            }
            if self.cmdline_cursor >= cmd_chars.len() {
                self.grid.put_char(cmd_row, current_vcol, ' ', Color::Black, Color::Cyan, true);
            }
        } else {
            let msg = " (Press ':' cmd, '/' search, 'v' visual, 'i' insert, 'u' undo) ";
            self.grid.put_str(cmd_row, 0, msg, Color::White, Color::Black, false);
        }

        if let Some(items) = &self.pum_items {
            let cursor = self.current_window().cursor();
            let mut pum_row = cursor.row.saturating_sub(win_topline) + 1;
            if pum_row + items.len() > win_height { pum_row = (cursor.row.saturating_sub(win_topline)).saturating_sub(items.len()); }
            for (i, item) in items.iter().enumerate() {
                let r = pum_row + i;
                if r < win_height {
                    let is_selected = Some(i) == self.pum_index;
                    let (fg, bg) = if is_selected { (Color::Black, Color::Yellow) } else { (Color::White, Color::Magenta) };
                    let display_str = format!("{:<20}", item);
                    self.grid.put_str(r, margin + cursor.col, &display_str, fg, bg, false);
                }
            }
        }
        self.grid.flush();
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if let Mode::Visual(vmode) = self.mode {
            self.last_visual_anchor = self.visual_anchor;
            self.last_visual_cursor = Some(self.current_window().cursor());
            self.last_visual_mode = Some(vmode);
        }
        if matches!(self.mode, Mode::Insert | Mode::InsertLiteral | Mode::InsertDigraph1(_)) && !matches!(mode, Mode::Insert | Mode::InsertLiteral | Mode::InsertDigraph1(_)) {
            self.last_insert_cursor = Some(self.current_window().cursor());
        }

        // アンドゥグループの管理
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
            self.cmd_history_idx = None;
        }
    }

    pub fn mode(&self) -> Mode { self.mode.clone() }
    pub fn cmdline(&self) -> &str { &self.cmdline }
    pub fn set_cmdline(&mut self, s: String) { self.cmdline = s; }
    pub fn push_cmdline(&mut self, c: char) { self.cmdline.push(c); }
    pub fn append_cmdline(&mut self, c: char) { self.cmdline.push(c); }
    pub fn pop_cmdline(&mut self) -> Option<char> { self.cmdline.pop() }
    pub fn quit(&mut self) { self.quit = true; }
    pub fn should_quit(&self) -> bool { self.quit }

    pub fn init_plugins(&self) {
        if let Err(e) = self.lua_env.register_api(self.buffers.clone(), self.sender.clone()) {
            eprintln!("Failed to register Lua API: {}", e);
        }
        self.lua_env.load_plugins(None);
    }

    pub fn buffers(&self) -> &Vec<Rc<RefCell<Buffer>>> { &self.buffers }

    pub fn format_statusline(&self) -> String {
        let b = self.current_window().buffer();
        let b = b.borrow();
        let cur = self.current_window().cursor();
        let mut res = match self.options.get("statusline") {
            Some(crate::nvim::state::OptionValue::String(s)) => s.clone(),
            _ => "File: %f | %l:%c | Total: %L".to_string(),
        };
        res = res.replace("%f", b.name().unwrap_or("[No Name]"));
        res = res.replace("%l", &cur.row.to_string());
        res = res.replace("%c", &cur.col.to_string());
        res = res.replace("%L", &b.line_count().to_string());
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_state_initialization() {
        let state = VimState::new();
        assert_eq!(state.mode, Mode::Normal);
        assert_eq!(state.buffers.len(), 1);
        assert_eq!(state.tabpages.len(), 1);
        assert!(!state.quit);
    }

    #[test]
    fn test_mode_transitions() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            for _ in 0..10 { b.append_line("          ").unwrap(); }
        }
        
        state.set_mode(Mode::Insert);
        assert_eq!(state.mode, Mode::Insert);
        
        state.set_mode(Mode::Normal);
        assert_eq!(state.mode, Mode::Normal);
        assert!(state.last_insert_cursor.is_some());

        state.current_window_mut().set_cursor(1, 5);
        state.set_mode(Mode::Visual(VisualMode::Char));
        state.visual_anchor = Some(Cursor { row: 1, col: 0 });
        
        state.set_mode(Mode::Normal);
        assert_eq!(state.last_visual_mode, Some(VisualMode::Char));
        assert_eq!(state.last_visual_anchor.unwrap().col, 0);
        assert_eq!(state.last_visual_cursor.unwrap().col, 5);
    }

    #[test]
    fn test_statusline_rendering() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            for _ in 0..10 { b.append_line("          ").unwrap(); }
        }
        state.current_window_mut().set_cursor(5, 10);
        
        let status = state.format_statusline();
        assert!(status.contains("5:10"));
        assert!(status.contains("[No Name]"));

        // Custom statusline
        state.options.insert("statusline".to_string(), OptionValue::String("POS: %l,%c".to_string()));
        let status = state.format_statusline();
        assert_eq!(status, "POS: 5,10");
    }

    #[test]
    fn test_tabpage_window_access() {
        let state = VimState::new();
        let tp = state.current_tabpage();
        assert_eq!(tp.windows.len(), 1);
        let win = state.current_window();
        assert_eq!(win.cursor().row, 1);
    }

    #[test]
    fn test_redraw_basic() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("Hello").unwrap();
        }
        state.redraw();
        
        // Check if "Hello" is in the grid
        // margin = 0 (sign) + 0 (num) + 2 (base) = 2
        assert_eq!(state.grid.get_char(0, 2), 'H');
        assert_eq!(state.grid.get_char(0, 3), 'e');
        assert_eq!(state.grid.get_char(0, 4), 'l');
    }

    #[test]
    fn test_vim_state_quit() {
        let mut state = VimState::new();
        assert!(!state.should_quit());
        state.quit();
        assert!(state.should_quit());
    }

    #[test]
    fn test_vim_state_cmdline() {
        let mut state = VimState::new();
        state.set_cmdline("test".to_string());
        assert_eq!(state.cmdline(), "test");
        state.push_cmdline('!');
        assert_eq!(state.cmdline(), "test!");
        assert_eq!(state.pop_cmdline(), Some('!'));
    }

    #[test]
    fn test_vim_state_options() {
        let mut state = VimState::new();
        state.options.insert("test".to_string(), OptionValue::Bool(true));
        assert!(matches!(state.options.get("test"), Some(OptionValue::Bool(true))));
    }

    #[test]
    fn test_vim_state_buffers() {
        let state = VimState::new();
        assert_eq!(state.buffers().len(), 1);
    }
}
