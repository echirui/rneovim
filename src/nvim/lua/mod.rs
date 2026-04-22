use crate::nvim::error::{NvimError, Result};
use mlua::{Lua, RegistryKey, Table, Value, MultiValue};
use std::rc::Rc;
use std::cell::RefCell;
use crate::nvim::buffer::Buffer;
use std::sync::mpsc::Sender;
use crate::nvim::state::{VimState, AutoCmd, AutoCmdCallback, AutoCmdEvent};
use crate::nvim::event::event_loop::EventCallback;

pub struct LuaEnv {
    pub(crate) lua: Lua,
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
        meta.set("__index", lua.create_function(move |lua, (_t, k): (Table, String)| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.log(&format!("MISSING API: {}.{}", prefix, k));
            }
            Ok(Value::Nil)
        })?)?;
        table.set_metatable(Some(meta));
        Ok(())
    }

    pub fn register_api(&self, _buffers: Vec<Rc<RefCell<Buffer>>>, sender: Option<Sender<EventCallback<VimState>>>) -> Result<()> {
        let globals = self.lua.globals();
        let vim = self.lua.create_table()?;
        let api = self.lua.create_table()?;

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

        let nvim_err_writeln = self.lua.create_function(|lua, msg: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("LUA nvim_err_writeln: {}", msg));
                state.messages.push(format!("ERROR: {}", msg));
                state.redraw();
            }
            Ok(())
        })?;
        api.set("nvim_err_writeln", nvim_err_writeln)?;

        let nvim_command = self.lua.create_function(|lua, cmd: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_command({})", cmd));
                let _ = crate::nvim::api::execute_cmd(state, &cmd);
            }
            Ok(())
        })?;
        api.set("nvim_command", nvim_command)?;

        let nvim_exec = self.lua.create_function(|lua, (cmd, output): (String, bool)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_exec({}, output={})", cmd, output));
                let _ = crate::nvim::api::execute_cmd(state, &cmd);
            }
            Ok("".to_string())
        })?;
        api.set("nvim_exec", nvim_exec)?;

        api.set("nvim_set_var", self.lua.create_function(|lua, (name, val): (String, Value)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let s = match val { Value::String(s) => s.to_string_lossy(), Value::Boolean(b) => b.to_string(), Value::Integer(i) => i.to_string(), _ => format!("{:?}", val) };
                state.log(&format!("API nvim_set_var({}, {})", name, s));
                state.globals.insert(name, s.to_string());
            }
            Ok(())
        })?)?;

        api.set("nvim_get_var", self.lua.create_function(|lua, name: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(val) = state.globals.get(&name) { return Ok(Value::String(lua.create_string(val)?)); }
            }
            Ok(Value::Nil)
        })?)?;

        api.set("nvim_get_vvar", self.lua.create_function(|lua, name: String| {
            let did_enter = if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.vim_did_enter
            } else { false };
            match name.as_str() {
                "vim_did_enter" => Ok(Value::Integer(if did_enter { 1 } else { 0 })),
                _ => Ok(Value::Nil),
            }
        })?)?;

        let nvim_buf_get_lines = self.lua.create_function(|lua, (buf_id, start, end, _): (i32, usize, i32, bool)| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.log(&format!("API nvim_buf_get_lines(buf={}, start={}, end={})", buf_id, start, end));
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
                state.log(&format!("API nvim_buf_set_lines(buf={}, start={}, end={}, count={})", buf_id, start, end, lines.len()));
                if let Some(buf_rc) = state.buffers.iter().find(|b| b.borrow().id() == buf_id) {
                    let mut b = buf_rc.borrow_mut();
                    let actual_end = if end < 0 { b.line_count() } else { end as usize };
                    for _ in start..actual_end { if start + 1 <= b.line_count() { let _ = b.delete_line(start + 1); } }
                    for (i, line) in lines.into_iter().enumerate() { let _ = b.insert_line(start + i + 1, &line); }
                }
            }
            Ok(())
        })?;

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

        api.set("nvim_buf_get_option", self.lua.create_function(|lua, (buf_id, name): (i32, String)| {
            match name.as_str() {
                "filetype" => Ok(Value::String(lua.create_string("text")?)),
                "buftype" => Ok(Value::String(lua.create_string("")?)),
                "modifiable" => Ok(Value::Boolean(true)),
                _ => Ok(Value::Nil),
            }
        })?)?;

        let nvim_open_win = self.lua.create_function(|lua, (buf_id, enter, config): (i32, bool, Table)| {
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
        })?;
        api.set("nvim_open_win", nvim_open_win)?;

        let nvim_create_user_command = self.lua.create_function(|lua, (name, callback, _opts): (String, Value, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_create_user_command(name={})", name));
                match callback {
                    Value::Function(f) => {
                        let key = lua.create_registry_value(f)?;
                        state.user_commands.insert(name, key);
                    }
                    Value::String(s) => {
                        // 文字列コマンドは暫定的に無視（必要なら execute_cmd で対応）
                        state.log(&format!("API nvim_create_user_command: string cmd '{}' ignored", s.to_string_lossy()));
                    }
                    _ => {}
                }
            }
            Ok(())
        })?;
        api.set("nvim_create_user_command", nvim_create_user_command)?;
        api.set("nvim_add_user_command", api.get::<Value>("nvim_create_user_command")?)?;

        let nvim_create_autocmd = self.lua.create_function(|lua, (event_val, opts): (Value, Table)| {
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
        })?;
        api.set("nvim_create_autocmd", nvim_create_autocmd)?;

        let nvim_get_option = self.lua.create_function(|lua, name: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if name == "columns" { return Ok(Value::Integer(state.grid.width as i64)); }
                if name == "lines" { return Ok(Value::Integer(state.grid.height as i64)); }
                if let Some(val) = state.options.get(&name) {
                    match val {
                        crate::nvim::state::OptionValue::Bool(b) => return Ok(Value::Boolean(*b)),
                        crate::nvim::state::OptionValue::Int(i) => return Ok(Value::Integer(*i)),
                        crate::nvim::state::OptionValue::String(s) => return Ok(Value::String(lua.create_string(s)?)),
                    }
                }
            }
            Ok(Value::Nil)
        })?;
        api.set("nvim_get_option", nvim_get_option)?;

        api.set("nvim_set_option", self.lua.create_function(|lua, (name, value): (String, Value)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let val = match value {
                    Value::Boolean(b) => crate::nvim::state::OptionValue::Bool(b),
                    Value::Integer(i) => crate::nvim::state::OptionValue::Int(i),
                    Value::String(s) => crate::nvim::state::OptionValue::String(s.to_string_lossy().to_string()),
                    _ => return Ok(()),
                };
                state.options.insert(name.clone(), val.clone());
                if name == "runtimepath" || name == "rtp" { if let crate::nvim::state::OptionValue::String(ref rtp) = val { let _ = Self::sync_package_path(lua, rtp); } }
            }
            Ok(())
        })?)?;

        api.set("nvim_get_runtime_file", self.lua.create_function(|lua, (name, _): (String, bool)| { 
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get("runtimepath") {
                    let mut files = Vec::new();
                    for p in rtp.split(',') {
                        let path = std::path::Path::new(p).join(&name);
                        if path.exists() { files.push(path.to_string_lossy().to_string()); }
                    }
                    return Ok(files);
                }
            }
            Ok(Vec::<String>::new()) 
        })?)?;

        Self::add_missing_tracker(&self.lua, &api, "vim.api")?;
        vim.set("api", api)?;

        // vim.fn
        let fn_table = self.lua.create_table()?;
        fn_table.set("stdpath", self.lua.create_function(|lua, name: String| {
            let res = match name.as_str() {
                "config" => Self::get_config_dir().to_string_lossy().to_string(),
                "data" => Self::get_data_dir().to_string_lossy().to_string(),
                "cache" => dirs::cache_dir().unwrap_or_default().join("rneovim").to_string_lossy().to_string(),
                _ => "".to_string(),
            };
            Ok(res)
        })?)?;
        fn_table.set("fnamemodify", self.lua.create_function(|_, (path, modifier): (String, String)| {
            if modifier.contains(":p") {
                if path.starts_with('~') { if let Some(home) = dirs::home_dir() { return Ok(path.replace('~', &home.to_string_lossy())); } }
                return Ok(std::fs::canonicalize(&path).unwrap_or_else(|_| std::path::PathBuf::from(path)).to_string_lossy().to_string());
            }
            Ok(path)
        })?)?;
        fn_table.set("has", self.lua.create_function(|_, name: String| {
            match name.as_str() { "nvim-0.9.0" | "nvim-0.8.0" | "nvim" => Ok(true), _ => Ok(false) }
        })?)?;
        fn_table.set("executable", self.lua.create_function(|_, name: String| {
            Ok(if std::env::var("PATH").unwrap_or_default().split(':').any(|p| std::path::Path::new(p).join(&name).exists()) { 1 } else { 0 })
        })?)?;
        fn_table.set("argc", self.lua.create_function(|_, _: ()| { Ok(std::env::args().len().saturating_sub(1)) })?)?;
        fn_table.set("argv", self.lua.create_function(|lua, idx: Option<usize>| {
            let args: Vec<String> = std::env::args().skip(1).collect();
            if let Some(i) = idx { return Ok(Value::String(lua.create_string(&args.get(i).cloned().unwrap_or_default())?)); }
            let t = lua.create_table()?;
            for (i, a) in args.iter().enumerate() { t.set(i + 1, a.clone())?; }
            Ok(Value::Table(t))
        })?)?;
        fn_table.set("exists", self.lua.create_function(|_, name: String| { Ok(if name.starts_with(':') { 2 } else { 0 }) })?)?;
        fn_table.set("filereadable", self.lua.create_function(|_, path: String| { Ok(if std::path::Path::new(&path).is_file() { 1 } else { 0 }) })?)?;
        fn_table.set("isdirectory", self.lua.create_function(|_, path: String| { Ok(if std::path::Path::new(&path).is_dir() { 1 } else { 0 }) })?)?;
        fn_table.set("glob", self.lua.create_function(|_, (_p, _, _): (String, bool, bool)| { Ok("".to_string()) })?)?;
        
        Self::add_missing_tracker(&self.lua, &fn_table, "vim.fn")?;
        vim.set("fn", fn_table)?;

        // vim.loop
        let loop_table = self.lua.create_table()?;
        loop_table.set("fs_stat", self.lua.create_function(|lua, path: String| {
            if std::path::Path::new(&path).exists() {
                let table = lua.create_table()?; table.set("type", "directory")?; Ok(Value::Table(table))
            } else { Ok(Value::Nil) }
        })?)?;
        loop_table.set("hrtime", self.lua.create_function(|_, _: ()| { Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64) })?)?;
        loop_table.set("cwd", self.lua.create_function(|_, _: ()| { Ok(std::env::current_dir().unwrap_or_default().to_string_lossy().to_string()) })?)?;
        
        Self::add_missing_tracker(&self.lua, &loop_table, "vim.loop")?;
        vim.set("loop", &loop_table)?;
        vim.set("uv", loop_table)?;

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
        cmd_meta.set("__call", self.lua.create_function(move |lua, (_t, cmd): (Table, String)| { cmd_fn.call::<()>(cmd) })?)?;
        cmd_meta.set("__index", self.lua.create_function(move |lua, (_t, k): (Table, String)| {
            Ok(lua.create_function(move |lua, args: MultiValue| {
                let arg_strs: Vec<String> = args.iter().map(|v| match v { Value::String(s) => s.to_string_lossy().to_string(), _ => format!("{:?}", v) }).collect();
                let cmd = format!("{} {}", k, arg_strs.join(" "));
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    state.log(&format!("API vim.cmd.{} called as function -> {}", k, cmd));
                    let _ = crate::nvim::api::execute_cmd(state, &cmd);
                }
                Ok(())
            })?)
        })?)?;
        cmd_table.set_metatable(Some(cmd_meta));
        vim.set("cmd", cmd_table)?;

        // vim.version
        vim.set("version", self.lua.create_function(|lua, _: ()| {
            let t = lua.create_table()?;
            t.set("major", 0)?; t.set("minor", 9)?; t.set("patch", 0)?;
            Ok(t)
        })?)?;

        // vim.v
        let v = self.lua.create_table()?;
        let v_meta = self.lua.create_table()?;
        v_meta.set("__index", self.lua.create_function(|lua, name: String| {
            let did_enter = if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.vim_did_enter
            } else { false };
            match name.as_str() { "vim_did_enter" => Ok(Value::Integer(if did_enter { 1 } else { 0 })), _ => Ok(Value::Nil) }
        })?)?;
        v.set_metatable(Some(v_meta));
        vim.set("v", v)?;

        Self::add_missing_tracker(&self.lua, &vim, "vim")?;
        globals.set("vim", vim)?;
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
