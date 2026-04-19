use rneovim::nvim::state::{VimState, Mode};
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::{Request, Operator};

#[test]
fn test_japanese_editing_integration() {
    let mut state = VimState::new();
    
    // iこんにちは
    handle_request(&mut state, Request::SetMode(Mode::Insert)).unwrap();
    for c in "こんにちは".chars() {
        handle_request(&mut state, Request::InsertChar(c)).unwrap();
    }
    
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("こんにちは"));
    }

    // ESC
    handle_request(&mut state, Request::SetMode(Mode::Normal)).unwrap();
    
    // カーソルを「に」 (index 2) に移動
    state.current_window_mut().set_cursor(1, 2);
    
    // rX (置換)
    handle_request(&mut state, Request::ReplaceChar('X')).unwrap();
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("こんXちは"));
    }
}

#[test]
fn test_visual_multilang_integration() {
    let mut state = VimState::new();
    {
        let buf = state.current_window().buffer();
        let mut b = buf.borrow_mut();
        b.append_line("日本語とEnglish").unwrap();
    }

    // v (Visual mode) on 「語」 (index 2)
    state.current_window_mut().set_cursor(1, 2);
    handle_request(&mut state, Request::SetVisualMode(rneovim::nvim::state::VisualMode::Char)).unwrap();
    
    // 移動して範囲拡大 (English の 'E' まで, index 4)
    state.current_window_mut().set_cursor(1, 4);
    
    // d (削除)
    handle_request(&mut state, Request::OpVisualSelection(Operator::Delete)).unwrap();
    
    {
        let buf = state.current_window().buffer();
        // 「日本語とEnglish」 -> 「日本nglish」
        assert_eq!(buf.borrow().get_line(1), Some("日本nglish"));
    }
}
