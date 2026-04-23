use crate::nvim::error::{NvimError, Result};
use mlua::{Lua, RegistryKey, Table, Value, MultiValue};
use std::rc::Rc;
use std::cell::RefCell;
use crate::nvim::buffer::Buffer;
use std::sync::mpsc::Sender;
use crate::nvim::state::{VimState, AutoCmd, AutoCmdCallback, AutoCmdEvent, OptionValue};
use crate::nvim::event::event_loop::EventCallback;

pub struct LuaEnv {
    pub(crate) lua: Lua,
}

struct StateWrapper(*mut VimState);

impl LuaEnv {
    pub fn new() -> Self {
        let lua = unsafe {
            Lua::unsafe_new_with(
                mlua::StdLib::ALL_SAFE | mlua::StdLib::DEBUG,
                mlua::LuaOptions::default(),
            )
        };
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
        let package: Table = lua.globals().get("package")?;
        let mut path: String = package.get("path")?;
        for p in rtp.split(',') {
            let lua_path1 = format!("{}/lua/?.lua", p);
            let lua_path2 = format!("{}/lua/?/init.lua", p);
            if !path.contains(&lua_path1) { path = format!("{};{};{}", lua_path1, lua_path2, path); }
        }
        package.set("path", path)?;
        Ok(())
    }

