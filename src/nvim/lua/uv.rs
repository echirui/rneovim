use mlua::{Lua, Table, Value, MultiValue, Function};
use crate::nvim::lua::{StateWrapper, LuaEnv};

pub fn register_uv(lua: &Lua, vim: &Table) -> mlua::Result<()> {
    let uv = lua.create_table()?;
    
    uv.set("fs_stat", lua.create_function(|lua, path: String| {
        use std::os::unix::fs::MetadataExt;
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("UV CALL: fs_stat({})", path));
        }

        if path.contains("lazy") && path.contains("cache") {
            return Ok(MultiValue::from_vec(vec![Value::Nil, Value::String(lua.create_string("ENOENT: no such file or directory")?)]));
        }

        match std::fs::metadata(&path) {
            Ok(meta) => {
                let t = lua.create_table()?;
                let is_dir = meta.is_dir();
                let kind = if is_dir { "directory" } else { "file" };
                t.set("type", kind)?;
                t.set("size", meta.len() as i64)?;
                t.set("mode", meta.mode() as i64)?;
                t.set("dev", meta.dev() as i64)?;
                t.set("ino", meta.ino() as i64)?;
                t.set("nlink", meta.nlink() as i64)?;
                t.set("uid", meta.uid() as i64)?;
                t.set("gid", meta.gid() as i64)?;
                t.set("rdev", meta.rdev() as i64)?;
                
                let mtime_sec = meta.mtime() as i64;
                let mtime_nsec = meta.mtime_nsec() as i64;
                
                let mtime_t = lua.create_table()?;
                mtime_t.set("sec", mtime_sec)?;
                mtime_t.set("nsec", mtime_nsec)?;
                t.set("mtime", mtime_t)?;
                
                let atime_t = lua.create_table()?; 
                atime_t.set("sec", meta.atime() as i64)?; 
                atime_t.set("nsec", meta.atime_nsec() as i64)?; 
                t.set("atime", atime_t)?;

                let ctime_t = lua.create_table()?; 
                ctime_t.set("sec", meta.ctime() as i64)?; 
                ctime_t.set("nsec", meta.ctime_nsec() as i64)?; 
                t.set("ctime", ctime_t)?;

                let btime_t = lua.create_table()?; 
                btime_t.set("sec", mtime_sec)?;
                btime_t.set("nsec", mtime_nsec)?;
                t.set("birthtime", btime_t)?;

                Ok(MultiValue::from_vec(vec![Value::Table(t), Value::Nil]))
            }
            Err(e) => {
                Ok(MultiValue::from_vec(vec![Value::Nil, Value::String(lua.create_string(&e.to_string())?)]))
            }
        }
    })?)?;

    uv.set("fs_lstat", lua.create_function(|lua, path: String| {
        use std::os::unix::fs::MetadataExt;
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("UV CALL: fs_lstat({})", path));
        }

        if path.contains("lazy") && path.contains("cache") {
            return Ok(MultiValue::from_vec(vec![Value::Nil, Value::String(lua.create_string("ENOENT: no such file or directory")?)]));
        }

        match std::fs::symlink_metadata(&path) {
            Ok(meta) => {
                let t = lua.create_table()?;
                let kind = if meta.is_dir() { "directory" } else if meta.is_symlink() { "link" } else { "file" };
                t.set("type", kind)?;
                t.set("size", meta.len() as i64)?;
                t.set("mode", meta.mode() as i64)?;
                t.set("dev", meta.dev() as i64)?;
                t.set("ino", meta.ino() as i64)?;
                
                let mtime_sec = meta.mtime() as i64;
                let mtime_nsec = meta.mtime_nsec() as i64;
                let mtime_t = lua.create_table()?;
                mtime_t.set("sec", mtime_sec)?;
                mtime_t.set("nsec", mtime_nsec)?;
                t.set("mtime", mtime_t)?;

                Ok(MultiValue::from_vec(vec![Value::Table(t), Value::Nil]))
            }
            Err(e) => {
                Ok(MultiValue::from_vec(vec![Value::Nil, Value::String(lua.create_string(&e.to_string())?)]))
            }
        }
    })?)?;

    uv.set("fs_utime", lua.create_function(|_, (_path, _atime, _mtime): (String, f64, f64)| {
        Ok(true)
    })?)?;

    uv.set("fs_access", lua.create_function(|_, (path, _mode): (String, String)| {
        Ok(std::path::Path::new(&path).exists())
    })?)?;

    uv.set("fs_chmod", lua.create_function(|_, (path, _mode): (String, u32)| {
        Ok(std::path::Path::new(&path).exists())
    })?)?;

    uv.set("fs_unlink", lua.create_function(|_, path: String| {
        let _ = std::fs::remove_file(path);
        Ok(true)
    })?)?;

    uv.set("fs_rename", lua.create_function(|lua, (old, new): (String, String)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("UV CALL: fs_rename({}, {})", old, new));
        }
        let _ = std::fs::rename(old, new);
        Ok(true)
    })?)?;

    uv.set("fs_scandir", lua.create_function(|lua, path: String| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Ok(entries) = std::fs::read_dir(&path) {
                let id = state.next_scandir_id; state.next_scandir_id += 1;
                state.active_scandirs.insert(id, entries);
                let ud = lua.create_table()?;
                ud.set("scandir_id", id)?;
                return Ok(MultiValue::from_vec(vec![Value::Table(ud), Value::Nil]));
            }
        }
        Ok(MultiValue::from_vec(vec![Value::Nil, Value::String(lua.create_string("failed to read dir")?)]))
    })?)?;

    uv.set("fs_scandir_next", lua.create_function(|lua, ud: Table| {
        let id: i32 = ud.get("scandir_id")?;
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            if let Some(entries) = state.active_scandirs.get_mut(&id) {
                if let Some(entry_res) = entries.next() {
                    if let Ok(entry) = entry_res {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let kind = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { "directory" } 
                                   else if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) { "link" }
                                   else { "file" };
                        return Ok(MultiValue::from_vec(vec![Value::String(lua.create_string(&name)?), Value::String(lua.create_string(kind)?)]));
                    }
                } else {
                    state.active_scandirs.remove(&id);
                }
            }
        }
        Ok(MultiValue::new())
    })?)?;

    uv.set("fs_mkdir", lua.create_function(|_, (path, _mode): (String, u32)| {
        std::fs::create_dir_all(&path).map(|_| true).map_err(|e| mlua::Error::runtime(format!("{}", e)))
    })?)?;

    uv.set("new_timer", lua.create_function(|lua, _: ()| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log("UV CALL: new_timer()");
        }
        Ok(LuaEnv::make_event_obj(lua)?)
    })?)?;
    uv.set("new_check", lua.create_function(|lua, _: ()| {
        let obj = lua.create_table()?;
        obj.set("start", lua.create_function(|lua, (t, cb): (Table, Function)| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                let env = state.lua_env.clone();
                if let Ok(id) = env.register_callback(Value::Function(cb)) {
                    state.check_callbacks.push(id);
                    t.set("__id", id)?;
                }
            }
            Ok(())
        })?)?;
        obj.set("stop", lua.create_function(|lua, t: Table| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                if let Ok(id) = t.get::<i32>("__id") {
                    state.check_callbacks.retain(|&x| x != id);
                }
            }
            Ok(())
        })?)?;
        obj.set("close", lua.create_function(|_, _: ()| Ok(()))?)?;
        Ok(obj)
    })?)?;
    uv.set("new_prepare", lua.create_function(|lua, _: ()| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log("UV CALL: new_prepare()");
        }
        Ok(LuaEnv::make_event_obj(lua)?)
    })?)?;
    uv.set("new_idle", lua.create_function(|lua, _: ()| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log("UV CALL: new_idle()");
        }
        Ok(LuaEnv::make_event_obj(lua)?)
    })?)?;
    uv.set("new_async", lua.create_function(|lua, cb: Function| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log("UV CALL: new_async()");
        }
        let obj = LuaEnv::make_event_obj(lua)?;
        let env = {
            let wrapper = lua.app_data_ref::<StateWrapper>().unwrap();
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.lua_env.clone()
        };
        let cb_id = env.register_callback(Value::Function(cb))?;
        obj.set("send", lua.create_function(move |lua, _: ()| {
            if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
                let state = unsafe { &mut **wrapper.0.as_ptr() };
                state.log("LUA: async:send() called");
                state.pending_callbacks.push(cb_id);
            }
            Ok(())
        })?)?;
        Ok(obj)
    })?)?;

    uv.set("hrtime", lua.create_function(|_, _: ()| {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let start = START.get_or_init(std::time::Instant::now);
        Ok(start.elapsed().as_nanos() as u64)
    })?)?;

    uv.set("os_uname", lua.create_function(|lua, _: ()| {
        let t = lua.create_table()?;
        t.set("sysname", if cfg!(target_os = "macos") { "Darwin" } else { "Linux" })?;
        t.set("release", "20.0.0")?; 
        t.set("version", "1.0")?; 
        t.set("machine", if cfg!(target_arch = "x86_64") { "x86_64" } else { "arm64" })?;
        Ok(t)
    })?)?;

    uv.set("os_homedir", lua.create_function(|lua, _: ()| {
        Ok(dirs::home_dir().map(|p| lua.create_string(p.to_string_lossy().as_ref()).unwrap()))
    })?)?;

    uv.set("fs_open", lua.create_function(|lua, (path, _flags, _mode): (String, String, i32)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("UV CALL: fs_open({})", path));
        }
        Ok(100) // Dummy fd
    })?)?;

    uv.set("fs_read", lua.create_function(|lua, (_fd, _len, _offset): (i32, usize, i32)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log("UV CALL: fs_read");
        }
        Ok("".to_string())
    })?)?;

    uv.set("fs_write", lua.create_function(|lua, (fd, data, _offset): (i32, String, i32)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("UV CALL: fs_write(fd={}, len={})", fd, data.len()));
        }
        Ok(data.len() as i32)
    })?)?;

    uv.set("fs_close", lua.create_function(|_, _: i32| Ok(true))?)?;

    uv.set("spawn", lua.create_function(|lua, (path, _opts, on_exit): (String, Table, Option<Function>)| {
        if let Some(wrapper) = lua.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("SPAWN (dummy): {}", path));
            if let Some(cb) = on_exit {
                let env = state.lua_env.clone();
                if let Ok(id) = env.register_callback(Value::Function(cb)) {
                    state.pending_callbacks.push(id);
                }
            }
        }
        Ok(LuaEnv::make_event_obj(lua)?)
    })?)?;

    uv.set("fs_fstat", lua.create_function(|lua, _fd: i32| {
        let t = lua.create_table()?;
        t.set("type", "file")?;
        t.set("size", 0)?;
        let mtime_t = lua.create_table()?;
        mtime_t.set("sec", 0)?;
        t.set("mtime", mtime_t)?;
        Ok(t)
    })?)?;

    uv.set("cwd", lua.create_function(|_, _: ()| {
        Ok(std::env::current_dir().unwrap_or_default().to_string_lossy().to_string())
    })?)?;

    uv.set("fs_realpath", lua.create_function(|lua, path: String| {
        if let Ok(p) = std::fs::canonicalize(&path) {
            Ok(MultiValue::from_vec(vec![Value::String(lua.create_string(p.to_string_lossy().as_ref())?), Value::Nil]))
        } else {
            Ok(MultiValue::from_vec(vec![Value::Nil, Value::String(lua.create_string("failed to get realpath")?)]))
        }
    })?)?;

    uv.set("fs_readdir", lua.create_function(|lua, path: String| {
        if let Ok(entries) = std::fs::read_dir(path) {
            let res = lua.create_table()?;
            for (i, e) in entries.enumerate() {
                if let Ok(entry) = e {
                    let name = entry.file_name().to_string_lossy().to_string();
                    res.set(i + 1, name)?;
                }
            }
            Ok(res)
        } else { Ok(lua.create_table()?) }
    })?)?;

    let _ = LuaEnv::add_missing_tracker(lua, &uv, "vim.uv");
    
    // Create vim.uv.fs as an alias for hierarchical access
    let fs = lua.create_table()?;
    fs.set("stat", uv.get::<Function>("fs_stat")?)?;
    fs.set("scandir", uv.get::<Function>("fs_scandir")?)?;
    fs.set("realpath", uv.get::<Function>("fs_realpath")?)?;
    uv.set("fs", fs)?;

    vim.set("loop", uv.clone())?;
    vim.set("uv", uv)?;

    Ok(())
}
