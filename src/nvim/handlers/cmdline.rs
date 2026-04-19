use crate::nvim::state::{VimState, Mode};
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::api::{execute_cmd, execute_search, handle_search_next, execute_search_internal};

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::CmdLineChar(c) => {
            let cursor = state.cmdline_cursor;
            let cmd = state.cmdline().to_string();
            let mut chars: Vec<char> = cmd.chars().collect();
            if cursor <= chars.len() {
                chars.insert(cursor, c);
            } else {
                chars.push(c);
            }
            state.set_cmdline(chars.into_iter().collect());
            state.cmdline_cursor += 1;

            if state.mode == Mode::Search {
                let incsearch = match state.options.get("incsearch") {
                    Some(crate::nvim::state::OptionValue::Bool(b)) => *b,
                    _ => true,
                };
                if incsearch {
                    let query = state.cmdline().to_string();
                    if let Some(start_pos) = state.search_start_cursor {
                        state.current_window_mut().set_cursor(start_pos.row, start_pos.col);
                        execute_search_internal(state, &query, false);
                    }
                }
            }
        }
        Request::CmdLineBackspace => {
            if state.cmdline().is_empty() {
                if state.mode == Mode::Search {
                    if let Some(start_pos) = state.search_start_cursor {
                        state.current_window_mut().set_cursor(start_pos.row, start_pos.col);
                    }
                }
                state.set_mode(Mode::Normal);
            } else {
                let cursor = state.cmdline_cursor;
                if cursor > 0 {
                    let cmd = state.cmdline().to_string();
                    let mut chars: Vec<char> = cmd.chars().collect();
                    chars.remove(cursor - 1);
                    state.set_cmdline(chars.into_iter().collect());
                    state.cmdline_cursor -= 1;
                    if state.mode == Mode::Search {
                        let query = state.cmdline().to_string();
                        if let Some(start_pos) = state.search_start_cursor {
                            state.current_window_mut().set_cursor(start_pos.row, start_pos.col);
                            if !query.is_empty() {
                                execute_search_internal(state, &query, false);
                            }
                        }
                    }
                }
            }
        }
        Request::CmdLineDeleteWord => {
            let cursor = state.cmdline_cursor;
            if cursor > 0 {
                let cmd = state.cmdline().to_string();
                let chars: Vec<char> = cmd.chars().collect();
                let mut start = cursor - 1;
                while start > 0 && chars[start].is_whitespace() { start -= 1; }
                while start > 0 && !chars[start].is_whitespace() { start -= 1; }
                if start > 0 && chars[start].is_whitespace() { start += 1; }
                let mut new_chars = chars[..start].to_vec();
                new_chars.extend(&chars[cursor..]);
                state.set_cmdline(new_chars.into_iter().collect());
                state.cmdline_cursor = start;
            }
        }
        Request::CmdLineDeleteLine => {
            state.set_cmdline(String::new());
            state.cmdline_cursor = 0;
        }
        Request::CmdLineMoveCursor { offset } => {
            let len = state.cmdline().chars().count();
            let new_pos = (state.cmdline_cursor as i32 + offset).clamp(0, len as i32) as usize;
            state.cmdline_cursor = new_pos;
        }
        Request::CmdLineMoveCursorTo { pos } => {
            let len = state.cmdline().chars().count();
            state.cmdline_cursor = pos.min(len);
        }
        Request::CmdLineHistory { forward } => {
            if state.cmd_history.is_empty() { return Ok(()); }
            let new_idx = if let Some(idx) = state.cmd_history_idx {
                if forward {
                    if idx + 1 < state.cmd_history.len() { Some(idx + 1) } else { None }
                } else {
                    if idx > 0 { Some(idx - 1) } else { Some(0) }
                }
            } else {
                if forward { None } else { Some(state.cmd_history.len() - 1) }
            };
            if let Some(idx) = new_idx {
                state.cmd_history_idx = Some(idx);
                let cmd = state.cmd_history[idx].clone();
                let len = cmd.chars().count();
                state.set_cmdline(cmd);
                state.cmdline_cursor = len;
            } else {
                state.cmd_history_idx = None;
                state.set_cmdline(String::new());
                state.cmdline_cursor = 0;
            }
        }
        Request::CmdLineTab => {
            if state.mode == Mode::CommandLine {
                let current = state.cmdline();
                let commands = vec!["quit", "write", "edit", "split", "vsplit", "tabnew", "set", "echo", "lua", "let"];
                let matches: Vec<_> = commands.into_iter().filter(|c| c.starts_with(current)).collect();
                if let Some(m) = matches.first() {
                    state.set_cmdline(m.to_string());
                }
            }
        }
        Request::ExecuteCommand => {
            let cmd = state.cmdline().to_string();
            if !cmd.is_empty() {
                state.cmd_history.push(cmd.clone());
                state.cmd_history_idx = None;
            }
            state.set_cmdline(String::new());
            state.set_mode(Mode::Normal);
            execute_cmd(state, &cmd)?;
        }
        Request::ExecuteCommandFromConfig(cmd) => {
            execute_cmd(state, &cmd)?;
        }
        Request::ExecuteSearch => {
            let query = state.cmdline().to_string();
            if !query.is_empty() {
                state.cmd_history.push(query.clone());
                state.cmd_history_idx = None;
            }
            state.set_cmdline(String::new());
            state.set_mode(Mode::Normal);
            execute_search(state, &query);
        }
        Request::SearchWordUnderCursor { forward, whole_word } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let word_opt = {
                let b = buf.borrow();
                if let Some((start, end)) = b.get_word_range(cur.row, cur.col, true) {
                    if let Some(line) = b.get_line(start.row) {
                        Some(line.chars().skip(start.col).take(end.col - start.col + 1).collect::<String>())
                    } else { None }
                } else { None }
            };
            if let Some(word) = word_opt {
                let query = if whole_word { format!("\\b{}\\b", word) } else { word };
                state.last_search = Some(query.clone());
                handle_search_next(state, &query, forward);
            }
        }
        Request::SearchNext { forward, count: _ } => {
            if let Some(query) = state.last_search.clone() {
                crate::nvim::api::handle_search_next(state, &query, forward);
            }
        }
        _ => {}
    }
    Ok(())
}
