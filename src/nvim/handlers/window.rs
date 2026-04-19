use crate::nvim::state::VimState;
use crate::nvim::request::{Request, MouseButton, MouseAction};
use crate::nvim::error::{NvimError, Result};
use crate::nvim::os::fs;
use crate::nvim::api::handle_request;
use std::rc::Rc;

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::SplitWindow { vertical: _ } => {
            let win = state.current_window();
            let buf = win.buffer();
            let new_win = crate::nvim::window::Window::new(buf);
            let tp = state.current_tabpage_mut();
            tp.windows.push(new_win);
            tp.current_win_idx = tp.windows.len() - 1;
        }
        Request::NextWindow => {
            let tp = state.current_tabpage_mut();
            if !tp.windows.is_empty() {
                tp.current_win_idx = (tp.current_win_idx + 1) % tp.windows.len();
            }
        }
        Request::PrevWindow => {
            let tp = state.current_tabpage_mut();
            if !tp.windows.is_empty() {
                tp.current_win_idx = if tp.current_win_idx == 0 { tp.windows.len() - 1 } else { tp.current_win_idx - 1 };
            }
        }
        Request::NewTabPage => {
            let buf = Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
            let mut win = crate::nvim::window::Window::new(buf);
            win.set_height(20);
            let tabpage = crate::nvim::state::TabPage::new(win);
            state.tabpages.push(tabpage);
            state.current_tab_idx = state.tabpages.len() - 1;
        }
        Request::OpenFloatWin { buf_id, row, col, width, height } => {
            let buf_opt = state.buffers.iter().find(|b| b.borrow().id() == buf_id).cloned();
            if let Some(buf) = buf_opt {
                let config = crate::nvim::window::WinConfig {
                    row: row as usize,
                    col: col as usize,
                    width: width as usize,
                    height: height as usize,
                    focusable: true,
                    external: false,
                };
                let new_win = crate::nvim::window::Window::new_floating(buf, config);
                let tp = state.current_tabpage_mut();
                tp.windows.push(new_win);
                tp.current_win_idx = tp.windows.len() - 1;
            }
        }
        Request::Resize { width, height } => {
            state.grid.resize(width, height);
            for tp in &mut state.tabpages {
                for win in &mut tp.windows {
                    win.set_width(width);
                    win.set_height(height.saturating_sub(2));
                }
            }
        }
        Request::OpenFile(path) => {
            let lines: Vec<String> = fs::os_read_file(&path)?;
            let current_name = state.current_window().buffer().borrow().name().map(|s| s.to_string());
            if let Some(name) = current_name {
                state.alternate_file = Some(name);
            }
            let buf = state.current_window().buffer();
            {
                let mut b = buf.borrow_mut();
                *b = crate::nvim::buffer::Buffer::new();
                for line in lines {
                    let _ = b.append_line(&line);
                }
                b.set_name(&path);
                b.clear_modified();
            }
            state.current_window_mut().set_cursor(1, 0);
        }
        Request::AlternateFile => {
            if let Some(alt) = state.alternate_file.clone() {
                handle_request(state, Request::OpenFile(alt))?;
            } else {
                state.set_cmdline("No alternate file".to_string());
            }
        }
        Request::SaveFile(maybe_path) => {
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            let path = maybe_path.or_else(|| b.name().map(|s| s.to_string()))
                .ok_or_else(|| NvimError::Api("No file name to save".to_string()))?;
            let lines = b.get_all_lines();
            fs::os_write_file(&path, &lines)?;
            b.clear_modified();
        }
        Request::SaveAll => {
            for buf_rc in &state.buffers {
                let mut b = buf_rc.borrow_mut();
                if b.is_modified() {
                    if let Some(path) = b.name().map(|s| s.to_string()) {
                        let lines = b.get_all_lines();
                        if fs::os_write_file(&path, &lines).is_ok() {
                            b.clear_modified();
                        }
                    }
                }
            }
        }
        Request::Scroll { lines } => {
            let win = state.current_window_mut();
            win.scroll(lines);
        }
        Request::ComputeFolds => {
            let win_rc = state.current_window();
            let buf_rc = win_rc.buffer();
            let b = buf_rc.borrow();
            let mut folds = Vec::new();
            let mut stack = Vec::new();
            for lnum in 1..=b.line_count() {
                if let Some(line) = b.get_line(lnum) {
                    if line.trim().ends_with('{') {
                        stack.push(lnum);
                    } else if line.trim().starts_with('}') {
                        if let Some(start) = stack.pop() {
                            if lnum > start + 1 {
                                folds.push((start, lnum));
                            }
                        }
                    }
                }
            }
            state.current_window_mut().folds = folds;
        }
        Request::MouseEvent { button, action, row, col } => {
            match button {
                MouseButton::Left => {
                    if action == MouseAction::Press || action == MouseAction::Drag {
                        let show_number = match state.options.get("number") {
                            Some(crate::nvim::state::OptionValue::Bool(b)) => *b,
                            _ => false,
                        };
                        let margin = if show_number { 4 } else { 2 };
                        if row >= 1 {
                            let win = state.current_window_mut();
                            let target_lnum = win.topline() + (row - 1);
                            let target_col = col.saturating_sub(margin);
                            win.set_cursor(target_lnum, target_col);
                        }
                    }
                }
                MouseButton::WheelUp => {
                    handle_request(state, Request::Scroll { lines: -3 })?;
                }
                MouseButton::WheelDown => {
                    handle_request(state, Request::Scroll { lines: 3 })?;
                }
                _ => {}
            }
        }
        Request::ShowPum(items) => {
            if !items.is_empty() {
                state.pum_items = Some(items);
                state.pum_index = Some(0);
            }
        }
        Request::HidePum => {
            state.pum_items = None;
            state.pum_index = None;
        }
        Request::SelectPumNext => {
            if let Some(items) = &state.pum_items {
                if let Some(idx) = state.pum_index {
                    state.pum_index = Some((idx + 1) % items.len());
                }
            }
        }
        Request::SelectPumPrev => {
            if let Some(items) = &state.pum_items {
                if let Some(idx) = state.pum_index {
                    state.pum_index = Some(if idx == 0 { items.len() - 1 } else { idx - 1 });
                }
            }
        }
        Request::TriggerCompletion { forward: _ } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let b = buf.borrow();
            if let Some(line) = b.get_line(cur.row) {
                let chars: Vec<char> = line.chars().collect();
                let mut start = cur.col;
                while start > 0 && (chars[start-1].is_alphanumeric() || chars[start-1] == '_') {
                    start -= 1;
                }
                let prefix = chars[start..cur.col].iter().collect::<String>();
                if !prefix.is_empty() {
                    let mut candidates = std::collections::HashSet::new();
                    for l in 1..=b.line_count() {
                        if let Some(content) = b.get_line(l) {
                            for word in content.split(|c: char| !c.is_alphanumeric() && c != '_') {
                                if word.starts_with(&prefix) && word != prefix {
                                    candidates.insert(word.to_string());
                                }
                            }
                        }
                    }
                    let mut items: Vec<String> = candidates.into_iter().collect();
                    items.sort();
                    if !items.is_empty() {
                        state.pum_items = Some(items);
                        state.pum_index = Some(0);
                    }
                }
            }
        }
        Request::CommitCompletion => {
            if let (Some(items), Some(idx)) = (state.pum_items.clone(), state.pum_index) {
                let selected = items[idx].clone();
                let win = state.current_window();
                let cur = win.cursor();
                let buf = win.buffer();
                let mut b = buf.borrow_mut();
                if let Some(line) = b.get_line(cur.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let mut start = cur.col;
                    while start > 0 && (chars[start-1].is_alphanumeric() || chars[start-1] == '_') {
                        start -= 1;
                    }
                    let mut new_chars: Vec<char> = chars[..start].to_vec();
                    new_chars.extend(selected.chars());
                    new_chars.extend(chars[cur.col..].iter());
                    let new_line: String = new_chars.iter().collect();
                    let _ = b.set_line(cur.row, &new_line);
                    let new_col = start + selected.chars().count();
                    state.current_window_mut().set_cursor(cur.row, new_col);
                }
            }
            state.pum_items = None;
            state.pum_index = None;
        }
        Request::JobStart(cmd) => {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if !parts.is_empty() {
                let mut child = std::process::Command::new(parts[0])
                    .args(&parts[1..])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(NvimError::Io)?;
                let id = child.id() as i32;
                if let Some(stdout) = child.stdout.take() {
                    if let Some(sender) = state.sender.clone() {
                        std::thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(stdout);
                            for line in reader.lines().flatten() {
                                let l = line.clone();
                                let _ = sender.send(Box::new(move |s| {
                                    s.job_outputs.entry(id).or_insert_with(Vec::new).push(l);
                                }));
                            }
                        });
                    }
                }
                if let Some(stderr) = child.stderr.take() {
                    if let Some(sender) = state.sender.clone() {
                        std::thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(stderr);
                            for line in reader.lines().flatten() {
                                let l = line.clone();
                                let _ = sender.send(Box::new(move |s| {
                                    s.job_outputs.entry(id).or_insert_with(Vec::new).push(l);
                                }));
                            }
                        });
                    }
                }
                state.jobs.insert(id, child);
            }
        }
        Request::OpenTerminal(cmd) => {
            let buf = Rc::new(std::cell::RefCell::new(crate::nvim::buffer::Buffer::new()));
            buf.borrow_mut().set_name(&format!("term://{}", cmd));
            let mut win = crate::nvim::window::Window::new(buf.clone());
            win.set_height(20);
            let tp = state.current_tabpage_mut();
            tp.windows.push(win);
            tp.current_win_idx = tp.windows.len() - 1;
            let buf_id = buf.borrow().id();
            state.buffers.push(buf);
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if !parts.is_empty() {
                if let Ok(mut child) = std::process::Command::new(parts[0])
                    .args(&parts[1..])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn() {
                    let id = child.id() as i32;
                    if let Some(stdout) = child.stdout.take() {
                        if let Some(sender) = state.sender.clone() {
                            std::thread::spawn(move || {
                                use std::io::{BufRead, BufReader};
                                let reader = BufReader::new(stdout);
                                for line in reader.lines().flatten() {
                                    let l = line.clone();
                                    let _ = sender.send(Box::new(move |s| {
                                        if let Some(b_rc) = s.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                                            let mut b = b_rc.borrow_mut();
                                            let lc = b.line_count();
                                            let _ = b.insert_line(lc + 1, &l);
                                        }
                                        s.redraw();
                                    }));
                                }
                            });
                        }
                    }
                    if let Some(stderr) = child.stderr.take() {
                        if let Some(sender) = state.sender.clone() {
                            std::thread::spawn(move || {
                                use std::io::{BufRead, BufReader};
                                let reader = BufReader::new(stderr);
                                for line in reader.lines().flatten() {
                                    let l = line.clone();
                                    let _ = sender.send(Box::new(move |s| {
                                        if let Some(b_rc) = s.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                                            let mut b = b_rc.borrow_mut();
                                            let lc = b.line_count();
                                            let _ = b.insert_line(lc + 1, &l);
                                        }
                                        s.redraw();
                                    }));
                                }
                            });
                        }
                    }
                    state.jobs.insert(id, child);
                }
            }
        }
        Request::LspStart(cmd) => {
            let sender = state.sender.clone();
            if let Ok(client) = crate::nvim::lsp::LspClient::new(&cmd, sender) {
                client.send_initialize();
                state.lsp_client = Some(client);
            }
        }
        Request::TreesitterParse => {
            let source_code = {
                let win = state.current_window();
                let buf = win.buffer();
                let b = buf.borrow();
                let mut lines = Vec::new();
                for lnum in 1..=b.line_count() {
                    if let Some(line) = b.get_line(lnum) {
                        lines.push(line);
                    }
                }
                lines.join("\n")
            };
            if let Some(ts) = &mut state.ts_manager {
                ts.parse(&source_code);
            }
        }
        Request::SaveUndo(path) => {
            let buf = state.current_window().buffer();
            let json = serde_json::to_string(&buf.borrow().undo_tree).map_err(|e| NvimError::Api(e.to_string()))?;
            std::fs::write(path, json)?;
        }
        Request::LoadUndo(path) => {
            let buf = state.current_window().buffer();
            let json = std::fs::read_to_string(path)?;
            let tree: crate::nvim::undo::UndoTree = serde_json::from_str(&json).map_err(|e| NvimError::Api(e.to_string()))?;
            buf.borrow_mut().undo_tree = tree;
        }
        _ => {}
    }
    Ok(())
}
