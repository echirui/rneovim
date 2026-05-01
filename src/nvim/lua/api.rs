use mlua::{Lua, Table, Value, Function};
use crate::nvim::lua::{StateWrapper};
use crate::nvim::state::{Mode, VisualMode, AutoCmd, AutoCmdEvent, AutoCmdCallback, OptionValue, VimState};
use std::rc::Rc;
use std::cell::RefCell;

pub fn get_api_functions(lua: &Lua) -> mlua::Result<Vec<(String, Function)>> {
    let mut fns = Vec::new();

    // Helper to get buffer RC
    let get_buf = |state: &mut VimState, id: i32| {
        if id == 0 {
            Some(state.current_window().buffer())
        } else {
            state.buffers.iter().find(|b| b.borrow().id() == id).cloned()
        }
    };

    fns.push(("nvim_command".to_string(), lua.create_function(|lua, cmd: String| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API CALL: nvim_command({})", cmd));
            let _ = crate::nvim::api::execute_cmd(state, &cmd);
        }
        Ok(())
    })?));

    fns.push(("nvim_exec".to_string(), lua.create_function(|lua, (cmd, _output): (String, bool)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API CALL: nvim_exec({})", cmd));
            let _ = crate::nvim::api::execute_cmd(state, &cmd);
        }
        Ok("".to_string())
    })?));

    fns.push(("nvim_create_buf".to_string(), lua.create_function(|lua, (_listed, _scratch): (bool, bool)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let mut buf = crate::nvim::buffer::Buffer::new();
            let id = state.next_fd; state.next_fd += 1;
            buf.set_id(id);
            let buf_rc = Rc::new(RefCell::new(buf));
            state.buffers.push(buf_rc.clone());
            state.log(&format!("API: nvim_create_buf() -> {}", id));
            return Ok(id);
        }
        Ok(0)
    })?));

    fns.push(("nvim_create_augroup".to_string(), lua.create_function(|_, _name: String| {
        static NEXT_GROUP_ID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);
        Ok(NEXT_GROUP_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    })?));

    fns.push(("nvim_win_get_cursor".to_string(), lua.create_function(|lua, win_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            for tp in &state.tabpages {
                if let Some(win) = tp.windows.iter().find(|w| w.id() == win_id) {
                    let cursor = win.cursor();
                    let res = lua.create_table()?;
                    res.set(1, cursor.row as i32)?;
                    res.set(2, cursor.col as i32)?;
                    return Ok(res);
                }
            }
        }
        let res = lua.create_table()?;
        res.set(1, 1)?; res.set(2, 0)?;
        Ok(res)
    })?));

    fns.push(("nvim_get_current_buf".to_string(),
 lua.create_function(|lua, _: ()| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            return Ok(state.current_window().buffer().borrow().id());
        }
        Ok(0)
    })?));

    fns.push(("nvim_get_current_win".to_string(), lua.create_function(|lua, _: ()| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            return Ok(state.current_window().id());
        }
        Ok(0)
    })?));

    fns.push(("nvim_buf_get_name".to_string(), lua.create_function(move |lua, buf_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                return Ok(buf_rc.borrow().name().unwrap_or("").to_string());
            }
        }
        Ok("".to_string())
    })?));

    fns.push(("nvim_win_get_buf".to_string(), lua.create_function(move |lua, win_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            let win = if win_id == 0 { Some(state.current_window()) } else {
                state.tabpages.iter().flat_map(|tp| tp.windows.iter()).find(|w| w.id() == win_id)
            };
            if let Some(w) = win {
                return Ok(w.buffer().borrow().id());
            }
        }
        Ok(0)
    })?));

    fns.push(("nvim_set_keymap".to_string(), lua.create_function(|lua, (mode, lhs, rhs, opts_opt): (String, String, String, Option<Table>)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let opts = opts_opt.unwrap_or_else(|| lua.create_table().unwrap());
            let noremap = opts.get::<bool>("noremap").unwrap_or(false);
            let silent = opts.get::<bool>("silent").unwrap_or(false);
            let expr = opts.get::<bool>("expr").unwrap_or(false);
            state.keymap.add_mapping(&mode, &lhs, crate::nvim::keymap::MappingTarget::String(rhs), !noremap, silent, expr);
        }
        Ok(())
    })?));

    fns.push(("nvim_buf_set_keymap".to_string(), lua.create_function(|lua, (_buf_id, mode, lhs, rhs, opts_opt): (i32, String, String, String, Option<Table>)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            // For now, treat buffer-local mapping as global (same as nvim_set_keymap)
            // Real implementation would need per-buffer keymaps
            let opts = opts_opt.unwrap_or_else(|| lua.create_table().unwrap());
            let noremap = opts.get::<bool>("noremap").unwrap_or(false);
            let silent = opts.get::<bool>("silent").unwrap_or(false);
            let expr = opts.get::<bool>("expr").unwrap_or(false);
            state.keymap.add_mapping(&mode, &lhs, crate::nvim::keymap::MappingTarget::String(rhs), !noremap, silent, expr);
        }
        Ok(())
    })?));

    fns.push(("nvim_buf_get_lines".to_string(),
 lua.create_function(move |lua, (buf_id, start, end, _strict): (i32, usize, i32, bool)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(b) = buf_rc.try_borrow() {
                    let actual_end = if end < 0 { b.line_count() } else { end as usize };
                    state.log(&format!("API: nvim_buf_get_lines(buf={}, start={}, end={}) - count={}", buf_id, start, end, b.line_count()));
                    let mut lines = Vec::new();
                    for i in start..actual_end {
                        if let Some(line) = b.get_line(i + 1) { lines.push(line.to_string()); }
                    }
                    return Ok(lines);
                }
            }
        }
        Ok(Vec::<String>::new())
    })?));

    fns.push(("nvim_replace_termcodes".to_string(), lua.create_function(|lua, (str, _from_part, _do_lt, _special): (String, bool, bool, bool)| {
        Ok(lua.create_string(&str)?)
    })?));

    fns.push(("nvim_feedkeys".to_string(), lua.create_function(|_, (_keys, _mode, _escape_ks): (String, String, bool)| {
        Ok(())
    })?));

    fns.push(("nvim_win_get_width".to_string(), lua.create_function(|lua, win_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            for tp in &state.tabpages {
                if let Some(win) = tp.windows.iter().find(|w| w.id() == win_id) {
                    if let Some(config) = &win.config { return Ok(config.width as i32); }
                    return Ok(state.grid.width as i32);
                }
            }
        }
        Ok(80)
    })?));

    fns.push(("nvim_win_get_height".to_string(), lua.create_function(|lua, win_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            for tp in &state.tabpages {
                if let Some(win) = tp.windows.iter().find(|w| w.id() == win_id) {
                    if let Some(config) = &win.config { return Ok(config.height as i32); }
                    return Ok(state.grid.height as i32 - 2); // Sub status line
                }
            }
        }
        Ok(24)
    })?));

    fns.push(("nvim_buf_set_lines".to_string(),
 lua.create_function(move |lua, (buf_id, start, end_idx, _strict, lines_val): (i32, i32, i32, bool, Value)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let mut lines = Vec::new();
            if let Value::Table(t) = &lines_val {
                for v in t.sequence_values::<Value>() {
                    if let Ok(Value::String(s)) = v { lines.push(s.to_string_lossy().to_string()); }
                    else if let Ok(v) = v { lines.push(format!("{:?}", v)); }
                }
            }
            state.log(&format!("API CALL: nvim_buf_set_lines(buf={}, start={}, end={}) - {} lines", 
                buf_id, start, end_idx, lines.len()));
            
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(mut b) = buf_rc.try_borrow_mut() {
                    let start = start as usize;
                    let actual_end = if end_idx < 0 { b.line_count() } else { end_idx as usize };
                    for _ in start..actual_end {
                        if start + 1 <= b.line_count() { let _ = b.delete_line(start + 1); }
                    }
                    for (i, line) in lines.into_iter().enumerate() {
                        let _ = b.insert_line(start + i + 1, &line);
                    }
                }
            } else {
                state.log(&format!("API ERROR: buf {} not found in nvim_buf_set_lines", buf_id));
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_buf_get_option".to_string(), lua.create_function(move |lua, (buf_id, name): (i32, String)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API: nvim_buf_get_option(buf={}, name={})", buf_id, name));
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Some(v) = buf_rc.borrow().get_option(&name) {
                    return Ok(match v {
                        OptionValue::Bool(b) => Value::Boolean(*b),
                        OptionValue::Int(i) => Value::Integer(*i),
                        OptionValue::String(s) => Value::String(lua.create_string(s)?),
                    });
                }
            }
        }
        Ok(Value::Nil)
    })?));

    fns.push(("nvim_buf_set_text".to_string(), lua.create_function(move |lua, (buf_id, sr, sc, er, ec, replacement_val): (i32, usize, usize, usize, usize, Value)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let mut replacement = Vec::new();
            if let Value::Table(t) = &replacement_val {
                for v in t.sequence_values::<Value>() {
                    if let Ok(Value::String(s)) = v { replacement.push(s.to_string_lossy().to_string()); }
                }
            }
            state.log(&format!("API: nvim_buf_set_text(buf={}, range={}:{} to {}:{}) - {} lines", 
                buf_id, sr, sc, er, ec, replacement.len()));
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(mut b) = buf_rc.try_borrow_mut() {
                    let _ = b.set_text(sr + 1, sc, er + 1, ec, replacement);
                }
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_put".to_string(), lua.create_function(|lua, (lines, _type, _after, _follow): (Vec<String>, String, bool, bool)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API: nvim_put - {} lines", lines.len()));
        }
        Ok(())
    })?));

    fns.push(("nvim_get_all_options_info".to_string(), lua.create_function(|lua, _: ()| {
        let res = lua.create_table()?;
        for name in ["runtimepath", "packpath", "loadplugins", "compatible", "shada"] {
            let opt = lua.create_table()?;
            opt.set("name", name)?;
            opt.set("type", if name == "loadplugins" || name == "compatible" { "boolean" } else { "string" })?;
            opt.set("default", if name == "loadplugins" { Value::Boolean(true) } else if name == "compatible" { Value::Boolean(false) } else { Value::String(lua.create_string("")?) })?;
            opt.set("was_set", false)?;
            opt.set("last_set_chan", 0)?;
            res.set(name, opt)?;
        }
        Ok(res)
    })?));

    fns.push(("nvim_set_current_buf".to_string(), lua.create_function(|lua, buf_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API CALL: nvim_set_current_buf(buf={})", buf_id));
            if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id).cloned() {
                state.current_window_mut().set_buffer(buf_rc);
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_get_option_value".to_string(), lua.create_function(|lua, (name, _opts): (String, Table)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if name == "columns" { return Ok(Value::Integer(state.grid.width as i64)); }
            if name == "lines" { return Ok(Value::Integer(state.grid.height as i64)); }
            let rname = if name == "rtp" { "runtimepath" } else { &name };
            if let Some(v) = state.options.get(rname) {
                return Ok(match v {
                    OptionValue::Bool(b) => Value::Boolean(*b),
                    OptionValue::Int(i) => Value::Integer(*i as i64),
                    OptionValue::String(s) => Value::String(lua.create_string(s)?),
                });
            }
        }
        Ok(Value::Nil)
    })?));

    fns.push(("nvim_set_option_value".to_string(), lua.create_function(|lua, (name, val, _opts): (String, Value, Table)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let opt_val = match val {
                Value::Boolean(b) => OptionValue::Bool(b),
                Value::Integer(i) => OptionValue::Int(i as i64),
                Value::String(s) => OptionValue::String(s.to_string_lossy().to_string()),
                _ => return Ok(()),
            };
            state.options.insert(name, opt_val);
        }
        Ok(())
    })?));

    fns.push(("nvim_buf_get_option".to_string(), lua.create_function(move |lua, (buf_id, name): (i32, String)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(b) = buf_rc.try_borrow() {
                    if let Some(v) = b.get_option(&name) {
                        return Ok(match v {
                            OptionValue::Bool(b) => Value::Boolean(*b),
                            OptionValue::Int(i) => Value::Integer(*i as i64),
                            OptionValue::String(s) => Value::String(lua.create_string(&s)?),
                        });
                    }
                }
            }
            if name == "filetype" { 
                if let Some(buf_rc) = get_buf(state, buf_id) {
                    if let Ok(b) = buf_rc.try_borrow() {
                        if let Some(ft) = b.get_option("filetype") {
                            if let OptionValue::String(s) = ft { return Ok(Value::String(lua.create_string(&s)?)); }
                        }
                    }
                }
                return Ok(Value::String(lua.create_string("lazy")?)); 
            }
            if name == "buftype" { return Ok(Value::String(lua.create_string("nofile")?)); }
            if name == "modifiable" { return Ok(Value::Boolean(true)); }
        }
        Ok(Value::Nil)
    })?));

    fns.push(("nvim_buf_set_option".to_string(), lua.create_function(move |lua, (buf_id, name, val): (i32, String, Value)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(mut b) = buf_rc.try_borrow_mut() {
                    let opt_val = match val {
                        Value::Boolean(b) => OptionValue::Bool(b),
                        Value::Integer(i) => OptionValue::Int(i as i64),
                        Value::String(s) => OptionValue::String(s.to_string_lossy().to_string()),
                        _ => return Ok(()),
                    };
                    b.set_option(&name, opt_val);
                }
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_buf_get_name".to_string(), lua.create_function(move |lua, buf_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(b) = buf_rc.try_borrow() { return Ok(b.name().unwrap_or("").to_string()); }
            }
        }
        Ok("".to_string())
    })?));

    fns.push(("nvim_buf_set_name".to_string(), lua.create_function(move |lua, (buf_id, name): (i32, String)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                if let Ok(mut b) = buf_rc.try_borrow_mut() { b.set_name(&name); }
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_get_mode".to_string(), lua.create_function(|lua, _: ()| {
        let t = lua.create_table()?;
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let mode_str = match state.mode {
                Mode::Normal => "n", Mode::Insert => "i", Mode::Visual(VisualMode::Char) => "v", 
                Mode::Visual(VisualMode::Line) => "V", Mode::Visual(VisualMode::Block) => "\x16",
                Mode::CommandLine => "c", _ => "n",
            };
            t.set("mode", mode_str)?; t.set("blocking", false)?;
        }
        Ok(t)
    })?));

    fns.push(("__nvim_register_autocmd_id".to_string(), lua.create_function(|lua, (event_str, id, pattern, once): (String, i32, String, bool)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let ev_type = match event_str.as_str() {
                "VimEnter" => AutoCmdEvent::VimEnter, "UIEnter" => AutoCmdEvent::UIEnter,
                "BufReadPost" => AutoCmdEvent::BufReadPost, "BufWritePost" => AutoCmdEvent::BufWritePost,
                "FileType" => AutoCmdEvent::FileType, "Syntax" => AutoCmdEvent::Syntax,
                "SafeState" => AutoCmdEvent::SafeState, "CursorHold" => AutoCmdEvent::CursorHold,
                "CursorHoldI" => AutoCmdEvent::CursorHoldI, "WinClosed" => AutoCmdEvent::WinClosed,
                "WinResized" => AutoCmdEvent::WinResized, "VimResized" => AutoCmdEvent::VimResized,
                "ColorScheme" => AutoCmdEvent::ColorScheme, "ColorSchemePre" => AutoCmdEvent::ColorSchemePre,
                "User" => AutoCmdEvent::User, _ => AutoCmdEvent::User,
            };
            state.autocmds.entry(ev_type).or_insert_with(Vec::new).push(AutoCmd {
                callback: AutoCmdCallback::Lua(id),
                pattern: Some(pattern),
                once,
            });
        }
        Ok(())
    })?));

    fns.push(("nvim_create_autocmd".to_string(),
 lua.create_function(|lua, (event_val, opts): (Value, Table)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let events = match event_val {
                Value::String(s) => vec![s.to_string_lossy().to_string()],
                Value::Table(t) => t.sequence_values::<String>().filter_map(|v| v.ok()).collect(),
                _ => vec![],
            };
            let once: bool = opts.get("once").unwrap_or(false);
            let callback_opt = if let Ok(id) = opts.get::<i32>("callback_id") {
                Some(AutoCmdCallback::Lua(id))
            } else {
                match opts.get::<Value>("callback") {
                    Ok(Value::Function(f)) => {
                        let env = state.lua_env.clone();
                        match env.register_callback(Value::Function(f)) {
                            Ok(id) => Some(AutoCmdCallback::Lua(id)),
                            Err(_) => None,
                        }
                    }
                    _ => opts.get::<String>("command").ok().map(AutoCmdCallback::Command),
                }
            };
            let pattern: Option<String> = opts.get("pattern").ok();
            if let Some(callback) = callback_opt {
                for ev_str in events {
                    let ev_type = match ev_str.as_str() {
                        "VimEnter" => AutoCmdEvent::VimEnter, "BufReadPost" => AutoCmdEvent::BufReadPost,
                        "BufWritePost" => AutoCmdEvent::BufWritePost, "BufEnter" => AutoCmdEvent::BufEnter,
                        "User" => AutoCmdEvent::User, "ColorSchemePre" => AutoCmdEvent::ColorSchemePre,
                        "SafeState" => AutoCmdEvent::SafeState, "CursorHold" => AutoCmdEvent::CursorHold,
                        "BufDelete" => AutoCmdEvent::BufDelete, "BufWinEnter" => AutoCmdEvent::BufWinEnter,
                        "FileType" => AutoCmdEvent::FileType, "Syntax" => AutoCmdEvent::Syntax,
                        "UIEnter" => AutoCmdEvent::UIEnter, _ => AutoCmdEvent::User,
                    };
                    state.autocmds.entry(ev_type).or_insert_with(Vec::new).push(AutoCmd { callback: callback.clone(), pattern: pattern.clone(), once });
                }
            }
        }
        Ok(1)
    })?));

    fns.push(("nvim_exec_autocmds".to_string(), lua.create_function(|lua, (event, opts): (Value, Table)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let events: Vec<String> = match event {
                Value::String(s) => vec![s.to_string_lossy().to_string()],
                Value::Table(t) => t.sequence_values::<String>().filter_map(|v| v.ok()).collect(),
                _ => return Ok(()),
            };
            let pattern: Option<String> = opts.get("pattern").ok();
            for ev_name in events {
                let ev = match ev_name.as_str() {
                    "VimEnter" => AutoCmdEvent::VimEnter, "UIEnter" => AutoCmdEvent::UIEnter, "User" => AutoCmdEvent::User,
                    "SafeState" => AutoCmdEvent::SafeState, "CursorHold" => AutoCmdEvent::CursorHold,
                    "FileType" => AutoCmdEvent::FileType, "Syntax" => AutoCmdEvent::Syntax, _ => AutoCmdEvent::User,
                };
                crate::nvim::api::trigger_autocmd(state, ev, pattern.as_deref());
            }
        }
        Ok(())
    })?));

    fns.push(("__nvim_create_user_command_id".to_string(), lua.create_function(|lua, (name, id, _opts): (String, i32, Option<Value>)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.user_commands.insert(name, id);
        }
        Ok(())
    })?));

    fns.push(("nvim_create_user_command".to_string(), lua.create_function(|lua, (name, callback, _opts): (String, Value, Option<Value>)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let env = state.lua_env.clone();
            match callback {
                Value::Function(f) => { if let Ok(id) = env.register_callback(Value::Function(f)) { state.user_commands.insert(name, id); } }
                Value::Table(t) => { if let Ok(id) = env.register_callback(Value::Table(t)) { state.user_commands.insert(name, id); } }
                Value::String(s) => {
                    let cmd_str = s.to_string_lossy().to_string();
                    let f = lua.create_function(move |lua, _: Value| {
                        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                            let state = unsafe { &mut **wrapper.0.as_ptr() };
                            let _ = crate::nvim::api::execute_cmd(state, &cmd_str);
                        }
                        Ok(())
                    })?;
                    if let Ok(id) = env.register_callback(Value::Function(f)) { state.user_commands.insert(name, id); }
                }
                _ => {}
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_set_current_win".to_string(), lua.create_function(|lua, win_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API CALL: nvim_set_current_win(win={})", win_id));
            for tp in &mut state.tabpages { if let Some(pos) = tp.windows.iter().position(|w| w.id() == win_id) { tp.current_win_idx = pos; return Ok(()); } }
        }
        Ok(())
    })?));

    fns.push(("nvim_open_win".to_string(), lua.create_function(move |lua, (buf_id, enter, config_tbl): (i32, bool, Table)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                let mut config = crate::nvim::window::WinConfig::default();
                config.relative = config_tbl.get("relative").unwrap_or_else(|_| "editor".to_string());
                config.row = config_tbl.get("row").unwrap_or(0.0);
                config.col = config_tbl.get("col").unwrap_or(0.0);
                config.width = config_tbl.get("width").unwrap_or(80);
                config.height = config_tbl.get("height").unwrap_or(24);
                config.anchor = config_tbl.get("anchor").unwrap_or_else(|_| "NW".to_string());
                config.zindex = config_tbl.get("zindex").unwrap_or(50);
                config.focusable = config_tbl.get("focusable").unwrap_or(true);
                
                if let Ok(b) = config_tbl.get::<Value>("border") {
                    match b {
                        Value::String(s) => config.border = Some(s.to_string_lossy().to_string()),
                        _ => {}
                    }
                }

                state.log(&format!("API: nvim_open_win(buf={}, rel={}, pos={},{})", buf_id, config.relative, config.row, config.col));
                let mut win = crate::nvim::window::Window::new_floating(buf_rc, config);
                let win_id = state.next_win_id; state.next_win_id += 1; win.set_id(win_id);
                state.current_tabpage_mut().windows.push(win);
                if enter { 
                    let last = state.current_tabpage().windows.len() - 1; 
                    state.current_tabpage_mut().current_win_idx = last; 
                }
                return Ok(win_id);
            }
        }
        Ok(1000)
    })?));

    fns.push(("nvim_win_get_config".to_string(), lua.create_function(|lua, win_id: i32| {
        let res = lua.create_table()?;
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            let win = if win_id == 0 { Some(state.current_window()) } else {
                state.tabpages.iter().flat_map(|tp| tp.windows.iter()).find(|w| w.id() == win_id)
            };

            if let Some(w) = win {
                if let Some(c) = &w.config {
                    res.set("relative", c.relative.clone())?;
                    res.set("row", c.row)?;
                    res.set("col", c.col)?;
                    res.set("width", c.width)?;
                    res.set("height", c.height)?;
                    res.set("anchor", c.anchor.clone())?;
                    res.set("zindex", c.zindex)?;
                    res.set("focusable", c.focusable)?;
                    if let Some(b) = &c.border { res.set("border", b.clone())?; }
                    return Ok(res);
                }
            }
        }
        res.set("relative", "editor")?; res.set("width", 80)?; res.set("height", 20)?; res.set("row", 1)?; res.set("col", 1)?;
        Ok(res)
    })?));

    fns.push(("nvim_win_set_cursor".to_string(), lua.create_function(|_lua, (win_id, pos): (i32, Vec<usize>)| {
        if let Some(wrapper) = _lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let win = if win_id == 0 { Some(state.current_window_mut()) } else {
                state.tabpages.iter_mut().flat_map(|tp| tp.windows.iter_mut()).find(|w| w.id() == win_id)
            };
            if let (Some(w), true) = (win, pos.len() >= 2) {
                w.set_cursor(pos[0], pos[1]);
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_buf_line_count".to_string(), lua.create_function(move |lua, buf_id: i32| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) { return Ok(buf_rc.borrow().line_count() as i32); }
        }
        Ok(1)
    })?));

    fns.push(("nvim_get_api_info".to_string(), lua.create_function(|lua, _: ()| {
        let res = lua.create_table()?; res.set(1, 1)?;
        let version = lua.create_table()?; version.set("major", 0)?; version.set("minor", 10)?; version.set("patch", 0)?;
        let metadata = lua.create_table()?; metadata.set("version", version)?;
        res.set(2, metadata)?;
        Ok(res)
    })?));

    fns.push(("nvim_list_uis".to_string(), lua.create_function(|lua, _: ()| {
        let res = lua.create_table()?;
        let ui = lua.create_table()?; ui.set("width", 80)?; ui.set("height", 24)?; ui.set("rgb", true)?; ui.set("chan", 1)?;
        ui.set("terminal", true)?; ui.set("stdout_tty", true)?; ui.set("stdin_tty", true)?;
        res.set(1, ui)?;
        Ok(res)
    })?));

    fns.push(("nvim_create_namespace".to_string(), lua.create_function(|_, _name: String| {
        static NEXT_NS_ID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);
        Ok(NEXT_NS_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    })?));

    fns.push(("nvim_get_runtime_file".to_string(), lua.create_function(|lua, (path, all): (String, bool)| {
        let mut files = Vec::new();
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(OptionValue::String(rtp)) = state.options.get("runtimepath") {
                for p in rtp.split(',') {
                    if p.is_empty() { continue; }
                    let base = std::path::Path::new(p);
                    if path.is_empty() { files.push(p.to_string()); if !all { break; } continue; }
                    if path.contains('*') {
                        let pattern = base.join(&path).to_string_lossy().to_string();
                        if let Ok(entries) = glob::glob(&pattern) {
                            for entry in entries.filter_map(|x| x.ok()) { files.push(entry.to_string_lossy().to_string()); if !all { break; } }
                        }
                    } else {
                        let full = base.join(&path); if full.exists() { files.push(full.to_string_lossy().to_string()); if !all { break; } }
                    }
                    if !all && !files.is_empty() { break; }
                }
            }
        }
        let res = lua.create_table()?;
        for (i, f) in files.into_iter().enumerate() { res.set(i + 1, f)?; }
        Ok(res)
    })?));

    fns.push(("nvim_buf_set_extmark".to_string(), lua.create_function(move |lua, (buf_id, ns_id, row, col, opts): (i32, i32, usize, usize, Table)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("API CALL: nvim_buf_set_extmark(buf={}, ns={}, pos={}:{})", buf_id, ns_id, row, col));
            if let Some(buf_rc) = get_buf(state, buf_id) {
                let mut b = buf_rc.borrow_mut();
                let mark_id = opts.get::<i32>("id").ok();
                let end_row = opts.get::<usize>("end_row").ok();
                let end_col = opts.get::<usize>("end_col").ok();
                let hl_group = opts.get::<String>("hl_group").ok();
                let priority = opts.get::<i32>("priority").unwrap_or(0);
                let virt_text_pos = opts.get::<String>("virt_text_pos").unwrap_or_else(|_| "eol".to_string());
                let mut virt_text = Vec::new();
                if let Ok(vt) = opts.get::<Table>("virt_text") {
                    for v in vt.sequence_values::<Table>() {
                        if let Ok(t) = v {
                            let text: String = t.get(1).unwrap_or_default();
                            let hl: String = t.get(2).unwrap_or_default();
                            virt_text.push((text, hl));
                        }
                    }
                }
                let id = b.set_extmark(ns_id, mark_id, row + 1, col, end_row.map(|r| r + 1), end_col, hl_group, virt_text, virt_text_pos, priority);
                return Ok(Value::Integer(id as i64));
            } else {
                state.log(&format!("API ERROR: buf {} not found in nvim_buf_set_extmark", buf_id));
            }
        }
        Ok(Value::Integer(0))
    })?));

    fns.push(("nvim_buf_get_extmarks".to_string(), lua.create_function(move |lua, (buf_id, ns_id, start, end, _opts): (i32, i32, Value, Value, Table)| {
        let mut res = Vec::<Table>::new();
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                let b = buf_rc.borrow();
                let parse_pos = |v: Value| -> (usize, usize) {
                    match v {
                        Value::Integer(i) => (i as usize + 1, 0),
                        Value::Table(t) => { let r: usize = t.get(1).unwrap_or(0); let c: usize = t.get(2).unwrap_or(0); (r + 1, c) }
                        _ => (1, 0)
                    }
                };
                let (s_row, s_col) = parse_pos(start);
                let (e_row, e_col) = match end { Value::Integer(i) if i == -1 => (b.line_count(), 999999), _ => parse_pos(end) };
                let marks = b.extmark_manager.get_extmarks_in_range(ns_id, s_row, s_col, e_row, e_col);
                for m in marks { let t = lua.create_table()?; t.set(1, m.id)?; t.set(2, m.row - 1)?; t.set(3, m.col)?; res.push(t); }
            }
        }
        Ok(res)
    })?));

    fns.push(("nvim_buf_add_highlight".to_string(), lua.create_function(|_, _: (i32, i32, String, i32, i32, i32)| Ok(1))?));
    
    fns.push(("nvim_buf_attach".to_string(), lua.create_function(|_, _: (i32, bool, Table)| Ok(true))?));

    fns.push(("nvim_buf_clear_namespace".to_string(), lua.create_function(move |lua, (buf_id, ns_id, start_row, end_row): (i32, i32, usize, i32)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(buf_rc) = get_buf(state, buf_id) {
                let mut b = buf_rc.borrow_mut();
                if start_row == 0 && end_row == -1 { b.extmark_manager.clear_namespace(ns_id); }
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_set_hl".to_string(), lua.create_function(|lua, (_ns_id, name, val): (i32, String, Value)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            let mut attrs = crate::nvim::state::HlAttrs::default();
            if let Value::String(s) = val {
                let s = s.to_string_lossy();
                for part in s.split(',') {
                    let kv: Vec<&str> = part.split('=').collect();
                    if kv.len() == 2 {
                        match kv[0] {
                            "fg" => { if let Ok(c) = kv[1].parse::<u32>() { attrs.fg = Some(c); } }
                            "bg" => { if let Ok(c) = kv[1].parse::<u32>() { attrs.bg = Some(c); } }
                            "bold" => attrs.bold = true,
                            "italic" => attrs.italic = true,
                            "underline" => attrs.underline = true,
                            "reverse" => attrs.reverse = true,
                            _ => {}
                        }
                    }
                }
                state.highlight_groups.insert(name, attrs);
            } else if let Value::Table(t) = val {
                let parse_color = |v: Value| -> Option<u32> { match v { Value::Integer(i) => Some(i as u32), Value::String(s) => { let s = s.to_string_lossy(); if s.starts_with('#') { u32::from_str_radix(&s[1..], 16).ok() } else { None } } _ => None } };
                if let Ok(fg) = t.get::<Value>("fg") { attrs.fg = parse_color(fg); }
                if let Ok(bg) = t.get::<Value>("bg") { attrs.bg = parse_color(bg); }
                attrs.bold = t.get::<bool>("bold").unwrap_or(false);
                attrs.italic = t.get::<bool>("italic").unwrap_or(false);
                attrs.underline = t.get::<bool>("underline").unwrap_or(false);
                attrs.reverse = t.get::<bool>("reverse").unwrap_or(false);
                state.highlight_groups.insert(name, attrs);
            }
        }
        Ok(())
    })?));

    fns.push(("nvim_get_hl".to_string(), lua.create_function(|lua, (_ns_id, opts): (i32, Table)| {
        let res = lua.create_table()?;
        if let Ok(name) = opts.get::<String>("name") {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &**wrapper.0.as_ptr() };
                if let Some(attrs) = state.highlight_groups.get(&name) {
                    if let Some(fg) = attrs.fg { res.set("fg", fg)?; }
                    if let Some(bg) = attrs.bg { res.set("bg", bg)?; }
                    res.set("bold", attrs.bold)?;
                    res.set("italic", attrs.italic)?;
                    res.set("underline", attrs.underline)?;
                    res.set("reverse", attrs.reverse)?;
                }
            }
        }
        Ok(res)
    })?));

    fns.push(("nvim_buf_get_changedtick".to_string(), lua.create_function(|_, _: i32| Ok(1))?));

    fns.push(("nvim_get_vvar".to_string(), lua.create_function(|lua, name: String| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            if name == "argv" {
                let res = lua.create_table()?;
                for (i, arg) in std::env::args().enumerate() { res.set(i + 1, arg)?; }
                return Ok(Value::Table(res));
            }
            if name == "dying" { return Ok(Value::Integer(0)); }
            if name == "exiting" { return Ok(Value::Nil); }
            if name == "vim_did_enter" { return Ok(Value::Integer(if state.vim_did_enter { 1 } else { 0 })); }
            if name == "MYVIMRC" {
                if let Some(ref d) = state.config_dir {
                    return Ok(Value::String(lua.create_string(d.join("init.lua").to_string_lossy().as_ref())?));
                }
            }
        }
        Ok(Value::Nil)
    })?));

    Ok(fns)
}
