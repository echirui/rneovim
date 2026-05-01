use mlua::{Lua, Table, Value};
use crate::nvim::lua::StateWrapper;

pub fn register_treesitter(lua: &Lua, vim: &Table) -> mlua::Result<()> {
    let ts = lua.create_table()?;

    ts.set("get_parser", lua.create_function(|ctx, (bufnr, lang): (Option<i32>, Option<String>)| {
        if let Some(wrapper) = ctx.app_data_ref::<StateWrapper>() {
            let state = unsafe { &mut **wrapper.0.as_ptr() };
            state.log(&format!("TS: get_parser for bufnr={:?}, lang={:?}", bufnr, lang));
        }
        
        let parser = ctx.create_table()?;
        parser.set("parse", ctx.create_function(|c, _: ()| {
            Ok(c.create_table()?)
        })?)?;
        parser.set("get_tree", ctx.create_function(|c, _: ()| {
            Ok(c.create_table()?)
        })?)?;
        Ok(parser)
    })?)?;

    ts.set("get_node", lua.create_function(|_, _: ()| {
        Ok(Value::Nil)
    })?)?;

    ts.set("get_string_parser", lua.create_function(|ctx, (_source, lang): (String, String)| {
        if let Some(wrapper) = ctx.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("TS: get_string_parser for lang={}", lang));
        }
        let parser = ctx.create_table()?;
        parser.set("parse", ctx.create_function(|c, _: ()| {
            Ok(c.create_table()?)
        })?)?;
        parser.set("get_tree", ctx.create_function(|c, _: ()| {
            Ok(c.create_table()?)
        })?)?;
        Ok(parser)
    })?)?;

    let query = lua.create_table()?;
    query.set("parse", lua.create_function(|ctx, (lang, query_str): (String, String)| {
        if let Some(wrapper) = ctx.app_data_ref::<StateWrapper>() {
            let state = unsafe { &**wrapper.0.as_ptr() };
            state.log(&format!("TS: query.parse for lang={}, query={}", lang, query_str));
        }
        let q = ctx.create_table()?;
        q.set("iter_captures", ctx.create_function(|c, _: ()| {
            Ok(c.create_function(|_, _: ()| Ok(Value::Nil))?)
        })?)?;
        Ok(q)
    })?)?;
    ts.set("query", query)?;

    vim.set("treesitter", ts)?;
    Ok(())
}