    fn add_missing_tracker(lua: &Lua, table: &Table, prefix: &str) -> mlua::Result<()> {
        let meta = lua.create_table()?;
        let prefix = prefix.to_string();
        meta.set("__index", lua.create_function(move |lua, (_t, k): (Table, Value)| {
            let key_str = match &k {
                Value::String(s) => s.to_string_lossy().to_string(),
                _ => format!("{:?}", k),
            };
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.log(&format!("MISSING API: {}.{}", prefix, key_str));
            }
            Ok(Value::Nil)
        })?)?;
        let _ = table.set_metatable(Some(meta));
        Ok(())
    }

    pub fn register_api(&mut self, _buffers: Vec<Rc<RefCell<Buffer>>>, _sender: Option<Sender<EventCallback<VimState>>>) -> Result<()> {
        let globals = self.lua.globals();
        let vim = self.lua.create_table()?;
        let api = self.lua.create_table()?;

        // print / notify
        let print = self.lua.create_function(|lua, msg: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("LUA print: {}", msg));
                state.messages.push(msg);
                state.redraw();
            }
            Ok(())
        })?;
        globals.set("print", print.clone())?;
        vim.set("notify", print.clone())?;

        // nvim_* APIs
        api.set("nvim_err_writeln", self.lua.create_function(|lua, msg: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("LUA nvim_err_writeln: {}", msg));
                state.messages.push(format!("ERROR: {}", msg));
                state.redraw();
            }
            Ok(())
        })?)?;

        api.set("nvim_command", self.lua.create_function(|lua, cmd: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_command({})", cmd));
                let _ = crate::nvim::api::execute_cmd(state, &cmd);
            }
            Ok(())
        })?)?;

        api.set("nvim_exec", self.lua.create_function(|lua, (cmd, output): (String, bool)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_exec({}, output={})", cmd, output));
                let _ = crate::nvim::api::execute_cmd(state, &cmd);
            }
            Ok("".to_string())
        })?)?;

        api.set("nvim_eval", self.lua.create_function(|lua, expr: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.log(&format!("API nvim_eval({})", expr));
            }
            Ok(Value::Nil)
        })?)?;

        api.set("nvim_get_commands", self.lua.create_function(|lua, _opts: Table| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                let res = lua.create_table()?;
                for name in state.user_commands.keys() {
                    let cmd_info = lua.create_table()?;
                    cmd_info.set("name", name.clone())?;
                    res.set(name.clone(), cmd_info)?;
                }
                return Ok(res);
            }
            Ok(lua.create_table()?)
        })?)?;

        api.set("nvim_buf_get_lines", self.lua.create_function(|lua, (buf_id, start, end, _): (i32, usize, i32, bool)| {
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
        })?)?;

        api.set("nvim_buf_set_lines", self.lua.create_function(|lua, (buf_id, start, end, _strict, lines): (i32, usize, i32, bool, Vec<String>)| {
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
        })?)?;

        api.set("nvim_buf_get_name", self.lua.create_function(|lua, buf_id: i32| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    return Ok(buf_rc.borrow().name().unwrap_or("").to_string());
                }
            }
            Ok("".to_string())
        })?)?;

        api.set("nvim_list_bufs", self.lua.create_function(|lua, _: ()| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                return Ok(state.buffers.iter().map(|b| b.borrow().id()).collect::<Vec<_>>());
            }
            Ok(Vec::new())
        })?)?;

        api.set("nvim_buf_get_option", self.lua.create_function(|lua, (_buf_id, name): (i32, String)| {
            match name.as_str() {
                "filetype" => Ok(Value::String(lua.create_string("text")?)),
                "buftype" => Ok(Value::String(lua.create_string("")?)),
                "modifiable" => Ok(Value::Boolean(true)),
                _ => Ok(Value::Nil),
            }
        })?)?;

        api.set("nvim_create_buf", self.lua.create_function(|lua, (listed, scratch): (bool, bool)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let buf = Rc::new(RefCell::new(Buffer::new()));
                let id = buf.borrow().id();
                state.log(&format!("API nvim_create_buf(listed={}, scratch={}) -> {}", listed, scratch, id));
                state.buffers.push(buf);
                return Ok(id);
            }
            Ok(0)
        })?)?;

        api.set("nvim_open_win", self.lua.create_function(|lua, (buf_id, enter, config): (i32, bool, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    let row = config.get::<Value>("row").map(|v| match v { Value::Number(f) => f.round() as usize, Value::Integer(i) => i as usize, _ => 0 }).unwrap_or(0);
                    let col = config.get::<Value>("col").map(|v| match v { Value::Number(f) => f.round() as usize, Value::Integer(i) => i as usize, _ => 0 }).unwrap_or(0);
                    let width = config.get::<Value>("width").map(|v| match v { Value::Number(f) => f.round() as usize, Value::Integer(i) => i as usize, _ => 40 }).unwrap_or(40);
                    let height = config.get::<Value>("height").map(|v| match v { Value::Number(f) => f.round() as usize, Value::Integer(i) => i as usize, _ => 10 }).unwrap_or(10);
                    state.log(&format!("API nvim_open_win(buf={}, enter={}, row={}, col={}, w={}, h={})", buf_id, enter, row, col, width, height));
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
        })?)?;

        api.set("nvim_win_get_buf", self.lua.create_function(|lua, win_id: i32| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                for win in &state.current_tabpage().windows { 
                    if win.id() == win_id { return Ok(win.buffer().borrow().id()); } 
                }
            }
            Ok(0)
        })?)?;

        api.set("nvim_set_current_win", self.lua.create_function(|lua, win_id: i32| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let tab = state.current_tabpage_mut();
                if let Some(idx) = tab.windows.iter().position(|w| w.id() == win_id) {
                    tab.current_win_idx = idx;
                    state.redraw();
                }
            }
            Ok(())
        })?)?;

        api.set("nvim_create_user_command", self.lua.create_function(|lua, (name, callback, _opts): (String, Value, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_create_user_command(name={})", name));
                if let Value::Function(f) = callback {
                    let key = lua.create_registry_value(f)?;
                    state.user_commands.insert(name, key);
                }
            }
            Ok(())
        })?)?;

        api.set("nvim_create_autocmd", self.lua.create_function(|lua, (event_val, opts): (Value, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let events = match event_val {
                    Value::String(s) => vec![s.to_string_lossy().to_string()],
                    Value::Table(t) => t.sequence_values::<String>().filter_map(|v| v.ok()).collect(),
                    _ => vec![],
                };
                state.log(&format!("API nvim_create_autocmd(events={:?})", events));
                let callback_opt = match opts.get::<Value>("callback") {
                    Ok(Value::Function(f)) => Some(AutoCmdCallback::Lua(Rc::new(lua.create_registry_value(f)?))),
                    _ => opts.get::<String>("command").ok().map(AutoCmdCallback::Command),
                };
                if let Some(callback) = callback_opt {
                    for ev_str in events {
                        let ev_type = match ev_str.as_str() {
                            "VimEnter" => AutoCmdEvent::VimEnter, "BufReadPost" => AutoCmdEvent::BufReadPost,
                            "BufWritePost" => AutoCmdEvent::BufWritePost, "BufEnter" => AutoCmdEvent::BufEnter,
                            _ => continue,
                        };
                        state.autocmds.entry(ev_type).or_insert_with(Vec::new).push(AutoCmd { callback: callback.clone() });
                    }
                }
            }
            Ok(1)
        })?)?;

        api.set("nvim_get_runtime_file", self.lua.create_function(|_lua, (_pattern, _): (String, bool)| { 
            Ok(Vec::<String>::new()) 
        })?)?;

        api.set("nvim_set_var", self.lua.create_function(|lua, (name, val): (String, Value)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let s = match val { Value::String(s) => s.to_string_lossy().to_string(), Value::Boolean(b) => b.to_string(), Value::Integer(i) => i.to_string(), _ => format!("{:?}", val) };
                state.log(&format!("API nvim_set_var({}, {})", name, s));
                state.globals.insert(name, s);
            }
            Ok(())
        })?)?;

        api.set("nvim_get_mode", self.lua.create_function(|lua, _: ()| {
            let res = lua.create_table()?;
            res.set("mode", "n")?; res.set("blocking", false)?;
            Ok(res)
        })?)?;

        api.set("nvim_create_augroup", self.lua.create_function(|lua, (name, _opts): (String, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_create_augroup(name={})", name));
            }
            Ok(1)
        })?)?;

        api.set("nvim_del_augroup_by_id", self.lua.create_function(|_, _: i32| { Ok(()) })?)?;
        api.set("nvim_del_augroup_by_name", self.lua.create_function(|_, _: String| { Ok(()) })?)?;

        api.set("nvim_get_autocmds", self.lua.create_function(|lua, _opts: Table| {
            Ok(lua.create_table()?)
        })?)?;

        api.set("nvim_create_namespace", self.lua.create_function(|lua, name: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_create_namespace(name={})", name));
            }
            Ok(1)
        })?)?;

        api.set("nvim_buf_add_highlight", self.lua.create_function(|lua, (buf, ns, hl, line, _s, _e): (i32, i32, String, usize, usize, usize)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_buf_add_highlight(buf={}, ns={}, hl={}, line={})", buf, ns, hl, line));
            }
            Ok(0)
        })?)?;

        api.set("nvim_buf_set_extmark", self.lua.create_function(|lua, (buf, ns, line, col, _opts): (i32, i32, usize, usize, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_buf_set_extmark(buf={}, ns={}, line={}, col={})", buf, ns, line, col));
            }
            Ok(1)
        })?)?;

        api.set("nvim_buf_clear_namespace", self.lua.create_function(|_, _: (i32, i32, i32, i32)| { Ok(()) })?)?;
        api.set("nvim_replace_termcodes", self.lua.create_function(|_, (str, _, _, _): (String, bool, bool, bool)| { Ok(str) })?)?;
        api.set("nvim_clear_autocmds", self.lua.create_function(|lua, opts: Table| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_clear_autocmds(opts={:?})", opts));
            }
            Ok(())
        })?)?;

        api.set("nvim_echo", self.lua.create_function(|lua, (chunks, _history, _opts): (Vec<Table>, bool, Table)| {
            let mut msg = String::new();
            for chunk in chunks { if let Ok(text) = chunk.get::<String>(1) { msg.push_str(&text); } }
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_echo: {}", msg));
                state.messages.push(msg);
                state.redraw();
            }
            Ok(())
        })?)?;

        api.set("nvim_list_uis", self.lua.create_function(|lua, _: ()| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                let ui = lua.create_table()?;
                ui.set("width", state.grid.width)?;
                ui.set("height", state.grid.height)?;
                return Ok(vec![ui]);
            }
            Ok(Vec::new())
        })?)?;

        api.set("nvim_get_all_options_info", self.lua.create_function(|lua, _: ()| {
            Ok(lua.create_table()?)
        })?)?;

        let _ = Self::add_missing_tracker(&self.lua, &api, "vim.api");
        vim.set("api", api)?;

        // vim.fn
        let fn_table = self.lua.create_table()?;
        fn_table.set("stdpath", self.lua.create_function(|_, name: String| {
            let data_dir = Self::get_data_dir();
            Ok(match name.as_str() {
                "config" => Self::get_config_dir().to_string_lossy().to_string(),
                "data" => data_dir.to_string_lossy().to_string(),
                "cache" => dirs::cache_dir().unwrap_or_else(|| data_dir.join("cache")).join("rneovim").to_string_lossy().to_string(),
                "state" => dirs::state_dir().unwrap_or_else(|| data_dir.join("state")).join("rneovim").to_string_lossy().to_string(),
                "run" => std::env::var("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| data_dir.join("run")).join("rneovim").to_string_lossy().to_string(),
                _ => data_dir.to_string_lossy().to_string(),
            })
        })?)?;
        fn_table.set("expand", self.lua.create_function(|_, path: String| {
            let mut res = path.clone();
            if path.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    res = path.replace('~', &home.to_string_lossy());
                }
            }
            // 環境変数の展開 (簡易版)
            if path.contains('$') {
                for (k, v) in std::env::vars() {
                    let key = format!("${}", k);
                    if res.contains(&key) {
                        res = res.replace(&key, &v);
                    }
                }
            }
            Ok(res)
        })?)?;
        fn_table.set("fnamemodify", self.lua.create_function(|_, (path_opt, modifier): (Option<String>, String)| {
            let path = path_opt.unwrap_or_default();
            let mut res = std::path::PathBuf::from(&path);
            
            // 修飾子を個別に処理する (:p, :h, :t 等)
            let mods: Vec<&str> = modifier.split(':').filter(|s| !s.is_empty()).collect();
            for m in mods {
                if m == "p" {
                    if path.starts_with('~') { if let Some(home) = dirs::home_dir() { res = home.join(&path[2..]); } }
                    res = std::fs::canonicalize(&res).unwrap_or(res);
                } else if m == "h" {
                    if let Some(parent) = res.parent() { res = parent.to_path_buf(); }
                } else if m == "t" {
                    if let Some(tail) = res.file_name() { res = std::path::PathBuf::from(tail); }
                } else if m == "e" {
                    if let Some(ext) = res.extension() { res = std::path::PathBuf::from(ext); }
                } else if m == "r" {
                    if let Some(stem) = res.file_stem() { 
                        if let Some(parent) = res.parent() { res = parent.join(stem); }
                        else { res = std::path::PathBuf::from(stem); }
                    }
                }
            }
            Ok(res.to_string_lossy().to_string())
        })?)?;
        fn_table.set("has", self.lua.create_function(|lua, name: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.log(&format!("API vim.fn.has({})", name));
            }
            Ok(match name.as_str() { "nvim-0.9.0" | "nvim-0.8.0" | "nvim" | "unix" | "mac" => 1, _ => 0 })
        })?)?;
        fn_table.set("executable", self.lua.create_function(|_, name: String| {
            Ok(if std::env::var("PATH").unwrap_or_default().split(':').any(|p| std::path::Path::new(p).join(&name).exists()) { 1 } else { 0 })
        })?)?;
        fn_table.set("argc", self.lua.create_function(|_, _: ()| { Ok(std::env::args().len().saturating_sub(1)) })?)?;
        fn_table.set("argv", self.lua.create_function(|lua, idx: Option<usize>| {
            let args: Vec<String> = std::env::args().skip(1).collect();
            if let Some(i) = idx { return Ok(Value::String(lua.create_string(&args.get(i).cloned().unwrap_or_default())?)); }
            let t = lua.create_table()?;
            for (i, a) in args.iter().enumerate() { t.set(i + 1, lua.create_string(a)?)?; }
            Ok(Value::Table(t))
        })?)?;
        fn_table.set("exists", self.lua.create_function(|lua, name: String| { 
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if name.starts_with(':') {
                    if state.user_commands.contains_key(&name[1..]) { return Ok(2); }
                    return Ok(0);
                }
            }
            Ok(0)
        })?)?;
        fn_table.set("filereadable", self.lua.create_function(|_, path: String| { Ok(if std::path::Path::new(&path).is_file() { 1 } else { 0 }) })?)?;
        fn_table.set("isdirectory", self.lua.create_function(|_, path: String| { Ok(if std::path::Path::new(&path).is_dir() { 1 } else { 0 }) })?)?;
        fn_table.set("mkdir", self.lua.create_function(|_, (path, _p): (String, Option<String>)| {
            let _ = std::fs::create_dir_all(path);
            Ok(1)
        })?)?;
        let _ = Self::add_missing_tracker(&self.lua, &fn_table, "vim.fn");
        vim.set("fn", fn_table)?;

        // vim.loop
        let loop_table = self.lua.create_table()?;
        loop_table.set("fs_stat", self.lua.create_function(|lua, path: String| {
            if let Ok(meta) = std::fs::metadata(&path) {
                let table = lua.create_table()?;
                table.set("type", if meta.is_dir() { "directory" } else { "file" })?;
                table.set("size", meta.len())?;
                if let Ok(mtime) = meta.modified() {
                    let duration = mtime.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                    let mtime_table = lua.create_table()?;
                    mtime_table.set("sec", duration.as_secs())?;
                    mtime_table.set("nsec", duration.subsec_nanos())?;
                    table.set("mtime", mtime_table)?;
                }
                Ok(Value::Table(table))
            } else { Ok(Value::Nil) }
        })?)?;
        loop_table.set("fs_mkdir", self.lua.create_function(|_, (path, _mode): (String, i32)| {
            let _ = std::fs::create_dir_all(path);
            Ok(true)
        })?)?;
        loop_table.set("fs_open", self.lua.create_function(|lua, (path, flags, _mode): (String, String, i32)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let mut opts = std::fs::OpenOptions::new();
                match flags.as_str() {
                    "r" => { opts.read(true); }
                    "w" => { opts.write(true).create(true).truncate(true); }
                    "a" => { opts.append(true).create(true); }
                    "w+" => { opts.read(true).write(true).create(true).truncate(true); }
                    _ => { opts.read(true); }
                }
                if let Ok(file) = opts.open(&path) {
                    let fd = state.next_fd;
                    state.open_files.insert(fd, file);
                    state.next_fd += 1;
                    return Ok(Value::Integer(fd as i64));
                }
            }
            Ok(Value::Nil)
        })?)?;
        loop_table.set("fs_close", self.lua.create_function(|lua, fd: i32| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.open_files.remove(&fd);
            }
            Ok(true)
        })?)?;
        loop_table.set("fs_fstat", self.lua.create_function(|lua, fd: i32| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(file) = state.open_files.get(&fd) {
                    if let Ok(meta) = file.metadata() {
                        let table = lua.create_table()?;
                        table.set("type", if meta.is_dir() { "directory" } else { "file" })?;
                        table.set("size", meta.len())?;
                        if let Ok(mtime) = meta.modified() {
                            let duration = mtime.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                            let mtime_table = lua.create_table()?;
                            mtime_table.set("sec", duration.as_secs())?;
                            mtime_table.set("nsec", duration.subsec_nanos())?;
                            table.set("mtime", mtime_table)?;
                        }
                        return Ok(Value::Table(table));
                    }
                }
            }
            Ok(Value::Nil)
        })?)?;
        loop_table.set("fs_access", self.lua.create_function(|_, (path, _mode): (String, Value)| {
            Ok(std::path::Path::new(&path).exists())
        })?)?;
        loop_table.set("fs_utime", self.lua.create_function(|_, (path, _atime, _mtime): (String, f64, f64)| {
            // 現在はスタブ
            Ok(true)
        })?)?;
        loop_table.set("fs_read", self.lua.create_function(|lua, (fd, size, offset): (i32, usize, i64)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(file) = state.open_files.get_mut(&fd) {
                    use std::io::{Read, Seek, SeekFrom};
                    let mut buf = vec![0u8; size];
                    if offset >= 0 { let _ = file.seek(SeekFrom::Start(offset as u64)); }
                    if let Ok(n) = file.read(&mut buf) {
                        return Ok(Some(lua.create_string(&buf[..n])?));
                    }
                }
            }
            Ok(None)
        })?)?;
        loop_table.set("fs_write", self.lua.create_function(|lua, (fd, data, offset): (i32, Value, i64)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(file) = state.open_files.get_mut(&fd) {
                    use std::io::{Write, Seek, SeekFrom};
                    let bytes = match data {
                        Value::String(s) => s.as_bytes().to_vec(),
                        _ => return Ok(Value::Integer(0)),
                    };
                    if offset >= 0 { let _ = file.seek(SeekFrom::Start(offset as u64)); }
                    if let Ok(_) = file.write_all(&bytes) {
                        return Ok(Value::Integer(bytes.len() as i64));
                    }
                }
            }
            Ok(Value::Integer(0))
        })?)?;
        loop_table.set("fs_realpath", self.lua.create_function(|_, path: String| {
            Ok(std::fs::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).ok())
        })?)?;
        loop_table.set("fs_scandir", self.lua.create_function(|lua, path: String| {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let items: Vec<(String, String)> = entries.filter_map(|e| e.ok()).map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let file_type = if e.path().is_dir() { "directory".to_string() } else { "file".to_string() };
                    (name, file_type)
                }).collect();
                let idx = std::cell::Cell::new(0);
                return Ok(Some(lua.create_function(move |lua, _: ()| {
                    let i = idx.get();
                    if i < items.len() {
                        idx.set(i + 1);
                        return Ok(MultiValue::from_iter(vec![
                            Value::String(lua.create_string(&items[i].0)?),
                            Value::String(lua.create_string(&items[i].1)?),
                        ]));
                    }
                    Ok(MultiValue::new())
                })?));
            }
            Ok(None)
        })?)?;
        loop_table.set("fs_scandir_next", self.lua.create_function(|_, handle: mlua::Function| {
            handle.call::<MultiValue>(())
        })?)?;
        loop_table.set("new_check", self.lua.create_function(|lua, _: ()| {
            let check = lua.create_table()?;
            check.set("start", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
            check.set("stop", lua.create_function(|_, _: ()| Ok(()))?)?;
            Ok(check)
        })?)?;
        loop_table.set("hrtime", self.lua.create_function(|_, _: ()| { Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64) })?)?;
        loop_table.set("cwd", self.lua.create_function(|_, _: ()| { Ok(std::env::current_dir().unwrap_or_default().to_string_lossy().to_string()) })?)?;
        loop_table.set("os_uname", self.lua.create_function(|lua, _: ()| {
            let table = lua.create_table()?;
            table.set("sysname", std::env::consts::OS)?; table.set("machine", std::env::consts::ARCH)?;
            Ok(table)
        })?)?;
        loop_table.set("os_homedir", self.lua.create_function(|_, _: ()| {
            Ok(dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
        })?)?;
        loop_table.set("os_environ", self.lua.create_function(|lua, _: ()| {
            let res = lua.create_table()?;
            for (k, v) in std::env::vars() { res.set(k, v)?; }
            Ok(res)
        })?)?;
        let _ = Self::add_missing_tracker(&self.lua, &loop_table, "vim.loop");
        vim.set("loop", &loop_table)?;
        vim.set("uv", loop_table)?;

        // vim.opt
        let opt = self.lua.create_table()?;
        let opt_meta = self.lua.create_table()?;
        let lua_ref = self.lua.clone();
        opt_meta.set("__index", self.lua.create_function(move |lua, (_table, k): (Table, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            let inner_opt = lua.create_table()?;
            let name_p = name.clone(); let lua_p = lua_ref.clone();
            inner_opt.set("prepend", lua.create_function(move |lua, args: MultiValue| {
                let value = if args.len() >= 2 { match args.get(1) { Some(Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } }
                else { match args.get(0) { Some(Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } };
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    state.log(&format!("API vim.opt.{}:prepend({})", name_p, value));
                    if name_p == "rtp" || name_p == "runtimepath" {
                        if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get_mut("runtimepath") {
                            *rtp = format!("{},{}", value, rtp); let _ = Self::sync_package_path(&lua_p, rtp);
                        }
                    }
                }
                Ok(())
            })?)?;
            let name_a = name.clone(); let lua_a = lua_ref.clone();
            inner_opt.set("append", lua.create_function(move |lua, args: MultiValue| {
                let value = if args.len() >= 2 { match args.get(1) { Some(Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } }
                else { match args.get(0) { Some(Value::String(s)) => s.to_string_lossy().to_string(), _ => return Ok(()) } };
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    state.log(&format!("API vim.opt.{}:append({})", name_a, value));
                    if name_a == "rtp" || name_a == "runtimepath" {
                        if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get_mut("runtimepath") {
                            *rtp = format!("{},{}", rtp, value); let _ = Self::sync_package_path(&lua_a, rtp);
                        }
                    }
                }
                Ok(())
            })?)?;
            Ok(Value::Table(inner_opt))
        })?)?;
        
        let lua_ref_new = self.lua.clone();
        opt_meta.set("__newindex", self.lua.create_function(move |lua, (_table, k, v): (Table, Value, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(()) };
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API vim.opt.{} = {:?}", name, v));
                
                let opt_val = match v {
                    Value::Boolean(b) => OptionValue::Bool(b),
                    Value::Integer(i) => OptionValue::Int(i),
                    Value::String(s) => OptionValue::String(s.to_string_lossy().to_string()),
                    Value::Table(t) => {
                        // テーブルの場合は要素をカンマ区切りで結合（rtp等）
                        let mut vals = Vec::new();
                        for val in t.sequence_values::<Value>() {
                            if let Ok(Value::String(s)) = val { vals.push(s.to_string_lossy().to_string()); }
                        }
                        OptionValue::String(vals.join(","))
                    }
                    _ => return Ok(()),
                };

                state.options.insert(name.clone(), opt_val.clone());
                if name == "rtp" || name == "runtimepath" {
                    if let OptionValue::String(rtp) = opt_val { let _ = Self::sync_package_path(&lua_ref_new, &rtp); }
                }
            }
            Ok(())
        })?)?;
        
        let _ = opt.set_metatable(Some(opt_meta));
        vim.set("opt", opt)?;

        // vim.cmd
        let cmd_fn = self.lua.create_function(|lua, cmd: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API vim.cmd({})", cmd));
                let _ = crate::nvim::api::execute_cmd(state, &cmd);
            }
            Ok(())
        })?;
        let cmd_table = self.lua.create_table()?;
        let cmd_meta = self.lua.create_table()?;
        cmd_meta.set("__call", self.lua.create_function(move |_lua, (_t, cmd): (Table, String)| { cmd_fn.call::<()>(cmd) })?)?;
        cmd_meta.set("__index", self.lua.create_function(move |lua, (_t, k): (Table, Value)| {
            let key_str = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            Ok(Value::Function(lua.create_function(move |lua, args: MultiValue| {
                let arg_strs: Vec<String> = args.iter().map(|v| match v { Value::String(s) => s.to_string_lossy().to_string(), _ => format!("{:?}", v) }).collect();
                let cmd = format!("{} {}", key_str, arg_strs.join(" "));
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    state.log(&format!("API vim.cmd.{} called as function -> {}", key_str, cmd));
                    let _ = crate::nvim::api::execute_cmd(state, &cmd);
                }
                Ok(())
            })?))
        })?)?;
        let _ = cmd_table.set_metatable(Some(cmd_meta));
        vim.set("cmd", cmd_table)?;

        vim.set("version", self.lua.create_function(|lua, _: ()| {
            let t = lua.create_table()?;
            t.set("major", 0)?; t.set("minor", 9)?; t.set("patch", 0)?;
            Ok(t)
        })?)?;

        let v = self.lua.create_table()?;
        let v_meta = self.lua.create_table()?;
        v_meta.set("__index", self.lua.create_function(|lua, k: Value| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            let did_enter = if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.vim_did_enter
            } else { false };
            match name.as_str() { "vim_did_enter" => Ok(Value::Integer(if did_enter { 1 } else { 0 })), _ => Ok(Value::Nil) }
        })?)?;
        let _ = v.set_metatable(Some(v_meta));
        vim.set("v", v)?;

        // vim.env
        let env_table = self.lua.create_table()?;
        let env_meta = self.lua.create_table()?;
        env_meta.set("__index", self.lua.create_function(|lua, k: Value| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            Ok(match std::env::var(name) {
                Ok(v) => Value::String(lua.create_string(&v)?),
                Err(_) => Value::Nil,
            })
        })?)?;
        let _ = env_table.set_metatable(Some(env_meta));
        vim.set("env", env_table)?;

        let g = self.lua.create_table()?;
        let g_meta = self.lua.create_table()?;
        g_meta.set("__index", self.lua.create_function(|lua, (_t, k): (Table, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(val) = state.globals.get(&name) { return Ok(Value::String(lua.create_string(val)?)); }
            }
            Ok(Value::Nil)
        })?)?;
        g_meta.set("__newindex", self.lua.create_function(|lua, (_t, k, val): (Table, Value, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(()) };
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let s = match val { Value::String(s) => s.to_string_lossy().to_string(), Value::Boolean(b) => b.to_string(), Value::Integer(i) => i.to_string(), _ => format!("{:?}", val) };
                state.log(&format!("API vim.g.{} = {}", name, s));
                state.globals.insert(name, s);
            }
            Ok(())
        })?)?;
        let _ = g.set_metatable(Some(g_meta));
        vim.set("g", g)?;

        let go = self.lua.create_table()?;
        let go_meta = self.lua.create_table()?;
        go_meta.set("__index", self.lua.create_function(|lua, (_t, k): (Table, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if name == "columns" { return Ok(Value::Integer(state.grid.width as i64)); }
                if name == "lines" { return Ok(Value::Integer(state.grid.height as i64)); }
                if let Some(val) = state.options.get(&name) {
                    match val {
                        OptionValue::Bool(b) => return Ok(Value::Boolean(*b)),
                        OptionValue::Int(i) => return Ok(Value::Integer(*i)),
                        OptionValue::String(s) => return Ok(Value::String(lua.create_string(s)?)),
                    }
                }
            }
            Ok(Value::Nil)
        })?)?;
        go_meta.set("__newindex", self.lua.create_function(|lua, (_t, k, val): (Table, Value, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(()) };
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let v = match val {
                    Value::Boolean(b) => OptionValue::Bool(b),
                    Value::Integer(i) => OptionValue::Int(i),
                    Value::String(s) => OptionValue::String(s.to_string_lossy().to_string()),
                    _ => return Ok(()),
                };
                state.options.insert(name, v);
            }
            Ok(())
        })?)?;
        let _ = go.set_metatable(Some(go_meta));
        vim.set("go", go)?;

        let log = self.lua.create_table()?;
        let levels = self.lua.create_table()?;
        levels.set("TRACE", 0)?; levels.set("DEBUG", 1)?; levels.set("INFO", 2)?; levels.set("WARN", 3)?; levels.set("ERROR", 4)?;
        log.set("levels", levels)?;
        vim.set("log", log)?;

        // HELPER FUNCTIONS
        vim.set("tbl_extend", self.lua.create_function(|lua, (_behavior, args): (String, MultiValue)| {
            let res = lua.create_table()?;
            for val in args { if let Value::Table(t) = val { for pair in t.pairs::<Value, Value>() { let (k, v) = pair?; res.set(k, v)?; } } }
            Ok(res)
        })?)?;
        vim.set("tbl_deep_extend", vim.get::<Value>("tbl_extend")?)?;

        vim.set("tbl_count", self.lua.create_function(|_, t: Table| {
            Ok(t.pairs::<Value, Value>().count())
        })?)?;

        vim.set("tbl_get", self.lua.create_function(|_, (t, keys): (Table, MultiValue)| {
            let mut current = Value::Table(t);
            for key in keys {
                if let Value::Table(tbl) = current {
                    current = tbl.get(key)?;
                    if current == Value::Nil { return Ok(Value::Nil); }
                } else {
                    return Ok(Value::Nil);
                }
            }
            Ok(current)
        })?)?;

        vim.set("tbl_contains", self.lua.create_function(|_, (t, value): (Table, Value)| {
            for pair in t.pairs::<Value, Value>() {
                let (_, v) = pair?;
                if v == value { return Ok(true); }
            }
            Ok(false)
        })?)?;

        vim.set("tbl_keys", self.lua.create_function(|lua, t: Table| {
            let keys = lua.create_table()?;
            for (i, pair) in t.pairs::<Value, Value>().enumerate() {
                let (k, _) = pair?;
                keys.set(i + 1, k)?;
            }
            Ok(keys)
        })?)?;

        vim.set("tbl_values", self.lua.create_function(|lua, t: Table| {
            let values = lua.create_table()?;
            for (i, pair) in t.pairs::<Value, Value>().enumerate() {
                let (_, v) = pair?;
                values.set(i + 1, v)?;
            }
            Ok(values)
        })?)?;

        vim.set("list_extend", self.lua.create_function(|_, (dst, src, start, end): (Table, Table, Option<usize>, Option<usize>)| {
            let start = start.unwrap_or(1);
            let len = src.len()? as usize;
            let end = end.unwrap_or(len);
            let mut dst_idx = dst.len()? + 1;
            for i in start..=end {
                dst.set(dst_idx, src.get::<Value>(i)?)?;
                dst_idx += 1;
            }
            Ok(dst)
        })?)?;

        vim.set("is_list", self.lua.create_function(|_, v: Value| {
            if let Value::Table(t) = v {
                let len = t.len()?;
                if len == 0 {
                    // 空テーブルは通常リストとして扱われる（または曖昧）
                    return Ok(true);
                }
                // 簡易的にインデックス 1 が存在するかで判定
                return Ok(t.get::<Value>(1)? != Value::Nil);
            }
            Ok(false)
        })?)?;

        vim.set("list_slice", self.lua.create_function(|lua, (t, start, end): (Table, Option<usize>, Option<usize>)| {
            let start = start.unwrap_or(1);
            let len = t.len()? as usize;
            let end = end.unwrap_or(len);
            let res = lua.create_table()?;
            for i in start..=end {
                res.set(i - start + 1, t.get::<Value>(i)?)?;
            }
            Ok(res)
        })?)?;

        vim.set("tbl_map", self.lua.create_function(|lua, (f, t): (mlua::Function, Table)| {
            let res = lua.create_table()?;
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                res.set(k.clone(), f.call::<Value>(v)?)?;
            }
            Ok(res)
        })?)?;

        vim.set("tbl_filter", self.lua.create_function(|lua, (f, t): (mlua::Function, Table)| {
            let res = lua.create_table()?;
            let mut idx = 1;
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                if f.call::<bool>(v.clone())? {
                    if let Value::Integer(_) = k {
                        res.set(idx, v)?;
                        idx += 1;
                    } else {
                        res.set(k, v)?;
                    }
                }
            }
            Ok(res)
        })?)?;

        vim.set("split", self.lua.create_function(|lua, (s, sep): (String, String)| {
            let res = lua.create_table()?;
            for (i, part) in s.split(&sep).enumerate() { res.set(i + 1, part)?; }
            Ok(res)
        })?)?;
        vim.set("trim", self.lua.create_function(|_, s: String| { Ok(s.trim().to_string()) })?)?;
        vim.set("inspect", self.lua.create_function(|_, v: Value| { Ok(format!("{:?}", v)) })?)?;

        vim.set("deepcopy", self.lua.create_function(|lua, v: Value| {
            fn deepcopy_val(lua: &Lua, v: Value) -> mlua::Result<Value> {
                match v {
                    Value::Table(t) => {
                        let res = lua.create_table()?;
                        for pair in t.pairs::<Value, Value>() {
                            let (k, v) = pair?;
                            res.set(deepcopy_val(lua, k)?, deepcopy_val(lua, v)?)?;
                        }
                        Ok(Value::Table(res))
                    }
                    _ => Ok(v),
                }
            }
            deepcopy_val(lua, v)
        })?)?;

        vim.set("in_fast_event", self.lua.create_function(|_, _: ()| { Ok(false) })?)?;
        vim.set("is_thread", self.lua.create_function(|_, _: ()| { Ok(false) })?)?;

        // vim.health
        let health = self.lua.create_table()?;
        health.set("start", self.lua.create_function(|_, _name: String| { Ok(()) })?)?;
        health.set("info", self.lua.create_function(|_, _msg: String| { Ok(()) })?)?;
        health.set("ok", self.lua.create_function(|_, _msg: String| { Ok(()) })?)?;
        health.set("warn", self.lua.create_function(|_, _msg: String| { Ok(()) })?)?;
        health.set("error", self.lua.create_function(|_, _msg: String| { Ok(()) })?)?;
        vim.set("health", health)?;

        // jit table
        let jit = self.lua.create_table()?;
        jit.set("version", "LuaJIT 2.1.0-beta3")?;
        jit.set("os", match std::env::consts::OS {
            "macos" => "OSX", "windows" => "Windows", "linux" => "Linux", _ => std::env::consts::OS,
        })?;
        jit.set("arch", match std::env::consts::ARCH {
            "x86_64" => "x64", "aarch64" => "arm64", _ => std::env::consts::ARCH,
        })?;
        globals.set("jit", jit)?;

        // mock ffi
        let package: Table = globals.get("package")?;
        let preload: Table = package.get("preload")?;
        preload.set("ffi", self.lua.create_function(|lua, _: ()| {
            let ffi = lua.create_table()?;
            ffi.set("os", std::env::consts::OS)?;
            ffi.set("arch", std::env::consts::ARCH)?;
            Ok(ffi)
        })?)?;

        // vim.schedule
        vim.set("schedule", self.lua.create_function(|lua, f: mlua::Function| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(sender) = &state.sender {
                    let key = lua.create_registry_value(f)?;
                    let _ = sender.send(Box::new(move |state| {
                        let _ = state.lua_env.borrow().execute_callback(&key);
                    }));
                }
            }
            Ok(())
        })?)?;

        // vim.loader (disabled to investigate leak)
        // let loader = self.lua.create_table()?;
        // loader.set("enable", self.lua.create_function(|_, _: ()| Ok(()))?)?;
        // vim.set("loader", loader)?;

        // vim._load_package
        vim.set("_load_package", self.lua.create_function(|_, _: String| Ok(()))?)?;

        // EXPOSE VIM TO GLOBALS
        globals.set("vim", vim.clone())?;

        // Add missing API tracker last to vim itself
        let _ = Self::add_missing_tracker(&self.lua, &vim, "vim");

        Ok(())
    }

    pub fn execute(&self, code: &str) -> Result<String> {
        match self.lua.load(code).eval::<Value>() {
            Ok(val) => match val {
                Value::Nil => Ok("".to_string()), Value::Boolean(b) => Ok(b.to_string()),
                Value::Integer(i) => Ok(i.to_string()), Value::Number(f) => Ok(f.to_string()),
                Value::String(s) => Ok(s.to_string_lossy().to_string()), _ => Ok(format!("{:?}", val)),
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

    pub fn execute_callback(&self, key: &RegistryKey) -> Result<()> {
        let func: mlua::Function = self.lua.registry_value(key).map_err(|e| NvimError::Api(format!("Failed to get callback: {}", e)))?;
        func.call::<()>(()).map_err(|e| NvimError::Api(format!("Callback error: {}", e)))
    }

    pub fn trigger_on_line(&self) {
        if let Some(key) = self.lua.app_data_mut::<RegistryKey>() {
            if let Ok(callbacks) = self.lua.registry_value::<Table>(&key) {
                if let Ok(on_line) = callbacks.get::<mlua::Function>("on_line") {
                    if let Err(e) = on_line.call::<()>(()) { eprintln!("Lua on_line error: {}", e); }
                }
            }
        }
    }

    pub fn call_user_command(&self, name: &str, args: &str) -> Result<()> {
        if let Some(mut wrapper) = self.lua.app_data_mut::<StateWrapper>() {
            let state = unsafe { &mut *wrapper.0 };
            state.log(&format!("COMMAND START: '{}' with '{}'", name, args));
            if let Some(key) = state.user_commands.get(name) {
                let func: mlua::Function = self.lua.registry_value(key)?;
                let arg_table = self.lua.create_table()?;
                arg_table.set("name", name)?;
                arg_table.set("args", args)?;
                let fargs: Vec<String> = args.split_whitespace().map(|s| s.to_string()).collect();
                arg_table.set("fargs", fargs)?;
                arg_table.set("bang", false)?;
                
                if let Err(e) = func.call::<()>(arg_table) {
                    let err_msg = format!("Command Error: {}", e);
                    state.log(&err_msg);
                    state.messages.push(err_msg);
                }
                state.log(&format!("COMMAND FINISHED: '{}'", name));
                state.redraw();
            } else {
                state.log(&format!("COMMAND NOT FOUND: '{}'", name));
            }
        }
        Ok(())
    }
}
