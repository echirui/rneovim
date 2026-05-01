use crate::nvim::state::{VimState, AutoCmdEvent, Mode};
use mlua::{Value, MultiValue};
use crate::nvim::window::TabPage;
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::os::fs;

use crate::nvim::handlers;

pub fn trigger_autocmd(state: &mut VimState, event: AutoCmdEvent, pattern: Option<&str>) {
    state.log(&format!("TRIGGER_AUTOCMD: {:?} (pattern={:?})", event, pattern));
    
    let mut to_remove = Vec::new();
    let mut callbacks = Vec::new();

    if let Some(autocmds) = state.autocmds.get(&event) {
        for (i, cmd) in autocmds.iter().enumerate() {
            if let Some(p) = pattern {
                if let Some(ref cmd_p) = cmd.pattern {
                    if cmd_p != p { continue; }
                }
            }
            callbacks.push(cmd.clone());
            if cmd.once { to_remove.push(i); }
        }
    }

    // Execute callbacks
    for cmd in callbacks {
        match cmd.callback {
            crate::nvim::state::AutoCmdCallback::Command(c) => {
                let _ = execute_cmd(state, &c);
            }
            crate::nvim::state::AutoCmdCallback::Lua(id) => {
                state.log(&format!("TRIGGER_AUTOCMD: Executing Lua callback (id={})", id));
                let env = state.lua_env.clone();
                let args = match env.lua.create_table() {
                    Ok(t) => {
                        let ev_name = match event {
                            AutoCmdEvent::VimEnter => "VimEnter", AutoCmdEvent::UIEnter => "UIEnter",
                            AutoCmdEvent::User => "User", AutoCmdEvent::BufReadPost => "BufReadPost",
                            AutoCmdEvent::BufWritePost => "BufWritePost", AutoCmdEvent::FileType => "FileType",
                            AutoCmdEvent::Syntax => "Syntax", AutoCmdEvent::SafeState => "SafeState",
                            AutoCmdEvent::CursorHold => "CursorHold", AutoCmdEvent::CursorHoldI => "CursorHoldI",
                            AutoCmdEvent::WinClosed => "WinClosed", AutoCmdEvent::WinResized => "WinResized",
                            AutoCmdEvent::VimResized => "VimResized", AutoCmdEvent::ColorScheme => "ColorScheme",
                            AutoCmdEvent::ColorSchemePre => "ColorSchemePre", _ => "User",
                        };
                        let _ = t.set("event", ev_name);
                        let _ = t.set("match", pattern.unwrap_or(""));
                        let _ = t.set("buf", 1); // Dummy buffer ID
                        MultiValue::from_vec(vec![Value::Table(t)])
                    }
                    Err(_) => MultiValue::new(),
                };
                let lua_res = env.execute_callback_with_args(id, args);
                if let Err(e) = lua_res {
                    state.log(&format!("TRIGGER_AUTOCMD: Lua error (id={}): {}", id, e));
                }
            }
        }
    }

    // Remove one-shot autocmds
    if !to_remove.is_empty() {
        if let Some(autocmds) = state.autocmds.get_mut(&event) {
            for i in to_remove.into_iter().rev() {
                if i < autocmds.len() { autocmds.remove(i); }
            }
        }
    }
}

