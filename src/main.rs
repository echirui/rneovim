use std::time::Duration;
use std::io::Write;
use rneovim::nvim::api::{handle_request, trigger_autocmd};
use rneovim::nvim::event::event_loop::EventLoop;
use rneovim::nvim::event::key_processor::KeyProcessor;
use rneovim::nvim::state::VimState;
use rneovim::nvim::request::{Request, MouseButton, MouseAction};

extern "C" {
    #[allow(dead_code)]
    fn os_setup_terminal();
    fn os_restore_terminal();
    fn os_read_char() -> i32;
    fn os_get_terminal_size(width: *mut i32, height: *mut i32);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let filename = if args.len() > 1 && !args[1].starts_with("--") { Some(args[1].clone()) } else { None };
    
    let mut test_keys = None;
    let mut test_output = None;
    let mut config_dir = None;

    for i in 1..args.len() {
        if args[i] == "--test-keys" && i + 1 < args.len() { test_keys = Some(args[i+1].clone()); }
        if args[i] == "--test-output" && i + 1 < args.len() { test_output = Some(args[i+1].clone()); }
        if args[i] == "--config-dir" && i + 1 < args.len() { config_dir = Some(std::path::PathBuf::from(&args[i+1])); }
    }

    let is_tty = terminal_size::terminal_size().is_some();
    if test_keys.is_none() && is_tty { unsafe { os_setup_terminal() }; }

    let mut state = VimState::new();
    
    if is_tty {
        let mut width = 80;
        let mut height = 24;
        unsafe { os_get_terminal_size(&mut width, &mut height); }
        let _ = handle_request(&mut state, Request::Resize { width: width as usize, height: height as usize });
    }

    let mut eloop = EventLoop::new();
    let processor = KeyProcessor::new();

    state.log("1. Starting rneovim");
    
    // 2. Initial Setup
    state.log("2. Setup START");
    state.process_callbacks();
    state.log("2. Setup DONE");

    // 3. Load Config
    state.log("3. Config and Plugin Loading START");
    {
        let env = state.lua_env.clone();
        env.load_plugins(&mut state, config_dir);
    }
    state.log("3. Config and Plugin Loading DONE");
    
    state.log("4. Triggering Autocmds START");
    trigger_autocmd(&mut state, rneovim::nvim::state::AutoCmdEvent::VimEnter, None);
    trigger_autocmd(&mut state, rneovim::nvim::state::AutoCmdEvent::UIEnter, None);

    let mut safety_count = 0;
    while !state.pending_callbacks.is_empty() && safety_count < 100 {
        state.process_callbacks();
        safety_count += 1;
    }
    if safety_count > 0 { state.log(&format!("MAIN: processed {} callback batches", safety_count)); }
    trigger_autocmd(&mut state, rneovim::nvim::state::AutoCmdEvent::SafeState, None);
    trigger_autocmd(&mut state, rneovim::nvim::state::AutoCmdEvent::CursorHold, None);
    state.log("4. Triggering Autocmds DONE");

    // 5. Open initial file
    if let Some(f) = filename {
        let _ = handle_request(&mut state, Request::OpenFile(f));
    }
    let _ = handle_request(&mut state, Request::Redraw);
    state.redraw();

    // 6. Execution Mode
    state.vim_did_enter = true;
    if let Some(keys) = test_keys {
        let mut processed_keys = String::new();
        let mut chars = keys.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.peek() {
                    Some(&'x') => {
                        chars.next();
                        let mut hex = String::new();
                        if let Some(h1) = chars.next() { hex.push(h1); }
                        if let Some(h2) = chars.next() { hex.push(h2); }
                        if let Ok(val) = u8::from_str_radix(&hex, 16) { processed_keys.push(val as char); }
                    }
                    Some(&'n') => { chars.next(); processed_keys.push('\n'); }
                    Some(&'r') => { chars.next(); processed_keys.push('\r'); }
                    _ => { processed_keys.push(c); }
                }
            } else { processed_keys.push(c); }
        }
        let _ = processor.process_keys(&mut state, &processed_keys);
        
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < end {
            state.step();
            let mut sc = 0;
            while !state.pending_callbacks.is_empty() && sc < 100 {
               state.process_callbacks();
               sc += 1;
            }
            state.redraw();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if let Some(out) = test_output {
            let buf = state.current_window().buffer();
            let b = buf.borrow();
            let mut result = String::new();
            for i in 1..=b.line_count() { if let Some(line) = b.get_line(i) { result.push_str(line); result.push('\n'); } }
            std::fs::write(&out, result).expect("Failed to write test output");
            
            // Also save the grid view for UI debugging
            let grid_view = state.grid.flush();
            let grid_file = format!("{}.grid", out);
            std::fs::write(grid_file, grid_view).ok();
        }
        return;
    }

    if test_keys.is_none() { 
        // Enter alternate screen, hide cursor, enable mouse tracking
        print!("\x1B[?1049h\x1B[H\x1B[?25l\x1B[?1000h\x1B[?1006h"); 
    }

    let mut last_size_check = std::time::Instant::now();

    while !state.should_quit() {
        eloop.poll_events(&mut state, Some(Duration::ZERO));
        state.step();

        if is_tty && last_size_check.elapsed() > Duration::from_millis(100) {
            let mut width = 0;
            let mut height = 0;
            unsafe { os_get_terminal_size(&mut width, &mut height); }
            if width as usize != state.grid.width || height as usize != state.grid.height {
                let _ = handle_request(&mut state, Request::Resize { width: width as usize, height: height as usize });
                trigger_autocmd(&mut state, rneovim::nvim::state::AutoCmdEvent::VimResized, None);
            }
            last_size_check = std::time::Instant::now();
        }
        
        let mut input_count = 0;
        loop {
            let c = unsafe { os_read_char() };
            if c == -1 { break; }
            input_count += 1;
            let key = c as u8 as char;
            let mut special_req = None;
            if key == '\x1B' {
                // Peek next char to see if it's an escape sequence
                let c2 = unsafe { os_read_char() };
                if c2 == '[' as i32 {
                    let mut buf = String::new();
                    loop {
                        let nc = unsafe { os_read_char() };
                        if nc == -1 {
                            // In raw mode, we might need to wait a tiny bit for the rest of the sequence
                            std::thread::sleep(Duration::from_millis(1));
                            let nc2 = unsafe { os_read_char() };
                            if nc2 == -1 { break; }
                            let nch = nc2 as u8 as char;
                            buf.push(nch);
                            if (nch >= 'a' && nch <= 'z') || (nch >= 'A' && nch <= 'Z') || nch == '~' { break; }
                            continue;
                        }
                        let nch = nc as u8 as char;
                        buf.push(nch);
                        if (nch >= 'a' && nch <= 'z') || (nch >= 'A' && nch <= 'Z') || nch == '~' { break; }
                    }
                    if buf.starts_with('<') {
                        let is_release = buf.ends_with('m');
                        let parts: Vec<&str> = buf[1..buf.len()-1].split(';').collect();
                        if parts.len() == 3 {
                            let pb = parts[0].parse::<i32>().unwrap_or(0);
                            let px = parts[1].parse::<usize>().unwrap_or(0);
                            let py = parts[2].parse::<usize>().unwrap_or(0);
                            let button = match pb & 0x43 { 0 => MouseButton::Left, 1 => MouseButton::Middle, 2 => MouseButton::Right, 64 => MouseButton::WheelUp, 65 => MouseButton::WheelDown, _ => MouseButton::None, };
                            let action = if pb & 0x20 != 0 { MouseAction::Drag } else if is_release { MouseAction::Release } else { MouseAction::Press };
                            special_req = Some(Request::MouseEvent { button, action, row: py, col: px });
                        }
                    } else {
                        special_req = match buf.as_str() {
                            "A" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Up, count: 1 }),
                            "B" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Down, count: 1 }),
                            "C" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Right, count: 1 }),
                            "D" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Left, count: 1 }),
                            _ => None,
                        };
                    }
                } else if c2 != -1 {
                    // Not a '[' sequence, but we read something after Esc. 
                    // Feed both to process_keys.
                    let _ = processor.process_keys(&mut state, "\x1B");
                    let _ = processor.process_keys(&mut state, &(c2 as u8 as char).to_string());
                    continue;
                }
            }

            if let Some(sr) = special_req {
                let _ = handle_request(&mut state, sr);
            } else {
                let _ = processor.process_keys(&mut state, &key.to_string());
            }
            
            // Limit characters processed in one go to keep UI responsive
            if input_count > 100 { break; }
        }

        if input_count == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
        state.redraw();
        print!("{}", state.grid.flush());
        use std::io::Write;
        std::io::stdout().flush().unwrap();
    }
    if test_keys.is_none() && is_tty { 
        // Disable mouse tracking, show cursor, exit alternate screen
        print!("\x1B[?1006l\x1B[?1000l\x1B[?25h\x1B[?1049l");
        let _ = std::io::stdout().flush();
        unsafe { os_restore_terminal() }; 
    }
}
