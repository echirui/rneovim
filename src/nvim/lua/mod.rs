use crate::nvim::error::{NvimError, Result};
use mlua::{Lua, RegistryKey};
use std::rc::Rc;
use std::cell::RefCell;
use crate::nvim::buffer::Buffer;
use std::sync::mpsc::Sender;
use crate::nvim::state::VimState;
use crate::nvim::event::event_loop::EventCallback;

pub struct LuaEnv {
    lua: Lua,
}

struct StateWrapper(*mut VimState);

impl LuaEnv {
    pub fn new() -> Self {
        let lua = Lua::new();
        Self { lua }
    }

    fn get_config_dir() -> std::path::PathBuf {
        if let Some(home) = dirs::home_dir() {
            let xdg_config = home.join(".config/rneovim");
            if xdg_config.exists() { return xdg_config; }
        }
        dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("rneovim")
    }

    fn get_data_dir() -> std::path::PathBuf {
        if let Some(home) = dirs::home_dir() {
            let xdg_data = home.join(".local/share/rneovim");
            if xdg_data.exists() || home.join(".local/share").exists() { return xdg_data; }
        }
        dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("rneovim")
    }

    fn sync_package_path(lua: &Lua, rtp: &str) -> mlua::Result<()> {
        let package: mlua::Table = lua.globals().get("package")?;
        let mut path: String = package.get("path")?;
        for p in rtp.split(',') {
            let lua_path1 = format!("{}/lua/?.lua", p);
            let lua_path2 = format!("{}/lua/?/init.lua", p);
            if !path.contains(&lua_path1) { path = format!("{};{};{}", lua_path1, lua_path2, path); }
        }
        package.set("path", path)?;
        Ok(())
    }

