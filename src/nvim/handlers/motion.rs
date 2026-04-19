use crate::nvim::state::{VimState, Mode, VisualMode};
use crate::nvim::window::Cursor;
use crate::nvim::request::{Request, Operator, Motion, TextObject};
use crate::nvim::error::Result;
use crate::nvim::api::handle_request;
use crate::nvim::handlers::set_register_text;

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::CursorMove { row, col } => {
            state.current_window_mut().set_cursor(row, col);
        }
        Request::OpMotion { op, motion } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            
            let target = match motion {
                Motion::Left => Cursor { row: cur.row, col: cur.col.saturating_sub(1) },
                Motion::Right => {
                    let b = buf.borrow();
                    let chars_count = b.get_line(cur.row).map(|l| l.chars().count()).unwrap_or(0);
                    Cursor { row: cur.row, col: (cur.col + 1).min(chars_count.saturating_sub(1)) }
                },
                Motion::Up => Cursor { row: cur.row.saturating_sub(1).max(1), col: cur.col },
                Motion::Down => {
                    let b = buf.borrow();
                    Cursor { row: (cur.row + 1).min(b.line_count()), col: cur.col }
                },
                Motion::LineStart => Cursor { row: cur.row, col: 0 },
                Motion::LineEnd => {
                    let b = buf.borrow();
                    let chars_count = b.get_line(cur.row).map(|l| l.chars().count()).unwrap_or(0);
                    Cursor { row: cur.row, col: chars_count.saturating_sub(1) }
                },
                Motion::LineFirstChar => {
                    let b = buf.borrow();
                    Cursor { row: cur.row, col: b.find_line_first_char(cur.row) }
                },
                Motion::LineLastChar => {
                    let b = buf.borrow();
                    Cursor { row: cur.row, col: b.find_line_last_char(cur.row) }
                },
                Motion::BufferStart => Cursor { row: 1, col: 0 },
                Motion::BufferEnd => {
                    let b = buf.borrow();
                    Cursor { row: b.line_count(), col: 0 }
                },
                Motion::LineMiddle => {
                    let b = buf.borrow();
                    let chars_count = b.get_line(cur.row).map(|l| l.chars().count()).unwrap_or(0);
                    Cursor { row: cur.row, col: chars_count / 2 }
                },
                Motion::BufferByteOffset => {
                    cur // Requires count input logic, which we can approximate or ignore if parameter isn't passed here
                },
                Motion::WordForward | Motion::WordForwardBlank => {
                    let b = buf.borrow();
                    b.find_word_boundary(cur.row, cur.col, true)
                },
                Motion::WordForwardEnd | Motion::WordForwardEndBlank => {
                    let b = buf.borrow();
                    b.find_word_end(cur.row, cur.col, true)
                },
                Motion::WordBackward | Motion::WordBack | Motion::WordBackwardBlank => {
                    let b = buf.borrow();
                    b.find_word_boundary(cur.row, cur.col, false)
                },
                Motion::WordBackwardEnd | Motion::WordBackwardEndBlank => {
                    let b = buf.borrow();
                    b.find_word_end(cur.row, cur.col, false)
                },
                Motion::SentenceForward => {
                    let b = buf.borrow();
                    b.find_sentence_boundary(cur.row, cur.col, true)
                },
                Motion::SentenceBackward => {
                    let b = buf.borrow();
                    b.find_sentence_boundary(cur.row, cur.col, false)
                },
                Motion::ParagraphForward => {
                    let b = buf.borrow();
                    b.find_paragraph_boundary(cur.row, cur.col, true)
                },
                Motion::ParagraphBackward => {
                    let b = buf.borrow();
                    b.find_paragraph_boundary(cur.row, cur.col, false)
                },
                Motion::SectionForward => {
                    let b = buf.borrow();
                    b.find_section_boundary(cur.row, cur.col, true, false)
                },
                Motion::SectionBackward => {
                    let b = buf.borrow();
                    b.find_section_boundary(cur.row, cur.col, false, false)
                },
                Motion::SectionEndForward => {
                    let b = buf.borrow();
                    b.find_section_boundary(cur.row, cur.col, true, true)
                },
                Motion::SectionEndBackward => {
                    let b = buf.borrow();
                    b.find_section_boundary(cur.row, cur.col, false, true)
                },
                Motion::BraceForward => {
                    let b = buf.borrow();
                    b.find_char_pos(cur.row, cur.col, '}', true).unwrap_or(cur)
                },
                Motion::BraceBackward => {
                    let b = buf.borrow();
                    b.find_char_pos(cur.row, cur.col, '{', false).unwrap_or(cur)
                },
                Motion::ParenForward => {
                    let b = buf.borrow();
                    b.find_char_pos(cur.row, cur.col, ')', true).unwrap_or(cur)
                },
                Motion::ParenBackward => {
                    let b = buf.borrow();
                    b.find_char_pos(cur.row, cur.col, '(', false).unwrap_or(cur)
                },
                Motion::DiagnosticForward | Motion::DiagnosticBackward => {
                    cur // TODO: Implement when diagnostic system is added
                },
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
                    } else { cur }
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
                    } else { cur }
                },
                Motion::SearchForward(ref _query) => {
                    cur // TODO
                }
                Motion::WindowTop => {
                    Cursor { row: win.topline(), col: 0 }
                }
                Motion::WindowMiddle => {
                    Cursor { row: win.topline() + win.height() / 2, col: 0 }
                }
                Motion::WindowBottom => {
                    Cursor { row: win.topline() + win.height().saturating_sub(1), col: 0 }
                }
            };

            match op {
                Operator::None => {
                    state.current_window_mut().set_cursor(target.row, target.col);
                },
                Operator::Delete => {
                    {
                        let mut b = buf.borrow_mut();
                        if cur.row == target.row {
                            let start = cur.col.min(target.col);
                            let end = cur.col.max(target.col);
                            for _ in start..=end {
                                b.delete_char(cur.row, start + 1)?;
                            }
                        } else {
                            let start_row = cur.row.min(target.row);
                            let end_row = cur.row.max(target.row);
                            for _ in start_row..=end_row {
                                b.delete_line(start_row)?;
                            }
                        }
                    }
                    let new_row = cur.row.min(buf.borrow().line_count());
                    state.current_window_mut().set_cursor(new_row, 0);
                    state.last_change = Some(Request::OpMotion { op, motion });
                },
                Operator::Yank => {
                    let b = buf.borrow();
                    let reg = state.pending_register.unwrap_or('"');
                    if cur.row == target.row {
                        if let Some(line) = b.get_line(cur.row) {
                            let start = cur.col.min(target.col);
                            let end = cur.col.max(target.col);
                            let text = line.chars().skip(start).take(end - start + 1).collect::<String>();
                            set_register_text(state, reg, text);
                        }
                    } else {
                        let mut content = String::new();
                        let start_row = cur.row.min(target.row);
                        let end_row = cur.row.max(target.row);
                        for r in start_row..=end_row {
                            if let Some(line) = b.get_line(r) {
                                content.push_str(line);
                                content.push('\n');
                            }
                        }
                        set_register_text(state, reg, content);
                    }
                },
                Operator::ToUpper | Operator::ToLower | Operator::SwapCase => {
                    let mut b = buf.borrow_mut();
                    let start_row = cur.row.min(target.row);
                    let end_row = cur.row.max(target.row);
                    for r in start_row..=end_row {
                        if let Some(line) = b.get_line(r) {
                            let mut chars: Vec<char> = line.chars().collect();
                            let s_col = if r == start_row { cur.col.min(target.col) } else { 0 };
                            let e_col = if r == end_row { cur.col.max(target.col).min(chars.len().saturating_sub(1)) } else { chars.len().saturating_sub(1) };
                            
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
                            let _ = b.set_line(r, &new_line);
                        }
                    }
                },
                Operator::Change => {
                    let m = motion.clone();
                    handle_request(state, Request::OpMotion { op: Operator::Delete, motion: m })?;
                    state.set_mode(Mode::Insert);
                },
                Operator::Indent | Operator::Outdent => {
                    let start_row = cur.row.min(target.row);
                    let end_row = cur.row.max(target.row);
                    let forward = matches!(op, Operator::Indent);
                    for r in start_row..=end_row {
                        state.current_window_mut().set_cursor(r, 0);
                        handle_request(state, Request::Indent { forward })?;
                    }
                    state.current_window_mut().set_cursor(start_row, 0);
                }
            }
        }
        Request::OpVisualSelection(op) => {
            let win = state.current_window();
            let cur = win.cursor();
            let anchor = state.visual_anchor.unwrap_or(cur);
            let buf = win.buffer();
            let vmode = match state.mode {
                Mode::Visual(m) => m,
                _ => VisualMode::Char,
            };

            let (start, end) = if anchor.row < cur.row || (anchor.row == cur.row && anchor.col <= cur.col) {
                (anchor, cur)
            } else {
                (cur, anchor)
            };

            match op {
                Operator::Delete => {
                    {
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
                                            let actual_max = max_col.min(chars.len() - 1);
                                            let count = actual_max - min_col + 1;
                                            for _ in 0..count {
                                                b.delete_char(r, min_col + 1)?;
                                            }
                                        }
                                    }
                                }
                            }
                            VisualMode::Char if start.row == end.row => {
                                let count = end.col - start.col + 1;
                                for _ in 0..count {
                                    b.delete_char(start.row, start.col + 1)?;
                                }
                            }
                            _ => {
                                // Visual Line or Multi-line Visual Char (approx)
                                let start_row = start.row;
                                let end_row = end.row;
                                for _ in start_row..=end_row {
                                    b.delete_line(start_row)?;
                                }
                            }
                        }
                        b.end_undo_group();
                    }
                    state.current_window_mut().set_cursor(start.row.min(buf.borrow().line_count()), 0);
                },
                Operator::Yank => {
                    let b = buf.borrow();
                    let reg = state.pending_register.unwrap_or('"');
                    match vmode {
                        VisualMode::Block => {
                            let mut content = String::new();
                            let min_col = anchor.col.min(cur.col);
                            let max_col = anchor.col.max(cur.col);
                            for r in start.row..=end.row {
                                if let Some(line) = b.get_line(r) {
                                    let chars: Vec<char> = line.chars().collect();
                                    if min_col < chars.len() {
                                        let actual_max = max_col.min(chars.len() - 1);
                                        let text: String = chars[min_col..=actual_max].iter().collect();
                                        content.push_str(&text);
                                    }
                                    content.push('\n');
                                }
                            }
                            set_register_text(state, reg, content);
                        }
                        VisualMode::Char if start.row == end.row => {
                            if let Some(line) = b.get_line(start.row) {
                                let text = line.chars().skip(start.col).take(end.col - start.col + 1).collect::<String>();
                                set_register_text(state, reg, text);
                            }
                        }
                        _ => {
                            let mut content = String::new();
                            for r in start.row..=end.row {
                                if let Some(line) = b.get_line(r) {
                                    let chars: Vec<char> = line.chars().collect();
                                    let s = if r == start.row && vmode == VisualMode::Char { start.col } else { 0 };
                                    let e = if r == end.row && vmode == VisualMode::Char { end.col.min(chars.len().saturating_sub(1)) } else { chars.len().saturating_sub(1) };
                                    let text: String = chars[s..=e].iter().collect();
                                    content.push_str(&text);
                                    content.push('\n');
                                }
                            }
                            set_register_text(state, reg, content);
                        }
                    }
                },
                Operator::ToUpper | Operator::ToLower | Operator::SwapCase => {
                    let mut b = buf.borrow_mut();
                    b.start_undo_group();
                    let min_col = anchor.col.min(cur.col);
                    let max_col = anchor.col.max(cur.col);
                    for r in start.row..=end.row {
                        if let Some(line) = b.get_line(r) {
                            let mut chars: Vec<char> = line.chars().collect();
                            let s_col = match vmode {
                                VisualMode::Block => min_col,
                                VisualMode::Char if r == start.row => start.col,
                                _ => 0,
                            };
                            let e_col = match vmode {
                                VisualMode::Block => max_col.min(chars.len().saturating_sub(1)),
                                VisualMode::Char if r == end.row => end.col.min(chars.len().saturating_sub(1)),
                                _ => chars.len().saturating_sub(1),
                            };
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
                            let _ = b.set_line(r, &new_line);
                        }
                    }
                    b.end_undo_group();
                },
                Operator::Change => {
                    handle_request(state, Request::OpVisualSelection(Operator::Delete))?;
                    state.set_mode(Mode::Insert);
                    return Ok(());
                },
                Operator::Indent | Operator::Outdent => {
                    let forward = matches!(op, Operator::Indent);
                    let tabstop = match state.options.get("tabstop") {
                        Some(crate::nvim::state::OptionValue::Int(n)) => *n as usize,
                        _ => 8,
                    };
                    {
                        let mut b = buf.borrow_mut();
                        b.start_undo_group();
                        for r in start.row..=end.row {
                            b.indent_line(r, forward, tabstop)?;
                        }
                        b.end_undo_group();
                    }
                    state.current_window_mut().set_cursor(start.row, 0);
                }
                _ => {}
            }
            state.visual_anchor = None;
            state.set_mode(Mode::Normal);
        }
        Request::RestoreVisualSelection => {
            if let (Some(anchor), Some(cur), Some(vmode)) = (state.last_visual_anchor, state.last_visual_cursor, state.last_visual_mode) {
                state.visual_anchor = Some(anchor);
                state.current_window_mut().set_cursor(cur.row, cur.col);
                state.set_mode(Mode::Visual(vmode));
            }
        }
        Request::SelectSearchMatch { forward } => {
            if let Some(query) = state.last_search.clone() {
                let query_converted = crate::nvim::regexp::convert_vim_regex(&query);
                let cur = state.current_window().cursor();
                let b = state.current_window().buffer();
                let b_ref = b.borrow();
                
                if let Ok(re) = regex::Regex::new(&query_converted) {
                    let mut found = None;
                    if forward {
                        // Search forward
                        for lnum in cur.row..=b_ref.line_count() {
                            if let Some(line) = b_ref.get_line(lnum) {
                                let start_col = if lnum == cur.row { cur.col + 1 } else { 0 };
                                if let Some(m) = re.find(&line[start_col..]) {
                                    found = Some((lnum, start_col + m.start(), start_col + m.end()));
                                    break;
                                }
                            }
                        }
                    } else {
                        // Search backward
                        for lnum in (1..=cur.row).rev() {
                            if let Some(line) = b_ref.get_line(lnum) {
                                let end_col = if lnum == cur.row { cur.col } else { line.len() };
                                let matches: Vec<_> = re.find_iter(&line[..end_col]).collect();
                                if let Some(m) = matches.last() {
                                    found = Some((lnum, m.start(), m.end()));
                                    break;
                                }
                            }
                        }
                    }
                    
                    if let Some((lnum, start, end)) = found {
                        state.visual_anchor = Some(Cursor { row: lnum, col: start });
                        state.current_window_mut().set_cursor(lnum, end.saturating_sub(1));
                        state.set_mode(Mode::Visual(crate::nvim::state::VisualMode::Char));
                    }
                }
            }
        }
        Request::OpTextObject { op, inner, obj } => {
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let range = {
                let b = buf.borrow();
                match obj {
                    TextObject::Word => b.get_word_range(cur.row, cur.col, inner),
                    TextObject::BigWord => b.get_big_word_range(cur.row, cur.col, inner),
                    TextObject::Paren => b.get_pair_range(cur.row, cur.col, '(', ')', inner),
                    TextObject::Bracket => b.get_pair_range(cur.row, cur.col, '[', ']', inner),
                    TextObject::Brace => b.get_pair_range(cur.row, cur.col, '{', '}', inner),
                    TextObject::AngleBracket => b.get_pair_range(cur.row, cur.col, '<', '>', inner),
                    TextObject::Quote => b.get_quote_range(cur.row, cur.col, '\'', inner),
                    TextObject::DoubleQuote => b.get_quote_range(cur.row, cur.col, '"', inner),
                    TextObject::Backtick => b.get_quote_range(cur.row, cur.col, '`', inner),
                    TextObject::Sentence => b.get_sentence_range(cur.row, cur.col, inner),
                    TextObject::Paragraph => b.get_paragraph_range(cur.row, cur.col, inner),
                    _ => None,
                }
            };

            if let Some((start, end)) = range {
                match op {
                    Operator::Delete => {
                        {
                            let mut b = buf.borrow_mut();
                            if start.row == end.row {
                                let count = end.col - start.col + 1;
                                for _ in 0..count {
                                    b.delete_char(start.row, start.col + 1)?;
                                }
                            } else {
                                for _ in start.row..=end.row {
                                    b.delete_line(start.row)?;
                                }
                            }
                        }
                        state.current_window_mut().set_cursor(start.row, start.col);
                        state.last_change = Some(Request::OpTextObject { op, inner, obj });
                    },
                    Operator::Yank => {
                        let b = buf.borrow();
                        let reg = state.pending_register.unwrap_or('"');
                        if start.row == end.row {
                            if let Some(line) = b.get_line(start.row) {
                                let text = line.chars().skip(start.col).take(end.col - start.col + 1).collect::<String>();
                                set_register_text(state, reg, text);
                            }
                        }
                    },
                    Operator::ToUpper | Operator::ToLower | Operator::SwapCase => {
                        let mut b = buf.borrow_mut();
                        for r in start.row..=end.row {
                            if let Some(line) = b.get_line(r) {
                                let mut chars: Vec<char> = line.chars().collect();
                                let s_col = if r == start.row { start.col } else { 0 };
                                let e_col = if r == end.row { end.col.min(chars.len().saturating_sub(1)) } else { chars.len().saturating_sub(1) };
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
                                let _ = b.set_line(r, &new_line);
                            }
                        }
                    },
                    Operator::Change => {
                        let o = obj.clone();
                        handle_request(state, Request::OpTextObject { op: Operator::Delete, inner, obj: o })?;
                        state.set_mode(Mode::Insert);
                    }
                    Operator::None => {
                        state.visual_anchor = Some(start);
                        state.current_window_mut().set_cursor(end.row, end.col);
                    }
                    _ => {}
                }
            }
        }
        Request::JumpToPair => {
            let target = state.current_window().buffer().borrow().find_matching_pair(state.current_window().cursor().row, state.current_window().cursor().col);
            if let Some(t) = target {
                state.current_window_mut().set_cursor(t.row, t.col);
            }
        }
        Request::MoveWord { forward, end: _ } => {
            let motion = if forward { Motion::WordForward } else { Motion::WordBackward };
            handle_request(state, Request::OpMotion { op: Operator::None, motion })?;
        }
        Request::JumpToLineBoundary { end } => {
            let motion = if end { Motion::LineEnd } else { Motion::LineStart };
            handle_request(state, Request::OpMotion { op: Operator::None, motion })?;
        }
        Request::JumpToBufferBoundary { end } => {
            let motion = if end { Motion::BufferEnd } else { Motion::BufferStart };
            handle_request(state, Request::OpMotion { op: Operator::None, motion })?;
        }
        Request::FindChar { forward, target, till } => {
            state.last_char_search = Some(crate::nvim::state::LastCharSearch { target, forward, till });
            let win = state.current_window();
            let cur = win.cursor();
            let buf = win.buffer();
            let b = buf.borrow();
            if let Some(line) = b.get_line(cur.row) {
                if forward {
                    let search_area = &line[cur.col + 1..];
                    if let Some(idx) = search_area.find(target) {
                        let mut target_col = cur.col + 1 + idx;
                        if till { target_col = target_col.saturating_sub(1); }
                        state.current_window_mut().set_cursor(cur.row, target_col);
                    }
                } else {
                    let search_area = &line[..cur.col];
                    if let Some(idx) = search_area.rfind(target) {
                        let mut target_col = idx;
                        if till { target_col += 1; }
                        state.current_window_mut().set_cursor(cur.row, target_col);
                    }
                }
            }
        }
        Request::RepeatLastCharSearch => {
            if let Some(last) = state.last_char_search.clone() {
                handle_request(state, Request::FindChar {
                    forward: last.forward,
                    target: last.target,
                    till: last.till,
                })?;
            }
        }
        Request::JumpBack => {
            if state.jump_idx > 0 {
                state.jump_idx -= 1;
                let (path, pos) = state.jump_list[state.jump_idx].clone();
                handle_request(state, Request::OpenFile(path))?;
                state.current_window_mut().set_cursor(pos.row, pos.col);
            }
        }
        Request::JumpForward => {
            if state.jump_list.len() > 0 && state.jump_idx + 1 < state.jump_list.len() {
                state.jump_idx += 1;
                let (path, pos) = state.jump_list[state.jump_idx].clone();
                handle_request(state, Request::OpenFile(path))?;
                state.current_window_mut().set_cursor(pos.row, pos.col);
            }
        }
        Request::SetMark(c) => {
            let cur = state.current_window().cursor();
            let buf = state.current_window().buffer();
            buf.borrow_mut().set_mark(c, cur);
        }
        Request::JumpToMark(c) => {
            let mark = state.current_window().buffer().borrow().get_mark(c);
            if let Some(m) = mark {
                state.current_window_mut().set_cursor(m.row, m.col);
            }
        }
        Request::CenterCursor => {
            state.current_window_mut().center_cursor();
        }
        Request::CursorToTop => {
            state.current_window_mut().cursor_to_top();
        }
        Request::CursorToBottom => {
            state.current_window_mut().cursor_to_bottom();
        }
        Request::CursorToTopWithCR => {
            state.current_window_mut().cursor_to_top();
            let row = state.current_window().cursor().row;
            let col = state.current_window().buffer().borrow().find_line_first_char(row);
            state.current_window_mut().set_cursor(row, col);
        }
        Request::CursorToCenterWithDot => {
            state.current_window_mut().center_cursor();
            let row = state.current_window().cursor().row;
            let col = state.current_window().buffer().borrow().find_line_first_char(row);
            state.current_window_mut().set_cursor(row, col);
        }
        Request::CursorToBottomWithMinus => {
            state.current_window_mut().cursor_to_bottom();
            let row = state.current_window().cursor().row;
            let col = state.current_window().buffer().borrow().find_line_first_char(row);
            state.current_window_mut().set_cursor(row, col);
        }
        _ => {}
    }
    Ok(())
}
