use rneovim::nvim::state::VimState;
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn test_lsp_mock_integration() {
    let mut state = VimState::new();
    let (sender, receiver) = mpsc::channel();
    state.sender = Some(sender);

    // 1. ファイルを開く
    let test_file = "test_lsp_dummy.py";
    std::fs::write(test_file, "print('hello')\n").unwrap();
    handle_request(&mut state, Request::OpenFile(test_file.to_string())).unwrap();

    // 2. モックLSPサーバーを起動する
    let cmd = format!("python3 mock_lsp.py");
    handle_request(&mut state, Request::LspStart(cmd)).unwrap();

    // 3. 通信を待機（非同期イベントのコールバックを処理する簡易ループ）
    let mut diagnostics_received = false;
    
    // イベントループを少し回す（最大2秒）
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(100));
        while let Ok(cb) = receiver.try_recv() {
            cb(&mut state);
        }
        
        // エラーメッセージ（virtual_text）がバッファにセットされているか確認
        let buf = state.current_window().buffer();
        if buf.borrow().get_virtual_text(1).is_some() {
            diagnostics_received = true;
            break;
        }
    }

    std::fs::remove_file(test_file).unwrap();

    assert!(diagnostics_received, "Failed to receive diagnostics from mock LSP");
    
    let buf = state.current_window().buffer();
    let vt = buf.borrow().get_virtual_text(1).cloned().unwrap();
    assert_eq!(vt, "■ Mock Error Found!");
}
