use crate::nvim::state::{VimState, Mode};
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::api::{execute_cmd, execute_search, handle_search_next, execute_search_internal};

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::CmdLineChar(c) => {
            let cmd = state.cmdline().to_string();
            let mut chars: Vec<char> = cmd.chars().collect();
            let cursor = state.cmdline_cursor.min(chars.len());
            
            chars.insert(cursor, c);
            state.set_cmdline(chars.into_iter().collect());
            state.cmdline_cursor = cursor + 1;

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
                let cmd = state.cmdline().to_string();
                let mut chars: Vec<char> = cmd.chars().collect();
                
                if cursor > 0 && cursor <= chars.len() {
                    chars.remove(cursor - 1);
                    state.set_cmdline(chars.into_iter().collect());
                    state.cmdline_cursor = cursor - 1;
                    
                    if state.mode == Mode::Search {
                        let query = state.cmdline().to_string();
                        if let Some(start_pos) = state.search_start_cursor {
                            state.current_window_mut().set_cursor(start_pos.row, start_pos.col);
                            if !query.is_empty() {
                                execute_search_internal(state, &query, false);
                            }
                        }
                    }
                } else if cursor == 0 {
                    // Already at start, do nothing or exit mode if empty (handled above)
                } else {
                    // Out of bounds cursor, reset it to end
                    state.cmdline_cursor = chars.len();
                }
            }
        }
        Request::CmdLineDeleteWord => {
            let cmd = state.cmdline().to_string();
            let mut chars: Vec<char> = cmd.chars().collect();
            let cursor = state.cmdline_cursor.min(chars.len());
            
            if cursor > 0 {
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
                let cmd = state.cmd_history[idx].clone();
                state.set_cmdline(cmd.clone());
                state.cmdline_cursor = cmd.chars().count();
                state.cmd_history_idx = Some(idx);
            } else {
                state.set_cmdline(String::new());
                state.cmdline_cursor = 0;
                state.cmd_history_idx = None;
            }
        }
        Request::CmdLineTab => {
            if state.mode == Mode::CommandLine {
                let current = state.cmdline();
                if current == "L" || current == "La" || current == "Laz" || current == "Lazy" {
                    state.set_cmdline("Lazy".to_string());
                    state.cmdline_cursor = 4;
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
            if !cmd.is_empty() {
                state.cmd_history.push(cmd.clone());
                state.cmd_history_idx = None;
            }
            execute_cmd(state, &cmd)?;
        }
        Request::ExecuteSearch => {
            let query = state.cmdline().to_string();
            if !query.is_empty() {
                state.cmd_history.push(query.clone());
                state.cmd_history_idx = None;
            }
            state.set_mode(Mode::Normal);
            execute_search(state, &query);
        }
        Request::SearchWordUnderCursor { forward, whole_word } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let word_opt = {
                let b = buf.borrow();
                if let Some(line) = b.get_line(cur.row) {
                    let chars: Vec<char> = line.chars().collect();
                    if cur.col < chars.len() {
                        let mut start = cur.col;
                        while start > 0 && (chars[start-1].is_alphanumeric() || chars[start-1] == '_') { start -= 1; }
                        let mut end = cur.col;
                        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') { end += 1; }
                        Some(chars[start..end].iter().collect::<String>())
                    } else { None }
                } else { None }
            };

            if let Some(word) = word_opt {
                let query = if whole_word { format!(r"\b{}\b", word) } else { word };
                state.last_search = Some(query.clone());
                handle_search_next(state, &query, forward);
            }
        }
        _ => {}
    }
    Ok(())
}
