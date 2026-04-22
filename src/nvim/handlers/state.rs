use crate::nvim::state::{VimState, Mode, AutoCmdEvent};
use crate::nvim::request::Request;
use crate::nvim::error::Result;
use crate::nvim::api::{handle_request, trigger_autocmd};

pub fn handle(state: &mut VimState, req: Request) -> Result<()> {
    match req {
        Request::SetMode(m) => {
            let old_mode = state.mode.clone();
            if m == Mode::Search && old_mode != Mode::Search {
                state.search_start_cursor = Some(state.current_window().cursor());
            }
            state.set_mode(m.clone());
            if m == Mode::CommandLine || m == Mode::Search {
                state.set_cmdline(String::new());
                state.cmdline_cursor = 0;
            }
            
            // オートコマンドの発火
            if m == Mode::Insert && old_mode != Mode::Insert {
                trigger_autocmd(state, AutoCmdEvent::InsertEnter);
            } else if old_mode == Mode::Insert && m != Mode::Insert {
                trigger_autocmd(state, AutoCmdEvent::InsertLeave);
            }
        }
        Request::SetVisualMode(vmode) => {
            if !matches!(state.mode, Mode::Visual(_)) {
                state.visual_anchor = Some(state.current_window().cursor());
            }
            state.set_mode(Mode::Visual(vmode));
        }
        Request::StartMacroRecord(reg) => {
            state.macro_recording = Some((reg, Vec::new()));
        }
        Request::StopMacroRecord => {
            if let Some((reg, recorded_reqs)) = state.macro_recording.take() {
                state.macros.insert(reg, recorded_reqs);
            }
        }
        Request::PlayMacro(reg) => {
            if let Some(reqs) = state.macros.get(&reg).cloned() {
                for r in reqs {
                    handle_request(state, r)?;
                }
            }
        }
        Request::Quit | Request::QuitAll => {
            state.quit();
        }
        Request::SaveShaDa(path) => {
            let data = crate::nvim::shada::ShaDaData::from_state(state);
            data.save(&path)?;
        }
        Request::LoadShaDa(path) => {
            let data = crate::nvim::shada::ShaDaData::load(&path)?;
            data.apply_to_state(state);
        }
        Request::SaveSession(path) => {
            let data = crate::nvim::session::SessionData::from_state(state);
            let json = serde_json::to_string(&data).map_err(|e| crate::nvim::error::NvimError::Api(e.to_string()))?;
            std::fs::write(path, json)?;
        }
        Request::LoadSession(path) => {
            let json = std::fs::read_to_string(path)?;
            let data: crate::nvim::session::SessionData = serde_json::from_str(&json).map_err(|e| crate::nvim::error::NvimError::Api(e.to_string()))?;
            data.apply_to_state(state)?;
        }
        Request::Redraw => {
            state.redraw();
        }
        Request::SetRegister { reg, text } => {
            crate::nvim::handlers::set_register_text(state, reg, text);
        }
        Request::SetMark(c) => {
            let win = state.current_window();
            let cur = win.cursor();
            win.buffer().borrow_mut().set_mark(c, cur);
        }
        _ => {}
    }
    Ok(())
}
