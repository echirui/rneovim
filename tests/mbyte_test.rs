use rneovim::nvim::state::{VimState, Mode};
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;

#[test]
fn test_mbyte_char_handling() {
    let mut state = VimState::new();
    
    // Insert 3-byte UTF-8 (Japanese)
    handle_request(&mut state, Request::SetMode(Mode::Insert)).unwrap();
    for c in "あいう".chars() {
        handle_request(&mut state, Request::InsertChar(c)).unwrap();
    }
    
    {
        let buf = state.current_window().buffer();
        let b = buf.borrow();
        assert_eq!(b.get_line(1), Some("あいう"));
        assert_eq!(b.get_line(1).unwrap().len(), 9); // 3 bytes each
        assert_eq!(b.get_line(1).unwrap().chars().count(), 3);
    }

    // Move left 1 char (to 'い')
    handle_request(&mut state, Request::SetMode(Mode::Normal)).unwrap();
    // 'あいう|' -> cursor at col 3 (after 'う')
    // Motion::Left should go to col 2 (at 'う')
    handle_request(&mut state, Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Left }).unwrap();
    assert_eq!(state.current_window().cursor().col, 2);
    
    // Replace 'う' with 'え'
    handle_request(&mut state, Request::ReplaceChar('え')).unwrap();
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("あいえ"));
    }
}

#[test]
fn test_mbyte_composition_visual() {
    let mut state = VimState::new();
    {
        let buf = state.current_window().buffer();
        let mut b = buf.borrow_mut();
        // Line with mixed widths
        b.append_line("AあBいC").unwrap();
    }

    // Cursor on 'あ' (index 1)
    state.current_window_mut().set_cursor(1, 1);
    
    // 'ga' equivalent (ascii)
    rneovim::nvim::api::execute_cmd(&mut state, "ascii").unwrap();
    assert!(state.cmdline().contains("あ"));
    assert!(state.cmdline().contains("Hex 3042"));
}
