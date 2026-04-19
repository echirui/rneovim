use rneovim::nvim::state::VimState;
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;
use std::fs;
use std::path::Path;

#[test]
fn test_session_management() {
    let session_path = "test.session";
    if Path::new(session_path).exists() {
        fs::remove_file(session_path).unwrap();
    }

    // 1. 状態を作って保存 (ウィンドウを分割)
    {
        let mut state = VimState::new();
        handle_request(&mut state, Request::SplitWindow { vertical: false }).unwrap();
        assert_eq!(state.current_tabpage().windows.len(), 2);
        
        // セッション保存
        handle_request(&mut state, Request::SaveSession(session_path.to_string())).unwrap();
    }

    // 2. 復元して確認
    {
        let mut state = VimState::new();
        // セッション読み込み
        handle_request(&mut state, Request::LoadSession(session_path.to_string())).unwrap();
        
        // ウィンドウ分割が復元されているか
        assert_eq!(state.current_tabpage().windows.len(), 2);
    }

    if Path::new(session_path).exists() {
        fs::remove_file(session_path).unwrap();
    }
}
