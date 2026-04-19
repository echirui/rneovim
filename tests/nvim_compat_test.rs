use std::process::Command;
use std::fs;
use rneovim::nvim::state::VimState;
use rneovim::nvim::api::handle_request;
use rneovim::nvim::request::Request;

/// 本家 Neovim を使って期待される結果を生成する
fn run_real_nvim(keys: &str, filename: &str) -> String {
    let temp_file = "real_nvim_result.txt";
    // nvim --headless -c "normal <keys>" -c "wq" filename
    // 注意: <keys> に特殊文字が含まれる場合はエスケープが必要です
    let status = Command::new("nvim")
        .arg("--headless")
        .arg("-u")
        .arg("NONE") // 設定を読み込まない
        .arg("-c")
        .arg(format!("normal {}", keys))
        .arg("-c")
        .arg(format!("w! {}", temp_file))
        .arg("-c")
        .arg("qa!")
        .arg(filename)
        .status()
        .expect("Failed to run real nvim");

    assert!(status.success());
    let result = fs::read_to_string(temp_file).expect("Failed to read nvim output");
    let _ = fs::remove_file(temp_file);
    result
}

/// rneovim (VimState) を使って結果を生成する
/// 簡易的なキー入力を Request に変換して実行する
fn run_rneovim_simulated(keys: &str, filename: &str) -> String {
    let mut state = VimState::new();
    
    // ファイルを開く
    if fs::metadata(filename).is_ok() {
        let _ = handle_request(&mut state, Request::OpenFile(filename.to_string()));
    }

    // TODO: ここで keys (文字列) を解析して Request に変換するロジックが必要
    // 現時点では、簡単な文字列挿入などのテスト用に手動でリクエストを送る形にするか、
    // キープロセッサをテスト環境で回す必要があります。
    
    // テスト用にバッファの内容を文字列として返す
    let buf = state.current_window().buffer();
    let b = buf.borrow();
    let mut result = String::new();
    for i in 1..=b.line_count() {
        if let Some(line) = b.get_line(i) {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

#[test]
#[ignore] // nvim がインストールされている環境でのみ実行
fn test_compat_simple_insert() {
    let filename = "test_input.py";
    fs::write(filename, "print('hello')\n").unwrap();
    
    let keys = "ggO# header\x1B"; // 行頭に移動して上に一行挿入
    
    let expected = run_real_nvim("ggO# header", filename);
    // rneovim 側でも同じことを行う（現在はシミュレーションが必要）
    // let actual = run_rneovim_simulated(keys, filename);
    
    // assert_eq!(actual, expected);
    
    fs::remove_file(filename).unwrap();
}
