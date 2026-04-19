use rneovim::nvim::state::{VimState, Mode};
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;

#[test]
fn test_macro_and_repeat_integration() {
    let mut state = VimState::new();
    {
        let buf = state.buffers[0].clone();
        let mut b = buf.borrow_mut();
        b.append_line("line 1").unwrap();
        b.append_line("line 2").unwrap();
    }

    // Record macro @a: 'x' (delete char)
    handle_request(&mut state, Request::StartMacroRecord('a')).unwrap();
    handle_request(&mut state, Request::DeleteCharAtCursor).unwrap();
    handle_request(&mut state, Request::StopMacroRecord).unwrap();

    // Move to line 2 and play @a
    state.current_window_mut().set_cursor(2, 0);
    handle_request(&mut state, Request::PlayMacro('a')).unwrap();
    
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(1), Some("ine 1"));
        assert_eq!(buf.borrow().get_line(2), Some("ine 2"));
    }

    // Repeat last change (should be 'x' delete)
    state.current_window_mut().set_cursor(2, 0);
    handle_request(&mut state, Request::RepeatLastChange).unwrap();
    {
        let buf = state.current_window().buffer();
        assert_eq!(buf.borrow().get_line(2), Some("ne 2"));
    }
}

#[test]
fn test_cmdline_history_integration() {
    let mut state = VimState::new();
    
    // Execute two commands
    handle_request(&mut state, Request::ExecuteCommandFromConfig("set number".to_string())).unwrap();
    handle_request(&mut state, Request::ExecuteCommandFromConfig("set nonumber".to_string())).unwrap();
    
    // Enter cmdline mode
    handle_request(&mut state, Request::SetMode(Mode::CommandLine)).unwrap();
    
    // Previous history (set nonumber)
    handle_request(&mut state, Request::CmdLineHistory { forward: false }).unwrap();
    assert_eq!(state.cmdline(), "set nonumber");
    
    // Previous history (set number)
    handle_request(&mut state, Request::CmdLineHistory { forward: false }).unwrap();
    assert_eq!(state.cmdline(), "set number");
    
    // Forward history (set nonumber)
    handle_request(&mut state, Request::CmdLineHistory { forward: true }).unwrap();
    assert_eq!(state.cmdline(), "set nonumber");
}
