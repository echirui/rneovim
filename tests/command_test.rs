use rneovim::nvim::state::{VimState, Mode};
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;

#[test]
fn test_motion_integration() {
    let mut state = VimState::new();
    {
        let buf = state.buffers[0].clone();
        let mut b = buf.borrow_mut();
        let _ = b.append_line("Hello World");
        let _ = b.append_line("Rust Programming");
    }

    // w (Next word)
    handle_request(&mut state, Request::MoveWord { forward: true, end: false }).unwrap();
    assert_eq!(state.current_window().cursor().col, 6); // 'World' の 'W'

    // $ (End of line)
    handle_request(&mut state, Request::JumpToLineBoundary { end: true }).unwrap();
    assert_eq!(state.current_window().cursor().col, 10); // 'World' の 'd'

    // 0 (Start of line)
    handle_request(&mut state, Request::JumpToLineBoundary { end: false }).unwrap();
    assert_eq!(state.current_window().cursor().col, 0);

    // j (Next line)
    handle_request(&mut state, Request::CursorMove { row: 2, col: 0 }).unwrap();
    assert_eq!(state.current_window().cursor().row, 2);
}

#[test]
fn test_editing_integration() {
    let mut state = VimState::new();
    
    // i (Insert mode)
    handle_request(&mut state, Request::SetMode(Mode::Insert)).unwrap();
    
    // Type "Rust"
    for c in "Rust".chars() {
        handle_request(&mut state, Request::InsertChar(c)).unwrap();
    }
    
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("Rust"));
    }

    // ESC (Normal mode)
    handle_request(&mut state, Request::SetMode(Mode::Normal)).unwrap();

    // u (Undo once) -> "Rus"
    handle_request(&mut state, Request::Undo).unwrap();
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("Rus"));
    }

    // u (Undo until empty)
    handle_request(&mut state, Request::Undo).unwrap();
    handle_request(&mut state, Request::Undo).unwrap();
    handle_request(&mut state, Request::Undo).unwrap();
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().line_count(), 0);
    }
}

#[test]
fn test_yank_paste_integration() {
    let mut state = VimState::new();
    {
        let buf = state.buffers[0].clone();
        let mut b = buf.borrow_mut();
        let _ = b.append_line("Copy Me");
    }

    // y (Yank line)
    handle_request(&mut state, Request::YankLine).unwrap();
    assert_eq!(state.registers.get(&'"').unwrap(), "Copy Me");

    // p (Paste)
    handle_request(&mut state, Request::Paste).unwrap();
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().line_count(), 2);
        assert_eq!(buf.borrow().get_line(2), Some("Copy Me"));
    }
}

#[test]
fn test_operator_textobject_integration() {
    let mut state = VimState::new();
    {
        let buf = state.current_window().buffer();
        let mut b = buf.borrow_mut();
        b.append_line("one (two three) four").unwrap();
    }

    // Move to "two"
    state.current_window_mut().set_cursor(1, 5);
    
    // ci( (change inner paren)
    handle_request(&mut state, Request::OpTextObject { 
        op: rneovim::nvim::request::Operator::Change, 
        inner: true, 
        obj: rneovim::nvim::request::TextObject::Paren 
    }).unwrap();
    
    assert_eq!(state.mode(), Mode::Insert);
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("one () four"));
    }
    
    // Type "X"
    handle_request(&mut state, Request::InsertChar('X')).unwrap();
    assert_eq!(state.current_window().buffer().borrow().get_line(1).unwrap(), "one (X) four");
}

#[test]
fn test_operator_motion_integration() {
    let mut state = VimState::new();
    {
        let buf = state.current_window().buffer();
        let mut b = buf.borrow_mut();
        b.append_line("line 1").unwrap();
        b.append_line("line 2").unwrap();
        b.append_line("line 3").unwrap();
    }

    // dG (delete to end of buffer)
    state.current_window_mut().set_cursor(2, 0);
    handle_request(&mut state, Request::OpMotion { 
        op: rneovim::nvim::request::Operator::Delete, 
        motion: rneovim::nvim::request::Motion::BufferEnd 
    }).unwrap();
    
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().line_count(), 1);
        assert_eq!(buf.borrow().get_line(1), Some("line 1"));
    }
}
