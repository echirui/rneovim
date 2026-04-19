use crate::nvim::state::{VimState, AutoCmdEvent};
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::os::fs;
use std::rc::Rc;

use crate::nvim::handlers;

pub fn trigger_autocmd(state: &mut VimState, event: AutoCmdEvent) {
    if let Some(cmds) = state.autocmds.get(&event).cloned() {
        for cmd in cmds {
            let _ = execute_cmd(state, &cmd);
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
        if let Some(parent) = search.parent() {
            search = parent;
        } else {
            break;
        }
    }
    std::env::current_dir().unwrap_or_default().display().to_string()
}

pub fn handle_request(state: &mut VimState, req: Request) -> Result<()> {
    // マクロ記録中ならリクエストを保存
    if state.macro_recording.is_some() {
        match req {
            Request::StopMacroRecord | Request::PlayMacro(_) => {}
            _ => {
                if let Some((_, ref mut recorded_reqs)) = state.macro_recording {
                    recorded_reqs.push(req.clone());
                }
            }
        }
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

    // LSP同期
    if state.lsp_client.is_some() {
        for buf in &state.buffers {
            let mut b = buf.borrow_mut();
            if let Some(path) = b.name().map(|n| n.to_string()) {
                if !b.is_lsp_opened {
                    let text = b.get_all_lines().join("\n");
                    let uri = format!("file://{}", path);
                    let lang = if path.ends_with(".py") { "python" } else if path.ends_with(".rs") { "rust" } else { "text" };
                    if let Some(lsp) = &state.lsp_client {
                        lsp.send_did_open(&uri, lang, &text);
                    }
                    b.is_lsp_opened = true;
                    b.lsp_synced_version = b.version;
                } else if b.version > b.lsp_synced_version {
                    let text = b.get_all_lines().join("\n");
                    let uri = format!("file://{}", path);
                    if let Some(lsp) = &state.lsp_client {
                        lsp.send_did_change(&uri, b.version, &text);
                    }
                    b.lsp_synced_version = b.version;
                }
            }
        }
    }

    Ok(())
}

pub fn execute_search_internal(state: &mut VimState, query: &str, save_history: bool) {
    if query.is_empty() { return; }
    if save_history {
        state.last_search = Some(query.to_string());
    }

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
    for lnum in 1..current_cursor.row {
        if let Some(line) = b.get_line(lnum) {
            if let Ok(re) = regex::Regex::new(query) {
                if let Some(m) = re.find(&line) {
                    state.current_window_mut().set_cursor(lnum, m.start());
                    return;
                }
            }
        }
    }
}

pub fn execute_search(state: &mut VimState, query: &str) {
    execute_search_internal(state, query, true);
}

pub fn handle_search_next(state: &mut VimState, query: &str, forward: bool) {
    let query_converted = crate::nvim::regexp::convert_vim_regex(query);
    let win = state.current_window_mut();
    let cur = win.cursor();
    let buf = win.buffer();
    let b = buf.borrow();
    let line_count = b.line_count();

    if let Ok(re) = regex::Regex::new(&query_converted) {
        if forward {
            // 前方検索: 現在位置の次から末尾まで
            for lnum in cur.row..=line_count {
                if let Some(line) = b.get_line(lnum) {
                    let start_col = if lnum == cur.row { cur.col + 1 } else { 0 };
                    if start_col < line.len() {
                        if let Some(m) = re.find(&line[start_col..]) {
                            state.current_window_mut().set_cursor(lnum, start_col + m.start());
                            return;
                        }
                    }
                }
            }
            // ラップして先頭から現在位置まで
            for lnum in 1..cur.row {
                if let Some(line) = b.get_line(lnum) {
                    if let Some(m) = re.find(line) {
                        state.current_window_mut().set_cursor(lnum, m.start());
                        return;
                    }
                }
            }
        } else {
            // 後方検索: 現在位置の手前から先頭まで
            for lnum in (1..=cur.row).rev() {
                if let Some(line) = b.get_line(lnum) {
                    let end_col = if lnum == cur.row { cur.col } else { line.len() };
                    if end_col > 0 {
                        let matches: Vec<_> = re.find_iter(&line[..end_col]).collect();
                        if let Some(m) = matches.last() {
                            state.current_window_mut().set_cursor(lnum, m.start());
                            return;
                        }
                    }
                }
            }
            // ラップして末尾から現在位置まで
            for lnum in (cur.row + 1..=line_count).rev() {
                if let Some(line) = b.get_line(lnum) {
                    let matches: Vec<_> = re.find_iter(line).collect();
                    if let Some(m) = matches.last() {
                        state.current_window_mut().set_cursor(lnum, m.start());
                        return;
                    }
                }
            }
        }
    }
}

pub fn execute_cmd(state: &mut VimState, cmd: &str) -> Result<()> {
    let cmd = cmd.trim();
    if cmd.is_empty() { return Ok(()); }
    
    state.cmd_history.push(cmd.to_string());

    // プロファイリングの開始時間を記録
    let start_time = if state.profiling_log.is_some() {
        Some(std::time::Instant::now())
    } else {
        None
    };

    if cmd.starts_with('!') {
        let shell_cmd = &cmd[1..];
        #[cfg(unix)]
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(shell_cmd)
            .spawn()?;
        #[cfg(windows)]
        let mut child = std::process::Command::new("cmd")
            .arg("/c")
            .arg(shell_cmd)
            .spawn()?;
        let _ = child.wait();
        return Ok(());
    }

    // Handle :s/foo/bar/g style without space
    if cmd.starts_with("s/") {
        return execute_cmd(state, &format!("s {}", &cmd[1..]));
    }
    // Handle :g/pattern/d style without space
    if cmd.starts_with("g/") {
        return execute_cmd(state, &format!("g {}", &cmd[1..]));
    }

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    match parts[0] {
        "profile" => {
            if parts.len() >= 3 && parts[1] == "start" {
                state.profiling_log = Some(parts[2].to_string());
                let _ = std::fs::File::create(parts[2]); // ファイルを空で作成
            } else if parts.len() >= 2 && parts[1] == "stop" {
                state.profiling_log = None;
            }
        }
        "q" | "quit" => { state.quit(); }
        "u" | "undo" => { handle_request(state, Request::Undo)?; }
        "red" | "redo" => { handle_request(state, Request::Redo)?; }
        "f" | "file" => {
            let buf = state.current_window().buffer();
            let b = buf.borrow();
            let name = b.name().unwrap_or("[No Name]");
            let modified = if b.is_modified() { "[Modified] " } else { "" };
            let lnum = state.current_window().cursor().row;
            let total = b.line_count();
            let pct = if total > 0 { (lnum * 100) / total } else { 0 };
            state.set_cmdline(format!("\"{}\" {}line {} of {} --{}%--", name, modified, lnum, total, pct));
        }
        "as" | "ascii" => {
            let (row, col) = {
                let win = state.current_window();
                let cur = win.cursor();
                (cur.row, cur.col)
            };
            let buf = state.current_window().buffer();
            let line_opt = buf.borrow().get_line(row).map(|s| s.to_string());
            if let Some(line) = line_opt {
                if let Some(c) = line.chars().nth(col) {
                    state.set_cmdline(format!("<{}> {}, Hex {:02x}, Octal {:o}", c, c as u32, c as u32, c as u32));
                }
            }
        }
        "j" | "join" | "j!" | "join!" => {
            let no_space = parts[0].ends_with('!');
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            if cur.row < b.line_count() {
                let next_line = b.get_line(cur.row + 1).unwrap().to_string();
                let curr_line = b.get_line(cur.row).unwrap().to_string();
                let new_line = if no_space {
                    format!("{}{}", curr_line, next_line)
                } else {
                    format!("{} {}", curr_line.trim_end(), next_line.trim_start())
                };
                b.set_line(cur.row, cur.col, &new_line)?;
                b.delete_line(cur.row + 1)?;
            }
        }
        "sav" | "saveas" => {
            if let Some(path) = parts.get(1) {
                state.current_window().buffer().borrow_mut().set_name(path);
                handle_request(state, Request::SaveFile(None))?;
            }
        }
        "tabs" => {
            let mut msgs = vec!["Tab page 1".to_string()]; // Simplified
            for (i, _tp) in state.tabpages.iter().enumerate() {
                msgs.push(format!("Tab page {}", i + 1));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "ve" | "version" => {
            state.set_cmdline("rneovim v0.1.0 (Rust port of Neovim)".to_string());
        }
        "pw" | "pwd" => {
            if let Ok(cwd) = std::env::current_dir() {
                state.set_cmdline(cwd.to_string_lossy().to_string());
            }
        }
        "cd" => {
            if let Some(path) = parts.get(1) {
                let _ = std::env::set_current_dir(path);
            }
        }
        "term" | "terminal" => {
            let cmd_str = parts[1..].join(" ");
            let shell = if cmd_str.is_empty() {
                std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
            } else {
                cmd_str
            };
            handle_request(state, Request::OpenTerminal(shell))?;
        }
        "echo" => {
            if parts.len() > 1 {
                let expr = parts[1..].join(" ");
                // Try eval, if fails treat as raw string
                if let Ok(val) = state.eval_context.eval(state, &expr) {
                    state.set_cmdline(format!("{:?}", val));
                } else {
                    state.set_cmdline(expr);
                }
            } else {
                state.set_cmdline(String::new());
            }
        }
        "echom" | "echomsg" => {
            let msg = parts[1..].join(" ");
            state.messages.push(msg.clone());
            state.set_cmdline(msg);
        }
        "mes" | "messages" => {
            state.set_cmdline(state.messages.join("\n"));
        }
        "enew" => {
            let new_buf = Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
            state.buffers.push(new_buf.clone());
            state.current_window_mut().buffer = new_buf;
        }
        "new" => {
            handle_request(state, Request::SplitWindow { vertical: false })?;
            let new_buf = Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
            state.buffers.push(new_buf.clone());
            state.current_window_mut().buffer = new_buf;
        }
        "vnew" => {
            handle_request(state, Request::SplitWindow { vertical: true })?;
            let new_buf = Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
            state.buffers.push(new_buf.clone());
            state.current_window_mut().buffer = new_buf;
        }
        "noh" | "nohlsearch" => {
            // Simplified: just clear last search for now if needed, 
            // but usually this just clears highlighting.
        }
        // ... (他のコマンド) ...
        "w" | "write" => {
            let path = parts.get(1).map(|s| s.to_string());
            handle_request(state, Request::SaveFile(path))?;
        }
        "e" | "edit" => {
            if let Some(path) = parts.get(1) {
                handle_request(state, Request::OpenFile(path.to_string()))?;
            }
        }
        "wq" => {
            handle_request(state, Request::SaveFile(None))?;
            state.quit();
        }
        "update" | "up" | "upd" => {
            if state.current_window().buffer().borrow().is_modified() {
                handle_request(state, Request::SaveFile(None))?;
            }
        }
        "x" | "xit" => {
            if state.current_window().buffer().borrow().is_modified() {
                handle_request(state, Request::SaveFile(None))?;
            }
            state.quit();
        }
        "qa" | "qall" => {
            handle_request(state, Request::QuitAll)?;
        }
        "wqa" | "wqall" | "xa" | "xall" => {
            handle_request(state, Request::SaveAll)?;
            handle_request(state, Request::QuitAll)?;
        }
        "wa" | "wall" => {
            handle_request(state, Request::SaveAll)?;
        }
        "d" | "delete" => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            let count = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1);
            {
                let mut b = buf.borrow_mut();
                for _ in 0..count {
                    if b.line_count() > 0 {
                        b.delete_line(cur.row)?;
                    }
                }
            }
            let new_row = cur.row.min(buf.borrow().line_count()).max(1);
            state.current_window_mut().set_cursor(new_row, 0);
        }
        "m" | "move" => {
            if let Some(target_str) = parts.get(1) {
                if let Ok(target_lnum) = target_str.parse::<usize>() {
                    let cur = state.current_window().cursor();
                    let buf = state.current_window().buffer();
                    let line_content = {
                        let b = buf.borrow();
                        b.get_line(cur.row).map(|s| s.to_string())
                    };
                    if let Some(content) = line_content {
                        let mut b = buf.borrow_mut();
                        b.delete_line(cur.row)?;
                        let count = b.line_count();
                        let final_target = if target_lnum > cur.row { target_lnum } else { target_lnum + 1 };
                        b.insert_line(final_target.min(count + 1), &content)?;
                    }
                    state.current_window_mut().set_cursor(target_lnum.max(1), 0);
                }
            }
        }
        "t" | "copy" | "co" => {
            if let Some(target_str) = parts.get(1) {
                if let Ok(target_lnum) = target_str.parse::<usize>() {
                    let cur = state.current_window().cursor();
                    let buf = state.current_window().buffer();
                    let line_content = {
                        let b = buf.borrow();
                        b.get_line(cur.row).map(|s| s.to_string())
                    };
                    if let Some(content) = line_content {
                        let mut b = buf.borrow_mut();
                        let count = b.line_count();
                        b.insert_line((target_lnum + 1).min(count + 1), &content)?;
                    }
                    state.current_window_mut().set_cursor((target_lnum + 1).max(1), 0);
                }
            }
        }
        "r" | "read" => {
            if let Some(path) = parts.get(1) {
                if let Ok(lines) = fs::os_read_file(path) {
                    let cur = state.current_window().cursor();
                    let buf = state.current_window().buffer();
                    let mut b = buf.borrow_mut();
                    for (i, line) in lines.into_iter().enumerate() {
                        b.insert_line(cur.row + 1 + i, &line)?;
                    }
                }
            }
        }
        "s" | "substitute" => {
            if let Some(arg) = parts.get(1) {
                // Better parsing for /pattern/replacement/flags
                let p: Vec<&str> = arg.split('/').collect();
                if p.len() >= 3 {
                    let pattern = p[1];
                    let replacement = p[2];
                    let flags = p.get(3).unwrap_or(&"");
                    let win = state.current_window();
                    let cur = win.cursor();
                    let buf = win.buffer();
                    let line_opt = buf.borrow().get_line(cur.row).map(|s| s.to_string());
                    if let Some(line) = line_opt {
                        if let Ok(re) = regex::Regex::new(pattern) {
                            let new_line = if flags.contains('g') {
                                re.replace_all(&line, replacement).to_string()
                            } else {
                                re.replace(&line, replacement).to_string()
                            };
                            buf.borrow_mut().set_line(cur.row, cur.col, &new_line)?;
                            state.last_substitute = Some((pattern.to_string(), replacement.to_string(), flags.to_string()));
                        }
                    }
                }
            }
        }
        "&" => {
            if let Some((pattern, replacement, flags)) = state.last_substitute.clone() {
                let cmd = format!("s/{}/{}/{}", pattern, replacement, flags);
                return execute_cmd(state, &cmd);
            }
        }
        "%&" => {
            if let Some((pattern, replacement, flags)) = state.last_substitute.clone() {
                let cmd = format!("g/.*/s/{}/{}/{}", pattern, replacement, flags);
                return execute_cmd(state, &cmd);
            }
        }
        "g" | "global" | "v" | "vglobal" => {
            let is_v = parts[0].starts_with('v');
            if let Some(arg) = parts.get(1) {
                let p: Vec<&str> = arg.split('/').collect();
                if p.len() >= 3 {
                    let pattern = p[1];
                    let command = p[2..].join("/");
                    let buf_rc = state.current_window().buffer();
                    let mut matching_lines = Vec::new();
                    if let Ok(re) = regex::Regex::new(pattern) {
                        let b = buf_rc.borrow();
                        for lnum in 1..=b.line_count() {
                            let matches = re.is_match(b.get_line(lnum).unwrap_or(""));
                            if matches != is_v {
                                matching_lines.push(lnum);
                            }
                        }
                    }
                    // Reverse to handle deletions correctly
                    matching_lines.reverse();
                    for lnum in matching_lines {
                        state.current_window_mut().set_cursor(lnum, 0);
                        execute_cmd(state, &command)?;
                    }
                }
            }
        }
        "split" | "sp" => {
            handle_request(state, Request::SplitWindow { vertical: false })?;
        }
        "vsplit" | "vs" => {
            handle_request(state, Request::SplitWindow { vertical: true })?;
        }
        "tabnew" => {
            handle_request(state, Request::NewTabPage)?;
        }
        "tabn" | "tabnext" => {
            state.current_tab_idx = (state.current_tab_idx + 1) % state.tabpages.len();
        }
        "tabp" | "tabprevious" | "tabPrev" => {
            state.current_tab_idx = if state.current_tab_idx == 0 { state.tabpages.len() - 1 } else { state.current_tab_idx - 1 };
        }
        "ls" | "buffers" => {
            let mut msgs = Vec::new();
            for (i, buf_rc) in state.buffers.iter().enumerate() {
                let b = buf_rc.borrow();
                let indicator = if Rc::ptr_eq(buf_rc, &state.current_window().buffer()) { "%a" } else { "  " };
                let modified = if b.is_modified() { "+" } else { " " };
                msgs.push(format!("{:3} {} {} \"{}\" line {}", i + 1, indicator, modified, b.name().unwrap_or("[No Name]"), 1));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "bn" | "bnext" => {
            let current_buf = state.current_window().buffer();
            if let Some(pos) = state.buffers.iter().position(|b| Rc::ptr_eq(b, &current_buf)) {
                let next_idx = (pos + 1) % state.buffers.len();
                state.current_window_mut().buffer = state.buffers[next_idx].clone();
            }
        }
        "bp" | "bprev" => {
            let current_buf = state.current_window().buffer();
            if let Some(pos) = state.buffers.iter().position(|b| Rc::ptr_eq(b, &current_buf)) {
                let prev_idx = if pos == 0 { state.buffers.len() - 1 } else { pos - 1 };
                state.current_window_mut().buffer = state.buffers[prev_idx].clone();
            }
        }
        "bd" | "bdelete" => {
            let current_buf = state.current_window().buffer();
            if let Some(pos) = state.buffers.iter().position(|b| Rc::ptr_eq(b, &current_buf)) {
                if state.buffers.len() > 1 {
                    let next_idx = (pos + 1) % state.buffers.len();
                    let next_buf = state.buffers[next_idx].clone();
                    state.current_window_mut().buffer = next_buf;
                    state.buffers.remove(pos);
                } else {
                    // Create a new empty buffer if it's the last one
                    let new_buf = Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
                    state.current_window_mut().buffer = new_buf.clone();
                    state.buffers = vec![new_buf];
                }
            }
        }
        "only" => {
            let win = state.current_window().clone();
            let tp = state.current_tabpage_mut();
            tp.windows = vec![win];
            tp.current_win_idx = 0;
        }
        "close" => {
            let tp = state.current_tabpage_mut();
            if tp.windows.len() > 1 {
                tp.windows.remove(tp.current_win_idx);
                tp.current_win_idx = tp.current_win_idx.saturating_sub(1);
            }
        }
        "tabclose" => {
            if state.tabpages.len() > 1 {
                state.tabpages.remove(state.current_tab_idx);
                state.current_tab_idx = state.current_tab_idx.saturating_sub(1);
            }
        }
        "marks" => {
            let buf = state.current_window().buffer();
            let mut msgs = vec!["mark line  col file/text".to_string()];
            let b = buf.borrow();
            let mut marks_list: Vec<_> = b.marks.iter().collect();
            marks_list.sort_by_key(|(c, _)| *c);
            for (c, pos) in marks_list {
                msgs.push(format!(" {}    {:4} {:4} {}", c, pos.row, pos.col, b.get_line(pos.row).unwrap_or("")));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "reg" | "registers" => {
            let mut msgs = vec!["--- Registers ---".to_string()];
            let mut regs: Vec<_> = state.registers.iter().collect();
            regs.sort_by_key(|(c, _)| *c);
            for (c, val) in regs {
                let display_val = val.replace('\n', "^J");
                msgs.push(format!("\"{}   {}", c, display_val));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "jumps" => {
            let mut msgs = vec!["jump line  col file/text".to_string()];
            for (i, (file, pos)) in state.jump_list.iter().enumerate() {
                msgs.push(format!("{:3} {:4} {:4} {}", i, pos.row, pos.col, file));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "history" => {
            let mut msgs = vec!["#  command history".to_string()];
            for (i, cmd) in state.cmd_history.iter().enumerate() {
                msgs.push(format!("{:3}  {}", i + 1, cmd));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "so" | "source" => {
            if let Some(path) = parts.get(1) {
                if let Ok(lines) = fs::os_read_file(path) {
                    for line in lines {
                        execute_cmd(state, &line)?;
                    }
                }
            }
        }
        "runtime" => {
            if let Some(file) = parts.get(1) {
                let path = format!("runtime/{}", file);
                if std::path::Path::new(&path).exists() {
                    if let Ok(lines) = fs::os_read_file(&path) {
                        for line in lines {
                            execute_cmd(state, &line)?;
                        }
                    }
                }
            }
        }
        "map" | "nmap" | "vmap" => {
            if parts.len() >= 3 {
                let from = parts[1];
                let to = parts[2];
                state.keymap.add_mapping(from, to, true);
            }
        }
        "unmap" => {
            if parts.len() >= 2 {
                // KeymapEngine doesn't have remove_mapping yet
            }
        }
        "grep" | "vimgrep" => {
            if let Some(pattern) = parts.get(1) {
                state.quickfix_list.clear();
                let buf_rc = state.current_window().buffer();
                let b = buf_rc.borrow();
                if let Ok(re) = regex::Regex::new(pattern) {
                    for lnum in 1..=b.line_count() {
                        if let Some(line) = b.get_line(lnum) {
                            if re.is_match(line) {
                                state.quickfix_list.push((b.name().unwrap_or("").to_string(), lnum, line.to_string()));
                            }
                        }
                    }
                }
                state.set_cmdline(format!("Found {} matches", state.quickfix_list.len()));
            }
        }
        "copen" => {
            let mut msgs = vec!["--- Quickfix List ---".to_string()];
            for (file, lnum, text) in &state.quickfix_list {
                msgs.push(format!("{}:{}: {}", file, lnum, text));
            }
            state.set_cmdline(msgs.join("\n"));
        }
        "cclose" => {
            state.set_cmdline(String::new());
        }
        "tag" => {
            if let Some(tagname) = parts.get(1) {
                state.set_cmdline(format!("Jump to tag: {}", tagname));
            }
        }

        "lspstart" => {
            let cmd_args = parts[1..].join(" ");
            if let Ok(client) = crate::nvim::lsp::LspClient::new(&cmd_args, state.sender.clone()) {
                let win = state.current_window();
                let buf = win.buffer();
                let b = buf.borrow();
                let root_dir = if let Some(path) = b.name() {
                    find_project_root(path)
                } else {
                    std::env::current_dir().unwrap_or_default().display().to_string()
                };
                
                let uri = format!("file://{}", root_dir);
                client.send_initialize(&uri);
                state.lsp_client = Some(client);
            }
        }
        "autocmd" | "au" => {
            if parts.len() >= 4 {
                let event_str = parts[1];
                let _pattern = parts[2];
                let command = parts[3..].join(" ");
                
                let event = match event_str {
                    "BufReadPost" => Some(AutoCmdEvent::BufReadPost),
                    "BufWritePost" => Some(AutoCmdEvent::BufWritePost),
                    "InsertEnter" => Some(AutoCmdEvent::InsertEnter),
                    "InsertLeave" => Some(AutoCmdEvent::InsertLeave),
                    "VimEnter" => Some(AutoCmdEvent::VimEnter),
                    _ => None,
                };
                
                if let Some(e) = event {
                    let entry = state.autocmds.entry(e).or_insert_with(Vec::new);
                    entry.push(command);
                }
            }
        }
        "command" | "com" => {
            if parts.len() >= 3 {
                let name = parts[1].to_string();
                let body = parts[2..].join(" ");
                state.user_commands.insert(name, body);
            }
        }
        "ab" | "abbreviate" => {
            if parts.len() >= 3 {
                let from = parts[1].to_string();
                let to = parts[2..].join(" ");
                state.abbreviations.insert(from, to);
            }
        }
        "breakadd" => {
            if parts.len() >= 3 && parts[1] == "line" {
                if let Ok(lnum) = parts[2].parse::<usize>() {
                    state.breakpoints.insert(lnum);
                }
            }
        }
        "breakdel" => {
            if parts.len() >= 3 && parts[1] == "line" {
                if let Ok(lnum) = parts[2].parse::<usize>() {
                    state.breakpoints.remove(&lnum);
                }
            }
        }
        "help" | "h" => {
            let topic = parts.get(1).unwrap_or(&"help");
            let path = format!("runtime/doc/{}.txt", topic);
            if std::path::Path::new(&path).exists() {
                handle_request(state, Request::OpenFile(path))?;
            } else {
                state.set_cmdline(format!("Help not found for {}", topic));
            }
        }
        "lines" | "=" => {
            let buf = state.current_window().buffer();
            let count = buf.borrow().line_count();
            state.set_cmdline(format!("{} lines", count));
        }
        "let" => {
            if parts.len() >= 4 && parts[2] == "=" {
                let var_name = parts[1];
                let expr = parts[3..].join(" ");
                if let Ok(val) = state.eval_context.eval(state, &expr) {
                    state.eval_context.set_var(var_name, val);
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
                        if let Ok(val) = kv[1].parse::<i32>() {
                            state.options.insert(name.to_string(), crate::nvim::state::OptionValue::Int(val));
                        } else {
                            state.options.insert(name.to_string(), crate::nvim::state::OptionValue::String(kv[1].to_string()));
                        }
                    }
                } else if opt.starts_with("no") {
                    let opt_name = &opt[2..];
                    state.options.insert(opt_name.to_string(), crate::nvim::state::OptionValue::Bool(false));
                } else {
                    state.options.insert(opt.to_string(), crate::nvim::state::OptionValue::Bool(true));
                }
                let _ = handle_request(state, Request::Redraw);
            }
        }
        "lua" => {
            if parts.len() > 1 {
                let code = parts[1..].join(" ");
                match state.lua_env.execute(&code) {
                    Ok(res) => {
                        if !res.is_empty() {
                            state.set_cmdline(res);
                        }
                    }
                    Err(e) => {
                        state.set_cmdline(format!("{}", e));
                    }
                }
            }
        }
        _ => {
            if let Some(body) = state.user_commands.get(parts[0]).cloned() {
                execute_cmd(state, &body)?;
            }
        }
    }

    if let (Some(start), Some(path)) = (start_time, &state.profiling_log) {
        use std::io::Write;
        let duration = start.elapsed();
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = writeln!(file, "{:?}: {}", duration, cmd);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nvim::state::{VimState, Mode};
    

    #[test]
    fn test_api_set_mode() {
        let mut state = VimState::new();
        assert_eq!(state.mode(), Mode::Normal);
        handle_request(&mut state, Request::SetMode(Mode::Insert)).unwrap();
        assert_eq!(state.mode(), Mode::Insert);
        handle_request(&mut state, Request::SetMode(Mode::CommandLine)).unwrap();
        assert_eq!(state.mode(), Mode::CommandLine);
        assert_eq!(state.cmdline(), "");
    }

    #[test]
    fn test_api_insert_and_move() {
        let mut state = VimState::new();
        handle_request(&mut state, Request::InsertChar('a')).unwrap();
        let buf = state.current_window().buffer();
        {
            let b = buf.borrow_mut();
            assert_eq!(b.get_line(1), Some("a"));
        }
        assert_eq!(state.current_window().cursor().col, 1);
        handle_request(&mut state, Request::CursorMove { row: 1, col: 0 }).unwrap();
        assert_eq!(state.current_window().cursor().col, 0);
        handle_request(&mut state, Request::InsertChar('b')).unwrap();
        {
            let b = buf.borrow_mut();
            assert_eq!(b.get_line(1), Some("ba"));
        }
    }

    #[test]
    fn test_api_cmdline_ops() {
        let mut state = VimState::new();
        handle_request(&mut state, Request::SetMode(Mode::CommandLine)).unwrap();
        handle_request(&mut state, Request::CmdLineChar('w')).unwrap();
        handle_request(&mut state, Request::CmdLineChar('q')).unwrap();
        assert_eq!(state.cmdline(), "wq");
        handle_request(&mut state, Request::CmdLineBackspace).unwrap();
        assert_eq!(state.cmdline(), "w");
    }

    #[test]
    fn test_api_set_option() {
        let mut state = VimState::new();
        assert_eq!(state.options.get("number"), Some(&crate::nvim::state::OptionValue::Bool(false)));
        execute_cmd(&mut state, "set number").unwrap();
        assert_eq!(state.options.get("number"), Some(&crate::nvim::state::OptionValue::Bool(true)));
        execute_cmd(&mut state, "set nonumber").unwrap();
        assert_eq!(state.options.get("number"), Some(&crate::nvim::state::OptionValue::Bool(false)));
    }

    #[test]
    fn test_api_regex_search() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("apple").unwrap();
            b.append_line("banana").unwrap();
            b.append_line("cherry").unwrap();
        }
        execute_search(&mut state, "^b");
        assert_eq!(state.current_window().cursor().row, 2);
        assert_eq!(state.current_window().cursor().col, 0);
        execute_search(&mut state, "r{2}");
        assert_eq!(state.current_window().cursor().row, 3);
        assert_eq!(state.current_window().cursor().col, 3);
    }

    #[test]
    fn test_api_search_continuation() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("test 1").unwrap();
            b.append_line("test 2").unwrap();
            b.append_line("test 3").unwrap();
        }
        execute_search(&mut state, "test");
        assert_eq!(state.current_window().cursor().row, 1);
        handle_request(&mut state, Request::SearchNext { forward: true, count: 1 }).unwrap();
        assert_eq!(state.current_window().cursor().row, 2);
        handle_request(&mut state, Request::SearchNext { forward: true, count: 1 }).unwrap();
        assert_eq!(state.current_window().cursor().row, 3);
        handle_request(&mut state, Request::SearchNext { forward: false, count: 1 }).unwrap();
        assert_eq!(state.current_window().cursor().row, 2);
    }

    #[test]
    fn test_api_incremental_search() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("apple").unwrap();
            b.append_line("banana").unwrap();
        }
        handle_request(&mut state, Request::SetMode(Mode::Search)).unwrap();
        handle_request(&mut state, Request::CmdLineChar('b')).unwrap();
        assert_eq!(state.current_window().cursor().row, 2);
    }

    #[test]
    fn test_api_smart_indent() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("if (cond) {").unwrap();
        }
        state.current_window_mut().set_cursor(1, 0);
        handle_request(&mut state, Request::OpenLine { below: true }).unwrap();
        assert_eq!(state.current_window().cursor().col, 8);
    }

    #[test]
    fn test_api_user_commands() {
        let mut state = VimState::new();
        execute_cmd(&mut state, "command Hello echo \"world\"").unwrap();
        execute_cmd(&mut state, "Hello").unwrap();
        assert_eq!(state.cmdline(), "String(\"world\")");
    }

    #[test]
    fn test_api_syntax_highlighting() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("fn main()").unwrap();
        }
        state.current_window_mut().set_cursor(1, 8);
        state.redraw();
        let fg = state.grid.get_fg(0, 2); 
        assert_eq!(fg, crate::nvim::ui::grid::Color::Yellow);
    }

    #[test]
    fn test_api_dot_command() {
        let mut state = VimState::new();
        {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            b.append_line("line 1").unwrap();
            b.append_line("line 2").unwrap();
        }
        state.current_window_mut().set_cursor(1, 0);
        handle_request(&mut state, Request::DeleteCharAtCursor { count: 1 }).unwrap();
        handle_request(&mut state, Request::RepeatLastChange { count: 1 }).unwrap();
        {
            let buf = state.current_window().buffer();
            assert_eq!(buf.borrow().get_line(1), Some("ne 1"));
        }
    }
}
