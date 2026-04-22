use rneovim::nvim::state::VimState;
use rneovim::nvim::api::{execute_cmd, handle_request};
use rneovim::nvim::request::Request;

#[test]
fn test_ex_buffer_management() {
    let mut state = VimState::new();
    let id1 = state.buffers[0].borrow().id();
    
    // :enew
    execute_cmd(&mut state, "enew").unwrap();
    assert_eq!(state.buffers.len(), 2);
    let id2 = state.buffers[1].borrow().id();
    
    // :ls
    execute_cmd(&mut state, "ls").unwrap();
    assert!(state.cmdline().contains(&id1.to_string()));
    assert!(state.cmdline().contains(&id2.to_string()));

    // :bn
    execute_cmd(&mut state, "bn").unwrap();
    // :bp
    execute_cmd(&mut state, "bp").unwrap();
    
    // :bd
    execute_cmd(&mut state, "bd").unwrap();
    assert_eq!(state.buffers.len(), 1);
}

#[test]
fn test_ex_display_commands() {
    let mut state = VimState::new();
    
    // :echo
    execute_cmd(&mut state, "echo Hello").unwrap();
    assert_eq!(state.cmdline(), "Hello");

    // :echomsg
    execute_cmd(&mut state, "echomsg Message").unwrap();
    assert_eq!(state.messages.last().unwrap(), "Message");
    
    // :messages
    execute_cmd(&mut state, "messages").unwrap();
    assert!(state.cmdline().contains("Message"));
    
    // :pwd
    execute_cmd(&mut state, "pwd").unwrap();
    assert!(!state.cmdline().is_empty());
}

#[test]
fn test_ex_marks_and_jumps() {
    let mut state = VimState::new();
    {
        let buf = state.current_window().buffer();
        buf.borrow_mut().set_mark('a', rneovim::nvim::window::Cursor { row: 1, col: 1 });
    }
    
    // :marks
    execute_cmd(&mut state, "marks").unwrap();
    assert!(state.cmdline().contains("a"));

    // :jumps
    execute_cmd(&mut state, "jumps").unwrap();
    assert!(state.cmdline().contains("jump"));
}

#[test]
fn test_ex_tab_management() {
    let mut state = VimState::new();
    
    // :tabnew
    execute_cmd(&mut state, "tabnew").unwrap();
    assert_eq!(state.tabpages.len(), 2);
    assert_eq!(state.current_tab_idx, 1);
    
    // :tabp
    execute_cmd(&mut state, "tabp").unwrap();
    assert_eq!(state.current_tab_idx, 0);

    // :tabn
    execute_cmd(&mut state, "tabn").unwrap();
    assert_eq!(state.current_tab_idx, 1);

    // :tabclose
    execute_cmd(&mut state, "tabclose").unwrap();
    assert_eq!(state.tabpages.len(), 1);
}

#[test]
fn test_ex_editing_commands() {
    let mut state = VimState::new();
    {
        let buf = state.current_window().buffer();
        let mut b = buf.borrow_mut();
        b.append_line("first").unwrap();
        b.append_line("second").unwrap();
    }

    // :undo
    state.current_window_mut().set_cursor(1, 0);
    handle_request(&mut state, Request::DeleteCharAtCursor { count: 1 }).unwrap();
    assert_eq!(state.current_window().buffer().borrow().get_line(1).unwrap(), "irst");
    execute_cmd(&mut state, "undo").unwrap();
    assert_eq!(state.current_window().buffer().borrow().get_line(1).unwrap(), "first");

    // :redo
    execute_cmd(&mut state, "redo").unwrap();
    assert_eq!(state.current_window().buffer().borrow().get_line(1).unwrap(), "irst");

    // :join
    state.current_window_mut().set_cursor(1, 0);
    execute_cmd(&mut state, "join").unwrap();
    assert_eq!(state.current_window().buffer().borrow().get_line(1).unwrap(), "irst second");

    // :ascii
    state.current_window_mut().set_cursor(1, 0);
    execute_cmd(&mut state, "ascii").unwrap();
    assert!(state.cmdline().contains("105")); // 'i' is 105
}

#[test]
fn test_ex_saveas() {
    let mut state = VimState::new();
    let temp_file = "test_saveas.txt";
    execute_cmd(&mut state, &format!("saveas {}", temp_file)).unwrap();
    assert_eq!(state.current_window().buffer().borrow().name().unwrap(), temp_file);
    assert!(std::path::Path::new(temp_file).exists());
    let _ = std::fs::remove_file(temp_file);
}
