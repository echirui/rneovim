pub mod nvim;

use crate::nvim::state::VimState;
use crate::nvim::request::Request;
use crate::nvim::api::handle_request;
use crate::nvim::os::fs;

pub fn load_config(state: &mut VimState, path: &str) {
    if let Ok(lines) = fs::os_read_file(path) {
        for line in lines {
            if line.trim().is_empty() || line.starts_with('"') {
                continue;
            }
            // 各行をコマンドとして実行（現在は ":" から始まることを想定せず直接パース）
            let _ = handle_request(state, Request::ExecuteCommandFromConfig(line));
        }
    }
}

// Requestに ExecuteCommandFromConfig を追加する必要があるため、後で api.rs を更新します