    pub fn register_api(&self, buffers: Vec<Rc<RefCell<Buffer>>>, sender: Option<Sender<EventCallback<VimState>>>) -> Result<()> {
        let globals = self.lua.globals();
        let vim = self.lua.create_table()?;
        let api = self.lua.create_table()?;

        let print = self.lua.create_function(|lua, msg: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.messages.push(msg);
                state.redraw();
            }
            Ok(())
        })?;
        globals.set("print", print.clone())?;
        vim.set("notify", print.clone())?;

        let nvim_err_writeln = self.lua.create_function(|lua, msg: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.messages.push(format!("ERROR: {}", msg));
                state.redraw();
            }
            Ok(())
        })?;
        api.set("nvim_err_writeln", nvim_err_writeln)?;

        let nvim_buf_get_lines = self.lua.create_function(|lua, (buf_id, start, end, _): (i32, usize, i32, bool)| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    let b = buf_rc.borrow();
                    let actual_end = if end < 0 { b.line_count() } else { end as usize };
                    let mut lines = Vec::new();
                    for i in (start + 1)..=actual_end { if let Some(line) = b.get_line(i) { lines.push(line.to_string()); } }
                    return Ok(lines);
                }
            }
            Ok(Vec::<String>::new())
        })?;

        let nvim_buf_set_lines = self.lua.create_function(|lua, (buf_id, start, end, _strict, lines): (i32, usize, i32, bool, Vec<String>)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    let mut b = buf_rc.borrow_mut();
                    let actual_end = if end < 0 { b.line_count() } else { end as usize };
                    for _ in start..actual_end { if start + 1 <= b.line_count() { let _ = b.delete_line(start + 1); } }
                    for (i, line) in lines.into_iter().enumerate() { let _ = b.insert_line(start + i + 1, &line); }
                }
            }
            Ok(())
        })?;

        let nvim_buf_set_name = self.lua.create_function(|lua, (buf_id, name): (i32, String)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) { buf_rc.borrow_mut().set_name(&name); }
            }
            Ok(())
        })?;

        let nvim_buf_get_name = self.lua.create_function(|lua, buf_id: i32| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) { return Ok(buf_rc.borrow().name().unwrap_or("").to_string()); }
            }
            Ok("".to_string())
        })?;

        let nvim_buf_line_count = self.lua.create_function(|lua, buf_id: i32| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) { return Ok(buf_rc.borrow().line_count()); }
            }
            Ok(0)
        })?;

        let nvim_create_buf = self.lua.create_function(|lua, (_listed, _scratch): (bool, bool)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let buf = Rc::new(RefCell::new(Buffer::new()));
                let id = buf.borrow().id();
                state.buffers.push(buf);
                return Ok(id);
            }
            Ok(0)
        })?;

        let nvim_get_current_buf = self.lua.create_function(|lua, _: ()| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                return Ok(state.current_window().buffer().borrow().id());
            }
            Ok(0)
        })?;

        let nvim_get_current_win = self.lua.create_function(|lua, _: ()| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                return Ok(state.current_window().id());
            }
            Ok(0)
        })?;

        let nvim_list_bufs = self.lua.create_function(|lua, _: ()| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                return Ok(state.buffers.iter().map(|b| b.borrow().id()).collect::<Vec<_>>());
            }
            Ok(Vec::new())
        })?;

        let nvim_list_wins = self.lua.create_function(|lua, _: ()| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                return Ok(state.current_tabpage().windows.iter().map(|w| w.id()).collect::<Vec<_>>());
            }
            Ok(Vec::new())
        })?;

        let nvim_open_win = self.lua.create_function(|lua, (buf_id, enter, config): (i32, bool, mlua::Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    let row = config.get::<mlua::Value>("row").map(|v| match v { mlua::Value::Number(f) => f.round() as usize, mlua::Value::Integer(i) => i as usize, _ => 0 }).unwrap_or(0);
                    let col = config.get::<mlua::Value>("col").map(|v| match v { mlua::Value::Number(f) => f.round() as usize, mlua::Value::Integer(i) => i as usize, _ => 0 }).unwrap_or(0);
                    let width = config.get::<mlua::Value>("width").map(|v| match v { mlua::Value::Number(f) => f.round() as usize, mlua::Value::Integer(i) => i as usize, _ => 40 }).unwrap_or(40);
                    let height = config.get::<mlua::Value>("height").map(|v| match v { mlua::Value::Number(f) => f.round() as usize, mlua::Value::Integer(i) => i as usize, _ => 10 }).unwrap_or(10);
                    let win_config = crate::nvim::window::WinConfig { row, col, width, height, focusable: true, external: false };
                    let win = crate::nvim::window::Window::new_floating(buf_rc.clone(), win_config);
                    let win_id = win.id();
                    let tab = state.current_tabpage_mut();
                    tab.windows.push(win);
                    if enter { tab.current_win_idx = tab.windows.len() - 1; }
                    state.redraw();
                    return Ok(win_id);
                }
            }
            Ok(0)
        })?;

        let nvim_win_get_buf = self.lua.create_function(|lua, win_id: i32| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                for win in &state.current_tabpage().windows { if win.id() == win_id { return Ok(win.buffer().borrow().id()); } }
            }
            Ok(0)
        })?;

        let nvim_win_set_buf = self.lua.create_function(|lua, (win_id, buf_id): (i32, i32)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let buf_rc_opt = state.buffers.iter().find(|b| b.borrow().id() == buf_id).cloned();
                if let Some(buf_rc) = buf_rc_opt {
                    for win in &mut state.current_tabpage_mut().windows { if win.id() == win_id { win.buffer = buf_rc; break; } }
                }
            }
            Ok(())
        })?;

        let nvim_set_current_win = self.lua.create_function(|lua, win_id: i32| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let tab = state.current_tabpage_mut();
                if let Some(idx) = tab.windows.iter().position(|w| w.id() == win_id) {
                    tab.current_win_idx = idx;
                    state.redraw();
                }
            }
            Ok(())
        })?;

        let nvim_buf_get_option = self.lua.create_function(|lua, (buf_id, name): (i32, String)| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    let _b = buf_rc.borrow();
                    match name.as_str() {
                        "filetype" => return Ok(mlua::Value::String(lua.create_string("text")?)),
                        "buftype" => return Ok(mlua::Value::String(lua.create_string("")?)),
                        "modifiable" => return Ok(mlua::Value::Boolean(true)),
                        _ => {}
                    }
                }
            }
            Ok(mlua::Value::Nil)
        })?;

        let nvim_buf_set_option = self.lua.create_function(|_, (_buf, _name, _val): (i32, String, mlua::Value)| { Ok(()) })?;
        let nvim_win_set_option = self.lua.create_function(|_, (_win, _name, _val): (i32, String, mlua::Value)| { Ok(()) })?;
        let nvim_set_hl = self.lua.create_function(|_, (_ns, _name, _val): (i32, String, mlua::Table)| { Ok(()) })?;
        let nvim_buf_add_highlight = self.lua.create_function(|_, (_buf, _ns, _hl, _line, _s, _e): (i32, i32, String, usize, usize, usize)| { Ok(0) })?;
        let nvim_buf_set_extmark = self.lua.create_function(|_, (_buf, _ns, _line, _col, _opts): (i32, i32, usize, usize, mlua::Table)| { Ok(1) })?;
        let nvim_buf_clear_namespace = self.lua.create_function(|_, (_buf, _ns, _s, _e): (i32, i32, i32, i32)| { Ok(()) })?;

        let nvim_win_get_config = self.lua.create_function(|lua, _win: i32| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                let res = lua.create_table()?;
                res.set("relative", "editor")?;
                res.set("width", state.grid.width)?;
                res.set("height", state.grid.height)?;
                return Ok(res);
            }
            Ok(lua.create_table()?)
        })?;

        let nvim_win_get_number = self.lua.create_function(|_, win_id: i32| { Ok(win_id) })?;

        let nvim_create_user_command = self.lua.create_function(|lua, (name, callback, _opts): (String, mlua::Value, mlua::Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let mlua::Value::Function(f) = callback {
                    let key = lua.create_registry_value(f)?;
                    state.user_commands.insert(name, key);
                }
            }
            Ok(())
        })?;

        let nvim_create_autocmd = self.lua.create_function(|_, (_event, _opts): (mlua::Value, mlua::Table)| { Ok(1) })?;

        let nvim_get_mode = self.lua.create_function(|lua, _: ()| {
            let res = lua.create_table()?;
            res.set("mode", "n")?; res.set("blocking", false)?;
            Ok(res)
        })?;

        let nvim_create_namespace = self.lua.create_function(|_, _name: String| { Ok(1) })?;
        let nvim_replace_termcodes = self.lua.create_function(|_, (str, _, _, _): (String, bool, bool, bool)| { Ok(str) })?;
        let nvim_clear_autocmds = self.lua.create_function(|_, _: mlua::Table| { Ok(()) })?;
        let nvim_list_uis = self.lua.create_function(|lua, _: ()| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                let ui = lua.create_table()?;
                ui.set("width", state.grid.width)?; ui.set("height", state.grid.height)?;
                return Ok(vec![ui]);
            }
            Ok(Vec::new())
        })?;
        let nvim_get_vvar = self.lua.create_function(|_, name: String| {
            match name.as_str() {
                "vim_did_enter" => Ok(mlua::Value::Integer(1)),
                _ => Ok(mlua::Value::Nil),
            }
        })?;

        let nvim_echo = self.lua.create_function(|lua, (chunks, _history, _opts): (Vec<mlua::Table>, bool, mlua::Table)| {
            let mut msg = String::new();
            for chunk in chunks { if let Ok(text) = chunk.get::<String>(1) { msg.push_str(&text); } }
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.messages.push(msg);
                state.redraw();
            }
            Ok(())
        })?;

        let nvim_get_all_options_info = self.lua.create_function(|lua, _: ()| {
            Ok(lua.create_table()?)
        })?;

        api.set("nvim_buf_get_lines", nvim_buf_get_lines)?;
        api.set("nvim_buf_set_lines", nvim_buf_set_lines)?;
        api.set("nvim_buf_set_name", nvim_buf_set_name)?;
        api.set("nvim_buf_get_name", nvim_buf_get_name)?;
        api.set("nvim_buf_line_count", nvim_buf_line_count)?;
        api.set("nvim_buf_get_option", nvim_buf_get_option)?;
        api.set("nvim_buf_set_option", nvim_buf_set_option)?;
        api.set("nvim_buf_add_highlight", nvim_buf_add_highlight)?;
        api.set("nvim_buf_set_extmark", nvim_buf_set_extmark)?;
        api.set("nvim_buf_clear_namespace", nvim_buf_clear_namespace)?;
        api.set("nvim_win_set_option", nvim_win_set_option)?;
        api.set("nvim_win_get_buf", nvim_win_get_buf)?;
        api.set("nvim_win_set_buf", nvim_win_set_buf)?;
        api.set("nvim_win_get_config", nvim_win_get_config)?;
        api.set("nvim_win_get_number", nvim_win_get_number)?;
        api.set("nvim_get_current_win", nvim_get_current_win)?;
        api.set("nvim_set_current_win", nvim_set_current_win)?;
        api.set("nvim_list_wins", nvim_list_wins)?;
        api.set("nvim_create_buf", nvim_create_buf)?;
        api.set("nvim_get_current_buf", nvim_get_current_buf)?;
        api.set("nvim_list_bufs", nvim_list_bufs)?;
        api.set("nvim_open_win", nvim_open_win)?;
        api.set("nvim_create_user_command", nvim_create_user_command)?;
        api.set("nvim_create_autocmd", nvim_create_autocmd)?;
        api.set("nvim_get_mode", nvim_get_mode)?;
        api.set("nvim_create_namespace", nvim_create_namespace)?;
        api.set("nvim_replace_termcodes", nvim_replace_termcodes)?;
        api.set("nvim_clear_autocmds", nvim_clear_autocmds)?;
        api.set("nvim_echo", nvim_echo)?;
        api.set("nvim_list_uis", nvim_list_uis)?;
        api.set("nvim_get_vvar", nvim_get_vvar)?;
        api.set("nvim_set_hl", nvim_set_hl)?;
        api.set("nvim_get_all_options_info", nvim_get_all_options_info)?;

        let nvim_list_runtime_paths = self.lua.create_function(|lua, _: ()| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get("runtimepath") {
                    return Ok(rtp.split(',').map(|s| s.to_string()).collect::<Vec<_>>());
                }
            }
            Ok(Vec::new())
        })?;

        let nvim_get_option = self.lua.create_function(|lua, name: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if name == "columns" { return Ok(mlua::Value::Integer(state.grid.width as i64)); }
                if name == "lines" { return Ok(mlua::Value::Integer(state.grid.height as i64)); }
                if let Some(val) = state.options.get(&name) {
                    match val {
                        crate::nvim::state::OptionValue::Bool(b) => return Ok(mlua::Value::Boolean(*b)),
                        crate::nvim::state::OptionValue::Int(i) => return Ok(mlua::Value::Integer(*i)),
                        crate::nvim::state::OptionValue::String(s) => return Ok(mlua::Value::String(lua.create_string(s)?)),
                    }
                }
            }
            Ok(mlua::Value::Nil)
        })?;

        let nvim_set_option = self.lua.create_function(|lua, (name, value): (String, mlua::Value)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let val = match value {
                    mlua::Value::Boolean(b) => crate::nvim::state::OptionValue::Bool(b),
                    mlua::Value::Integer(i) => crate::nvim::state::OptionValue::Int(i),
                    mlua::Value::String(s) => crate::nvim::state::OptionValue::String(s.to_string_lossy().to_string()),
                    _ => return Ok(()),
                };
                state.options.insert(name.clone(), val.clone());
                if name == "runtimepath" || name == "rtp" { if let crate::nvim::state::OptionValue::String(ref rtp) = val { let _ = Self::sync_package_path(lua, rtp); } }
            }
            Ok(())
        })?;

        api.set("nvim_list_runtime_paths", nvim_list_runtime_paths)?;
        api.set("nvim_get_option", nvim_get_option)?;
        api.set("nvim_set_option", nvim_set_option)?;
        api.set("nvim_get_runtime_file", self.lua.create_function(|_, (_name, _): (String, bool)| { Ok(Vec::<String>::new()) })?)?;

        vim.set("api", api)?;

        let keymap = self.lua.create_table()?;
        keymap.set("set", self.lua.create_function(|_, (_mode, _lhs, _rhs, _opts): (String, String, mlua::Value, mlua::Table)| { Ok(()) })?)?;
        vim.set("keymap", keymap)?;

        let sender_clone = sender.clone();
        vim.set("schedule", self.lua.create_function(move |lua, f: mlua::Function| {
            if let Some(s) = &sender_clone {
                let key = lua.create_registry_value(f)?;
                let _ = s.send(Box::new(move |state| {
                    let env = state.lua_env.borrow();
                    let func: mlua::Function = match env.lua.registry_value(&key) {
                        Ok(f) => f,
                        Err(e) => { eprintln!("vim.schedule error: {}", e); return; }
                    };
                    let _ = func.call::<()>(());
                    let _ = env.lua.remove_registry_value(key);
                }));
            }
            Ok(())
        })?)?;

        let sender_clone_2 = sender.clone();
        vim.set("cmd", self.lua.create_function(move |_, cmd: String| {
            if let Some(s) = &sender_clone_2 {
                let _ = s.send(Box::new(move |state| { let _ = crate::nvim::api::execute_cmd(state, &cmd); }));
            }
            Ok(())
        })?)?;

        let fn_table = self.lua.create_table()?;
        fn_table.set("stdpath", self.lua.create_function(|_, name: String| {
            match name.as_str() {
                "config" => Ok(Self::get_config_dir().to_string_lossy().to_string()),
                "data" => Ok(Self::get_data_dir().to_string_lossy().to_string()),
                "cache" => Ok(dirs::cache_dir().unwrap_or_default().join("rneovim").to_string_lossy().to_string()),
                _ => Ok("".to_string()),
            }
        })?)?;
        fn_table.set("expand", self.lua.create_function(|_, path: String| {
            if path.starts_with('~') { if let Some(home) = dirs::home_dir() { return Ok(path.replace('~', &home.to_string_lossy())); } }
            Ok(path)
        })?)?;
        fn_table.set("system", self.lua.create_function(|lua, args: mlua::Value| {
            let mut cmd_args = Vec::new();
            match args {
                mlua::Value::String(s) => cmd_args.push(s.to_string_lossy().to_string()),
                mlua::Value::Table(t) => { for v in t.sequence_values::<String>() { if let Ok(arg) = v { cmd_args.push(arg); } } }
                _ => {}
            }
            if cmd_args.is_empty() { return Ok("".to_string()); }
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.messages.push(format!("system: running {:?}", cmd_args));
                state.redraw();
            }
            let mut cmd = std::process::Command::new(&cmd_args[0]);
            if cmd_args.len() > 1 { cmd.args(&cmd_args[1..]); }
            match cmd.output() {
                Ok(output) => {
                    let out = String::from_utf8_lossy(&output.stdout).to_string();
                    if !output.status.success() {
                        let err = String::from_utf8_lossy(&output.stderr).to_string();
                        return Ok(format!("{}{}", out, err));
                    }
                    Ok(out)
                }
                Err(e) => Ok(format!("Error: {}", e)),
            }
        })?)?;
        fn_table.set("has", self.lua.create_function(|_, name: String| {
            match name.as_str() {
                "nvim-0.9.0" | "nvim-0.8.0" | "nvim" => Ok(true),
                _ => Ok(false),
            }
        })?)?;
        vim.set("fn", fn_table)?;

        let opt = self.lua.create_table()?;
        let opt_meta = self.lua.create_table()?;
        let lua_ref = self.lua.clone();
        opt_meta.set("__index", self.lua.create_function(move |lua, (_table, name): (mlua::Table, String)| {
            let inner_opt = lua.create_table()?;
            let name_p = name.clone(); let lua_p = lua_ref.clone();
            inner_opt.set("prepend", lua.create_function(move |lua, args: mlua::MultiValue| {
                let value = if args.len() >= 2 { match args.get(1) { Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } }
                else { match args.get(0) { Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } };
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    if name_p == "rtp" || name_p == "runtimepath" {
                        if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get_mut("runtimepath") {
                            *rtp = format!("{},{}", value, rtp); let _ = Self::sync_package_path(&lua_p, rtp);
                        }
                    }
                }
                Ok(())
            })?)?;
            let name_a = name.clone(); let lua_a = lua_ref.clone();
            inner_opt.set("append", lua.create_function(move |lua, args: mlua::MultiValue| {
                let value = if args.len() >= 2 { match args.get(1) { Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } }
                else { match args.get(0) { Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } };
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    if name_a == "rtp" || name_a == "runtimepath" {
                        if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get_mut("runtimepath") {
                            *rtp = format!("{},{}", rtp, value); let _ = Self::sync_package_path(&lua_a, rtp);
                        }
                    }
                }
                Ok(())
            })?)?;
            Ok(inner_opt)
        })?)?;
        let _ = opt.set_metatable(Some(opt_meta));
        vim.set("opt", opt)?;

        let g = self.lua.create_table()?;
        let g_meta = self.lua.create_table()?;
        g_meta.set("__index", self.lua.create_function(|lua, (_table, name): (mlua::Table, String)| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(val) = state.globals.get(&name) { return Ok(mlua::Value::String(lua.create_string(val)?)); }
            }
            Ok(mlua::Value::Nil)
        })?)?;
        g_meta.set("__newindex", self.lua.create_function(|lua, (_table, name, value): (mlua::Table, String, mlua::Value)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let val_str = match value { mlua::Value::String(s) => s.to_string_lossy().to_string(), mlua::Value::Boolean(b) => b.to_string(), mlua::Value::Integer(i) => i.to_string(), _ => "".to_string() };
                state.globals.insert(name, val_str);
            }
            Ok(())
        })?)?;
        let _ = g.set_metatable(Some(g_meta));
        vim.set("g", g)?;

        let v = self.lua.create_table()?;
        let v_meta = self.lua.create_table()?;
        v_meta.set("__index", self.lua.create_function(|_, name: String| {
            match name.as_str() {
                "vim_did_enter" => Ok(mlua::Value::Integer(1)),
                _ => Ok(mlua::Value::Nil),
            }
        })?)?;
        let _ = v.set_metatable(Some(v_meta));
        vim.set("v", v)?;

        let go = self.lua.create_table()?;
        let go_meta = self.lua.create_table()?;
        go_meta.set("__index", self.lua.create_function(|lua, (_table, name): (mlua::Table, String)| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if name == "columns" { return Ok(mlua::Value::Integer(state.grid.width as i64)); }
                if name == "lines" { return Ok(mlua::Value::Integer(state.grid.height as i64)); }
                if let Some(val) = state.options.get(&name) {
                    match val {
                        crate::nvim::state::OptionValue::Bool(b) => return Ok(mlua::Value::Boolean(*b)),
                        crate::nvim::state::OptionValue::Int(i) => return Ok(mlua::Value::Integer(*i)),
                        crate::nvim::state::OptionValue::String(s) => return Ok(mlua::Value::String(lua.create_string(s)?)),
                    }
                }
            }
            Ok(mlua::Value::Nil)
        })?)?;
        go_meta.set("__newindex", self.lua.create_function(|lua, (_table, name, value): (mlua::Table, String, mlua::Value)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let val = match value { mlua::Value::Boolean(b) => crate::nvim::state::OptionValue::Bool(b), mlua::Value::Integer(i) => crate::nvim::state::OptionValue::Int(i), mlua::Value::String(s) => crate::nvim::state::OptionValue::String(s.to_string_lossy().to_string()), _ => return Ok(()) };
                state.options.insert(name, val);
            }
            Ok(())
        })?)?;
        let _ = go.set_metatable(Some(go_meta));
        vim.set("go", go)?;

        let loop_table = self.lua.create_table()?;
        loop_table.set("fs_stat", self.lua.create_function(|lua, path: String| {
            if std::path::Path::new(&path).exists() {
                let table = lua.create_table()?;
                table.set("type", "directory")?;
                Ok(mlua::Value::Table(table))
            } else { Ok(mlua::Value::Nil) }
        })?)?;
        loop_table.set("hrtime", self.lua.create_function(|_, _: ()| { Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64) })?)?;
        loop_table.set("cwd", self.lua.create_function(|_, _: ()| { Ok(std::env::current_dir().unwrap_or_default().to_string_lossy().to_string()) })?)?;
        loop_table.set("os_uname", self.lua.create_function(|lua, _: ()| {
            let table = lua.create_table()?;
            table.set("sysname", std::env::consts::OS)?; table.set("machine", std::env::consts::ARCH)?;
            Ok(table)
        })?)?;
        vim.set("loop", &loop_table)?;
        vim.set("uv", loop_table)?;

        globals.set("vim", vim)?;
        Ok(())
    }

    pub fn execute(&self, code: &str) -> Result<String> {
        match self.lua.load(code).eval::<mlua::Value>() {
            Ok(val) => match val {
                mlua::Value::Nil => Ok("".to_string()),
                mlua::Value::Boolean(b) => Ok(b.to_string()),
                mlua::Value::Integer(i) => Ok(i.to_string()),
                mlua::Value::Number(f) => Ok(f.to_string()),
                mlua::Value::String(s) => Ok(s.to_string_lossy().to_string()),
                _ => Ok(format!("{:?}", val)),
            },
            Err(e) => Err(NvimError::Api(format!("Lua error: {}", e))),
        }
    }

    pub fn execute_file(&self, path: &std::path::Path) -> Result<String> {
        let code = std::fs::read_to_string(path).map_err(|e| NvimError::Api(format!("Failed to read lua file: {}", e)))?;
        self.execute(&code)
    }

    pub fn load_plugins(&self, state: &mut VimState, config_dir: Option<std::path::PathBuf>) {
        self.lua.set_app_data(StateWrapper(state as *mut VimState));
        let rneovim_config = config_dir.unwrap_or_else(Self::get_config_dir);
        let rneovim_data = Self::get_data_dir();
        if !rneovim_data.exists() { let _ = std::fs::create_dir_all(rneovim_data.join("lazy")); }
        if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get("runtimepath") { let _ = Self::sync_package_path(&self.lua, rtp); }
        let init_lua = rneovim_config.join("init.lua");
        if init_lua.exists() {
            if let Err(e) = self.execute_file(&init_lua) {
                let msg = format!("Error executing init.lua: {}", e);
                eprintln!("{}", msg);
                state.messages.push(msg);
                state.redraw();
            }
        }
    }

    pub fn trigger_on_line(&self) {
        if let Some(key) = self.lua.app_data_mut::<RegistryKey>() {
            if let Ok(callbacks) = self.lua.registry_value::<mlua::Table>(&key) {
                if let Ok(on_line) = callbacks.get::<mlua::Function>("on_line") {
                    if let Err(e) = on_line.call::<()>(()) { eprintln!("Lua on_line error: {}", e); }
                }
            }
        }
    }

    pub fn call_user_command(&self, name: &str, args: &str) -> Result<()> {
        if let Some(mut wrapper) = self.lua.app_data_mut::<StateWrapper>() {
            let state = unsafe { &mut *wrapper.0 };
            state.messages.push(format!("DEBUG: call_user_command '{}' with '{}'", name, args));
            if let Some(key) = state.user_commands.get(name) {
                let func: mlua::Function = self.lua.registry_value(key)?;
                let arg_table = self.lua.create_table()?;
                arg_table.set("name", name)?;
                arg_table.set("args", args)?;
                let fargs: Vec<String> = args.split_whitespace().map(|s| s.to_string()).collect();
                arg_table.set("fargs", fargs)?;
                arg_table.set("bang", false)?;
                
                if let Err(e) = func.call::<()>(arg_table) {
                    state.messages.push(format!("Command Error: {}", e));
                }
                state.redraw();
            } else {
                state.messages.push(format!("DEBUG: command '{}' not found in user_commands", name));
            }
        }
        Ok(())
    }
}
