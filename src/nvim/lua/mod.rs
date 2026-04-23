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

        let original_require: mlua::Function = globals.get("require")?;
        globals.set("require", self.lua.create_function(move |lua, modname: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let debug: Table = lua.globals().get("debug")?;
                let getinfo: mlua::Function = debug.get("getinfo")?;
                let info: Option<Table> = getinfo.call((2, "Sl"))?;
                let caller = if let Some(t) = info {
                    format!("{}:{}", t.get::<String>("short_src").unwrap_or_default(), t.get::<i32>("currentline").unwrap_or_default())
                } else {
                    "unknown".to_string()
                };
                state.log(&format!("LUA require: {} (from {})", modname, caller));
            }
            original_require.call::<Value>(modname)
        })?)?;

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
                            "User" => AutoCmdEvent::User, "ColorSchemePre" => AutoCmdEvent::ColorSchemePre,
                            _ => continue,
                        };
                        state.autocmds.entry(ev_type).or_insert_with(Vec::new).push(AutoCmd { callback: callback.clone() });
                    }
                }
            }
            Ok(1)
        })?)?;
        api.set("nvim_create_namespace", self.lua.create_function(|lua, name: String| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_create_namespace(name={})", name));
            }
            Ok(1)
        })?)?;
        api.set("nvim_create_augroup", self.lua.create_function(|lua, (name, _opts): (String, Table)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                state.log(&format!("API nvim_create_augroup(name={})", name));
            }
            Ok(1)
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
                _ => data_dir.to_string_lossy().to_string(),
            })
        })?)?;
        fn_table.set("fnamemodify", self.lua.create_function(|_, (path_opt, modifier): (Option<String>, String)| {
            let path = path_opt.unwrap_or_default();
            let mut res = std::path::PathBuf::from(&path);
            let mods: Vec<&str> = modifier.split(':').filter(|s| !s.is_empty()).collect();
            for m in mods {
                if m == "p" {
                    if path.starts_with('~') { if let Some(home) = dirs::home_dir() { res = home.join(&path[2..]); } }
                    res = std::fs::canonicalize(&res).unwrap_or(res);
                } else if m == "h" { if let Some(parent) = res.parent() { res = parent.to_path_buf(); } }
                else if m == "t" { if let Some(tail) = res.file_name() { res = std::path::PathBuf::from(tail); } }
            }
            Ok(res.to_string_lossy().to_string())
        })?)?;
        fn_table.set("has", self.lua.create_function(|lua, name: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                state.log(&format!("API vim.fn.has({})", name));
            }
            Ok(match name.as_str() { "nvim-0.9.0" | "nvim-0.8.0" | "mac" | "unix" => 1, _ => 0 })
        })?)?;
        let _ = Self::add_missing_tracker(&self.lua, &fn_table, "vim.fn");
        vim.set("fn", fn_table)?;

        // vim.loop / vim.uv
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
        loop_table.set("fs_open", self.lua.create_function(|lua, (path, flags, _mode): (String, Value, i32)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                let mut opts = std::fs::OpenOptions::new();
                let flags_str = match flags {
                    Value::String(ref s) => s.to_string_lossy().to_string(),
                    Value::Integer(n) => {
                        if n & 1 != 0 || n & 2 != 0 { opts.write(true); } else { opts.read(true); }
                        if n & 512 != 0 { opts.create(true); }
                        if n & 1024 != 0 { opts.truncate(true); }
                        "int".to_string()
                    },
                    _ => { opts.read(true); "default".to_string() }
                };
                if flags_str != "int" {
                    match flags_str.as_str() {
                        "r" => { opts.read(true); }
                        "w" => { opts.write(true).create(true).truncate(true); }
                        "a" => { opts.append(true).create(true); }
                        "w+" => { opts.read(true).write(true).create(true).truncate(true); }
                        _ => { opts.read(true); }
                    }
                }
                if let Ok(file) = opts.open(&path) {
                    let fd = state.next_fd;
                    state.log(&format!("FS_OPEN SUCCESS: path={}, fd={}, total={}", path, fd, state.open_files.len() + 1));
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
                state.log(&format!("FS_CLOSE SUCCESS: fd={}, total={}", fd, state.open_files.len()));
            }
            Ok(true)
        })?)?;
        loop_table.set("fs_read", self.lua.create_function(|lua, (fd, size, offset): (i32, usize, i64)| {
            if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &mut *wrapper.0 };
                if let Some(file) = state.open_files.get_mut(&fd) {
                    use std::io::{Read, Seek, SeekFrom};
                    if offset >= 0 { let _ = file.seek(SeekFrom::Start(offset as u64)); }
                    let mut buf = vec![0u8; size];
                    if let Ok(n) = file.read(&mut buf) {
                        state.log(&format!("FS_READ: fd={}, size={}, actual={}", fd, size, n));
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
                    let bytes = match data { Value::String(s) => s.as_bytes().to_vec(), _ => return Ok(Value::Integer(0)) };
                    if offset >= 0 { let _ = file.seek(SeekFrom::Start(offset as u64)); }
                    if let Ok(_) = file.write_all(&bytes) {
                        state.log(&format!("FS_WRITE: fd={}, size={}", fd, bytes.len()));
                        return Ok(Value::Integer(bytes.len() as i64));
                    }
                }
            }
            Ok(Value::Integer(0))
        })?)?;
        loop_table.set("hrtime", self.lua.create_function(|_, _: ()| { Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64) })?)?;
        let _ = Self::add_missing_tracker(&self.lua, &loop_table, "vim.loop");
        vim.set("loop", loop_table.clone())?;
        vim.set("uv", loop_table)?;

        // vim.opt
        let opt = self.lua.create_table()?;
        let opt_meta = self.lua.create_table()?;
        opt_meta.set("__index", self.lua.create_function(|lua, (_t, k): (Table, Value)| {
            let name = match k { Value::String(s) => s.to_string_lossy().to_string(), _ => return Ok(Value::Nil) };
            let inner = lua.create_table()?;
            let name_p = name.clone();
            inner.set("prepend", lua.create_function(move |lua, val: String| {
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    if name_p == "rtp" { if let Some(OptionValue::String(rtp)) = state.options.get_mut("runtimepath") { *rtp = format!("{},{}", val, rtp); } }
                }
                Ok(())
            })?)?;
            Ok(Value::Table(inner))
        })?)?;
        let _ = opt.set_metatable(Some(opt_meta));
        vim.set("opt", opt)?;

        // vim.loader
        let loader = self.lua.create_table()?;
        loader.set("enable", self.lua.create_function(|_, _: ()| Ok(()))?)?;
        vim.set("loader", loader)?;

        globals.set("vim", vim.clone())?;
        let _ = Self::add_missing_tracker(&self.lua, &vim, "vim");

        Ok(())
    }

    pub fn execute(&self, code: &str) -> Result<String> {
        match self.lua.load(code).eval::<Value>() {
            Ok(val) => match val {
                Value::Nil => Ok("".to_string()), Value::String(s) => Ok(s.to_string_lossy().to_string()),
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
        let init_lua = rneovim_config.join("init.lua");
        if init_lua.exists() { let _ = self.execute_file(&init_lua); }
    }

    pub fn execute_callback(&self, key: &RegistryKey) -> Result<()> {
        let func: mlua::Function = self.lua.registry_value(key).map_err(|e| NvimError::Api(format!("Failed to get callback: {}", e)))?;
        func.call::<()>(()).map_err(|e| NvimError::Api(format!("Callback error: {}", e)))
    }

    pub fn call_user_command(&self, name: &str, args: &str) -> Result<()> {
        if let Some(mut wrapper) = self.lua.app_data_mut::<StateWrapper>() {
            let state = unsafe { &mut *wrapper.0 };
            if let Some(key) = state.user_commands.get(name) {
                let func: mlua::Function = self.lua.registry_value(key)?;
                let arg_table = self.lua.create_table()?;
                arg_table.set("name", name)?; arg_table.set("args", args)?;
                let _ = func.call::<()>(arg_table);
            }
        }
        Ok(())
    }
}
