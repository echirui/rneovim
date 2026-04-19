use crate::nvim::error::{NvimError, Result};
use mlua::{Lua, RegistryKey};
use std::rc::Rc;
use std::cell::RefCell;
use crate::nvim::buffer::Buffer;

pub struct LuaEnv {
    lua: Lua,
}

impl LuaEnv {
    pub fn new() -> Self {
        let lua = Lua::new();
        Self { lua }
    }

    pub fn register_api(&self, buffers: Vec<Rc<RefCell<Buffer>>>) -> Result<()> {
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

        api.set("nvim_buf_get_lines", nvim_buf_get_lines)?;
        api.set("nvim_set_decoration_provider", nvim_set_decoration_provider)?;
        vim.set("api", api)?;
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
}
