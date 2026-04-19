use crate::nvim::memline::MemLine;
use crate::nvim::memfile::MemFile;
use crate::nvim::error::{NvimError, Result};
use crate::nvim::undo::{UndoTree, Action};
use crate::nvim::window::Cursor;
use std::sync::atomic::{AtomicI32, Ordering};
use std::collections::HashMap;

/// Buffer ID (handle_T) generator
static NEXT_BUFFER_ID: AtomicI32 = AtomicI32::new(1);

/// Represents a Neovim buffer (buf_T).
pub struct Buffer {
    /// Unique identifier for the buffer (b_fnum / handle)
    id: i32,
    /// The actual text content
    memline: MemLine,
    /// スワップファイル
    #[allow(dead_code)]
    memfile: MemFile,
    /// Full path of the file (b_ffname)
    full_name: Option<String>,
    /// Short name of the file (b_sfname)
    short_name: Option<String>,
    /// アンドゥ履歴
    pub undo_tree: UndoTree,
    /// マーク位置の記録 ('a' ~ 'z')
    pub marks: HashMap<char, Cursor>,
    /// バーチャルテキスト (lnum -> text)
    pub virtual_text: HashMap<usize, String>,
    /// サインカラム情報 (lnum -> sign text)
    pub signs: HashMap<usize, String>,
    /// Extmark 管理
    pub extmark_manager: crate::nvim::extmark::ExtmarkManager,
    /// 'modified' flag (b_changed)
    modified: bool,
    /// 'readonly' flag
    readonly: bool,
    /// Stack for collecting grouped undo actions
    undo_group_stack: Vec<Vec<Action>>,
}

