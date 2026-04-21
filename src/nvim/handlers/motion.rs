use crate::nvim::state::{VimState, Mode, VisualMode};
use crate::nvim::window::Cursor;
use crate::nvim::request::{Request, Operator, Motion, TextObject};
use crate::nvim::error::Result;
use crate::nvim::api::handle_request;
use crate::nvim::handlers::set_register_text;

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::CursorMove { row, col } => {
            let cur = state.current_window().cursor();
            if row != cur.row && col == cur.col {
                state.current_window_mut().set_cursor_vertical(row);
            } else {
                state.current_window_mut().set_cursor(row, col);
            }
        }
        Request::OpMotion { op, motion, count } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            
            let mut target = cur;
            for _ in 0..count {
                target = match &motion {
                    Motion::Left => Cursor { row: target.row, col: target.col.saturating_sub(1) },
                    Motion::Right => {
                        let b = buf.borrow();
                        let chars_count = b.get_line(target.row).map(|l| l.chars().count()).unwrap_or(0);
                        Cursor { row: target.row, col: (target.col + 1).min(chars_count.saturating_sub(1)) }
                    },
                    Motion::Up => {
                        let mut next_row = target.row.saturating_sub(1).max(1);
                        state.current_window_mut().set_cursor_vertical(next_row);
                        state.current_window().cursor()
                    },
                    Motion::Down => {
                        let line_count = buf.borrow().line_count();
                        let next_row = (target.row + 1).min(line_count);
                        state.current_window_mut().set_cursor_vertical(next_row);
                        state.current_window().cursor()
                    },
                    Motion::LineStart => Cursor { row: target.row, col: 0 },
                    Motion::LineEnd => {
                        let b = buf.borrow();
                        let chars_count = b.get_line(target.row).map(|l| l.chars().count()).unwrap_or(0);
                        Cursor { row: target.row, col: chars_count.saturating_sub(1) }
                    },
                    Motion::LineFirstChar => {
                        let b = buf.borrow();
                        Cursor { row: target.row, col: b.find_line_first_char(target.row) }
                    },
                    Motion::LineLastChar => {
                        let b = buf.borrow();
                        Cursor { row: target.row, col: b.find_line_last_char(target.row) }
                    },
                    Motion::BufferStart => Cursor { row: 1, col: 0 },
                    Motion::BufferEnd => {
                        let b = buf.borrow();
                        Cursor { row: b.line_count(), col: 0 }
                    },
                    Motion::LineMiddle => {
                        let b = buf.borrow();
                        let chars_count = b.get_line(target.row).map(|l| l.chars().count()).unwrap_or(0);
                        Cursor { row: target.row, col: chars_count / 2 }
                    },
                    Motion::BufferByteOffset => target,
                    Motion::WordForward => {
                        let b = buf.borrow();
                        b.find_word_boundary(target.row, target.col, true)
                    },
                    Motion::WordForwardBlank => {
                        let b = buf.borrow();
                        b.find_big_word_boundary(target.row, target.col, true)
                    },
                    Motion::WordForwardEnd => {
                        let b = buf.borrow();
                        b.find_word_end(target.row, target.col, true)
                    },
                    Motion::WordForwardEndBlank => {
                        let b = buf.borrow();
                        b.find_big_word_end(target.row, target.col, true)
                    },
                    Motion::WordBackward | Motion::WordBack => {
                        let b = buf.borrow();
                        b.find_word_boundary(target.row, target.col, false)
                    },
                    Motion::WordBackwardBlank => {
                        let b = buf.borrow();
                        b.find_big_word_boundary(target.row, target.col, false)
                    },
                    Motion::WordBackwardEnd => {
                        let b = buf.borrow();
                        b.find_word_end(target.row, target.col, false)
                    },
                    Motion::WordBackwardEndBlank => {
                        let b = buf.borrow();
                        b.find_big_word_end(target.row, target.col, false)
                    },
                    Motion::SentenceForward => {
                        let b = buf.borrow();
                        b.find_sentence_boundary(target.row, target.col, true)
                    },
                    Motion::SentenceBackward => {
                        let b = buf.borrow();
                        b.find_sentence_boundary(target.row, target.col, false)
                    },
                    Motion::ParagraphForward => {
                        let b = buf.borrow();
                        b.find_paragraph_boundary(target.row, target.col, true)
                    },
                    Motion::ParagraphBackward => {
                        let b = buf.borrow();
                        b.find_paragraph_boundary(target.row, target.col, false)
                    },
                    Motion::SectionForward => {
                        let b = buf.borrow();
                        b.find_section_boundary(target.row, target.col, true, false)
                    },
                    Motion::SectionBackward => {
                        let b = buf.borrow();
                        b.find_section_boundary(target.row, target.col, false, false)
                    },
                    Motion::SectionEndForward => {
                        let b = buf.borrow();
                        b.find_section_boundary(target.row, target.col, true, true)
                    },
                    Motion::SectionEndBackward => {
                        let b = buf.borrow();
                        b.find_section_boundary(target.row, target.col, false, true)
                    },
                    Motion::BraceForward => {
                        let b = buf.borrow();
                        b.find_char_pos(target.row, target.col, '}', true).unwrap_or(target)
                    },
                    Motion::BraceBackward => {
                        let b = buf.borrow();
                        b.find_char_pos(target.row, target.col, '{', false).unwrap_or(target)
                    },
                    Motion::ParenForward => {
                        let b = buf.borrow();
                        b.find_char_pos(target.row, target.col, ')', true).unwrap_or(target)
                    },
                    Motion::ParenBackward => {
                        let b = buf.borrow();
                        b.find_char_pos(target.row, target.col, '(', false).unwrap_or(target)
                    },
                    Motion::DiagnosticForward | Motion::DiagnosticBackward => target,
                    Motion::NextChange => {
                        let mut b = buf.borrow_mut();
                        if let Some(action) = b.undo_tree.pop_redo() {
                            let lnum = match &action {
                                crate::nvim::undo::Action::InsertLine { lnum, .. } => *lnum,
                                crate::nvim::undo::Action::DeleteLine { lnum, .. } => *lnum,
                                crate::nvim::undo::Action::ReplaceLine { lnum, .. } => *lnum,
                                crate::nvim::undo::Action::Group(actions) => {
                                    match actions.first() {
                                        Some(crate::nvim::undo::Action::InsertLine { lnum, .. }) => *lnum,
                                        Some(crate::nvim::undo::Action::DeleteLine { lnum, .. }) => *lnum,
                                        Some(crate::nvim::undo::Action::ReplaceLine { lnum, .. }) => *lnum,
                                        _ => 1,
                                    }
                                }
                            };
                            Cursor { row: lnum, col: 0 }
                        } else { target }
                    },
                    Motion::PrevChange => {
                        let mut b = buf.borrow_mut();
                        if let Some(action) = b.undo_tree.pop_undo() {
                            let lnum = match &action {
                                crate::nvim::undo::Action::InsertLine { lnum, .. } => *lnum,
                                crate::nvim::undo::Action::DeleteLine { lnum, .. } => *lnum,
                                crate::nvim::undo::Action::ReplaceLine { lnum, .. } => *lnum,
                                crate::nvim::undo::Action::Group(actions) => {
                                    match actions.first() {
                                        Some(crate::nvim::undo::Action::InsertLine { lnum, .. }) => *lnum,
                                        Some(crate::nvim::undo::Action::DeleteLine { lnum, .. }) => *lnum,
                                        Some(crate::nvim::undo::Action::ReplaceLine { lnum, .. }) => *lnum,
                                        _ => 1,
                                    }
                                }
                            };
                            Cursor { row: lnum, col: 0 }
                        } else { target }
                    },
                    _ => target,
                };
            }

            // 特殊対応: オペレータ付きの Word 移動が行を跨ぐ場合は行末で止める
            if op != Operator::None && !motion.is_linewise() && target.row > cur.row {
                match motion {
                    Motion::WordForward | Motion::WordForwardBlank => {
                        let b = buf.borrow();
                        if let Some(line) = b.get_line(cur.row) {
                            target = Cursor { row: cur.row, col: line.chars().count().saturating_sub(1) };
                        }
                    }
                    _ => {}
                }
            }

            match op {
                Operator::None => {
                    if !matches!(motion, Motion::Up | Motion::Down) {
                        state.current_window_mut().set_cursor(target.row, target.col);
                        if let Motion::LineEnd = motion {
                            state.current_window_mut().set_curswant(usize::MAX);
                        }
                    }
                },
                Operator::Delete => {
                    let start_col = if cur.row == target.row { cur.col.min(target.col) } else { 0 };
                    {
                        let mut b = buf.borrow_mut();
                        b.start_undo_group();
                        if cur.row == target.row {
                            let start = cur.col.min(target.col);
                            let mut end = cur.col.max(target.col);
                            if !motion.is_inclusive() && end > start {
                                end -= 1;
                            }
                            for _ in start..=end {
                                b.delete_char(cur.row, start + 1)?;
                            }
                        } else {
                            let (start, end) = if cur.row < target.row || (cur.row == target.row && cur.col <= target.col) { (cur, target) } else { (target, cur) };
                            if motion.is_linewise() {
                                for _ in start.row..=end.row { b.delete_line(start.row)?; }
                            } else {
                                let first_line = b.get_line(start.row).unwrap_or("").to_string();
                                let last_line = b.get_line(end.row).unwrap_or("").to_string();
                                let first_chars: Vec<char> = first_line.chars().collect();
                                let last_chars: Vec<char> = last_line.chars().collect();
                                let mut new_first_line: String = first_chars[..start.col].iter().collect();
                                let last_part_start = if motion.is_inclusive() { end.col + 1 } else { end.col };
                                if last_part_start < last_chars.len() {
                                    new_first_line.push_str(&last_chars[last_part_start..].iter().collect::<String>());
                                }
                                for r in (start.row..=end.row).rev() { b.delete_line(r)?; }
                                b.insert_line(start.row, &new_first_line)?;
                            }
                        }
                        b.end_undo_group();
                    }
                    let line_count = buf.borrow().line_count();
                    state.current_window_mut().set_cursor(cur.row.min(line_count), start_col);
                    state.last_change = Some(Request::OpMotion { op, motion, count });
                },
                Operator::Yank => {
                    let b = buf.borrow();
                    let reg = state.pending_register.unwrap_or('"');
                    if cur.row == target.row {
                        if let Some(line) = b.get_line(cur.row) {
                            let start = cur.col.min(target.col);
                            let mut end = cur.col.max(target.col);
                            if !motion.is_inclusive() && end > start { end -= 1; }
                            let text = line.chars().skip(start).take(end - start + 1).collect::<String>();
                            set_register_text(state, reg, text);
                        }
                    } else {
                        let (start, end) = if cur.row < target.row || (cur.row == target.row && cur.col <= target.col) { (cur, target) } else { (target, cur) };
                        let mut content = String::new();
                        if motion.is_linewise() {
                            for r in start.row..=end.row {
                                if let Some(line) = b.get_line(r) {
                                    content.push_str(line); content.push('\n');
                                }
                            }
                        } else {
                            for r in start.row..=end.row {
                                if let Some(line) = b.get_line(r) {
                                    let chars: Vec<char> = line.chars().collect();
                                    let s = if r == start.row { start.col } else { 0 };
                                    let mut e = if r == end.row { end.col } else { chars.len().saturating_sub(1) };
                                    if r == end.row && !motion.is_inclusive() && e > 0 { e -= 1; }
                                    content.push_str(&chars[s..=e].iter().collect::<String>());
                                    if r < end.row { content.push('\n'); }
                                }
                            }
                        }
                        set_register_text(state, reg, content);
                    }
                },
                Operator::ToUpper | Operator::ToLower | Operator::SwapCase => {
                    let (start, end) = if cur.row < target.row || (cur.row == target.row && cur.col <= target.col) { (cur, target) } else { (target, cur) };
                    {
                        let mut b = buf.borrow_mut();
                        b.start_undo_group();
                        for r in start.row..=end.row {
                            if let Some(line) = b.get_line(r) {
                                let mut chars: Vec<char> = line.chars().collect();
                                let s_col = if r == start.row { start.col } else { 0 };
                                let e_col = if r == end.row { 
                                    let mut ec = end.col;
                                    if !motion.is_inclusive() && ec > s_col && r == start.row { ec -= 1; }
                                    ec.min(chars.len().saturating_sub(1))
                                } else { chars.len().saturating_sub(1) };
                                for c_idx in s_col..=e_col {
                                    if let Some(&c) = chars.get(c_idx) {
                                        let new_c = match op {
                                            Operator::ToUpper => c.to_uppercase().next().unwrap(),
                                            Operator::ToLower => c.to_lowercase().next().unwrap(),
                                            Operator::SwapCase => if c.is_uppercase() { c.to_lowercase().next().unwrap() } else { c.to_uppercase().next().unwrap() },
                                            _ => c,
                                        };
                                        chars[c_idx] = new_c;
                                    }
                                }
                                let new_line: String = chars.into_iter().collect();
                                let _ = b.set_line(r, s_col, &new_line);
                            }
                        }
                        b.end_undo_group();
                    }
                },
                Operator::Change => {
                    let mut m = motion.clone();
                    if let Motion::WordForward = m {
                        let b = buf.borrow();
                        if let Some(line) = b.get_line(cur.row) {
                            if cur.col < line.chars().count() && (line.chars().nth(cur.col).unwrap().is_alphanumeric() || line.chars().nth(cur.col).unwrap() == '_') {
                                m = Motion::WordForwardEnd;
                            }
                        }
                    }
                    state.set_mode(Mode::Insert);
                    handle_request(state, Request::OpMotion { op: Operator::Delete, motion: m, count })?;
                },
                _ => {}
            }
        }
        Request::OpVisualSelection(op) => {
            let win = state.current_window();
            let cur = win.cursor();
            let anchor = state.visual_anchor.unwrap_or(cur);
            let buf = win.buffer();
            let vmode = match state.mode { Mode::Visual(m) => m, _ => VisualMode::Char };
            let (start, end) = if anchor.row < cur.row || (anchor.row == cur.row && anchor.col <= cur.col) { (anchor, cur) } else { (cur, anchor) };
            match op {
                Operator::Delete => {
                    let mut b = buf.borrow_mut();
                    b.start_undo_group();
                    match vmode {
                        VisualMode::Block => {
                            let min_col = anchor.col.min(cur.col);
                            let max_col = anchor.col.max(cur.col);
                            for r in start.row..=end.row {
                                if let Some(line) = b.get_line(r) {
                                    let chars: Vec<char> = line.chars().collect();
                                    if min_col < chars.len() {
                                        let count = max_col.min(chars.len() - 1) - min_col + 1;
                                        for _ in 0..count { b.delete_char(r, min_col + 1)?; }
                                    }
                                }
                            }
                        }
                        VisualMode::Char if start.row == end.row => {
                            for _ in 0..(end.col - start.col + 1) { b.delete_char(start.row, start.col + 1)?; }
                        }
                        _ => {
                            for _ in start.row..=end.row { b.delete_line(start.row)?; }
                        }
                    }
                    b.end_undo_group();
                    let line_count = b.line_count();
                    state.current_window_mut().set_cursor(start.row.min(line_count), if start.row == end.row && vmode != VisualMode::Line { start.col } else { 0 });
                },
                _ => {}
            }
            state.visual_anchor = None;
            state.set_mode(Mode::Normal);
        }
        Request::OpTextObject { op, inner, obj, count } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let range = {
                let b = buf.borrow();
                match obj {
                    TextObject::Word => b.get_word_range(cur.row, cur.col, inner),
                    TextObject::Paren => b.get_pair_range(cur.row, cur.col, '(', ')', inner),
                    _ => None,
                }
            };
            if let Some((start, end)) = range {
                match op {
                    Operator::Delete => {
                        let mut b = buf.borrow_mut();
                        b.start_undo_group();
                        if start.row == end.row {
                            for _ in 0..(end.col - start.col + 1) { b.delete_char(start.row, start.col + 1)?; }
                        }
                        b.end_undo_group();
                        let line_count = b.line_count();
                        state.current_window_mut().set_cursor(start.row.min(line_count), start.col);
                    },
                    Operator::Change => {
                        state.set_mode(Mode::Insert);
                        handle_request(state, Request::OpTextObject { op: Operator::Delete, inner, obj, count })?;
                    },
                    _ => {}
                }
            }
        }
        Request::DeleteCurrentLine { count } => {
            let buf = state.current_window().buffer();
            let row = state.current_window().cursor().row;
            let mut b = buf.borrow_mut();
            b.start_undo_group();
            for _ in 0..count { if row <= b.line_count() { b.delete_line(row)?; } }
            b.end_undo_group();
            let line_count = b.line_count();
            state.current_window_mut().set_cursor(row.min(line_count), 0);
        }
        Request::SearchNext { forward, count } => {
            for _ in 0..count {
                if let Some(query) = state.last_search.clone() {
                    crate::nvim::api::handle_search_next(state, &query, forward);
                }
            }
        }
        Request::JumpToMark(c) => {
            let mark = state.current_window().buffer().borrow().get_mark(c);
            if let Some(m) = mark { state.current_window_mut().set_cursor(m.row, m.col); }
        }
        Request::CenterCursor => state.current_window_mut().center_cursor(),
        Request::CursorToTop => state.current_window_mut().cursor_to_top(),
        Request::CursorToBottom => state.current_window_mut().cursor_to_bottom(),
        _ => {}
    }
    Ok(())
}
