use mlua::{Lua, Table, Value};

#[test]
fn test_mlua_meta_error() {
    let lua = Lua::new();
    let globals = lua.globals();
    let t = lua.create_table().unwrap();
    let meta = lua.create_table().unwrap();
    meta.set("__index", lua.create_function(|_, (_t, k): (Table, String)| {
        Ok(Value::Nil)
    }).unwrap()).unwrap();
    t.set_metatable(Some(meta));
    globals.set("vim", t).unwrap();

    let res = lua.load("return vim[function() end]").eval::<Value>();
    println!("{:?}", res);
}
