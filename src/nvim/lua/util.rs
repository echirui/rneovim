use mlua::{Lua, Table, Value, MultiValue, Function};

pub fn register_utils(lua: &Lua, vim: &Table) -> mlua::Result<()> {
    vim.set("deepcopy", lua.create_function(|lua, v: Value| {
        match v {
            Value::Table(t) => {
                let copy = lua.create_table()?;
                for pair in t.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    copy.set(k, v)?;
                }
                Ok(Value::Table(copy))
            }
            _ => Ok(v),
        }
    })?)?;


    vim.set("tbl_contains", lua.create_function(|_, (t, val): (Table, Value)| {
        for pair in t.pairs::<Value, Value>() {
            let (_, v) = pair?;
            if v == val { return Ok(true); }
        }
        Ok(false)
    })?)?;

    vim.set("tbl_map", lua.create_function(|lua, (f, t): (Function, Table)| {
        let res = lua.create_table()?;
        for pair in t.pairs::<Value, Value>() {
            let (k, v) = pair?;
            res.set(k, f.call::<Value>(v)?)?;
        }
        Ok(res)
    })?)?;

    vim.set("tbl_count", lua.create_function(|_, t: Table| {
        let mut count = 0;
        for _ in t.pairs::<Value, Value>() { count += 1; }
        Ok(count)
    })?)?;

    vim.set("tbl_keys", lua.create_function(|lua, t: Table| {
        let res = lua.create_table()?;
        for (i, pair) in t.pairs::<Value, Value>().enumerate() { 
            let (k, _) = pair?; 
            res.set(i + 1, k)?; 
        }
        Ok(res)
    })?)?;

    vim.set("tbl_values", lua.create_function(|lua, t: Table| {
        let res = lua.create_table()?;
        for (i, pair) in t.pairs::<Value, Value>().enumerate() { 
            let (_, v) = pair?; 
            res.set(i + 1, v)?; 
        }
        Ok(res)
    })?)?;

    vim.set("tbl_isempty", lua.create_function(|_, t: Table| Ok(t.pairs::<Value, Value>().next().is_none()))?)?;

    vim.set("tbl_extend", lua.create_function(|lua, (behavior, args): (String, MultiValue)| {
        let res = lua.create_table()?;
        for arg in args {
            if let Value::Table(t) = arg {
                for pair in t.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    if behavior == "force" || matches!(res.get::<Value>(k.clone())?, Value::Nil) {
                        res.set(k, v)?;
                    }
                }
            }
        }
        Ok(res)
    })?)?;

    vim.set("tbl_deep_extend", lua.create_function(|lua, (behavior, args): (String, MultiValue)| {
        let res = lua.create_table()?;
        for arg in args {
            if let Value::Table(t) = arg {
                deep_extend(lua, &behavior, res.clone(), t)?;
            }
        }
        Ok(res)
    })?)?;

    vim.set("tbl_filter", lua.create_function(|lua, (f, t): (mlua::Function, Table)| {
        let res = lua.create_table()?;
        let mut i = 1;
        for val in t.sequence_values::<Value>() {
            let v = val?;
            if f.call::<bool>(v.clone())? {
                res.set(i, v)?;
                i += 1;
            }
        }
        Ok(res)
    })?)?;

    vim.set("tbl_get", lua.create_function(|_, (t, keys): (Table, MultiValue)| {
        let mut cur = Value::Table(t);
        for k_val in keys {
            let k = match k_val {
                Value::String(s) => s.to_string_lossy().to_string(),
                _ => format!("{:?}", k_val),
            };
            if let Value::Table(tbl) = cur {
                cur = tbl.get(k).unwrap_or(Value::Nil);
                if matches!(cur, Value::Nil) { return Ok(Value::Nil); }
            } else {
                return Ok(Value::Nil);
            }
        }
        Ok(cur)
    })?)?;

    vim.set("list_extend", lua.create_function(|_, (dst, src, start, end): (Table, Table, Option<usize>, Option<usize>)| {
        let mut i = dst.len()? + 1;
        let start = start.unwrap_or(1);
        let end = end.unwrap_or(src.len()? as usize);
        for (idx, val) in src.sequence_values::<Value>().enumerate() {
            let idx = idx + 1;
            if idx >= start && idx <= end {
                dst.set(i, val?)?;
                i += 1;
            }
        }
        Ok(dst)
    })?)?;

    Ok(())
}

fn deep_extend(lua: &Lua, behavior: &str, dst: Table, src: Table) -> mlua::Result<()> {
    for pair in src.pairs::<Value, Value>() {
        let (k, v) = pair?;
        if let Value::Table(src_inner) = v {
            let dst_inner: Table = match dst.get(k.clone())? {
                Value::Table(t) => t,
                _ => {
                    let t = lua.create_table()?;
                    dst.set(k.clone(), t.clone())?;
                    t
                }
            };
            deep_extend(lua, behavior, dst_inner, src_inner)?;
        } else {
            if behavior == "force" || matches!(dst.get::<Value>(k.clone())?, Value::Nil) {
                dst.set(k, v)?;
            }
        }
    }
    Ok(())
}
