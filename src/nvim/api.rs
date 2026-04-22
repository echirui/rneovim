use crate::nvim::state::{VimState, AutoCmdEvent};
use crate::nvim::window::TabPage;
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::os::fs;

use crate::nvim::handlers;

pub fn trigger_autocmd(state: &mut VimState, event: AutoCmdEvent) {
    state.log(&format!("TRIGGER_AUTOCMD: {:?}", event));
    let cmds = if let Some(c) = state.autocmds.get(&event) { c.clone() } else { return; };
    for cmd in cmds {
        match cmd.callback {
            crate::nvim::state::AutoCmdCallback::Command(c) => {
                let _ = execute_cmd(state, &c);
            }
            crate::nvim::state::AutoCmdCallback::Lua(key) => {
                state.log("TRIGGER_AUTOCMD: Executing Lua callback");
                let env = state.lua_env.clone();
                let lua_res = env.borrow().execute_callback(&key);
                if let Err(e) = lua_res {
                    state.log(&format!("TRIGGER_AUTOCMD: Lua error: {}", e));
                }
            }
        }
    }
}

pub fn find_project_root(path: &str) -> String {
    let mut current = std::path::Path::new(path);
    if current.is_file() {
        if let Some(p) = current.parent() { current = p; }
    }
    let mut search = current;
    loop {
        if search.join("Cargo.toml").exists() || search.join(".git").exists() || search.join("pyrightconfig.json").exists() {
            return search.display().to_string();
        }
        if let Some(parent) = search.parent() { search = parent; } else { break; }
    }
    std::env::current_dir().unwrap_or_default().display().to_string()
}

pub fn handle_request(state: &mut VimState, req: Request) -> Result<()> {
    state.log(&format!("HANDLE_REQUEST: {:?}", req));
    if state.macro_recording.is_some() {
        match req {
            Request::StopMacroRecord | Request::PlayMacro(_) => {}
            _ => { if let Some((_, ref mut recorded_reqs)) = state.macro_recording { recorded_reqs.push(req.clone()); } }
        }
    }
    
    // RepeatLastChange is handled before other handlers to avoid recording IT as the last change
    if let Request::RepeatLastChange { count } = &req {
        if let Some(last) = state.last_change.clone() {
            for _ in 0..*count {
                handle_request(state, last.clone())?;
            }
        }
        return Ok(());
    }

    handlers::state::handle(state, req.clone())?;
    handlers::edit::handle(state, req.clone())?;
    handlers::motion::handle(state, req.clone())?;
    handlers::window::handle(state, req.clone())?;
    handlers::cmdline::handle(state, req.clone())?;

    match req {
        Request::LspDefinition => {
            if let Some(lsp) = &state.lsp_client {
                let win = state.current_window();
                let cur = win.cursor();
                let buf = win.buffer();
                let path_opt = buf.borrow().name().map(|n| n.to_string());
                if let Some(path) = path_opt {
                    let uri = format!("file://{}", path);
                    lsp.send_definition(100, &uri, cur.row - 1, cur.col);
                }
            }
        }
        Request::LspHover => {
            if let Some(lsp) = &state.lsp_client {
                let win = state.current_window();
                let cur = win.cursor();
                let buf = win.buffer();
                let path_opt = buf.borrow().name().map(|n| n.to_string());
                if let Some(path) = path_opt {
                    let uri = format!("file://{}", path);
                    lsp.send_hover(101, &uri, cur.row - 1, cur.col);
                }
            }
        }
        _ => {}
    }

    if state.lsp_client.is_some() {
        for buf in &state.buffers {
            let mut b = buf.borrow_mut();
            if let Some(path) = b.name().map(|n| n.to_string()) {
                if !b.is_lsp_opened {
                    let text = b.get_all_lines().join("\n");
                    let uri = format!("file://{}", path);
                    let lang = if path.ends_with(".py") { "python" } else if path.ends_with(".rs") { "rust" } else { "text" };
                    if let Some(lsp) = &state.lsp_client { lsp.send_did_open(&uri, lang, &text); }
                    b.is_lsp_opened = true;
                    b.lsp_synced_version = b.version;
                } else if b.version > b.lsp_synced_version {
                    let text = b.get_all_lines().join("\n");
                    let uri = format!("file://{}", path);
                    if let Some(lsp) = &state.lsp_client { lsp.send_did_change(&uri, b.version, &text); }
                    b.lsp_synced_version = b.version;
                }
            }
        }
    }
    Ok(())
}

