use crate::nvim::state::{VimState, Mode};
use crate::nvim::window::Cursor;
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::api::handle_request;
use crate::nvim::handlers::{get_system_clipboard, set_register_text};

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::InsertAtFirstColumn => {
            let cur = state.current_window().cursor();
            state.current_window_mut().set_cursor(cur.row, 0);
            state.set_mode(Mode::Insert);
        }
        Request::InsertAtLastInsert => {
            if let Some(pos) = state.last_insert_cursor {
                state.current_window_mut().set_cursor(pos.row, pos.col);
            }
            state.set_mode(Mode::Insert);
        }
        Request::ReplaceCharUnderCursor(c) => {
            let (row, col) = {
                let win = state.current_window();
                let cur = win.cursor();
                (cur.row, cur.col)
            };
            let buf = state.current_window().buffer();
            let mut b = buf.borrow_mut();
            let chars_count = b.get_line(row).map(|l| l.chars().count()).unwrap_or(0);
            if col < chars_count {
                let _ = b.delete_char(row, col + 1);
                let _ = b.insert_char(row, col, c);
                drop(b);
                state.current_window_mut().set_cursor(row, col + 1);
            }
        }
        Request::BlockInsert { append } => {
            if let Mode::Visual(crate::nvim::state::VisualMode::Block) = state.mode {
                let cur = state.current_window().cursor();
                let anchor = state.visual_anchor.unwrap_or(cur);
                state.set_mode(Mode::BlockInsert { append, anchor, cursor: cur });
            }
        }
        Request::InsertChar(c) => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            
            if let Mode::BlockInsert { append, anchor, cursor } = state.mode {
                let start_row = anchor.row.min(cursor.row);
                let end_row = anchor.row.max(cursor.row);
                let col = if append { anchor.col.max(cursor.col) + 1 } else { anchor.col.min(cursor.col) };
                
                let mut b = buf.borrow_mut();
                b.start_undo_group();
                for r in start_row..=end_row {
                    let _ = b.insert_char(r, col, c);
                }
                b.end_undo_group();
                drop(b);
                state.current_window_mut().set_cursor(cur.row, cur.col + 1);
                state.mode = Mode::BlockInsert { append, anchor: Cursor { row: anchor.row, col: anchor.col + 1 }, cursor: Cursor { row: cursor.row, col: cursor.col + 1 } };
                return Ok(());
            }

            if !c.is_alphanumeric() {
                let mut expanded_info = None;
                {
                    let b = buf.borrow();
                    if let Some(line) = b.get_line(cur.row) {
                        let chars: Vec<char> = line.chars().collect();
                        let text_before: String = chars.iter().take(cur.col).collect();
                        if let Some(last_word) = text_before.split_whitespace().last() {
                            if let Some(expanded) = state.abbreviations.get(last_word).cloned() {
                                let start_byte_idx = text_before.rfind(last_word).unwrap();
                                let start_char_idx = text_before[..start_byte_idx].chars().count();
                                expanded_info = Some((start_char_idx, expanded));
                            }
                        }
                    }
                }
                
                if let Some((start_char_idx, expanded)) = expanded_info {
                    let mut b = buf.borrow_mut();
                    let line = b.get_line(cur.row).unwrap().to_string();
                    let mut chars: Vec<char> = line.chars().collect();
                    let expanded_chars: Vec<char> = expanded.chars().collect();
                    chars.splice(start_char_idx..cur.col, expanded_chars);
                    let new_line: String = chars.into_iter().collect();
                    let _ = b.set_line(cur.row, cur.col, &new_line);
                    drop(b);
                    state.current_window_mut().set_cursor(cur.row, start_char_idx + expanded.chars().count());
                }
            }

            let cur = state.current_window().cursor();
            {
                let mut b = buf.borrow_mut();
                b.insert_char(cur.row, cur.col, c)?;
            }
            state.current_window_mut().set_cursor(cur.row, cur.col + 1);
            state.last_change = Some(Request::InsertChar(c));
        }
        Request::InsertModeDeleteWord => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            let target = {
                let b = buf.borrow();
                b.find_word_boundary(cur.row, cur.col, false)
            };
            if target.row == cur.row {
                let mut b = buf.borrow_mut();
                for _ in target.col..cur.col {
                    let _ = b.delete_char(cur.row, target.col + 1);
                }
            }
            state.current_window_mut().set_cursor(target.row, target.col);
        }
        Request::InsertModeDeleteLine => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            {
                let mut b = buf.borrow_mut();
                for _ in 0..cur.col {
                    let _ = b.delete_char(cur.row, 1);
                }
            }
            state.current_window_mut().set_cursor(cur.row, 0);
        }
        Request::DeleteCharAtCursor { count } => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            {
                let mut b = buf.borrow_mut();
                for _ in 0..count {
                    let _ = b.delete_char(cur.row, cur.col + 1);
                }
            }
            state.last_change = Some(Request::DeleteCharAtCursor { count });
        }
        Request::DeleteLine => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            {
                let mut b = buf.borrow_mut();
                if let Some(line) = b.get_line(cur.row) {
                    let reg = state.pending_register.unwrap_or('"');
                    set_register_text(state, reg, line.to_string());
                }
                b.delete_line(cur.row)?;
            }
            let new_row = cur.row.min(buf.borrow().line_count()).max(1);
            state.current_window_mut().set_cursor(new_row, 0);
            state.last_change = Some(Request::DeleteLine);
        }
        Request::OpenLine { below } => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            let new_lnum = if below { cur.row + 1 } else { cur.row };
            let mut indent = {
                if let Some(line) = buf.borrow().get_line(cur.row) {
                    line.chars().take_while(|c| c.is_whitespace()).collect::<String>()
                } else {
                    String::new()
                }
            };
            
            if let Some(line) = buf.borrow().get_line(cur.row) {
                let trimmed = line.trim_end();
                if trimmed.ends_with('{') || trimmed.ends_with('(') || trimmed.ends_with('[') {
                    let tabstop = match state.options.get("tabstop") {
                        Some(crate::nvim::state::OptionValue::Int(n)) => *n as usize,
                        _ => 8,
                    };
                    indent.push_str(&" ".repeat(tabstop));
                }
            }

            state.set_mode(Mode::Insert);
            {
                let mut b = buf.borrow_mut();
                b.insert_line(new_lnum, &indent)?;
            }
            state.current_window_mut().set_cursor(new_lnum, indent.chars().count());
        }
        Request::Backspace => {
            if let Mode::BlockInsert { append, anchor, cursor } = state.mode {
                let start_row = anchor.row.min(cursor.row);
                let end_row = anchor.row.max(cursor.row);
                let col = if append { anchor.col.max(cursor.col) } else { anchor.col.min(cursor.col) };

                let buf = state.current_window().buffer();
                let mut b = buf.borrow_mut();
                if col > 0 {
                    for r in start_row..=end_row {
                        let _ = b.delete_char(r, col);
                    }
                    drop(b);
                    state.mode = Mode::BlockInsert { 
                        append, 
                        anchor: Cursor { row: anchor.row, col: anchor.col.saturating_sub(1) }, 
                        cursor: Cursor { row: cursor.row, col: cursor.col.saturating_sub(1) }
                    };
                    let current_cur = state.current_window().cursor();
                    state.current_window_mut().set_cursor(current_cur.row, current_cur.col.saturating_sub(1));
                }
                return Ok(());
            }

            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            let res = {
                let mut b = buf.borrow_mut();
                b.delete_char(cur.row, cur.col)?
            };
            if let Some(new_col) = res {
                let new_row = if cur.col == 0 { cur.row - 1 } else { cur.row };
                state.current_window_mut().set_cursor(new_row, new_col);
            }
        }
        Request::NewLine => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            let indent = {
                if let Some(line) = buf.borrow().get_line(cur.row) {
                    line.chars().take_while(|c| c.is_whitespace()).collect::<String>()
                } else {
                    String::new()
                }
            };
            {
                let mut b = buf.borrow_mut();
                b.insert_newline(cur.row, cur.col)?;
                if let Some(line) = b.get_line(cur.row + 1) {
                    let new_line = format!("{}{}", indent, line);
                    let _ = b.set_line(cur.row + 1, 0, &new_line);
                }
            }
            state.current_window_mut().set_cursor(cur.row + 1, indent.len());
        }
        Request::Undo => {
            let buf = state.current_window().buffer();
            let res = {
                let mut b = buf.borrow_mut();
                b.undo()?
            };
            if let Some(cur) = res {
                state.current_window_mut().set_cursor(cur.row, cur.col);
            }
        }
        Request::Redo => {
            let buf = state.current_window().buffer();
            let res = {
                let mut b = buf.borrow_mut();
                b.redo()?
            };
            if let Some(cur) = res {
                state.current_window_mut().set_cursor(cur.row, cur.col);
            }
        }
        Request::EndUndoGroup => {
            let buf = state.current_window().buffer();
            buf.borrow_mut().end_undo_group();
        }
        Request::YankLine => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            {
                let b = buf.borrow();
                if let Some(line) = b.get_line(cur.row) {
                    let reg = state.pending_register.unwrap_or('"');
                    set_register_text(state, reg, line.to_string());
                }
            }
        }
        Request::Paste => {
            let reg = state.pending_register.unwrap_or('"');
            let content_opt = if reg == '+' || reg == '*' {
                get_system_clipboard().or_else(|| state.registers.get(&reg).cloned())
            } else {
                state.registers.get(&reg).cloned()
            };

            if let Some(content) = content_opt {
                if !content.is_empty() {
                    let cur = state.current_window().cursor();
                    let buf = state.current_window().buffer();
                    {
                        let mut b = buf.borrow_mut();
                        b.insert_line(cur.row + 1, &content)?;
                    }
                    state.current_window_mut().set_cursor(cur.row + 1, 0);
                }
            }
        }
        Request::PasteBefore => {
            let reg = state.pending_register.unwrap_or('"');
            let content_opt = if reg == '+' || reg == '*' {
                get_system_clipboard().or_else(|| state.registers.get(&reg).cloned())
            } else {
                state.registers.get(&reg).cloned()
            };

            if let Some(content) = content_opt {
                if !content.is_empty() {
                    let cur = state.current_window().cursor();
                    let buf = state.current_window().buffer();
                    {
                        let mut b = buf.borrow_mut();
                        b.insert_line(cur.row, &content)?;
                    }
                    state.current_window_mut().set_cursor(cur.row, 0);
                }
            }
        }
        Request::FormatRange { start_row, end_row } => {
            let tw = match state.options.get("textwidth") {
                Some(crate::nvim::state::OptionValue::Int(n)) => *n as usize,
                _ => 80,
            };
            if tw == 0 { return Ok(()); }

            let buf_rc = state.current_window().buffer();
            let mut b = buf_rc.borrow_mut();
            let mut current_lnum = start_row;
            let mut last_lnum = end_row;
            while current_lnum <= last_lnum {
                if let Some(line) = b.get_line(current_lnum) {
                    if line.len() > tw {
                        let text = line.to_string();
                        if let Some(break_pos) = text[..tw].rfind(' ') {
                            let (first, second) = text.split_at(break_pos);
                            let second_trimmed = second.trim_start().to_string();
                            b.set_line(current_lnum, 0, first)?;
                            b.insert_line(current_lnum + 1, &second_trimmed)?;
                            last_lnum += 1;
                        }
                    }
                }
                current_lnum += 1;
            }
        }
        Request::Indent { forward } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let mut b = buf.borrow_mut();
            let tabstop = match state.options.get("tabstop") {
                Some(crate::nvim::state::OptionValue::Int(n)) => *n as usize,
                _ => 8,
            };
            b.indent_line(cur.row, forward, tabstop)?;
        }
        Request::ReplaceChar(c) => {
            handle_request(state, Request::DeleteCharAtCursor { count: 1 })?;
            handle_request(state, Request::InsertChar(c))?;
        }
        Request::SwitchCase => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let mut b = buf.borrow_mut();
            let char_opt = b.get_line(cur.row).and_then(|l| l.chars().nth(cur.col));
            if let Some(c) = char_opt {
                let new_c = if c.is_lowercase() {
                    c.to_uppercase().next().unwrap()
                } else if c.is_uppercase() {
                    c.to_lowercase().next().unwrap()
                } else {
                    c
                };
                let line = b.get_line(cur.row).unwrap().to_string();
                let mut chars: Vec<char> = line.chars().collect();
                chars[cur.col] = new_c;
                let new_line: String = chars.into_iter().collect();
                let _ = b.set_line(cur.row, cur.col, &new_line);
            }
            state.current_window_mut().set_cursor(cur.row, cur.col + 1);
        }
        Request::IncrementNumber { offset } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let mut b = buf.borrow_mut();
            if let Some(line) = b.get_line(cur.row) {
                let chars: Vec<char> = line.chars().collect();
                let mut start = cur.col;
                while start < chars.len() && !chars[start].is_ascii_digit() && chars[start] != '-' {
                    start += 1;
                }
                if start < chars.len() {
                    let mut end = start + 1;
                    while end < chars.len() && chars[end].is_ascii_digit() {
                        end += 1;
                    }
                    let num_str: String = chars[start..end].iter().collect();
                    if let Ok(num) = num_str.parse::<i32>() {
                        let new_num = num.saturating_add(offset);
                        let new_num_str = new_num.to_string();
                        let mut new_chars = chars[..start].to_vec();
                        new_chars.extend(new_num_str.chars());
                        new_chars.extend(&chars[end..]);
                        let _ = b.set_line(cur.row, start, &new_chars.into_iter().collect::<String>());
                        state.current_window_mut().set_cursor(cur.row, start + new_num_str.len().saturating_sub(1));
                    }
                }
            }
        }
        Request::InsertCopyFromLine { offset } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let target_row = (cur.row as i32 + offset).max(1) as usize;
            let mut c_to_insert = None;
            {
                let b = buf.borrow();
                if target_row <= b.line_count() {
                    if let Some(line) = b.get_line(target_row) {
                        let chars: Vec<char> = line.chars().collect();
                        if cur.col < chars.len() {
                            c_to_insert = Some(chars[cur.col]);
                        }
                    }
                }
            }
            if let Some(c) = c_to_insert {
                let mut b = buf.borrow_mut();
                let _ = b.insert_char(cur.row, cur.col, c);
                state.current_window_mut().set_cursor(cur.row, cur.col + 1);
            }
        }
        _ => {}
    }
    Ok(())
}
