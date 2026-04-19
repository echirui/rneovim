use rneovim::nvim::state::VimState;
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;
use std::fs;
use std::path::Path;

#[test]
fn test_shada_save_load() {
    let shada_path = "test_shada.json";
    if Path::new(shada_path).exists() {
        fs::remove_file(shada_path).unwrap();
    }

    // 1. 状態を作成して保存
    {
        let mut state = VimState::new();
        {
            let buf = state.buffers[0].clone();
            let mut b = buf.borrow_mut();
            b.append_line("line 1").unwrap();
        }
        
        // マークをセット ('a' を 1行目, 0列目にセット)
        handle_request(&mut state, Request::SetMark('a')).unwrap();
        // レジスタにセット
        state.registers.insert('r', "hello shada".to_string());
        
        // 保存実行
        handle_request(&mut state, Request::SaveShaDa(shada_path.to_string())).unwrap();
    }

    // 2. 新しいインスタンスで復元
    {
        let mut state = VimState::new();
        // 読み込み実行
        handle_request(&mut state, Request::LoadShaDa(shada_path.to_string())).unwrap();
        
        // マークが復元されているか
        let buf = state.buffers[0].clone();
        let mark = buf.borrow().get_mark('a');
        assert!(mark.is_some());
        
        // レジスタが復元されているか
        assert_eq!(state.registers.get(&'r').unwrap(), "hello shada");
    }

    if Path::new(shada_path).exists() {
        fs::remove_file(shada_path).unwrap();
    }
}