impl Buffer {
    /// Creates a new empty buffer with a unique ID.
    pub fn new() -> Self {
        let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::SeqCst);
        let mut memfile = MemFile::new(id);
        let _ = memfile.open();
        Self {
            id,
            memline: MemLine::new(),
            memfile,
            full_name: None,
            short_name: None,
            undo_tree: UndoTree::new(),
            marks: HashMap::new(),
            virtual_text: HashMap::new(),
            signs: HashMap::new(),
            extmark_manager: crate::nvim::extmark::ExtmarkManager::new(),
            modified: false,
            readonly: false,
            undo_group_stack: Vec::new(),
        }
    }

    pub fn start_undo_group(&mut self) {
        self.undo_group_stack.push(Vec::new());
    }

    pub fn end_undo_group(&mut self) {
        if let Some(group) = self.undo_group_stack.pop() {
            if !group.is_empty() {
                self.push_action(Action::Group(group));
            }
        }
    }

    fn push_action(&mut self, action: Action) {
        if let Some(current_group) = self.undo_group_stack.last_mut() {
            current_group.push(action);
        } else {
            self.undo_tree.push(action);
        }
    }

    pub fn set_extmark(&mut self, row: usize, col: usize) -> i32 {
        self.extmark_manager.set_extmark(row, col)
    }

    pub fn get_extmark(&self, id: i32) -> Option<crate::nvim::extmark::Extmark> {
        self.extmark_manager.get_extmark(id)
    }

    pub fn set_mark(&mut self, name: char, cursor: Cursor) {
        self.marks.insert(name, cursor);
    }

    pub fn get_mark(&self, name: char) -> Option<Cursor> {
        self.marks.get(&name).copied()
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn line_count(&self) -> usize {
        self.memline.line_count()
    }

    pub fn get_line(&self, lnum: usize) -> Option<&str> {
        self.memline.get_line(lnum)
    }

    pub fn set_line(&mut self, lnum: usize, col: usize, text: &str) -> Result<()> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }
        
        let old_text = self.get_line(lnum).unwrap_or("").to_string();
        self.memline.replace_line(lnum, text).map_err(|e| NvimError::Buffer(e.to_string()))?;
        
        // 履歴に記録
        self.push_action(Action::ReplaceLine {
            lnum,
            col,
            old_text,
            new_text: text.to_string(),
        });

        self.modified = true;
        Ok(())
    }

    pub fn append_line(&mut self, text: &str) -> Result<()> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }
        self.memline.append_line(text);
        let lnum = self.line_count();
        
        // 履歴に記録
        self.push_action(Action::InsertLine {
            lnum,
            col: 0,
            text: text.to_string(),
        });

        self.modified = true;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<Option<Cursor>> {
        if let Some(action) = self.undo_tree.pop_undo() {
            return self.apply_undo_action(action);
        }
        Ok(None)
    }

    fn apply_undo_action(&mut self, action: Action) -> Result<Option<Cursor>> {
        match action {
            Action::InsertLine { lnum, col, .. } => {
                self.memline.delete_line(lnum).map_err(|e| NvimError::Buffer(e.to_string()))?;
                Ok(Some(Cursor { row: lnum.saturating_sub(1).max(1), col }))
            }
            Action::DeleteLine { lnum, col, text } => {
                self.memline.insert_line(lnum, &text).map_err(|e| NvimError::Buffer(e.to_string()))?;
                Ok(Some(Cursor { row: lnum, col }))
            }
            Action::ReplaceLine { lnum, col, old_text, .. } => {
                self.memline.replace_line(lnum, &old_text).map_err(|e| NvimError::Buffer(e.to_string()))?;
                Ok(Some(Cursor { row: lnum, col }))
            }
            Action::Group(actions) => {
                let mut last_pos = None;
                for action in actions.into_iter().rev() {
                    last_pos = self.apply_undo_action(action)?;
                }
                Ok(last_pos)
            }
        }
    }

    pub fn redo(&mut self) -> Result<Option<Cursor>> {
        if let Some(action) = self.undo_tree.pop_redo() {
            return self.apply_redo_action(action);
        }
        Ok(None)
    }

    fn apply_redo_action(&mut self, action: Action) -> Result<Option<Cursor>> {
        match action {
            Action::InsertLine { lnum, col, text } => {
                self.memline.insert_line(lnum, &text).map_err(|e| NvimError::Buffer(e.to_string()))?;
                Ok(Some(Cursor { row: lnum, col }))
            }
            Action::DeleteLine { lnum, col, .. } => {
                self.memline.delete_line(lnum).map_err(|e| NvimError::Buffer(e.to_string()))?;
                Ok(Some(Cursor { row: lnum.saturating_sub(1).max(1), col }))
            }
            Action::ReplaceLine { lnum, col, new_text, .. } => {
                self.memline.replace_line(lnum, &new_text).map_err(|e| NvimError::Buffer(e.to_string()))?;
                Ok(Some(Cursor { row: lnum, col }))
            }
            Action::Group(actions) => {
                let mut last_pos = None;
                for action in actions {
                    last_pos = self.apply_redo_action(action)?;
                }
                Ok(last_pos)
            }
        }
    }

    pub fn indent_line(&mut self, lnum: usize, forward: bool, tabstop: usize) -> Result<()> {
        if let Some(line) = self.get_line(lnum) {
            let mut new_line = line.to_string();
            if forward {
                let spaces = " ".repeat(tabstop);
                new_line.insert_str(0, &spaces);
            } else {
                let mut count = 0;
                while count < tabstop && new_line.starts_with(' ') {
                    new_line.remove(0);
                    count += 1;
                }
            }
            self.set_line(lnum, 0, &new_line)?;
        }
        Ok(())
    }

    pub fn insert_char(&mut self, lnum: usize, col: usize, c: char) -> Result<()> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }
        
        let line_count = self.line_count();
        if lnum <= line_count {
            if let Some(line) = self.get_line(lnum) {
                let mut chars: Vec<char> = line.chars().collect();
                if col <= chars.len() {
                    chars.insert(col, c);
                } else {
                    chars.push(c);
                }
                let new_line: String = chars.into_iter().collect();
                self.set_line(lnum, col, &new_line)?;
                self.extmark_manager.on_change(lnum, col, 0, 1);
            }
        } else {
            let mut s = String::new();
            s.push(c);
            self.append_line(&s)?;
            self.extmark_manager.on_change(lnum, 0, 0, 1);
        }
        Ok(())
    }

    /// Deletes a character at the given position. 
    /// If col is 0, joins the current line with the previous one.
    pub fn delete_char(&mut self, lnum: usize, col: usize) -> Result<Option<usize>> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }

        let line_count = self.line_count();
        if lnum == 0 || lnum > line_count {
            return Err(NvimError::Buffer("Invalid line number".to_string()));
        }

        if col == 0 {
            if lnum == 1 {
                return Ok(None); // Nothing to delete at the start of buffer
            }
            
            // Get data first to avoid double borrow
            let prev_line = self.get_line(lnum - 1).unwrap().to_string();
            let curr_line = self.get_line(lnum).unwrap().to_string();
            let new_col = prev_line.chars().count();
            
            let mut joined = prev_line;
            joined.push_str(&curr_line);
            
            self.set_line(lnum - 1, 0, &joined)?;
            self.delete_line(lnum)?;
            Ok(Some(new_col))
        } else {
            let line = self.get_line(lnum).unwrap().to_string();
            let mut chars: Vec<char> = line.chars().collect();
            if col <= chars.len() {
                chars.remove(col - 1);
                let new_line: String = chars.into_iter().collect();
                self.set_line(lnum, col - 1, &new_line)?;
                self.extmark_manager.on_change(lnum, col, 0, -1);
            }
            Ok(Some(col - 1))
        }
    }

    /// Splits the line at the given position.
    pub fn insert_newline(&mut self, lnum: usize, col: usize) -> Result<()> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }

        let line = self.get_line(lnum).unwrap_or("").to_string();
        let chars: Vec<char> = line.chars().collect();
        let (first_chars, second_chars) = if col < chars.len() {
            chars.split_at(col)
        } else {
            (chars.as_slice(), [].as_slice())
        };

        let first_str: String = first_chars.iter().collect();
        let second_str: String = second_chars.iter().collect();

        self.set_line(lnum, col, &first_str)?;
        self.memline.insert_line(lnum + 1, &second_str).map_err(|e| NvimError::Buffer(e.to_string()))?;
        self.modified = true;
        Ok(())
    }

    pub fn insert_line(&mut self, lnum: usize, text: &str) -> Result<()> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }
        self.memline.insert_line(lnum, text).map_err(|e| NvimError::Buffer(e.to_string()))?;
        
        // Extmark の位置更新
        self.extmark_manager.on_change(lnum, 0, 1, 0);

        // 履歴記録（簡易的にInsertLineとして）
        self.push_action(Action::InsertLine {
            lnum,
            col: 0,
            text: text.to_string(),
        });
        
        self.modified = true;
        Ok(())
    }

    pub fn delete_line(&mut self, lnum: usize) -> Result<()> {
        if self.readonly {
            return Err(NvimError::ReadOnly);
        }
        
        let text = self.get_line(lnum).unwrap_or("").to_string();
        self.memline.delete_line(lnum).map_err(|e| NvimError::Buffer(e.to_string()))?;
        
        // Extmark の位置更新
        self.extmark_manager.on_change(lnum, 0, -1, 0);

        // 履歴に記録
        self.push_action(Action::DeleteLine { lnum, col: 0, text });

        self.modified = true;
        Ok(())
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn set_name(&mut self, name: &str) {
        self.full_name = Some(name.to_string());
        self.short_name = Some(name.to_string());
    }

    pub fn name(&self) -> Option<&str> {
        self.full_name.as_deref()
    }

    /// Returns all lines in the buffer.
    pub fn get_all_lines(&self) -> Vec<String> {
        let mut all = Vec::new();
        for i in 1..=self.line_count() {
            if let Some(line) = self.get_line(i) {
                all.push(line.to_string());
            }
        }
        all
    }

    /// Clears the modified flag (e.g., after saving).
    pub fn clear_modified(&mut self) {
        self.modified = false;
    }

    pub fn set_virtual_text(&mut self, lnum: usize, text: String) {
        self.virtual_text.insert(lnum, text);
    }

    pub fn get_virtual_text(&self, lnum: usize) -> Option<&String> {
        self.virtual_text.get(&lnum)
    }

    pub fn set_sign(&mut self, lnum: usize, text: &str) {
        self.signs.insert(lnum, text.to_string());
    }

    pub fn get_sign(&self, lnum: usize) -> Option<&String> {
        self.signs.get(&lnum)
    }

    /// 単語の境界を見つける
    pub fn find_word_boundary(&self, lnum: usize, col: usize, forward: bool) -> Cursor {
        let line = match self.get_line(lnum) {
            Some(s) => s,
            None => return Cursor { row: lnum, col },
        };
        let chars: Vec<char> = line.chars().collect();
        if forward {
            let mut found_space = false;
            for i in (col + 1)..chars.len() {
                let c = chars[i];
                if c.is_whitespace() {
                    found_space = true;
                } else if found_space {
                    return Cursor { row: lnum, col: i };
                }
            }
            if lnum < self.line_count() {
                return Cursor { row: lnum + 1, col: 0 };
            }
            Cursor { row: lnum, col: chars.len().saturating_sub(1) }
        } else {
            // Backward
            let mut found_non_space = false;
            for i in (0..col).rev() {
                if !chars[i].is_whitespace() {
                    found_non_space = true;
                } else if found_non_space {
                    return Cursor { row: lnum, col: i + 1 };
                }
            }
            Cursor { row: lnum, col: 0 }
        }
    }

    pub fn find_sentence_boundary(&self, lnum: usize, col: usize, forward: bool) -> Cursor {
        if forward {
            for r in lnum..=self.line_count() {
                if let Some(line) = self.get_line(r) {
                    let chars: Vec<char> = line.chars().collect();
                    let start_c = if r == lnum { col + 1 } else { 0 };
                    for i in start_c..chars.len() {
                        let c = chars[i];
                        if c == '.' || c == '!' || c == '?' {
                            if i + 1 < chars.len() && chars[i+1].is_whitespace() {
                                return Cursor { row: r, col: i + 1 };
                            } else if i + 1 == chars.len() {
                                return Cursor { row: r, col: i };
                            }
                        }
                    }
                    if line.trim().is_empty() && r > lnum {
                        return Cursor { row: r, col: 0 };
                    }
                }
            }
            Cursor { row: self.line_count().max(1), col: self.get_line(self.line_count()).map(|l| l.chars().count().saturating_sub(1)).unwrap_or(0) }
        } else {
            for r in (1..=lnum).rev() {
                if let Some(line) = self.get_line(r) {
                    let chars: Vec<char> = line.chars().collect();
                    let start_c = if r == lnum { col.saturating_sub(1) } else { chars.len().saturating_sub(1) };
                    for i in (0..=start_c).rev() {
                        let c = chars[i];
                        if c == '.' || c == '!' || c == '?' {
                            if i + 1 < chars.len() && chars[i+1].is_whitespace() {
                                // Find start of the sentence
                                let mut start_of_sentence = i + 1;
                                while start_of_sentence < chars.len() && chars[start_of_sentence].is_whitespace() {
                                    start_of_sentence += 1;
                                }
                                return Cursor { row: r, col: start_of_sentence };
                            } else if i + 1 == chars.len() && r < lnum {
                                return Cursor { row: r + 1, col: 0 };
                            }
                        }
                    }
                    if line.trim().is_empty() && r < lnum {
                        return Cursor { row: r + 1, col: 0 };
                    }
                }
            }
            Cursor { row: 1, col: 0 }
        }
    }

    pub fn find_section_boundary(&self, lnum: usize, _col: usize, forward: bool, close_brace: bool) -> Cursor {
        let target = if close_brace { '}' } else { '{' };
        if forward {
            for r in (lnum + 1)..=self.line_count() {
                if let Some(line) = self.get_line(r) {
                    if line.starts_with(target) || line.starts_with('\x0c') /* form feed */ {
                        return Cursor { row: r, col: 0 };
                    }
                }
            }
            Cursor { row: self.line_count().max(1), col: 0 }
        } else {
            for r in (1..lnum).rev() {
                if let Some(line) = self.get_line(r) {
                    if line.starts_with(target) || line.starts_with('\x0c') {
                        return Cursor { row: r, col: 0 };
                    }
                }
            }
            Cursor { row: 1, col: 0 }
        }
    }

    pub fn find_paragraph_boundary(&self, lnum: usize, _col: usize, forward: bool) -> Cursor {
        if forward {
            let start_line_empty = self.get_line(lnum).map(|l| l.trim().is_empty()).unwrap_or(true);
            let mut found_non_empty = !start_line_empty;
            for r in (lnum + 1)..=self.line_count() {
                if let Some(line) = self.get_line(r) {
                    let is_empty = line.trim().is_empty();
                    if !is_empty {
                        found_non_empty = true;
                    } else if found_non_empty && is_empty {
                        return Cursor { row: r, col: 0 };
                    }
                }
            }
            Cursor { row: self.line_count().max(1), col: self.get_line(self.line_count()).map(|l| l.chars().count().saturating_sub(1)).unwrap_or(0) }
        } else {
            let start_line_empty = self.get_line(lnum).map(|l| l.trim().is_empty()).unwrap_or(true);
            let mut found_non_empty = !start_line_empty;
            if lnum > 1 {
                for r in (1..lnum).rev() {
                    if let Some(line) = self.get_line(r) {
                        let is_empty = line.trim().is_empty();
                        if !is_empty {
                            found_non_empty = true;
                        } else if found_non_empty && is_empty {
                            return Cursor { row: r, col: 0 };
                        }
                    }
                }
            }
            Cursor { row: 1, col: 0 }
        }
    }

    pub fn find_char_pos(&self, lnum: usize, col: usize, target: char, forward: bool) -> Option<Cursor> {
        if forward {
            for r in lnum..=self.line_count() {
                if let Some(line) = self.get_line(r) {
                    let chars: Vec<char> = line.chars().collect();
                    let start_c = if r == lnum { col + 1 } else { 0 };
                    for i in start_c..chars.len() {
                        if chars[i] == target {
                            return Some(Cursor { row: r, col: i });
                        }
                    }
                }
            }
        } else {
            for r in (1..=lnum).rev() {
                if let Some(line) = self.get_line(r) {
                    let chars: Vec<char> = line.chars().collect();
                    let start_c = if r == lnum { col.saturating_sub(1) } else { chars.len().saturating_sub(1) };
                    if r == lnum && col == 0 { continue; }
                    for i in (0..=start_c).rev() {
                        if chars[i] == target {
                            return Some(Cursor { row: r, col: i });
                        }
                    }
                }
            }
        }
        None
    }

    /// 単語の範囲を取得する
    pub fn get_word_range(&self, lnum: usize, col: usize, inner: bool) -> Option<(Cursor, Cursor)> {
        let line = self.get_line(lnum)?;
        let chars: Vec<char> = line.chars().collect();
        if col >= chars.len() { return None; }
        
        let c = chars[col];
        if c.is_whitespace() { return None; }

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        let target_is_word = is_word_char(c);

        let mut start = col;
        while start > 0 && is_word_char(chars[start-1]) == target_is_word && !chars[start-1].is_whitespace() {
            start -= 1;
        }
        let mut end = col;
        while end + 1 < chars.len() && is_word_char(chars[end+1]) == target_is_word && !chars[end+1].is_whitespace() {
            end += 1;
        }

        if !inner {
            while end + 1 < chars.len() && chars[end+1].is_whitespace() {
                end += 1;
            }
        }

        Some((Cursor { row: lnum, col: start }, Cursor { row: lnum, col: end }))
    }

    pub fn get_sentence_range(&self, lnum: usize, col: usize, inner: bool) -> Option<(Cursor, Cursor)> {
        let start = self.find_sentence_boundary(lnum, col, false);
        let mut end = self.find_sentence_boundary(lnum, col, true);
        
        if inner {
            // end is usually one char after . or !, so move back if it's whitespace
            if let Some(line) = self.get_line(end.row) {
                let chars: Vec<_> = line.chars().collect();
                while end.col > 0 && end.col < chars.len() && chars[end.col].is_whitespace() {
                    end.col -= 1;
                }
            }
        }
        Some((start, end))
    }

    pub fn get_paragraph_range(&self, lnum: usize, col: usize, inner: bool) -> Option<(Cursor, Cursor)> {
        let mut start = self.find_paragraph_boundary(lnum, col, false);
        let mut end = self.find_paragraph_boundary(lnum, col, true);
        
        if inner {
            if start.row < lnum && self.get_line(start.row).map(|l| l.trim().is_empty()).unwrap_or(false) {
                start.row += 1;
                start.col = 0;
            }
            if end.row > lnum && self.get_line(end.row).map(|l| l.trim().is_empty()).unwrap_or(false) {
                end.row -= 1;
                if let Some(line) = self.get_line(end.row) {
                    end.col = line.chars().count().saturating_sub(1);
                }
            }
        }
        Some((start, end))
    }

    /// BIG単語の範囲を取得する (W)
    pub fn get_big_word_range(&self, lnum: usize, col: usize, inner: bool) -> Option<(Cursor, Cursor)> {
        let line = self.get_line(lnum)?;
        let chars: Vec<char> = line.chars().collect();
        if col >= chars.len() { return None; }
        if chars[col].is_whitespace() { return None; }
        
        let mut start = col;
        while start > 0 && !chars[start-1].is_whitespace() {
            start -= 1;
        }
        let mut end = col;
        while end + 1 < chars.len() && !chars[end+1].is_whitespace() {
            end += 1;
        }

        if !inner {
            while end + 1 < chars.len() && chars[end+1].is_whitespace() {
                end += 1;
            }
        }

        Some((Cursor { row: lnum, col: start }, Cursor { row: lnum, col: end }))
    }

    /// 括弧のペアの範囲を取得する
    pub fn get_pair_range(&self, lnum: usize, col: usize, open: char, close: char, inner: bool) -> Option<(Cursor, Cursor)> {
        let line = self.get_line(lnum)?;
        let chars: Vec<char> = line.chars().collect();
        
        // 前方に開始括弧を探す
        let mut start_idx = None;
        let mut depth = 0;
        for i in (0..=col.min(chars.len().saturating_sub(1))).rev() {
            if chars[i] == close { depth += 1; }
            else if chars[i] == open {
                if depth == 0 {
                    start_idx = Some(i);
                    break;
                } else {
                    depth -= 1;
                }
            }
        }

        let start = start_idx?;
        // 後方に終了括弧を探す
        let mut end_idx = None;
        depth = 0;
        for i in start..chars.len() {
            if chars[i] == open { depth += 1; }
            else if chars[i] == close {
                if depth == 1 {
                    end_idx = Some(i);
                    break;
                } else {
                    depth -= 1;
                }
            }
        }

        let end = end_idx?;
        if inner {
            Some((Cursor { row: lnum, col: start + 1 }, Cursor { row: lnum, col: end - 1 }))
        } else {
            Some((Cursor { row: lnum, col: start }, Cursor { row: lnum, col: end }))
        }
    }

    /// 引用符の範囲を取得する
    pub fn get_quote_range(&self, lnum: usize, col: usize, quote: char, inner: bool) -> Option<(Cursor, Cursor)> {
        let line = self.get_line(lnum)?;
        let chars: Vec<char> = line.chars().collect();
        
        let mut first = None;
        let mut second = None;
        for (i, &c) in chars.iter().enumerate() {
            if c == quote {
                if first.is_none() {
                    first = Some(i);
                } else {
                    second = Some(i);
                    if i >= col { break; }
                    // col を超えていない場合は、次のペアを探すためにリセット
                    if i < col {
                        first = None;
                        second = None;
                    }
                }
            }
        }

        let start = first?;
        let end = second?;
        if inner {
            Some((Cursor { row: lnum, col: start + 1 }, Cursor { row: lnum, col: end - 1 }))
        } else {
            Some((Cursor { row: lnum, col: start }, Cursor { row: lnum, col: end }))
        }
    }

    /// 行の最初の非空白文字を見つける (^)
    pub fn find_line_first_char(&self, lnum: usize) -> usize {
        if let Some(line) = self.get_line(lnum) {
            line.chars().take_while(|c| c.is_whitespace()).count()
        } else {
            0
        }
    }

    /// 行の最後の非空白文字を見つける (g_)
    pub fn find_line_last_char(&self, lnum: usize) -> usize {
        if let Some(line) = self.get_line(lnum) {
            let chars: Vec<char> = line.chars().collect();
            let mut i = chars.len().saturating_sub(1);
            while i > 0 && chars[i].is_whitespace() {
                i -= 1;
            }
            i
        } else {
            0
        }
    }

    /// 単語の末尾を見つける (e / ge)
    pub fn find_word_end(&self, lnum: usize, col: usize, forward: bool) -> Cursor {
        let line = match self.get_line(lnum) {
            Some(s) => s,
            None => return Cursor { row: lnum, col },
        };
        let chars: Vec<char> = line.chars().collect();

        if forward {
            let mut i = col + 1;
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            if i >= chars.len() {
                if lnum < self.line_count() { return self.find_word_end(lnum + 1, 0, true); }
                return Cursor { row: lnum, col: chars.len().saturating_sub(1) };
            }
            while i + 1 < chars.len() && !chars[i+1].is_whitespace() { i += 1; }
            Cursor { row: lnum, col: i }
        } else {
            if col == 0 {
                if lnum > 1 {
                    let prev_chars_count = self.get_line(lnum - 1).map(|l| l.chars().count()).unwrap_or(1);
                    return self.find_word_end(lnum - 1, prev_chars_count, false);
                }
                return Cursor { row: 1, col: 0 };
            }
            let mut i = col - 1;
            while i > 0 && chars[i].is_whitespace() { i -= 1; }
            while i > 0 && !chars[i-1].is_whitespace() { i -= 1; }
            Cursor { row: lnum, col: i }
        }
    }

                    /// 対応する括弧の位置を見つける (%)
    pub fn find_matching_pair(&self, lnum: usize, col: usize) -> Option<Cursor> {
        let line = self.get_line(lnum)?;
        let chars: Vec<char> = line.chars().collect();
        if col >= chars.len() { return None; }
        
        let c = chars[col];
        let (pair, forward) = match c {
            '(' => (')', true),
            '[' => (']', true),
            '{' => ('}', true),
            ')' => ('(', false),
            ']' => ('[', false),
            '}' => ('{', false),
            _ => return None,
        };

        let mut depth = 0;
        if forward {
            for i in col..chars.len() {
                if chars[i] == c { depth += 1; }
                else if chars[i] == pair {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Cursor { row: lnum, col: i });
                    }
                }
            }
        } else {
            for i in (0..=col).rev() {
                if chars[i] == c { depth += 1; }
                else if chars[i] == pair {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Cursor { row: lnum, col: i });
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_creation() {
        let b1 = Buffer::new();
        let b2 = Buffer::new();
        assert_ne!(b1.id(), b2.id());
        assert_eq!(b1.line_count(), 0);
    }

    #[test]
    fn test_insert_char() {
        let mut buf = Buffer::new();
        buf.append_line("abc").unwrap();
        
        // Middle
        buf.insert_char(1, 1, 'X').unwrap();
        assert_eq!(buf.get_line(1), Some("aXbc"));
        
        // Start
        buf.insert_char(1, 0, 'S').unwrap();
        assert_eq!(buf.get_line(1), Some("SaXbc"));
        
        // End
        let len = buf.get_line(1).unwrap().chars().count();
        buf.insert_char(1, len, 'E').unwrap();
        assert_eq!(buf.get_line(1), Some("SaXbcE"));
        
        // Beyond (behaves like append to line)
        buf.insert_char(1, 100, '!').unwrap();
        assert_eq!(buf.get_line(1), Some("SaXbcE!"));
    }

    #[test]
    fn test_delete_char() {
        let mut buf = Buffer::new();
        buf.append_line("line1").unwrap();
        buf.append_line("line2").unwrap();

        // Normal delete
        buf.delete_char(2, 5).unwrap(); // 'line2' -> 'line'
        assert_eq!(buf.get_line(2), Some("line"));

        // Join lines (Backspace at start of line)
        buf.delete_char(2, 0).unwrap();
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.get_line(1), Some("line1line"));

        // Backspace at start of buffer (should do nothing)
        let res = buf.delete_char(1, 0).unwrap();
        assert!(res.is_none());
        assert_eq!(buf.line_count(), 1);
    }

    #[test]
    fn test_insert_newline() {
        let mut buf = Buffer::new();
        buf.append_line("HelloWorld").unwrap();

        // Split in middle
        buf.insert_newline(1, 5).unwrap();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.get_line(1), Some("Hello"));
        assert_eq!(buf.get_line(2), Some("World"));

        // Split at start
        buf.insert_newline(1, 0).unwrap();
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.get_line(1), Some(""));
        assert_eq!(buf.get_line(2), Some("Hello"));

        // Split at end
        let len = buf.get_line(3).unwrap().chars().count();
        buf.insert_newline(3, len).unwrap();
        assert_eq!(buf.line_count(), 4);
        assert_eq!(buf.get_line(3), Some("World"));
        assert_eq!(buf.get_line(4), Some(""));
    }

    #[test]
    fn test_japanese_support() {
        let mut buf = Buffer::new();
        buf.append_line("こんにちは世界").unwrap();
        assert_eq!(buf.get_line(1).unwrap().chars().count(), 7);

        // 「こんにちは」と「世界」の間に "Rust" を挿入
        buf.insert_char(1, 5, 'R').unwrap();
        buf.insert_char(1, 6, 'u').unwrap();
        buf.insert_char(1, 7, 's').unwrap();
        buf.insert_char(1, 8, 't').unwrap();
        assert_eq!(buf.get_line(1), Some("こんにちはRust世界"));

        // 'R' を削除
        buf.delete_char(1, 6).unwrap();
        assert_eq!(buf.get_line(1), Some("こんにちはust世界"));
    }

    #[test]
    fn test_word_range_basic() {
        let mut buf = Buffer::new();
        buf.append_line("hello world! rust_neovim").unwrap();
        // "hello"
        let range = buf.get_word_range(1, 2, true).unwrap();
        assert_eq!(range.0.col, 0);
        assert_eq!(range.1.col, 4);
    }

    #[test]
    fn test_word_range_a_word() {
        let mut buf = Buffer::new();
        buf.append_line("hello world! rust_neovim").unwrap();
        // "world" (a-word should include following space if any, but '!' is there)
        let range = buf.get_word_range(1, 7, false).unwrap();
        assert_eq!(range.0.col, 6);
        assert_eq!(range.1.col, 10); 
    }

    #[test]
    fn test_word_range_identifier() {
        let mut buf = Buffer::new();
        buf.append_line("hello world! rust_neovim").unwrap();
        // "rust_neovim" (identifier)
        let range = buf.get_word_range(1, 15, true).unwrap();
        assert_eq!(range.0.col, 13);
        assert_eq!(range.1.col, 23);
    }

    #[test]
    fn test_big_word_range() {
        let mut buf = Buffer::new();
        buf.append_line("hello world! rust_neovim").unwrap();
        // BigWord
        let range = buf.get_big_word_range(1, 11, true).unwrap(); // on '!'
        assert_eq!(range.0.col, 6); // start of "world!"
        assert_eq!(range.1.col, 11);
    }

    #[test]
    fn test_sentence_and_paragraph_boundaries() {
        let mut buf = Buffer::new();
        buf.append_line("Sentence one. Sentence two!").unwrap();
        buf.append_line("").unwrap();
        buf.append_line("Paragraph two starts here.").unwrap();

        // Sentence Forward
        let pos = buf.find_sentence_boundary(1, 0, true);
        assert_eq!(pos.row, 1);
        assert_eq!(pos.col, 13); // end of "Sentence one."

        // Paragraph Forward
        let pos = buf.find_paragraph_boundary(1, 0, true);
        assert_eq!(pos.row, 2); // To empty line

        // Paragraph Backward
        let pos = buf.find_paragraph_boundary(3, 0, false);
        assert_eq!(pos.row, 2);
    }

    #[test]
    fn test_section_boundaries() {
        let mut buf = Buffer::new();
        buf.append_line("fn main() {").unwrap();
        buf.append_line("  println!(\"hi\");").unwrap();
        buf.append_line("}").unwrap();
        buf.append_line("{").unwrap();
        buf.append_line("  next section").unwrap();

        // Section Forward (to '{' at col 0)
        let pos = buf.find_section_boundary(1, 0, true, false);
        assert_eq!(pos.row, 4);

        // Section Backward
        let pos = buf.find_section_boundary(5, 0, false, false);
        assert_eq!(pos.row, 4);
    }

    #[test]
    fn test_buffer_signs() {
        let mut buf = Buffer::new();
        buf.append_line("line").unwrap();
        buf.set_sign(1, ">>");
        assert_eq!(buf.get_sign(1).unwrap(), ">>");
    }

    #[test]
    fn test_buffer_virtual_text() {
        let mut buf = Buffer::new();
        buf.append_line("line").unwrap();
        buf.set_virtual_text(1, "VT".to_string());
        assert_eq!(buf.get_virtual_text(1).unwrap(), "VT");
    }

    #[test]
    fn test_pair_and_quote_ranges() {
        let mut buf = Buffer::new();
        buf.append_line("print('hello', \"world\")").unwrap();
        
        // Single quote
        let range = buf.get_quote_range(1, 8, '\'', true).unwrap();
        assert_eq!(range.0.col, 7); // '
        assert_eq!(range.1.col, 11); // o
        
        // Double quote
        let range = buf.get_quote_range(1, 17, '"', false).unwrap();
        assert_eq!(range.0.col, 15); // "
        assert_eq!(range.1.col, 21); // "
        
        // Paren
        let range = buf.get_pair_range(1, 10, '(', ')', true).unwrap();
        assert_eq!(range.0.col, 6); // (
        assert_eq!(range.1.col, 21); // )
    }
}
