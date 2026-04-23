use std::time::Duration;
use rneovim::nvim::state::{VimState, Mode};
use rneovim::nvim::event::event_loop::EventLoop;
use rneovim::nvim::event::key_processor::KeyProcessor;
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::{Request, MouseButton, MouseAction};

extern "C" {
    fn os_setup_terminal();
    fn os_restore_terminal();
    fn os_read_char() -> std::ffi::c_int;
    fn os_get_terminal_size(width: *mut std::ffi::c_int, height: *mut std::ffi::c_int);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut state = VimState::new();
    state.log("=== RNEOVIM STARTUP ===");
    let mut eloop = EventLoop::new();
    let processor = KeyProcessor::new();

    // テストモードの判定
    let mut test_keys = None;
    let mut test_output = None;
    let mut filename = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--test-keys" => {
                if i + 1 < args.len() {
                    test_keys = Some(args[i+1].clone());
                    i += 1;
                }
            }
            "--test-output" => {
                if i + 1 < args.len() {
                    test_output = Some(args[i+1].clone());
                    i += 1;
                }
            }
            _ if !args[i].starts_with('-') => {
                filename = Some(args[i].clone());
            }
            _ => {}
        }
        i += 1;
    }

    let sender = eloop.sender();
    state.sender = Some(sender.clone());

    // プラグインとAPIの初期化
    let bufs = state.buffers.clone();
    let s = state.sender.clone();
    if let Err(e) = state.lua_env.borrow_mut().register_api(bufs, s) {
        state.log(&format!("Failed to register API: {}", e));
    }

    state.init_plugins();
    state.vim_did_enter = true;
    rneovim::nvim::api::trigger_autocmd(&mut state, rneovim::nvim::state::AutoCmdEvent::VimEnter);

    if let Some(keys) = test_keys {
        if let Some(f) = filename {
            let _ = handle_request(&mut state, Request::OpenFile(f));
        }
        
        // 解析: \xNN 形式をバイトに変換
        let mut processed_keys = String::new();
        let mut chars = keys.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek() == Some(&'x') {
                chars.next(); // skip 'x'
                let mut hex = String::new();
                if let Some(h1) = chars.next() { hex.push(h1); }
                if let Some(h2) = chars.next() { hex.push(h2); }
                if let Ok(val) = u8::from_str_radix(&hex, 16) {
                    processed_keys.push(val as char);
                }
            } else {
                processed_keys.push(c);
            }
        }

        let _ = processor.process_keys(&mut state, &processed_keys);
        
        if let Some(out) = test_output {
            let buf = state.current_window().buffer();
            let b = buf.borrow();
            let mut result = String::new();
            for i in 1..=b.line_count() {
                if let Some(line) = b.get_line(i) {
                    result.push_str(line);
                    result.push('\n');
                }
            }
            std::fs::write(out, result).expect("Failed to write test output");
        }
        return;
    }

    unsafe { os_setup_terminal() };
    
    // ターミナルサイズの取得と反映
    if let Some((terminal_size::Width(w), terminal_size::Height(h))) = terminal_size::terminal_size() {
        state.grid.resize(w as usize, h as usize);
        let win = state.current_window_mut();
        win.set_width(w as usize);
        win.set_height((h as usize).saturating_sub(2));
    }

    print!("\x1B[2J\x1B[H\x1B[?25l\x1B[?1000h\x1B[?1006h");
    state.redraw();

    let sender_clone = sender.clone();
    let _sig_thread = std::thread::spawn(move || {
        let mut signals = signal_hook::iterator::Signals::new(&[
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGWINCH,
        ]).unwrap();

        for signal in signals.forever() {
            match signal {
                signal_hook::consts::SIGINT => {
                    let _ = sender_clone.send(Box::new(|state| {
                        state.set_cmdline("Interrupted!".to_string());
                        let _ = handle_request(state, Request::Redraw);
                    }));
                }
                signal_hook::consts::SIGTERM | signal_hook::consts::SIGHUP => {
                    let _ = sender_clone.send(Box::new(|state| { state.quit(); }));
                }
                signal_hook::consts::SIGWINCH => {
                    let _ = sender_clone.send(Box::new(|state| {
                        let mut w: std::ffi::c_int = 0; let mut h: std::ffi::c_int = 0;
                        unsafe { os_get_terminal_size(&mut w, &mut h) };
                        state.grid.resize(w as usize, h as usize);
                        let _ = handle_request(state, Request::Resize { width: w as usize, height: h as usize });
                        state.redraw();
                    }));
                }
                _ => {}
            }
        }
    });

    let _ = rneovim::load_config(&mut state, ".rneovimrc");
    if filename.is_some() {
        let _ = handle_request(&mut state, Request::OpenFile(filename.unwrap()));
    }
    let _ = handle_request(&mut state, Request::Redraw);

    while !state.should_quit() {
        // 非同期リクエストの処理
        eloop.poll_events(&mut state, Some(Duration::ZERO));

        let c = unsafe { os_read_char() };
        if c != -1 {
            let key = c as u8 as char;
            let mut special_req = None;

            if key == '\x1B' {
                // Escape sequence parser
                let c2 = unsafe { os_read_char() };
                if c2 == '[' as i32 {
                    let mut buf = String::new();
                    loop {
                        let nc = unsafe { os_read_char() };
                        if nc == -1 { break; }
                        let nch = nc as u8 as char;
                        buf.push(nch);
                        if (nch >= 'a' && nch <= 'z') || (nch >= 'A' && nch <= 'Z') || nch == '~' { break; }
                    }
                    if buf.starts_with('<') {
                        // SGR mouse mode: \x1B[<Pb;Px;Py;m or M
                        let is_release = buf.ends_with('m');
                        let parts: Vec<&str> = buf[1..buf.len()-1].split(';').collect();
                        if parts.len() == 3 {
                            let pb = parts[0].parse::<i32>().unwrap_or(0);
                            let px = parts[1].parse::<usize>().unwrap_or(0);
                            let py = parts[2].parse::<usize>().unwrap_or(0);
                            let button = match pb & 0x43 {
                                0 => MouseButton::Left, 1 => MouseButton::Middle, 2 => MouseButton::Right,
                                64 => MouseButton::WheelUp, 65 => MouseButton::WheelDown, _ => MouseButton::None,
                            };
                            let action = if pb & 0x20 != 0 { MouseAction::Drag } else if is_release { MouseAction::Release } else { MouseAction::Press };
                            special_req = Some(Request::MouseEvent { button, action, row: py, col: px });
                        }
                    } else {
                        // Arrow keys and others
                        special_req = match buf.as_str() {
                            "A" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Up, count: 1 }),
                            "B" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Down, count: 1 }),
                            "C" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Right, count: 1 }),
                            "D" => Some(Request::OpMotion { op: rneovim::nvim::request::Operator::None, motion: rneovim::nvim::request::Motion::Left, count: 1 }),
                            _ => None,
                        };
                    }
                }
            }

            let req = if let Some(sr) = special_req {
                sr
            } else {
                processor.process_key(&mut state, key).unwrap_or(Request::Redraw)
            };
            let _ = handle_request(&mut state, req);
            state.redraw();
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    unsafe { os_restore_terminal() };
}
