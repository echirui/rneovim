use serde::{Serialize, Deserialize};
use crate::nvim::state::VimState;
use crate::nvim::api::handle_request;
use crate::nvim::request::Request;
use crate::nvim::error::Result;

#[derive(Serialize, Deserialize, Debug)]
pub struct SessionData {
    pub window_count: usize,
    // 今後はバッファ名やレイアウト情報も追加
}

impl SessionData {
    pub fn from_state(state: &VimState) -> Self {
        Self {
            window_count: state.current_tabpage().windows.len(),
        }
    }

    pub fn apply_to_state(&self, state: &mut VimState) -> Result<()> {
        // 簡易的にウィンドウを分割して数を合わせる
        for _ in 1..self.window_count {
            handle_request(state, Request::SplitWindow { vertical: false })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_data_basic() {
        let state = VimState::new();
        // 1 window initially
        let data = SessionData::from_state(&state);
        assert_eq!(data.window_count, 1);
        
        // Mock apply
        let mut state2 = VimState::new();
        let data2 = SessionData { window_count: 3 };
        data2.apply_to_state(&mut state2).unwrap();
        assert_eq!(state2.current_tabpage().windows.len(), 3);
    }
}
