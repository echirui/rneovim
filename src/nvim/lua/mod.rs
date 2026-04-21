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

    pub fn register_api(&self, buffers: Vec<Rc<RefCell<Buffer>>>, sender: Option<Sender<EventCallback<VimState>>>) -> Result<()> {
        let globals = self.lua.globals();
        let vim = self.lua.create_table()?;
        let api = self.lua.create_table()?;

        let buffers_clone = buffers.clone();
        let nvim_buf_get_lines = self.lua.create_function(move |_, (buf_id, start, end, _): (i32, usize, i32, bool)| {
            if let Some(buf_rc) = buffers_clone.iter().find(|b| b.borrow().id() == buf_id) {
                let b = buf_rc.borrow();
                let actual_end = if end < 0 { b.line_count() } else { end as usize };
                let mut lines = Vec::new();
                for i in (start + 1)..=actual_end {
                    if let Some(line) = b.get_line(i) { lines.push(line.to_string()); }
                }
                Ok(lines)
            } else {
                Ok(Vec::new())
            }
        })?;

        let nvim_set_decoration_provider = self.lua.create_function(|lua, (_ns_id, callbacks): (i32, mlua::Table)| {
            let key = lua.create_registry_value(callbacks)?;
            lua.set_app_data(key);
            Ok(())
        })?;

        let sender_clone = sender.clone();
        let nvim_command = self.lua.create_function(move |_, cmd: String| {
            if let Some(s) = &sender_clone {
                let _ = s.send(Box::new(move |state| {
                    let _ = crate::nvim::api::execute_cmd(state, &cmd);
                }));
            }
            Ok(())
        })?;

        let sender_clone_2 = sender.clone();
        let cmd_alias = self.lua.create_function(move |_, cmd: String| {
            if let Some(s) = &sender_clone_2 {
                let _ = s.send(Box::new(move |state| {
                    let _ = crate::nvim::api::execute_cmd(state, &cmd);
                }));
            }
            Ok(())
        })?;

        api.set("nvim_buf_get_lines", nvim_buf_get_lines)?;
        api.set("nvim_set_decoration_provider", nvim_set_decoration_provider)?;
        api.set("nvim_command", nvim_command)?;

        let nvim_list_runtime_paths = self.lua.create_function(|lua, _: ()| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get("runtimepath") {
                    let paths: Vec<String> = rtp.split(',').map(|s| s.to_string()).collect();
                    return Ok(paths);
                }
            }
            Ok(Vec::new())
        })?;

        let nvim_get_option = self.lua.create_function(|lua, name: String| {
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
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
                state.options.insert(name, val);
            }
            Ok(())
        })?;

        let nvim_get_runtime_file = self.lua.create_function(|lua, (name, all): (String, bool)| {
            let mut results = Vec::new();
            if let Some(wrapper) = lua.app_data_mut::<StateWrapper>() {
                let state = unsafe { &*wrapper.0 };
                if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get("runtimepath") {
                    for path in rtp.split(',') {
                        let full_path = std::path::Path::new(path).join(&name);
                        if full_path.exists() {
                            results.push(full_path.to_string_lossy().to_string());
                            if !all { break; }
                        }
                    }
                }
            }
            Ok(results)
        })?;

        api.set("nvim_list_runtime_paths", nvim_list_runtime_paths)?;
        api.set("nvim_get_option", nvim_get_option)?;
        api.set("nvim_set_option", nvim_set_option)?;
        api.set("nvim_get_runtime_file", nvim_get_runtime_file)?;

        vim.set("api", api)?;
        vim.set("cmd", cmd_alias)?;

        let fn_table = self.lua.create_table()?;
        let stdpath = self.lua.create_function(|_, name: String| {
            match name.as_str() {
                "config" => Ok(dirs::config_dir().unwrap_or_default().join("rneovim").to_string_lossy().to_string()),
                "data" => Ok(dirs::data_dir().unwrap_or_default().join("rneovim").to_string_lossy().to_string()),
                "cache" => Ok(dirs::cache_dir().unwrap_or_default().join("rneovim").to_string_lossy().to_string()),
                _ => Ok("".to_string()),
            }
        })?;
        fn_table.set("stdpath", stdpath)?;
        
        let expand = self.lua.create_function(|_, path: String| {
            if path.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    return Ok(path.replace('~', &home.to_string_lossy()));
                }
            }
            Ok(path)
        })?;
        fn_table.set("expand", expand)?;

        let system = self.lua.create_function(|_, args: mlua::Value| {
            let mut cmd_args = Vec::new();
            match args {
                mlua::Value::String(s) => cmd_args.push(s.to_string_lossy().to_string()),
                mlua::Value::Table(t) => {
                    for v in t.sequence_values::<String>() {
                        if let Ok(arg) = v { cmd_args.push(arg); }
                    }
                }
                _ => {}
            }
            if cmd_args.is_empty() { return Ok("".to_string()); }
            let mut cmd = std::process::Command::new(&cmd_args[0]);
            if cmd_args.len() > 1 { cmd.args(&cmd_args[1..]); }
            match cmd.output() {
                Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).to_string()),
                Err(e) => Ok(format!("Error: {}", e)),
            }
        })?;
        fn_table.set("system", system)?;

        vim.set("fn", fn_table)?;

        // vim.opt implementation
        let opt = self.lua.create_table()?;
        let opt_meta = self.lua.create_table()?;
        opt_meta.set("__index", self.lua.create_function(move |lua, (_table, name): (mlua::Table, String)| {
            let inner_opt = lua.create_table()?;
            
            // prepend method
            let name_clone_2 = name.clone();
            inner_opt.set("prepend", lua.create_function(move |lua, value: String| {
                if let Some(mut wrapper) = lua.app_data_mut::<StateWrapper>() {
                    let state = unsafe { &mut *wrapper.0 };
                    if name_clone_2 == "rtp" || name_clone_2 == "runtimepath" {
                        if let Some(crate::nvim::state::OptionValue::String(rtp)) = state.options.get_mut("runtimepath") {
                            *rtp = format!("{},{}", value, rtp);
                        }
                    }
                }
                Ok(())
            })?)?;
            
            Ok(inner_opt)
        })?)?;
        let _ = opt.set_metatable(Some(opt_meta));
        vim.set("opt", opt)?;

        let loop_table = self.lua.create_table()?;
        let fs_stat = self.lua.create_function(|_, path: String| {
            let p = std::path::Path::new(&path);
            if p.exists() {
                let table = mlua::Lua::new().create_table()?; // use dummy for now
                table.set("type", "directory")?; // Simplified
                Ok(mlua::Value::Table(table))
            } else {
                Ok(mlua::Value::Nil)
            }
        })?;
        loop_table.set("fs_stat", fs_stat)?;
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

        let base_dir = config_dir.or_else(|| dirs::config_dir()).unwrap_or_else(|| std::path::PathBuf::from("."));
        let rneovim_dir = base_dir.join("rneovim");
        
        // Ensure standard paths exist
        if let Some(data) = dirs::data_dir() {
            let lazy_path = data.join("rneovim/lazy");
            if !lazy_path.exists() {
                let _ = std::fs::create_dir_all(&lazy_path);
            }
        }

        // Source init.lua
        let init_lua = rneovim_dir.join("init.lua");
        if init_lua.exists() {
            if let Err(e) = self.execute_file(&init_lua) {
                eprintln!("Error executing init.lua: {}", e);
            }
        }

        // Source files in plugin/
        let plugin_dir = rneovim_dir.join("plugin");
        if plugin_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(plugin_dir) {
                let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                files.sort_by_key(|e| e.path());
                for entry in files {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("lua") {
                        if let Err(e) = self.execute_file(&path) {
                            eprintln!("Error executing plugin {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }

    pub fn trigger_on_line(&self) {
        if let Some(key) = self.lua.app_data_mut::<RegistryKey>() {
            if let Ok(callbacks) = self.lua.registry_value::<mlua::Table>(&key) {
                if let Ok(on_line) = callbacks.get::<mlua::Function>("on_line") {
                    if let Err(e) = on_line.call::<()>(()) {
                        eprintln!("Lua on_line error: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_execution() {
        let env = LuaEnv::new();
        assert_eq!(env.execute("return 1 + 1").unwrap(), "2");
        assert_eq!(env.execute("return 'hello'").unwrap(), "hello");
        assert_eq!(env.execute("return true").unwrap(), "true");
        assert_eq!(env.execute("return nil").unwrap(), "");
    }

    #[test]
    fn test_lua_table_and_types() {
        let env = LuaEnv::new();
        let code = "return {a=1, b='foo'}";
        // evaluate table returns debug print string in execute implementation
        let res = env.execute(code).unwrap();
        assert!(res.contains("a") && res.contains("b"));
    }

    #[test]
    fn test_lua_error() {
        let env = LuaEnv::new();
        let res = env.execute("non_existent_function()");
        assert!(res.is_err());
    }

    #[test]
    fn test_load_plugins() {
        let env = LuaEnv::new();
        let temp_dir = std::env::temp_dir().join("rneovim_test_plugins");
        let rneovim_dir = temp_dir.join("rneovim");
        std::fs::create_dir_all(rneovim_dir.join("plugin")).unwrap();
        
        let init_lua = rneovim_dir.join("init.lua");
        std::fs::write(&init_lua, "TEST_INIT = true").unwrap();
        
        let plugin_lua = rneovim_dir.join("plugin").join("test.lua");
        std::fs::write(&plugin_lua, "TEST_PLUGIN = true").unwrap();
        
        env.load_plugins(Some(temp_dir.clone()));
        
        assert_eq!(env.execute("return TEST_INIT").unwrap(), "true");
        assert_eq!(env.execute("return TEST_PLUGIN").unwrap(), "true");
        
        std::fs::remove_dir_all(temp_dir).unwrap();
    }
}