pub fn find_project_root(path: &str) -> String {
    let mut current = std::path::Path::new(path);
    if current.is_file() { if let Some(p) = current.parent() { current = p; } }
    let mut search = current;
    loop {
        if search.join("Cargo.toml").exists() || search.join(".git").exists() || search.join("pyrightconfig.json").exists() { return search.display().to_string(); }
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
    
    if let Request::RepeatLastChange { count } = &req {
        if let Some(last) = state.last_change.clone() {
            let buf = state.current_window().buffer();
            let _ = buf.try_borrow_mut().map(|mut b| b.start_undo_group());
            for _ in 0..*count { handle_request(state, last.clone())?; }
            let _ = buf.try_borrow_mut().map(|mut b| b.end_undo_group());
        }
        return Ok(());
    }

    let is_normal = matches!(state.mode, Mode::Normal);
    if is_normal { let _ = state.current_window().buffer().try_borrow_mut().map(|mut b| b.start_undo_group()); }

    let res: Result<()> = (|| {
        handlers::state::handle(state, req.clone())?;
        handlers::edit::handle(state, req.clone())?;
        handlers::motion::handle(state, req.clone())?;
        handlers::window::handle(state, req.clone())?;
        handlers::cmdline::handle(state, req.clone())?;
        Ok(())
    })();

    if is_normal { let _ = state.current_window().buffer().try_borrow_mut().map(|mut b| b.end_undo_group()); }
    res?;

    match req {
        Request::LspDefinition => {
            if let Some(lsp) = &state.lsp_client {
                let win = state.current_window();
                let cur = win.cursor();
                let buf = win.buffer();
                let path_opt = buf.try_borrow().ok().and_then(|b| b.name().map(|n| n.to_string()));
                if let Some(path) = path_opt {
                    let uri = format!("file://{}", path);
                    lsp.send_definition(100, &uri, cur.row - 1, cur.col);
                }
            }
        }
        Request::SaveFile(path_opt) => {
            let buf_rc = state.current_window().buffer();
            let mut name_to_save = None;
            if let Ok(mut b) = buf_rc.try_borrow_mut() {
                if let Some(p) = path_opt { b.set_name(&p); }
                name_to_save = b.name().map(|s| s.to_string());
                if name_to_save.is_some() {
                    let lines = b.get_all_lines();
                    let _ = fs::os_write_file(name_to_save.as_ref().unwrap(), &lines);
                    b.clear_modified();
                }
            }
            if name_to_save.is_some() {
                trigger_autocmd(state, AutoCmdEvent::BufWritePost, None);
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn execute_search_internal(state: &mut VimState, query: &str, save_history: bool) {
    if query.is_empty() { return; }
    if save_history { state.last_search = Some(query.to_string()); }
    let buf_ref = state.current_window().buffer();
    let current_cursor = state.current_window().cursor();
    
    let mut found_pos = None;
    if let Ok(b) = buf_ref.try_borrow() {
        for lnum in current_cursor.row..=b.line_count() {
            if let Some(line) = b.get_line(lnum) {
                let start_col = if lnum == current_cursor.row { current_cursor.col } else { 0 };
                if let Ok(re) = regex::Regex::new(query) {
                    if let Some(m) = re.find(&line[start_col..]) {
                        found_pos = Some((lnum, start_col + m.start()));
                        break;
                    }
                }
            }
        }
    }
    if let Some((lnum, col)) = found_pos {
        state.current_window_mut().set_cursor(lnum, col);
    }
}

pub fn execute_search(state: &mut VimState, query: &str) { execute_search_internal(state, query, true); }

pub fn handle_search_next(state: &mut VimState, query: &str, forward: bool) {
    let query_converted = crate::nvim::regexp::convert_vim_regex(query);
    let cur = state.current_window().cursor();
    let buf = state.current_window().buffer();
    let mut found_pos = None;
    if let Ok(b) = buf.try_borrow() {
        if let Ok(re) = regex::Regex::new(&query_converted) {
            if forward {
                for lnum in cur.row..=b.line_count() {
                    if let Some(line) = b.get_line(lnum) {
                        let start_col = if lnum == cur.row { cur.col + 1 } else { 0 };
                        if start_col < line.len() {
                            if let Some(m) = re.find(&line[start_col..]) {
                                found_pos = Some((lnum, start_col + m.start()));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some((lnum, col)) = found_pos {
        state.current_window_mut().set_cursor(lnum, col);
    }
}

pub fn execute_cmd(state: &mut VimState, cmd: &str) -> Result<()> {
    state.log(&format!("EXECUTE_CMD: {}", cmd));
    let cmd = cmd.trim();
    if cmd.is_empty() { return Ok(()); }
    let cmd = if cmd.starts_with(':') { &cmd[1..].trim() } else { cmd };
    if cmd.is_empty() { return Ok(()); }
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() { return Ok(()); }
    let cmd_name = parts[0];
    if state.user_commands.contains_key(cmd_name) {
        let env = state.lua_env.clone();
        let args = if parts.len() > 1 { parts[1..].join(" ") } else { "".to_string() };
        if let Err(e) = env.call_user_command(cmd_name, &args) { state.log(&format!("USER_COMMAND ERROR ({}): {}", cmd_name, e)); }
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
                let _ = state.current_window().buffer().try_borrow_mut().map(|mut b| b.set_name(parts[1]));
                let _ = execute_cmd(state, "w");
            }
        }
        "w" | "write" => {
            let buf_rc = state.current_window().buffer();
            let path_opt = if let Ok(b) = buf_rc.try_borrow() { b.name().map(|s| s.to_string()) } else { None };
            if let Some(path) = path_opt {
                if let Ok(b) = buf_rc.try_borrow() { let _ = fs::os_write_file(&path, &b.get_all_lines()); }
            }
        }
        "q" | "quit" => state.quit(),
        "ls" | "buffers" => {
            let mut info = Vec::new();
            for b in &state.buffers { if let Ok(b_ref) = b.try_borrow() { info.push(format!("{} {}", b_ref.id(), b_ref.name().unwrap_or("[No Name]"))); } }
            state.set_cmdline(info.join("\n"));
        }
        "bn" | "bnext" | "bp" | "bprev" | "bd" | "bdelete" => {
            let cur_buf_id = state.current_window().buffer().try_borrow().map(|b| b.id()).unwrap_or(0);
            if let Some(pos) = state.buffers.iter().position(|b| b.try_borrow().map(|b_ref| b_ref.id() == cur_buf_id).unwrap_or(false)) {
                if cmd_name.starts_with('b') && cmd_name.contains('d') {
                    if state.buffers.len() > 1 { state.buffers.remove(pos); let next_idx = pos % state.buffers.len(); state.current_window_mut().buffer = state.buffers[next_idx].clone(); }
                } else {
                    let next_idx = if cmd_name.contains('n') { (pos + 1) % state.buffers.len() } else { (pos + state.buffers.len() - 1) % state.buffers.len() };
                    state.current_window_mut().buffer = state.buffers[next_idx].clone();
                }
            }
        }
        "echo" | "let" => {
            if parts.len() > 1 {
                let expr = if cmd_name == "echo" { 
                    parts[1..].join(" ") 
                } else { 
                    if parts.len() > 3 { parts[3..].join(" ") } else { "".to_string() }
                };
                if let Ok(val) = state.eval_context.eval(state, &expr) {
                    if cmd_name == "echo" { state.set_cmdline(format!("{:?}", val)); }
                    else { if parts.len() > 1 { state.eval_context.set_var(parts[1], val); } }
                } else if cmd_name == "echo" { state.set_cmdline(expr); }
            }
        }
        "echomsg" => { if parts.len() > 1 { let msg = parts[1..].join(" "); state.messages.push(msg); } }
        "messages" => state.set_cmdline(state.messages.join("\n")),
        "pwd" => { if let Ok(cwd) = std::env::current_dir() { state.set_cmdline(cwd.display().to_string()); } }
        "tabnew" => {
            let buf = if state.buffers.is_empty() { let b = std::rc::Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new())); state.buffers.push(b.clone()); b } else { state.buffers[0].clone() };
            state.tabpages.push(TabPage::new(crate::nvim::window::Window::new(buf)));
            state.current_tab_idx = state.tabpages.len() - 1;
        }
        "tabn" | "tabnext" => {
            if !state.tabpages.is_empty() {
                state.current_tab_idx = (state.current_tab_idx + 1) % state.tabpages.len();
            }
        }
        "tabp" | "tabprevious" | "tabN" | "tabNext" => {
            if !state.tabpages.is_empty() {
                state.current_tab_idx = if state.current_tab_idx == 0 { state.tabpages.len() - 1 } else { state.current_tab_idx - 1 };
            }
        }
        "tabclose" => {
            if state.tabpages.len() > 1 {
                state.tabpages.remove(state.current_tab_idx);
                state.current_tab_idx = state.current_tab_idx.min(state.tabpages.len() - 1);
            }
        }
        "undo" | "u" => { let _ = handle_request(state, Request::Undo); }
        "redo" => { let _ = handle_request(state, Request::Redo); }
        "join" | "j" => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let mut b = buf.borrow_mut();
            if let (Some(l1), Some(l2)) = (b.get_line(cur.row), b.get_line(cur.row + 1)) {
                let joined = format!("{} {}", l1.trim_end(), l2.trim_start());
                let _ = b.set_line(cur.row, 0, &joined);
                let _ = b.delete_line(cur.row + 1);
            }
        }
        "ascii" | "as" => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let char_info = {
                buf.borrow().get_line(cur.row).and_then(|l| l.chars().nth(cur.col))
            };
            if let Some(c) = char_info {
                let code = c as u32;
                state.set_cmdline(format!("<{}>  {},  Hex {:02x},  Octal {:o}", c, code, code, code));
            }
        }
        "marks" => {
            let mut info = Vec::new();
            let buf = state.current_window().buffer();
            if let Ok(b) = buf.try_borrow() {
                for (name, pos) in &b.marks {
                    info.push(format!("{} {} {}", name, pos.row, pos.col));
                }
            }
            state.set_cmdline(info.join("\n"));
        }
        "jumps" => {
            state.set_cmdline("jump 1 1".to_string());
        }
        "Lazy" => {
            let env = state.lua_env.clone();
            if let Err(e) = env.execute("require('lazy').home()") {
                state.log(&format!("LAZY HOME ERROR: {}", e));
            }
            // Process callbacks for a while to let async rendering happen
            let start = std::time::Instant::now();
            while start.elapsed().as_millis() < 500 {
                state.process_callbacks();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let _ = handle_request(state, crate::nvim::request::Request::Redraw);
        }
        "source" | "so" => {
            if parts.len() > 1 {
                let _ = source_file(state, parts[1]);
            }
        }
        "command" | "command!" | "com" | "com!" => {
            let mut i = 1;
            while i < parts.len() && parts[i].starts_with('-') {
                i += 1;
            }
            if i + 1 < parts.len() {
                let name = parts[i].to_string();
                let body = parts[i+1..].join(" ");
                let lua = state.lua_env.lua.clone();
                let body_c = body.clone();
                let func = lua.create_function(move |lua, _args: mlua::Value| {
                    if let Some(wrapper) = lua.app_data_ref::<crate::nvim::lua::StateWrapper>() {
                        let state = unsafe { &mut **wrapper.0.as_ptr() };
                        let _ = execute_cmd(state, &body_c);
                    }
                    Ok(())
                });
                if let Ok(f) = func {
                    let env = state.lua_env.clone();
                    if let Ok(id) = env.register_callback(mlua::Value::Function(f)) {
                        state.user_commands.insert(name, id);
                    }
                }
            }
        }
        "if" | "else" | "elseif" | "endif" | "endfunction" | "endfunc" | "endfor" | "endwhile" | "endtry" | "try" | "catch" | "finally" | "break" | "continue" | "return" | "finish" | "au!" | "autocmd" | "autocmd!" | "au" | "set" | "setlocal" | "unlet" | "unlet!" | "call" | "func" | "function" | "function!" | "delfunction" | "runtime" | "runtime!" | "packadd" | "execute" | "exe" | "silent" | "silent!" | "noau" | "noautocmd" => {
            // Stub for noise reduction and ignoring flags like ++once
        }
        "augroup" => {
            state.log(&format!("AUGROUP stub: {}", parts[1..].join(" ")));
        }
        "lua" => {
            if parts.len() > 1 {
                let code = parts[1..].join(" ");
                let env = state.lua_env.clone();
                if let Err(e) = env.execute(&code) {
                    state.log(&format!("LUA ERROR: {}", e));
                }
            }
        }
        _ => {
            state.log(&format!("COMMAND NOT FOUND: {}", cmd_name));
        }
    }
    Ok(())
}

pub fn source_file(state: &mut VimState, path: &str) -> Result<()> {
    state.log(&format!("SOURCING: {}", path));
    let p = std::path::Path::new(path);
    if p.extension().map_or(false, |ext| ext == "lua") {
        let env = state.lua_env.clone();
        if let Err(e) = env.execute_file(p) {
            state.log(&format!("SOURCE LUA ERROR ({}): {}", path, e));
        }
    } else {
        if let Ok(content) = std::fs::read_to_string(p) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('"') { continue; }
                let _ = execute_cmd(state, trimmed);
            }
        } else {
            state.log(&format!("SOURCE ERROR: could not read file {}", path));
        }
    }
    Ok(())
}
