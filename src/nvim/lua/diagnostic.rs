use mlua::{Lua, Table};

pub fn register_diagnostic(lua: &Lua, vim: &Table) -> mlua::Result<()> {
    let diagnostic = lua.create_table()?;

    diagnostic.set("config", lua.create_function(|_, _: Option<Table>| Ok(()))?)?;
    diagnostic.set("get", lua.create_function(|lua, _: Option<i32>| Ok(lua.create_table()?))?)?;
    diagnostic.set("set", lua.create_function(|_, (_bufnr, _ns_id, _diagnostics, _opts): (Option<i32>, i32, Table, Option<Table>)| Ok(()))?)?;
    diagnostic.set("show", lua.create_function(|_, _: (Option<i32>, Option<i32>, Option<Table>, Option<Table>)| Ok(()))?)?;
    diagnostic.set("hide", lua.create_function(|_, _: (Option<i32>, Option<i32>)| Ok(()))?)?;
    diagnostic.set("get_namespaces", lua.create_function(|lua, _: ()| Ok(lua.create_table()?))?)?;

    let severity = lua.create_table()?;
    severity.set("ERROR", 1)?;
    severity.set("WARN", 2)?;
    severity.set("INFO", 3)?;
    severity.set("HINT", 4)?;
    diagnostic.set("severity", severity)?;

    vim.set("diagnostic", diagnostic)?;
    Ok(())
}