pub fn execute_cmd(state: &mut VimState, cmd: &str) -> Result<()> {
    state.log(&format!("EXECUTE_CMD: {}", cmd));
    let cmd = cmd.trim();
    if cmd.is_empty() { return Ok(()); }
    
    // Strip leading colon if present
    let cmd = if cmd.starts_with(':') { &cmd[1..].trim() } else { cmd };
    if cmd.is_empty() { return Ok(()); }

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() { return Ok(()); }

    let cmd_name = parts[0];
    if state.user_commands.contains_key(cmd_name) {
        let env = state.lua_env.clone();
        let args = if parts.len() > 1 { parts[1..].join(" ") } else { "".to_string() };
        let _ = env.borrow().call_user_command(cmd_name, &args);
        return Ok(());
    }

    match cmd_name {
        "e" | "edit" => {
            if parts.len() > 1 {
                let path = parts[1];
                let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path));
                handle_request(state, Request::OpenFile(abs_path.to_string_lossy().to_string()))?;
            }
        }
        "enew" => {
            let buf = std::rc::Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
            state.buffers.push(buf.clone());
            state.current_window_mut().buffer = buf;
            state.current_window_mut().set_cursor(1, 0);
        }
        "saveas" => {
            if parts.len() > 1 {
                let path = parts[1];
                state.current_window().buffer().borrow_mut().set_name(path);
                let _ = execute_cmd(state, "w");
            }
        }
        "w" | "write" => {
            let buf_rc = state.current_window().buffer();
            let b = buf_rc.borrow();
            if let Some(path) = b.name() {
                let _ = fs::os_write_file(path, &b.get_all_lines());
            }
        }
        "q" | "quit" => state.quit(),
        "ls" | "buffers" => {
            let mut info = Vec::new();
            for b in &state.buffers {
                let b = b.borrow();
                info.push(format!("{} {}", b.id(), b.name().unwrap_or("[No Name]")));
            }
            state.set_cmdline(info.join("\n"));
        }
        "bn" | "bnext" => {
            let cur_buf_id = state.current_window().buffer().borrow().id();
            if let Some(pos) = state.buffers.iter().position(|b| b.borrow().id() == cur_buf_id) {
                let next_idx = (pos + 1) % state.buffers.len();
                state.current_window_mut().buffer = state.buffers[next_idx].clone();
            }
        }
        "bp" | "bprev" => {
            let cur_buf_id = state.current_window().buffer().borrow().id();
            if let Some(pos) = state.buffers.iter().position(|b| b.borrow().id() == cur_buf_id) {
                let prev_idx = (pos + state.buffers.len() - 1) % state.buffers.len();
                state.current_window_mut().buffer = state.buffers[prev_idx].clone();
            }
        }
        "bd" | "bdelete" => {
            let cur_buf_id = state.current_window().buffer().borrow().id();
            if state.buffers.len() > 1 {
                if let Some(pos) = state.buffers.iter().position(|b| b.borrow().id() == cur_buf_id) {
                    state.buffers.remove(pos);
                    let next_idx = pos % state.buffers.len();
                    state.current_window_mut().buffer = state.buffers[next_idx].clone();
                }
            }
        }
        "echo" => {
            if parts.len() > 1 { state.set_cmdline(parts[1..].join(" ")); }
        }
        "echomsg" => {
            if parts.len() > 1 {
                let msg = parts[1..].join(" ");
                state.messages.push(msg);
            }
        }
        "messages" => {
            state.set_cmdline(state.messages.join("\n"));
        }
        "pwd" => {
            if let Ok(cwd) = std::env::current_dir() {
                state.set_cmdline(cwd.display().to_string());
            }
        }
        "tabnew" => {
            let buf = if state.buffers.is_empty() {
                let b = std::rc::Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
                state.buffers.push(b.clone());
                b
            } else {
                state.buffers[0].clone()
            };
            let win = crate::nvim::window::Window::new(buf);
            state.tabpages.push(TabPage::new(win));
            state.current_tab_idx = state.tabpages.len() - 1;
        }
        "tabn" | "tabnext" => {
            state.current_tab_idx = (state.current_tab_idx + 1) % state.tabpages.len();
        }
        "tabp" | "tabprev" => {
            state.current_tab_idx = (state.current_tab_idx + state.tabpages.len() - 1) % state.tabpages.len();
        }
        "tabclose" => {
            if state.tabpages.len() > 1 {
                state.tabpages.remove(state.current_tab_idx);
                state.current_tab_idx %= state.tabpages.len();
            }
        }
        "marks" => {
            let b = state.current_window().buffer();
            let b = b.borrow();
            let mut res = vec!["mark line  col file/text".to_string()];
            for (c, pos) in b.marks.iter() {
                res.push(format!(" {}    {}    {} {}", c, pos.row, pos.col, b.get_line(pos.row).unwrap_or("")));
            }
            state.set_cmdline(res.join("\n"));
        }
        "jumps" => {
            state.set_cmdline("jump line  col file/text\n > 0    1    0 [No Name]".to_string());
        }
        "undo" => {
            let b = state.current_window().buffer();
            let mut b = b.borrow_mut();
            if let Ok(Some(pos)) = b.undo() {
                state.current_window_mut().set_cursor(pos.row, pos.col);
            }
        }
        "redo" => {
            let b = state.current_window().buffer();
            let mut b = b.borrow_mut();
            if let Ok(Some(pos)) = b.redo() {
                state.current_window_mut().set_cursor(pos.row, pos.col);
            }
        }
        "join" => {
            let win = state.current_window();
            let row = win.cursor().row;
            let buf = win.buffer();
            let mut b = buf.borrow_mut();
            if let (Some(l1), Some(l2)) = (b.get_line(row), b.get_line(row + 1)) {
                let joined = format!("{} {}", l1, l2.trim_start());
                b.start_undo_group();
                let _ = b.delete_line(row + 1);
                let _ = b.set_line(row, 0, &joined);
                b.end_undo_group();
            }
        }
        "ascii" => {
            let win = state.current_window();
            let cur = win.cursor();
            let b = win.buffer();
            let b = b.borrow();
            if let Some(line) = b.get_line(cur.row) {
                if let Some(c) = line.chars().nth(cur.col) {
                    state.set_cmdline(format!("<{}>  {},  Hex {:02x},  Octal {:03o}", c, c as u32, c as u32, c as u32));
                }
            }
        }
        "set" => {
            if parts.len() > 1 {
                let opt = parts[1];
                if opt.contains('=') {
                    let kv: Vec<&str> = opt.splitn(2, '=').collect();
                    if kv.len() == 2 {
                        let name = kv[0];
                        let val_str = kv[1];
                        if let Ok(val) = val_str.parse::<i32>() {
                            state.options.insert(name.to_string(), crate::nvim::state::OptionValue::Int(val as i64));
                        } else {
                            state.options.insert(name.to_string(), crate::nvim::state::OptionValue::String(val_str.to_string()));
                        }
                    }
                } else if opt.starts_with("no") {
                    let opt_name = &opt[2..];
                    state.options.insert(opt_name.to_string(), crate::nvim::state::OptionValue::Bool(false));
                } else {
                    state.options.insert(opt.to_string(), crate::nvim::state::OptionValue::Bool(true));
                }
            }
        }
        "lua" => {
            if parts.len() > 1 {
                let code = parts[1..].join(" ");
                let res = state.lua_env.borrow().execute(&code);
                match res {
                    Ok(val) => { if !val.is_empty() { state.set_cmdline(val); } }
                    Err(e) => { state.set_cmdline(format!("{}", e)); }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn execute_search(state: &mut VimState, query: &str) {
    execute_search_internal(state, query, true);
}

pub fn execute_search_internal(state: &mut VimState, query: &str, _save_history: bool) {
    let buf_ref = state.current_window().buffer();
    let b = buf_ref.borrow();
    let current_cursor = state.current_window().cursor();
    for lnum in current_cursor.row..=b.line_count() {
        if let Some(line) = b.get_line(lnum) {
            let start_col = if lnum == current_cursor.row { current_cursor.col } else { 0 };
            if let Ok(re) = regex::Regex::new(query) {
                if let Some(m) = re.find(&line[start_col..]) {
                    state.current_window_mut().set_cursor(lnum, start_col + m.start());
                    return;
                }
            }
        }
    }
}

pub fn handle_search_next(state: &mut VimState, query: &str, forward: bool) {
    let query_converted = crate::nvim::regexp::convert_vim_regex(query);
    let cur = state.current_window().cursor();
    let win = state.current_window_mut();
    let buf = win.buffer();
    let b = buf.borrow();
    let line_count = b.line_count();
    if let Ok(re) = regex::Regex::new(&query_converted) {
        if forward {
            for lnum in cur.row..=line_count {
                if let Some(line) = b.get_line(lnum) {
                    let start_offset = if lnum == cur.row { cur.col + 1 } else { 0 };
                    if start_offset < line.chars().count() {
                        if let Some(m) = re.find(&line.chars().skip(start_offset).collect::<String>()) {
                            state.current_window_mut().set_cursor(lnum, start_offset + m.start());
                            return;
                        }
                    }
                }
            }
        } else {
            for lnum in (1..=cur.row).rev() {
                if let Some(line) = b.get_line(lnum) {
                    let end_offset = if lnum == cur.row { cur.col } else { line.chars().count() };
                    if end_offset > 0 {
                        let search_area = line.chars().take(end_offset).collect::<String>();
                        if let Some(m) = re.find_iter(&search_area).last() {
                            state.current_window_mut().set_cursor(lnum, m.start());
                            return;
                        }
                    }
                }
            }
        }
    }
}
