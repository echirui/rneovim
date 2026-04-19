use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Extmark {
    pub id: i32,
    pub row: usize, // 1-based
    pub col: usize, // 0-based
}

pub struct ExtmarkManager {
    marks: HashMap<i32, Extmark>,
    next_id: i32,
}

impl ExtmarkManager {
    pub fn new() -> Self {
        Self {
            marks: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn set_extmark(&mut self, row: usize, col: usize) -> i32 {
        let id = self.next_id;
        self.marks.insert(id, Extmark { id, row, col });
        self.next_id += 1;
        id
    }

    pub fn get_extmark(&self, id: i32) -> Option<Extmark> {
        self.marks.get(&id).copied()
    }

    /// テキストの変更に合わせて位置を更新する
    pub fn on_change(&mut self, row: usize, col: usize, added_rows: i32, added_cols: i32) {
        for mark in self.marks.values_mut() {
            if added_rows != 0 {
                if mark.row > row {
                    mark.row = (mark.row as i32 + added_rows) as usize;
                } else if mark.row == row && added_cols == 0 {
                    // 行全体の挿入/削除の場合、その行にあるマークも移動させる（暫定）
                    mark.row = (mark.row as i32 + added_rows) as usize;
                }
            }
            
            if mark.row == row && added_cols != 0 {
                // 同じ行内の文字単位の変更
                if mark.col >= col {
                    mark.col = (mark.col as i32 + added_cols) as usize;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nvim::buffer::Buffer;

    #[test]
    fn test_extmark_tracking() {
        let mut buf = Buffer::new();
        buf.append_line("hello world").unwrap();
        
        // (行1, 列6) に Extmark を設置 ("world" の 'w')
        let id = buf.set_extmark(1, 6);
        
        // "hello" の前に "say " を挿入 (4文字追加)
        buf.insert_char(1, 0, 's').unwrap();
        buf.insert_char(1, 1, 'a').unwrap();
        buf.insert_char(1, 2, 'y').unwrap();
        buf.insert_char(1, 3, ' ').unwrap();
        
        // Extmark の位置が (1, 10) に更新されているはず
        let pos = buf.get_extmark(id).unwrap();
        assert_eq!(pos.row, 1);
        assert_eq!(pos.col, 10);
    }
}
