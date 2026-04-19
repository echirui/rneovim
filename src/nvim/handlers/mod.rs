pub mod state;
pub mod edit;
pub mod motion;
pub mod window;
pub mod cmdline;

use crate::nvim::state::VimState;

pub(crate) fn set_system_clipboard(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}

pub(crate) fn get_system_clipboard() -> Option<String> {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        cb.get_text().ok()
    } else {
        None
    }
}

pub(crate) fn set_register_text(state: &mut VimState, reg: char, value: String) {
    state.registers.insert(reg, value.clone());
    if reg == '+' || reg == '*' {
        set_system_clipboard(&value);
    }
}
