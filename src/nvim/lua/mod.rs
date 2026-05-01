use crate::nvim::buffer::Buffer;
use crate::nvim::state::{VimState, OptionValue};
use crate::nvim::error::{NvimError, Result};
use mlua::{Lua, Table, Value, MultiValue, Function};
use std::rc::Rc;
use std::cell::RefCell;
use crate::nvim::event::event_loop::EventCallback;

pub mod api;
pub mod uv;
pub mod util;
pub mod diagnostic;
pub mod treesitter;

pub struct StateWrapper(pub RefCell<*mut VimState>);

pub struct LuaEnv {
    pub lua: Lua,
}

impl LuaEnv {
    pub fn new() -> Self {
        use mlua::StdLib;
        let lua = unsafe { Lua::unsafe_new_with(StdLib::ALL, mlua::LuaOptions::default()) };
        Self { lua }
    }

    fn get_config_dir() -> std::path::PathBuf {
        dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".config")).join("rneovim")
    }

    pub fn register_api(&self, _buffers: Vec<Rc<RefCell<Buffer>>>, _sender: Option<std::sync::mpsc::Sender<EventCallback<VimState>>>) -> Result<()> {
        let vim = self.lua.create_table().map_err(|e| NvimError::Api(format!("Failed to create vim table: {}", e)))?;
        
        let api = self.lua.create_table().map_err(|e| NvimError::Api(format!("Failed to create api table: {}", e)))?;
        for (name, func) in api::get_api_functions(&self.lua).map_err(|e| NvimError::Api(format!("Failed to get API functions: {}", e)))? {
            api.set(name, func).map_err(|e| NvimError::Api(format!("Failed to set API function: {}", e)))?;
        }
        
        let api_meta = self.lua.create_table().map_err(|e| NvimError::Api(format!("Failed to create api metadata: {}", e)))?;
        api_meta.set("__index", self.lua.create_function(|lua, (t, key): (Table, String)| {
            if let Ok(v) = t.raw_get::<Value>(key.clone()) {
                if !matches!(v, Value::Nil) { return Ok(v); }
            }
            let api_name = if key.starts_with("nvim_") { key.clone() } else { format!("nvim_{}", key) };
            if let Ok(f) = lua.globals().get::<Table>("vim")?.get::<Table>("api")?.get::<Function>(api_name.clone()) {
                Ok(Value::Function(f))
            } else {
                if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                    let state = unsafe { &**wrapper.0.as_ptr() };
                    state.log(&format!("MISSING API: {}", api_name));
                }
                Ok(Value::Nil)
            }
        })?)?;
        let _ = api.set_metatable(Some(api_meta));
        vim.set("api", api)?;

        uv::register_uv(&self.lua, &vim).map_err(|e| NvimError::Api(format!("UV error: {}", e)))?;
        util::register_utils(&self.lua, &vim).map_err(|e| NvimError::Api(format!("Util error: {}", e)))?;
        diagnostic::register_diagnostic(&self.lua, &vim).map_err(|e| NvimError::Api(format!("Diagnostic error: {}", e)))?;
        treesitter::register_treesitter(&self.lua, &vim).map_err(|e| NvimError::Api(format!("Treesitter error: {}", e)))?;

        let uv_table: Table = vim.get("uv")?;
        vim.set("loop", uv_table)?;

        vim.set("g", self.lua.create_table()?)?;
        
        let options_meta = self.lua.create_table()?;
        options_meta.set("__index", self.lua.create_function(|lua, (_t, key): (Table, String)| {
            if key == "headless" { return Ok(Value::Boolean(false)); }
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &**wrapper.0.as_ptr() };
                if let Some(val) = state.options.get(&key) {
                    match val {
                        OptionValue::Bool(b) => return Ok(Value::Boolean(*b)),
                        OptionValue::Int(i) => return Ok(Value::Integer(*i)),
                        OptionValue::String(s) => return Ok(Value::String(lua.create_string(s)?)),
                    }
                }
            }
            Ok(Value::Nil)
        })?)?;
        options_meta.set("__newindex", self.lua.create_function(|lua, (_t, key, val): (Table, String, Value)| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let opt_val = match val {
                    Value::Boolean(b) => OptionValue::Bool(b),
                    Value::Integer(i) => OptionValue::Int(i),
                    Value::String(s) => OptionValue::String(s.to_string_lossy().to_string()),
                    _ => return Ok(()),
                };
                state.options.insert(key, opt_val);
            }
            Ok(())
        })?)?;

        let go = self.lua.create_table()?;
        let _ = go.set_metatable(Some(options_meta.clone()));
        vim.set("go", go)?;
        
        let o = self.lua.create_table()?;
        let _ = o.set_metatable(Some(options_meta.clone()));
        vim.set("o", o)?;

        let bo = self.lua.create_table()?;
        let bo_meta = self.lua.create_table()?;
        let options_meta_clone = options_meta.clone();
        bo_meta.set("__index", self.lua.create_function(move |lua, (_t, _id): (Table, Value)| {
            let proxy = lua.create_table()?;
            let _ = proxy.set_metatable(Some(options_meta_clone.clone()));
            Ok(proxy)
        })?)?;
        let _ = bo.set_metatable(Some(bo_meta));
        vim.set("bo", bo)?;

        let wo = self.lua.create_table()?;
        let wo_meta = self.lua.create_table()?;
        let options_meta_clone2 = options_meta.clone();
        wo_meta.set("__index", self.lua.create_function(move |lua, (_t, _id): (Table, Value)| {
            let proxy = lua.create_table()?;
            let _ = proxy.set_metatable(Some(options_meta_clone2.clone()));
            Ok(proxy)
        })?)?;
        let _ = wo.set_metatable(Some(wo_meta));
        vim.set("wo", wo)?;

        let env = self.lua.create_table()?;
        for (k, v) in std::env::vars() { env.set(k, v)?; }
        if let Ok(cwd) = std::env::current_dir() {
            let rt = cwd.join("neovim-src/runtime");
            if rt.exists() {
                env.set("VIMRUNTIME", rt.to_string_lossy().to_string())?;
            }
        }
        vim.set("env", env)?;

        let v_vars = self.lua.create_table()?;
        v_vars.set("version", 10000)?; // 0.10.0
        vim.set("v", v_vars)?;

        vim.set("version", self.lua.create_function(|lua, _: ()| {
            let res = lua.create_table()?;
            res.set("major", 0)?;
            res.set("minor", 10)?;
            res.set("patch", 0)?;
            res.set("api_level", 12)?;
            Ok(res)
        })?)?;

        let loader = self.lua.create_table()?;
        loader.set("enable", self.lua.create_function(|_, _: ()| Ok(()))?)?;
        vim.set("loader", loader)?;

        let log = self.lua.create_table()?;
        let levels = self.lua.create_table()?;
        levels.set("TRACE", 0)?; levels.set("DEBUG", 1)?; levels.set("INFO", 2)?;
        levels.set("WARN", 3)?; levels.set("ERROR", 4)?; levels.set("OFF", 5)?;
        log.set("levels", levels)?;
        log.set("set_level", self.lua.create_function(|_, _: Value| Ok(()))?)?;
        vim.set("log", log)?;

        vim.set("notify", self.lua.create_function(|lua, (msg, level, _opts): (String, Option<i32>, Option<Table>)| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                state.show_notification(&msg, level);
            } else {
                println!("LUA NOTIFY: {}", msg);
            }
            Ok(())
        })?)?;

        let fn_table = self.lua.create_table().map_err(|e| NvimError::Api(format!("Failed to create fn table: {}", e)))?;
        let fn_meta = self.lua.create_table().map_err(|e| NvimError::Api(format!("Failed to create fn metadata: {}", e)))?;
        fn_meta.set("__index", self.lua.create_function(|lua_ctx, (_t, key): (Table, String)| {
            Ok(lua_ctx.create_function(move |lua, args: MultiValue| {
                if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                    let state = unsafe { &mut **wrapper.0.as_ptr() };
                    state.log(&format!("FN CALL: vim.fn.{} with {:?} args", key, args.len()));
                    
                    if key == "exists" { return Ok(Value::Integer(0)); }
                    if key == "has" {
                        if let Some(Value::String(s)) = args.get(0) {
                            let feat = s.to_string_lossy();
                            if feat.starts_with("nvim-") || feat == "nvim" { return Ok(Value::Integer(1)); }
                        }
                        return Ok(Value::Integer(0));
                    }
                    if key == "expand" {
                        if let Some(Value::String(s)) = args.get(0) {
                             let s = s.to_string_lossy();
                             if s == "<sfile>" { return Ok(Value::String(lua.create_string("init.lua")?)); }
                        }
                    }
                    if key == "fnamemodify" {
                        if let Some(Value::String(s)) = args.get(0) {
                            return Ok(Value::String(s.clone()));
                        }
                        return Ok(Value::String(lua.create_string("")?));
                    }
                    if key == "executable" {
                        if let Some(Value::String(s)) = args.get(0) {
                            let cmd = s.to_string_lossy();
                            let exists = which::which::<&str>(cmd.as_ref()).is_ok();
                            return Ok(Value::Integer(if exists { 1 } else { 0 }));
                        }
                    }
                    if key == "mkdir" {
                        if let Some(Value::String(s)) = args.get(0) {
                            let path = s.to_string_lossy();
                            let _ = std::fs::create_dir_all(path.as_ref() as &std::path::Path);
                            return Ok(Value::Integer(1));
                        }
                    }
                    if key == "stdpath" {
                        if let Some(Value::String(s)) = args.get(0) {
                            let path_type = s.to_string_lossy();
                            let base = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
                            let path = match path_type.as_ref() {
                                "config" => base.join(".config/rneovim"),
                                "data" => base.join(".local/share/rneovim"),
                                "state" => base.join(".local/state/rneovim"),
                                "cache" => base.join(".cache/rneovim"),
                                _ => base.join(".local/share/rneovim"),
                            };
                            return Ok(Value::String(lua.create_string(path.to_string_lossy().as_ref())?));
                        }
                    }
                }
                Ok(Value::Nil)
            })?)
        })?)?;
        let _ = fn_table.set_metatable(Some(fn_meta));
        vim.set("fn", fn_table)?;

        let lsp = self.lua.create_table()?;
        vim.set("lsp", lsp)?;

        vim.set("schedule", self.lua.create_function(|lua, cb: Function| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let env = state.lua_env.clone();
                if let Ok(id) = env.register_callback(Value::Function(cb)) {
                    state.pending_callbacks.push(id);
                }
            }
            Ok(())
        })?)?;

        // Implement schedule_wrap in pure Lua to avoid registry exhaustion with arguments
        let schedule_wrap_code = r#"
            return function(cb)
                return function(...)
                    local args = {...}
                    local n = select('#', ...)
                    vim.schedule(function()
                        cb(unpack(args, 1, n))
                    end)
                end
            end
        "#;
        let schedule_wrap_func: Function = self.lua.load(schedule_wrap_code).eval()?;
        vim.set("schedule_wrap", schedule_wrap_func)?;

        vim.set("system", self.lua.create_function(|lua, (cmd_val, _opts, on_exit): (Value, Option<Table>, Option<Function>)| {
            let mut cmd_args: Vec<String> = Vec::new();
            match cmd_val {
                Value::String(s) => {
                    let s = s.to_string_lossy().to_string();
                    cmd_args = s.split_whitespace().map(|x| x.to_string()).collect();
                }
                Value::Table(t) => {
                    for v in t.sequence_values::<String>() { if let Ok(s) = v { cmd_args.push(s); } }
                }
                _ => {}
            }
            if cmd_args.is_empty() { return Ok(Value::Nil); }

            if let Some(cb) = on_exit {
                if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                    let state = unsafe { &mut **wrapper.0.as_ptr() };
                    let env = state.lua_env.clone();
                    if let Ok(id) = env.register_callback(Value::Function(cb)) {
                        // For now we don't actually spawn the process and call it,
                        // but we store the callback to avoid RegistryKey.
                        let _ = id; 
                    }
                }
                return Ok(Value::Table(lua.create_table()?));
            } else {
                let mut command = std::process::Command::new(&cmd_args[0]);
                if cmd_args.len() > 1 { command.args(&cmd_args[1..]); }
                match command.output() {
                    Ok(output) => {
                        let res = lua.create_table()?;
                        res.set("code", output.status.code().unwrap_or(0))?;
                        res.set("stdout", String::from_utf8_lossy(&output.stdout).to_string())?;
                        res.set("stderr", String::from_utf8_lossy(&output.stderr).to_string())?;
                        Ok(Value::Table(res))
                    }
                    Err(_) => Ok(Value::Nil)
                }
            }
        })?)?;

        let keymap = self.lua.create_table()?;
        keymap.set("set", self.lua.create_function(|lua, (mode, lhs, rhs, opts_opt): (Value, String, Value, Option<Table>)| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let target = match rhs {
                    Value::String(s) => crate::nvim::keymap::MappingTarget::String(s.to_string_lossy().to_string()),
                    Value::Function(f) => {
                        let env = state.lua_env.clone();
                        crate::nvim::keymap::MappingTarget::Lua(env.register_callback(Value::Function(f))?)
                    }
                    _ => return Ok(()),
                };
                let opts = opts_opt.unwrap_or_else(|| lua.create_table().unwrap());
                let remap = opts.get::<bool>("remap").unwrap_or(false);
                let noremap = opts.get::<bool>("noremap").unwrap_or(!remap);
                let silent = opts.get::<bool>("silent").unwrap_or(false);
                let expr = opts.get::<bool>("expr").unwrap_or(false);
                let modes = match mode {
                    Value::String(s) => vec![s.to_string_lossy().to_string()],
                    Value::Table(t) => t.sequence_values::<String>().filter_map(|x| x.ok()).collect(),
                    _ => vec!["".to_string()],
                };
                for m in modes { state.keymap.add_mapping(&m, &lhs, target.clone(), !noremap, silent, expr); }
            }
            Ok(())
        })?)?;
        keymap.set("del", self.lua.create_function(|lua, (mode, lhs): (Value, String)| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let modes = match mode {
                    Value::String(s) => vec![s.to_string_lossy().to_string()],
                    Value::Table(t) => t.sequence_values::<String>().filter_map(|x| x.ok()).collect(),
                    _ => vec!["".to_string()],
                };
                for m in modes { state.keymap.del_mapping(&m, &lhs); }
            }
            Ok(())
        })?)?;

        vim.set("keymap", keymap)?;

        let f_table = self.lua.create_table()?;
        f_table.set("if_nil", self.lua.create_function(|_, (a, b): (Value, Value)| Ok(if matches!(a, Value::Nil) { b } else { a }))?)?;
        f_table.set("pack_len", self.lua.create_function(|lua, args: MultiValue| {
            let res = lua.create_table()?; let n = args.len();
            for (i, arg) in args.into_iter().enumerate() { res.set(i + 1, arg)?; }
            res.set("n", n)?; Ok(res)
        })?)?;
        f_table.set("unpack_len", self.lua.create_function(|_lua, t: Table| {
            let n: usize = t.get("n")?;
            let mut res = Vec::new();
            for i in 1..=n { res.push(t.get::<Value>(i)?); }
            Ok(MultiValue::from_vec(res))
        })?)?;
        vim.set("F", f_table)?;

        vim.set("cmd", self.lua.create_function(|lua, cmd_val: Value| {
            let cmd_str = match cmd_val {
                Value::String(s) => s.to_string_lossy().to_string(),
                Value::Table(t) => t.get::<String>(1).unwrap_or_default(),
                _ => return Ok(()),
            };
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let _ = crate::nvim::api::execute_cmd(state, &cmd_str);
            }
            Ok(())
        })?)?;

        vim.set("print", self.lua.create_function(|lua, args: MultiValue| {
            let mut s = String::new();
            for arg in args {
                if !s.is_empty() { s.push('\t'); }
                s.push_str(&match arg {
                    Value::String(s) => s.to_string_lossy().to_string(),
                    Value::Table(_) => "<table>".to_string(),
                    Value::Function(_) => "<function>".to_string(),
                    Value::Nil => "nil".to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Integer(i) => i.to_string(),
                    Value::Number(n) => n.to_string(),
                    _ => format!("{:?}", arg),
                });
            }
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                state.log(&format!("LUA PRINT: {}", s));
            }
            Ok(())
        })?)?;

        vim.set("NIL", self.lua.create_table()?)?;
        
        self.lua.globals().set("vim", vim)?;

        // Lua-side callback manager
        let callback_mgr = r#"
            vim.print("DEBUG: starting callback_mgr bootstrap")
            vim.print("DEBUG: _G.debug type is " .. type(_G.debug))
            _G.__rneovim_callbacks = {}
            _G.__rneovim_next_callback_id = 1
            function _G.__rneovim_register_callback(f)
                if type(f) ~= "function" then return nil end
                local id = _G.__rneovim_next_callback_id
                _G.__rneovim_callbacks[id] = f
                _G.__rneovim_next_callback_id = id + 1
                return id
            end
            function _G.__rneovim_call_callback(id, ...)
                local f = _G.__rneovim_callbacks[id]
                if f then return f(...) end
                -- Fallback for autocmds
                local au = _G.__rneovim_autocmds[id]
                if au and au.callback then return au.callback(...) end
            end
            function _G.__rneovim_unregister_callback(id)
                _G.__rneovim_callbacks[id] = nil
            end

            -- Pure Lua loadfile to avoid RegistryKey exhaustion
            local old_loadfile = _G.loadfile
            _G.loadfile = function(path)
                if path:find("pkg%-cache%.lua") then
                    return function() return nil end
                end
                -- vim.print("LUA: loadfile(" .. path .. ")")
                local f = io.open(path, "r")
                if not f then return nil, "failed to open " .. path end
                local content = f:read("*a")
                f:close()
                local func, err = load(content, "@" .. path)
                if not func then vim.print("LUA ERROR: load failed for " .. path .. ": " .. tostring(err)) end
                return func, err
            end
        "#;
        self.lua.load(callback_mgr).exec()?;

        // Final bootstrapping of vim.opt in Lua, ensuring it's available globally
        let opt_bootstrap = r#"
            local aliases = { rtp = "runtimepath", pp = "packpath" }
            local function split(str, sep)
                if not str or str == "" then return {} end
                local res = {}
                for s in string.gmatch(str, "([^" .. sep .. "]+)") do
                    table.insert(res, s)
                end
                return res
            end
            local opt_obj_mt = {
                __index = function(t, key)
                    local name = aliases[t._name] or t._name
                    if key == "append" then
                        return function(self, val)
                            local current = vim.o[name] or ""
                            vim.o[name] = (tostring(current) == "" and "" or (tostring(current) .. ",")) .. tostring(val)
                        end
                    elseif key == "prepend" then
                        return function(self, val)
                            local current = vim.o[name] or ""
                            vim.o[name] = tostring(val) .. (tostring(current) == "" and "" or ("," .. tostring(current)))
                        end
                    elseif key == "remove" then
                        return function(self, val)
                            local current = vim.o[name] or ""
                            local parts = split(current, ",")
                            local new_parts = {}
                            for _, p in ipairs(parts) do
                                if p ~= tostring(val) then table.insert(new_parts, p) end
                            end
                            vim.o[name] = table.concat(new_parts, ",")
                        end
                    elseif key == "get" then
                        return function(self)
                            local val = vim.o[name]
                            if name == "runtimepath" or name == "packpath" then
                                return split(val or "", ",")
                            end
                            return val
                        end
                    end
                end,
                __call = function(t)
                    return t:get()
                end
            }
            vim.opt = setmetatable({}, {
                __index = function(_, key)
                    return setmetatable({ _name = key }, opt_obj_mt)
                end,
                __newindex = function(_, key, val)
                    local name = aliases[key] or key
                    vim.o[name] = val
                end
            })
            vim.opt_global = vim.opt

            -- Better deepcopy
            function vim.deepcopy(orig)
                local orig_type = type(orig)
                local copy
                if orig_type == 'table' then
                    copy = {}
                    for orig_key, orig_value in next, orig, nil do
                        copy[vim.deepcopy(orig_key)] = vim.deepcopy(orig_value)
                    end
                    setmetatable(copy, vim.deepcopy(getmetatable(orig)))
                else -- number, string, boolean, etc
                    copy = orig
                end
                return copy
            end

            -- Lua-side storage to avoid RegistryKey / Stack exhaustion
            _G.__rneovim_autocmds = {}
            local old_create_autocmd = vim.api.nvim_create_autocmd
            vim.api.nvim_create_autocmd = function(event, opts)
                local id = #_G.__rneovim_autocmds + 1
                _G.__rneovim_autocmds[id] = opts
                local events = type(event) == "table" and event or {event}
                for _, ev in ipairs(events) do
                    vim.api.__nvim_register_autocmd_id(ev, id, opts.pattern or "*", opts.once or false)
                end
                return id
            end

            -- High-volume API: nvim_set_hl
            local old_set_hl = vim.api.nvim_set_hl
            vim.api.nvim_set_hl = function(ns, name, opts)
                if type(opts) ~= "table" then return old_set_hl(ns, name, "") end
                local parts = {}
                if opts.fg then table.insert(parts, "fg=" .. tostring(opts.fg)) end
                if opts.bg then table.insert(parts, "bg=" .. tostring(opts.bg)) end
                if opts.bold then table.insert(parts, "bold=1") end
                if opts.italic then table.insert(parts, "italic=1") end
                if opts.underline then table.insert(parts, "underline=1") end
                if opts.reverse then table.insert(parts, "reverse=1") end
                return old_set_hl(ns, name, table.concat(parts, ","))
            end

            -- High-frequency API: nvim_get_option_value
            local old_get_option_value = vim.api.nvim_get_option_value
            vim.api.nvim_get_option_value = function(name, opts)
                if name == "columns" then return 80 end
                if name == "lines" then return 24 end
                if name == "termguicolors" then return true end
                return old_get_option_value(name, opts)
            end
        "#;
        self.lua.load(opt_bootstrap).exec()?;

        // let _ = LuaEnv::add_missing_tracker(&self.lua, &self.lua.globals().get::<Table>("vim")?, "vim");
        // let _ = LuaEnv::add_missing_tracker(&self.lua, &self.lua.globals(), "_G");

        Ok(())
    }

    pub fn make_event_obj(lua: &Lua) -> mlua::Result<Table> {
        let obj = lua.create_table()?;
        obj.set("start", lua.create_function(|lua, (_t, cb): (Table, Function)| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let env = state.lua_env.clone();
                if let Ok(id) = env.register_callback(Value::Function(cb)) {
                    state.pending_callbacks.push(id);
                }
            }
            Ok(())
        })?)?;
        obj.set("stop", lua.create_function(|_, _: ()| Ok(()))?)?;
        obj.set("close", lua.create_function(|_, _: ()| Ok(()))?)?;
        obj.set("is_active", lua.create_function(|_, _: ()| Ok(false))?)?;
        obj.set("is_closing", lua.create_function(|_, _: ()| Ok(false))?)?;
        Ok(obj)
    }

    pub fn add_missing_tracker(lua: &Lua, table: &Table, prefix: &str) -> mlua::Result<()> {
        let meta = lua.create_table()?;
        let prefix_clone = prefix.to_string();
        meta.set("__index", lua.create_function(move |lua, (t, key): (Table, String)| {
            if let Ok(v) = t.raw_get::<Value>(key.clone()) {
                if !matches!(v, Value::Nil) { return Ok(v); }
            }
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &**wrapper.0.as_ptr() };
                state.log(&format!("MISSING API: {}.{}", prefix_clone, key));
            }
            Ok(Value::Nil)
        })?)?;
        let _ = table.set_metatable(Some(meta));
        Ok(())
    }

    pub fn register_callback(&self, func: Value) -> mlua::Result<i32> {
        let register: Function = self.lua.globals().get("__rneovim_register_callback")?;
        register.call::<i32>(func)
    }

    pub fn call_callback(&self, id: i32, args: MultiValue) -> Result<()> {
        let caller: Function = self.lua.globals().get("__rneovim_call_callback").map_err(|e| NvimError::Api(format!("{}", e)))?;
        caller.call::<()>( (id, args) ).map_err(|e| NvimError::Api(format!("Callback error (id={}): {}", id, e)))
    }

    pub fn unregister_callback(&self, id: i32) -> mlua::Result<()> {
        let unregister: Function = self.lua.globals().get("__rneovim_unregister_callback")?;
        unregister.call::<()>(id)
    }

    pub fn execute(&self, code: &str) -> Result<()> {
        self.lua.load(code).exec().map_err(|e| NvimError::Api(format!("Lua error: {}", e)))
    }

    pub fn execute_file(&self, path: &std::path::Path) -> Result<()> {
        let content = std::fs::read_to_string(path).map_err(|e| NvimError::Api(format!("Failed to read lua file: {}", e)))?;
        self.lua.load(&content).set_name(path.to_string_lossy()).exec().map_err(|e| {
            NvimError::Api(format!("Lua execution error in {}: {}", path.display(), e))
        })
    }

    pub fn execute_callback(&self, id: i32) -> Result<()> {
        if let Some(wrapper) = self.lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("EXEC: callback start (id={})", id));
        }
        let res = self.call_callback(id, MultiValue::new());
        if let Some(wrapper) = self.lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("EXEC: callback done (id={})", id));
        }
        res
    }

    pub fn execute_callback_with_args(&self, id: i32, args: MultiValue) -> Result<()> {
        self.call_callback(id, args)
    }

    pub fn call_user_command(&self, name: &str, args: &str) -> Result<()> {
        if let Some(wrapper) = self.lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            if let Some(&id) = state.user_commands.get(name) {
                let arg_table = self.lua.create_table().map_err(|e| NvimError::Api(format!("Failed to create arg table: {}", e)))?;
                arg_table.set("name", name).map_err(|e| NvimError::Api(format!("Failed to set name: {}", e)))?;
                arg_table.set("args", args).map_err(|e| NvimError::Api(format!("Failed to set args: {}", e)))?;
                self.call_callback(id, MultiValue::from_vec(vec![Value::Table(arg_table)]))?;
            }
        }
        Ok(())
    }

    pub fn load_plugins(&self, state: &mut VimState, config_dir: Option<std::path::PathBuf>) {
        state.config_dir = config_dir.clone();
        self.lua.set_app_data(StateWrapper(RefCell::new(state as *mut VimState)));
        
        // Register Vim API before loading any plugins or config
        if let Err(e) = self.register_api(state.buffers.clone(), state.sender.clone()) {
            state.log(&format!("Failed to register API: {}", e));
        }

        let rneovim_config = config_dir.unwrap_or_else(Self::get_config_dir);
        let init_lua = rneovim_config.join("init.lua");
        if init_lua.exists() {
            std::env::set_var("MYVIMRC", init_lua.to_string_lossy().to_string());
            state.log(&format!("Loading init.lua from {}", init_lua.display()));
            
            // Add config dir to package.path
            let config_dir_str = rneovim_config.to_string_lossy();
            let pkg_setup = format!(r#"package.path = package.path .. ";{}/?.lua;{}/?/init.lua""#, config_dir_str, config_dir_str);
            let _ = self.execute(&pkg_setup);

            let abs_path = std::fs::canonicalize(&init_lua).unwrap_or(init_lua);
            let code = format!(r#"
                local status, err = pcall(function()
                    return loadfile([[{}]])()
                end)
                if not status then
                    vim.notify("Init Error: " .. tostring(err), vim.log.levels.ERROR)
                    return err
                end
            "#, abs_path.to_string_lossy());
            if let Err(e) = self.execute(&code) { state.log(&format!("Init Error: {}", e)); }
        }
    }
}
