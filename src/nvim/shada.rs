use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::nvim::window::Cursor;
use crate::nvim::state::VimState;
use crate::nvim::error::Result;
use std::fs;

#[derive(Serialize, Deserialize, Debug)]
pub struct ShaDaData {
    /// マーク (レジスタ名 -> 位置)
    pub marks: HashMap<char, Cursor>,
    /// レジスタ (名前 -> コンテンツ)
    pub registers: HashMap<char, String>,
    /// 最後に実行した検索クエリ
    pub last_search: Option<String>,
}

impl ShaDaData {
    pub fn from_state(state: &VimState) -> Self {
        let mut marks = HashMap::new();
        // 現在のアクティブなバッファのマークを保存（簡易版）
        if let Some(buf_rc) = state.buffers.first() {
            marks = buf_rc.borrow().marks.clone();
        }

        Self {
            marks,
            registers: state.registers.clone(),
            last_search: state.last_search.clone(),
        }
    }

    pub fn apply_to_state(&self, state: &mut VimState) {
        state.registers = self.registers.clone();
        state.last_search = self.last_search.clone();
        
        if let Some(buf_rc) = state.buffers.first() {
            let mut b = buf_rc.borrow_mut();
            for (name, cursor) in &self.marks {
                b.set_mark(*name, *cursor);
            }
        }
    }

    pub fn save(&self, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| crate::nvim::error::NvimError::Api(e.to_string()))?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let data = serde_json::from_str(&json).map_err(|e| crate::nvim::error::NvimError::Api(e.to_string()))?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shada_serialization() {
        let mut marks = HashMap::new();
        marks.insert('a', Cursor { row: 10, col: 5 });
        let mut registers = HashMap::new();
        registers.insert('"', "hello".to_string());
        
        let data = ShaDaData {
            marks,
            registers,
            last_search: Some("rust".to_string()),
        };
        
        let path = "test_shada.json";
        data.save(path).unwrap();
        
        let loaded = ShaDaData::load(path).unwrap();
        assert_eq!(loaded.last_search, Some("rust".to_string()));
        assert_eq!(loaded.registers.get(&'"').unwrap(), "hello");
        assert_eq!(loaded.marks.get(&'a').unwrap().row, 10);
        
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_shada_from_state() {
        let mut state = VimState::new();
        state.registers.insert('a', "reg a".to_string());
        state.last_search = Some("query".to_string());
        
        let data = ShaDaData::from_state(&state);
        assert_eq!(data.registers.get(&'a').unwrap(), "reg a");
        assert_eq!(data.last_search, Some("query".to_string()));
    }

    #[test]
    fn test_shada_apply_to_state() {
        let mut state = VimState::new();
        let mut marks = HashMap::new();
        marks.insert('x', Cursor { row: 3, col: 4 });
        let mut registers = HashMap::new();
        registers.insert('r', "register content".to_string());
        
        let data = ShaDaData {
            marks,
            registers,
            last_search: Some("search_query".to_string()),
        };
        
        data.apply_to_state(&mut state);
        
        assert_eq!(state.registers.get(&'r').unwrap(), "register content");
        assert_eq!(state.last_search.as_ref().unwrap(), "search_query");
        // marks are in buffer
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_mark('x').unwrap().row, 3);
    }
}
