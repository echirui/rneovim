use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Extmark {
    pub id: i32,
    pub ns_id: i32,
    pub row: usize, // 1-based
    pub col: usize, // 0-based
    pub end_row: Option<usize>,
    pub end_col: Option<usize>,
    pub hl_group: Option<String>,
    pub virt_text: Vec<(String, String)>, // (text, hl_group)
    pub virt_text_pos: String, // "eol", "overlay", "right_align", "inline"
    pub priority: i32,
}

pub struct ExtmarkManager {
    /// ns_id -> (mark_id -> Extmark)
    namespaces: HashMap<i32, HashMap<i32, Extmark>>,
    next_mark_id: i32,
}

impl ExtmarkManager {
    pub fn new() -> Self {
        Self {
            namespaces: HashMap::new(),
            next_mark_id: 1,
        }
    }

    pub fn ns_count(&self) -> usize {
        self.namespaces.len()
    }

    pub fn total_mark_count(&self) -> usize {
        self.namespaces.values().map(|m| m.len()).sum()
    }

    pub fn set_extmark(
        &mut self,
        ns_id: i32,
        mark_id: Option<i32>,
        row: usize,
        col: usize,
        end_row: Option<usize>,
        end_col: Option<usize>,
        hl_group: Option<String>,
        virt_text: Vec<(String, String)>,
        virt_text_pos: String,
        priority: i32,
    ) -> i32 {
        let id = mark_id.unwrap_or_else(|| {
            let nid = self.next_mark_id;
            self.next_mark_id += 1;
            nid
        });

        let mark = Extmark {
            id,
            ns_id,
            row,
            col,
            end_row,
            end_col,
            hl_group,
            virt_text,
            virt_text_pos,
            priority,
        };

        self.namespaces
            .entry(ns_id)
            .or_insert_with(HashMap::new)
            .insert(id, mark);
        id
    }

    pub fn get_extmark(&self, ns_id: i32, id: i32) -> Option<&Extmark> {
        self.namespaces.get(&ns_id).and_then(|ns| ns.get(&id))
    }

    pub fn get_extmarks_in_range(
        &self,
        ns_id: i32, // -1 for all
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) -> Vec<Extmark> {
        let mut result = Vec::new();

        let process_ns = |ns: &HashMap<i32, Extmark>, res: &mut Vec<Extmark>| {
            for m in ns.values() {
                if m.row < start_row || m.row > end_row { continue; }
                if m.row == start_row && m.col < start_col { continue; }
                if m.row == end_row && m.col > end_col { continue; }
                res.push(m.clone());
            }
        };

        if ns_id == -1 {
            for ns in self.namespaces.values() {
                process_ns(ns, &mut result);
            }
        } else if let Some(ns) = self.namespaces.get(&ns_id) {
            process_ns(ns, &mut result);
        }

        result.sort_by(|a, b| {
            if a.row != b.row { a.row.cmp(&b.row) }
            else { a.col.cmp(&b.col) }
        });
        result
    }

    pub fn clear_namespace(&mut self, ns_id: i32) {
        if ns_id == -1 {
            self.namespaces.clear();
        } else {
            self.namespaces.remove(&ns_id);
        }
    }

    /// テキストの変更に合わせて位置を更新する
    pub fn on_change(&mut self, row: usize, col: usize, added_rows: i32, added_cols: i32) {
        for ns in self.namespaces.values_mut() {
            for mark in ns.values_mut() {
                // 開始位置の更新
                if added_rows != 0 {
                    if mark.row > row {
                        mark.row = (mark.row as i32 + added_rows) as usize;
                    } else if mark.row == row && added_cols == 0 {
                        mark.row = (mark.row as i32 + added_rows) as usize;
                    }
                }
                
                if mark.row == row && added_cols != 0 {
                    if mark.col >= col {
                        mark.col = (mark.col as i32 + added_cols) as usize;
                    }
                }

                // 終了位置の更新 (もしあれば)
                if let Some(ref mut er) = mark.end_row {
                    if added_rows != 0 {
                        if *er > row {
                            *er = (*er as i32 + added_rows) as usize;
                        } else if *er == row && added_cols == 0 {
                            *er = (*er as i32 + added_rows) as usize;
                        }
                    }
                    if let Some(ref mut ec) = mark.end_col {
                        if *er == row && added_cols != 0 {
                            if *ec >= col {
                                *ec = (*ec as i32 + added_cols) as usize;
                            }
                        }
                    }
                }
            }
        }
    }
}
