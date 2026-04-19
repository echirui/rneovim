use rneovim::nvim::state::VimState;
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;
use std::fs;
use std::path::Path;

#[test]
fn test_persistent_undo() {
    let undo_path = "test_file.undo";
    if Path::new(undo_path).exists() {
        fs::remove_file(undo_path).unwrap();
    }

    // 1. 変更を加えて保存
    {
        let mut state = VimState::new();
        let buf = state.current_window().buffer();
        buf.borrow_mut().set_name("test_file.txt");
        
        handle_request(&mut state, Request::InsertChar('A')).unwrap();
        handle_request(&mut state, Request::InsertChar('B')).unwrap();
        
        // 履歴を明示的に保存 (Request::SaveUndo)
        handle_request(&mut state, Request::SaveUndo(undo_path.to_string())).unwrap();
    }

    // 2. 復元してアンドゥ
    {
        let mut state = VimState::new();
        let buf = state.current_window().buffer();
        buf.borrow_mut().set_name("test_file.txt");
        // 初期状態として文字を入れておく（アンドゥの対象）
        {
            let mut b = buf.borrow_mut();
            b.insert_char(1, 0, 'A').unwrap();
            b.insert_char(1, 1, 'B').unwrap();
        }

        // 履歴を読み込み (Request::LoadUndo)
        handle_request(&mut state, Request::LoadUndo(undo_path.to_string())).unwrap();
        
        // アンドゥ実行
        handle_request(&mut state, Request::Undo).unwrap();
        
        // 'B' が消えて 'A' だけになっているはず
        assert_eq!(buf.borrow().get_line(1).unwrap(), "A");
    }

    if Path::new(undo_path).exists() {
        fs::remove_file(undo_path).unwrap();
    }
}
