use rneovim::nvim::state::{VimState, Mode};
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;

#[test]
fn test_buffer_line_manipulation_integration() {
    let mut state = VimState::new();
    
    // Initial state
    {
        let buf = state.current_window().buffer();
        let mut b = buf.borrow_mut();
        b.append_line("line1").unwrap();
        b.append_line("line2").unwrap();
        b.append_line("line3").unwrap();
        b.append_line("line4").unwrap();
    }
    
    // Set cursor to {3, 2} (1-based row, 0-based col)
    state.current_window_mut().set_cursor(3, 2);
    
    // add 2 lines and delete 1 line above the current cursor position.
    // In rneovim, we don't have nvim_buf_set_lines yet, so we use primitive requests.
    
    // Delete line 2
    state.current_window_mut().set_cursor(2, 0);
    handle_request(&mut state, Request::DeleteCurrentLine).unwrap();
    
    // Insert line 5, 6
    state.current_window_mut().set_cursor(1, 0);
    handle_request(&mut state, Request::OpenLine { below: true }).unwrap();
    for c in "line5".chars() { handle_request(&mut state, Request::InsertChar(c)).unwrap(); }
    handle_request(&mut state, Request::SetMode(Mode::Normal)).unwrap();
    
    state.current_window_mut().set_cursor(2, 0);
    handle_request(&mut state, Request::OpenLine { below: true }).unwrap();
    for c in "line6".chars() { handle_request(&mut state, Request::InsertChar(c)).unwrap(); }
    handle_request(&mut state, Request::SetMode(Mode::Normal)).unwrap();

    let buf = state.current_window().buffer();
    let b = buf.borrow();
    assert_eq!(b.get_line(1), Some("line1"));
    assert_eq!(b.get_line(2), Some("line5"));
    assert_eq!(b.get_line(3), Some("line6"));
    assert_eq!(b.get_line(4), Some("line3"));
    assert_eq!(b.get_line(5), Some("line4"));
}

#[test]
fn test_extmark_tracking_complex() {
    let mut state = VimState::new();
    let buf = state.current_window().buffer();
    
    {
        let mut b = buf.borrow_mut();
        b.append_line("first line").unwrap();
        b.append_line("second line").unwrap();
        b.append_line("third line").unwrap();
    }

    // Mark on "second" at col 0
    let id1 = buf.borrow_mut().set_extmark(2, 0);
    // Mark on "third" at col 6
    let id2 = buf.borrow_mut().set_extmark(3, 6);

    // Insert a line at the top
    state.current_window_mut().set_cursor(1, 0);
    handle_request(&mut state, Request::OpenLine { below: false }).unwrap();
    handle_request(&mut state, Request::SetMode(Mode::Normal)).unwrap();

    {
        let b = buf.borrow();
        let m1 = b.get_extmark(id1).unwrap();
        let m2 = b.get_extmark(id2).unwrap();
        // Rows should be shifted
        assert_eq!(m1.row, 3);
        assert_eq!(m2.row, 4);
    }

    // Insert text in front of mark 2
    state.current_window_mut().set_cursor(4, 0);
    for c in "prefix ".chars() {
        handle_request(&mut state, Request::InsertChar(c)).unwrap();
    }
    
    {
        let b = buf.borrow();
        let m2 = b.get_extmark(id2).unwrap();
        assert_eq!(m2.row, 4);
        assert_eq!(m2.col, 6 + "prefix ".chars().count());
    }
}
