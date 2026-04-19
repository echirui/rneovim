use crate::nvim::state::{VimState, Mode, VisualMode};
use crate::nvim::request::{Request, Operator, Motion, TextObject};
use crate::nvim::api::handle_request;

pub struct KeyProcessor;

impl KeyProcessor {
    pub fn new() -> Self {
        Self
    }

    pub fn process_key(&self, state: &mut VimState, key: char) -> Option<Request> {
        if state.pending_key == Some('\x1C') {
            state.pending_key = None;
            if key == '\x0E' { // <C-\><C-N>
                return Some(Request::SetMode(Mode::Normal));
            }
        } else if key == '\x1C' {
            state.pending_key = Some('\x1C');
            return None;
        }

        match state.mode() {
            Mode::InsertLiteral => {
                state.set_mode(Mode::Insert);
                Some(Request::InsertChar(key))
            }
            Mode::InsertDigraph1(c1) => {
                if c1 == '\0' {
                    state.mode = Mode::InsertDigraph1(key);
                    None
                } else {
                    state.set_mode(Mode::Insert);
                    Some(Request::InsertChar(digraph_lookup(c1, key)))
                }
            }
            Mode::InsertOnce => {
                let req = self.handle_normal_mode(state, key);
                state.set_mode(Mode::Insert);
                req
            }
            Mode::BlockInsert { .. } => {
                if key == '\x1B' {
                    state.set_mode(Mode::Normal);
                    None
                } else {
                    Some(Request::InsertChar(key))
                }
            }
            Mode::Normal => self.handle_normal_mode(state, key),
            Mode::Insert => self.handle_insert_mode(state, key),
            Mode::Replace | Mode::VirtualReplace => self.handle_replace_mode(state, key),
            Mode::InsertWaitRegister => {
                let reg_content = state.registers.get(&key).cloned().unwrap_or_default();
                state.set_mode(Mode::Insert); 
                for c in reg_content.chars() { let _ = handle_request(state, Request::InsertChar(c)); }
                None
            }
            Mode::Visual(vmode) => self.handle_visual_mode(state, vmode, key),
            Mode::CommandLine | Mode::Search => self.handle_cmdline_mode(state, key),
            Mode::CommandLineWaitRegister => {
                let reg_content = state.registers.get(&key).cloned().unwrap_or_default();
                state.set_mode(Mode::CommandLine); 
                for c in reg_content.chars() { let _ = handle_request(state, Request::CmdLineChar(c)); }
                None
            }
            Mode::WaitMacroReg => {
                state.set_mode(Mode::Normal);
                Some(Request::PlayMacro(key))
            }
            Mode::WaitReplaceChar => {
                state.set_mode(Mode::Normal);
                Some(Request::ReplaceChar(key))
            }
        }
    }

    fn handle_normal_mode(&self, state: &mut VimState, key: char) -> Option<Request> {
        match (state.pending_op, state.pending_key, key) {
            (Some(_), Some('"'), _) | (None, Some('"'), _) => { state.pending_key = None; state.pending_register = Some(key); None }
            (Some(op), Some('i'), key) => {
                let obj = match key {
                    'w' => Some(TextObject::Word), 'W' => Some(TextObject::BigWord),
                    '(' | ')' | 'b' => Some(TextObject::Paren), '[' | ']' => Some(TextObject::Bracket),
                    '{' | '}' | 'B' => Some(TextObject::Brace), '<' | '>' => Some(TextObject::AngleBracket),
                    '\'' => Some(TextObject::Quote), '"' => Some(TextObject::DoubleQuote), '`' => Some(TextObject::Backtick),
                    's' => Some(TextObject::Sentence), 'p' => Some(TextObject::Paragraph),
                    _ => None,

                };
                if let Some(obj) = obj {
                    state.pending_op = None; state.pending_key = None; 
                    Some(Request::OpTextObject { op, inner: true, obj })
                } else { None }
            }
            (Some(op), Some('a'), key) => {
                let obj = match key {
                    'w' => Some(TextObject::Word), 'W' => Some(TextObject::BigWord),
                    '(' | ')' | 'b' => Some(TextObject::Paren), '[' | ']' => Some(TextObject::Bracket),
                    '{' | '}' | 'B' => Some(TextObject::Brace), '<' | '>' => Some(TextObject::AngleBracket),
                    '\'' => Some(TextObject::Quote), '"' => Some(TextObject::DoubleQuote), '`' => Some(TextObject::Backtick),
                    's' => Some(TextObject::Sentence), 'p' => Some(TextObject::Paragraph),
                    _ => None,

                };
                if let Some(obj) = obj {
                    state.pending_op = None; state.pending_key = None; 
                    Some(Request::OpTextObject { op, inner: false, obj })
                } else { None }
            }
            
            (Some(Operator::Delete), None, 'd') => { state.pending_op = None; Some(Request::DeleteCurrentLine) }
            (Some(Operator::Yank), None, 'y') => { state.pending_op = None; Some(Request::YankLine) }
            (Some(Operator::Indent), None, '>') => { state.pending_op = None; Some(Request::Indent { forward: true }) }
            (Some(Operator::Outdent), None, '<') => { state.pending_op = None; Some(Request::Indent { forward: false }) }
            (Some(op), None, 'w') => { state.pending_op = None; Some(Request::OpMotion { op, motion: Motion::WordForward }) }
            (Some(op), None, 'b') => { state.pending_op = None; Some(Request::OpMotion { op, motion: Motion::WordBack }) }
            (Some(op), None, 'e') => { state.pending_op = None; Some(Request::OpMotion { op, motion: Motion::WordForwardEnd }) }
            (Some(op), None, '$') => { state.pending_op = None; Some(Request::OpMotion { op, motion: Motion::LineEnd }) }
            (Some(op), None, '0') => { state.pending_op = None; Some(Request::OpMotion { op, motion: Motion::LineStart }) }

            (None, Some('f'), _) => { state.pending_key = None; Some(Request::FindChar { forward: true, target: key, till: false }) }
            (None, Some('F'), _) => { state.pending_key = None; Some(Request::FindChar { forward: false, target: key, till: false }) }
            (None, Some('t'), _) => { state.pending_key = None; Some(Request::FindChar { forward: true, target: key, till: true }) }
            (None, Some('T'), _) => { state.pending_key = None; Some(Request::FindChar { forward: false, target: key, till: true }) }
            (None, Some('\x17'), key) => { // <C-W>
                state.pending_key = None;
                match key {
                    's' => Some(Request::SplitWindow { vertical: false }),
                    'v' => Some(Request::SplitWindow { vertical: true }),
                    'w' | 'j' => Some(Request::NextWindow),
                    'W' | 'k' => Some(Request::PrevWindow),
                    'h' => Some(Request::OpMotion { op: Operator::None, motion: Motion::Left }),
                    'l' => Some(Request::OpMotion { op: Operator::None, motion: Motion::Right }),
                    'q' | 'c' => Some(Request::ExecuteCommandFromConfig("close".to_string())),
                    'o' => Some(Request::ExecuteCommandFromConfig("only".to_string())),
                    _ => None,
                }
            }
            (None, Some('m'), _) => { state.pending_key = None; Some(Request::SetMark(key)) }
            (None, Some('\''), _) | (None, Some('`'), _) => { state.pending_key = None; Some(Request::JumpToMark(key)) }
            (None, None, 'r') => { state.set_mode(Mode::WaitReplaceChar); None }

            (None, Some('g'), 'a') => { state.pending_key = None; Some(Request::ExecuteCommandFromConfig("ascii".to_string())) }
            (None, Some('g'), '&') => { state.pending_key = None; Some(Request::ExecuteCommandFromConfig("%&".to_string())) }
            (None, Some('g'), 'v') => { state.pending_key = None; Some(Request::RestoreVisualSelection) }
            (None, Some('g'), 'I') => { state.pending_key = None; Some(Request::InsertAtFirstColumn) }
            (None, Some('g'), 'i') => { state.pending_key = None; Some(Request::InsertAtLastInsert) }
            (None, Some('g'), 'J') => { state.pending_key = None; Some(Request::ExecuteCommandFromConfig("join!".to_string())) }
            (None, Some('g'), 'm') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::LineMiddle }) }
            (None, Some('g'), 'o') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::BufferByteOffset }) }
            (None, Some('g'), 'j') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::Down }) }
            (None, Some('g'), 'k') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::Up }) }
            (None, Some('g'), '0') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::LineStart }) }
            (None, Some('g'), '^') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::LineFirstChar }) }
            (None, Some('g'), '$') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::LineEnd }) }
            (None, Some('g'), 'u') => { state.pending_op = Some(Operator::ToLower); state.pending_key = None; None }
            (None, Some('g'), 'U') => { state.pending_op = Some(Operator::ToUpper); state.pending_key = None; None }
            (None, Some('g'), '~') => { state.pending_op = Some(Operator::SwapCase); state.pending_key = None; None }
            (None, Some('g'), 'g') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::BufferStart }) }
            (None, Some('g'), '_') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::LineLastChar }) }
            (None, Some('g'), 'e') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::WordBackwardEnd }) }
            (None, Some('g'), 'E') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::WordBackwardEndBlank }) }
            (None, Some('g'), '*') => { state.pending_key = None; Some(Request::SearchWordUnderCursor { forward: true, whole_word: false }) }
            (None, Some('g'), '#') => { state.pending_key = None; Some(Request::SearchWordUnderCursor { forward: false, whole_word: false }) }
            (None, Some('g'), 'n') => { state.pending_key = None; Some(Request::SelectSearchMatch { forward: true }) }
            (None, Some('g'), 'N') => { state.pending_key = None; Some(Request::SelectSearchMatch { forward: false }) }
            (None, Some('g'), ';') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::PrevChange }) }
            (None, Some('g'), ',') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::NextChange }) }

            (None, Some('z'), 'z') => { state.pending_key = None; Some(Request::CenterCursor) }
            (None, Some('z'), 't') => { state.pending_key = None; Some(Request::CursorToTop) }
            (None, Some('z'), 'b') => { state.pending_key = None; Some(Request::CursorToBottom) }
            (None, Some('z'), '\r') | (None, Some('z'), '\n') => { state.pending_key = None; Some(Request::CursorToTopWithCR) }
            (None, Some('z'), '.') => { state.pending_key = None; Some(Request::CursorToCenterWithDot) }
            (None, Some('z'), '-') => { state.pending_key = None; Some(Request::CursorToBottomWithMinus) }

            (None, Some('Z'), 'Z') => { state.pending_key = None; Some(Request::ExecuteCommandFromConfig("x".to_string())) }
            (None, Some('Z'), 'Q') => { state.pending_key = None; Some(Request::ExecuteCommandFromConfig("qa!".to_string())) }

            (None, Some('['), '[') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionBackward }) }
            (None, Some(']'), ']') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionForward }) }
            (None, Some('['), ']') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionEndBackward }) }
            (None, Some(']'), '[') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionEndForward }) }
            (None, Some('['), '(') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::ParenBackward }) }
            (None, Some(']'), ')') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::ParenForward }) }
            (None, Some('['), '{') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::BraceBackward }) }
            (None, Some(']'), '}') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::BraceForward }) }
            (None, Some('['), 'd') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::DiagnosticBackward }) }
            (None, Some(']'), 'd') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::DiagnosticForward }) }
            (None, None, 'd') => { state.pending_op = Some(Operator::Delete); None }
            (None, None, 'y') => { state.pending_op = Some(Operator::Yank); None }
            (None, None, 'c') => { state.pending_op = Some(Operator::Change); None }
            (None, None, 'D') => Some(Request::OpMotion { op: Operator::Delete, motion: Motion::LineEnd }),
            (None, None, 'C') => {
                let _ = handle_request(state, Request::OpMotion { op: Operator::Delete, motion: Motion::LineEnd });
                Some(Request::SetMode(Mode::Insert))
            }
            (None, None, 'Y') => Some(Request::OpMotion { op: Operator::Yank, motion: Motion::LineEnd }),
            (None, None, 'J') => Some(Request::ExecuteCommandFromConfig("join".to_string())),
            (None, None, '\x05') => Some(Request::Scroll { lines: 1 }), // <C-E>
            (None, None, '\x19') => Some(Request::Scroll { lines: -1 }), // <C-Y>
            (None, None, '>') => { state.pending_op = Some(Operator::Indent); None }
            (None, None, '<') => { state.pending_op = Some(Operator::Outdent); None }
            (None, None, 'f') | (None, None, 'F') | (None, None, 't') | (None, None, 'T') | (None, None, 'g') | (None, None, 'm') | (None, None, '\'') | (None, None, '`') | (None, None, '"') | (None, None, '\x17') | (None, None, 'z') | (None, None, '[') | (None, None, ']') | (None, None, 'Z') => { state.pending_key = Some(key); None }


            (None, None, 'j') => Some(Request::OpMotion { op: Operator::None, motion: Motion::Down }),
            (None, None, 'k') => Some(Request::OpMotion { op: Operator::None, motion: Motion::Up }),
            (None, None, 'h') => Some(Request::OpMotion { op: Operator::None, motion: Motion::Left }),
            (None, None, 'l') => Some(Request::OpMotion { op: Operator::None, motion: Motion::Right }),
            (None, None, 'w') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WordForward }),
            (None, None, 'W') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WordForwardBlank }),
            (None, None, 'b') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WordBackward }),
            (None, None, 'B') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WordBackwardBlank }),
            (None, None, 'e') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WordForwardEnd }),
            (None, None, 'E') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WordForwardEndBlank }),
            (None, None, '0') => Some(Request::OpMotion { op: Operator::None, motion: Motion::LineStart }),
            (None, None, '$') => Some(Request::OpMotion { op: Operator::None, motion: Motion::LineEnd }),
            (None, None, '^') => Some(Request::OpMotion { op: Operator::None, motion: Motion::LineFirstChar }),
            (None, None, '(') => Some(Request::OpMotion { op: Operator::None, motion: Motion::SentenceBackward }),
            (None, None, ')') => Some(Request::OpMotion { op: Operator::None, motion: Motion::SentenceForward }),
            (None, None, '{') => Some(Request::OpMotion { op: Operator::None, motion: Motion::ParagraphBackward }),
            (None, None, '}') => Some(Request::OpMotion { op: Operator::None, motion: Motion::ParagraphForward }),
            (None, None, 'G') => Some(Request::OpMotion { op: Operator::None, motion: Motion::BufferEnd }),
            (None, None, 'H') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WindowTop }),
            (None, None, 'M') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WindowMiddle }),
            (None, None, 'L') => Some(Request::OpMotion { op: Operator::None, motion: Motion::WindowBottom }),
            (None, None, 'p') => Some(Request::Paste),
            (None, None, 'P') => Some(Request::PasteBefore),
            (None, None, 'x') => Some(Request::DeleteCharAtCursor),
            (None, None, 'X') => Some(Request::Backspace),
            (None, None, 's') => { let _ = handle_request(state, Request::DeleteCharAtCursor); Some(Request::SetMode(Mode::Insert)) }
            (None, None, 'S') => { let _ = handle_request(state, Request::DeleteCurrentLine); let _ = handle_request(state, Request::OpenLine { below: false }); Some(Request::SetMode(Mode::Insert)) }
            (None, None, '~') => Some(Request::SwitchCase),
            (None, None, '&') => Some(Request::ExecuteCommandFromConfig("&".to_string())),
            (None, None, ';') => Some(Request::RepeatLastCharSearch),
            (None, None, '%') => Some(Request::JumpToPair),
            (None, None, '*') => Some(Request::SearchWordUnderCursor { forward: true, whole_word: true }),
            (None, None, '#') => Some(Request::SearchWordUnderCursor { forward: false, whole_word: true }),
            (None, None, 'n') => Some(Request::SearchNext { forward: true }),
            (None, None, 'N') => Some(Request::SearchNext { forward: false }),
            (None, None, '\x06') => { let h = state.current_window().height(); Some(Request::Scroll { lines: h as i32 }) }
            (None, None, '\x02') => { let h = state.current_window().height(); Some(Request::Scroll { lines: -(h as i32) }) }
            (None, None, '\x04') => { let h = state.current_window().height(); Some(Request::Scroll { lines: (h / 2) as i32 }) }
            (None, None, '\x15') => { let h = state.current_window().height(); Some(Request::Scroll { lines: -((h / 2) as i32) }) }
            (None, None, '\x0C') => Some(Request::Redraw), // <C-L>
            (None, None, '\x1E') => Some(Request::AlternateFile), // <C-^>
            (None, None, '\x07') => Some(Request::ExecuteCommandFromConfig("file".to_string())), // <C-G>
            (None, None, '\x0f') => Some(Request::JumpBack), // <C-O>
            (None, None, '\x09') => Some(Request::JumpForward), // <C-I>
            (None, None, 'u') => Some(Request::Undo),
            (None, None, '\x12') => Some(Request::Redo), // <C-R>
            (None, None, 'o') => Some(Request::OpenLine { below: true }),
            (None, None, 'O') => Some(Request::OpenLine { below: false }),
            (None, None, 'R') => { state.set_mode(Mode::Replace); None }
            (None, None, 'i') => Some(Request::SetMode(Mode::Insert)),
            (None, None, 'I') => { 
                let _ = handle_request(state, Request::OpMotion { op: Operator::None, motion: Motion::LineFirstChar });
                Some(Request::SetMode(Mode::Insert))
            }
            (None, None, 'a') => {
                let _ = handle_request(state, Request::OpMotion { op: Operator::None, motion: Motion::Right });
                Some(Request::SetMode(Mode::Insert))
            }
            (None, None, 'A') => {
                let _ = handle_request(state, Request::OpMotion { op: Operator::None, motion: Motion::LineEnd });
                let cur = state.current_window().cursor();
                let _ = handle_request(state, Request::CursorMove { row: cur.row, col: cur.col + 1 });
                Some(Request::SetMode(Mode::Insert))
            }
            (None, None, '\x01') => Some(Request::IncrementNumber { offset: 1 }), // <C-A>
            (None, None, '\x18') => Some(Request::IncrementNumber { offset: -1 }), // <C-X>
            (None, None, ':') => Some(Request::SetMode(Mode::CommandLine)),
            (None, None, '/') => Some(Request::SetMode(Mode::Search)),
            (None, None, '?') => Some(Request::SetMode(Mode::Search)),
            (None, None, 'q') => {
                if state.macro_recording.is_some() {
                    Some(Request::StopMacroRecord)
                } else {
                    state.pending_key = Some('q');
                    None
                }
            }
            (None, Some('q'), reg) => {
                state.pending_key = None;
                Some(Request::StartMacroRecord(reg))
            }
            (None, None, '@') => { state.set_mode(Mode::WaitMacroReg); None }
            (None, None, '.') => Some(Request::RepeatLastChange),
            (None, None, 'v') => Some(Request::SetVisualMode(VisualMode::Char)),
            (None, None, 'V') => Some(Request::SetVisualMode(VisualMode::Line)),
            (None, None, '\x16') => Some(Request::SetVisualMode(VisualMode::Block)),
            _ => None,
        }
    }

    fn handle_insert_mode(&self, state: &mut VimState, key: char) -> Option<Request> {
        match key {
            '\x1B' => Some(Request::SetMode(Mode::Normal)),
            '\x7F' | '\x08' => Some(Request::Backspace),
            '\x17' => Some(Request::InsertModeDeleteWord),
            '\x15' => Some(Request::InsertModeDeleteLine),
            '\x12' => { state.set_mode(Mode::InsertWaitRegister); None }
            '\x19' => Some(Request::InsertCopyFromLine { offset: -1 }),
            '\x05' => Some(Request::InsertCopyFromLine { offset: 1 }),
            '\x14' => Some(Request::Indent { forward: true }),
            '\x04' => Some(Request::Indent { forward: false }),
            '\x16' => { state.set_mode(Mode::InsertLiteral); None }
            '\x0B' => { state.set_mode(Mode::InsertDigraph1('\0')); None }
            '\x0E' => { if state.pum_items.is_some() { Some(Request::SelectPumNext) } else { Some(Request::TriggerCompletion { forward: true }) } }
            '\x10' => { if state.pum_items.is_some() { Some(Request::SelectPumPrev) } else { Some(Request::TriggerCompletion { forward: false }) } }
            '\x0f' => { state.set_mode(Mode::InsertOnce); None }
            '\r' | '\n' => { if state.pum_items.is_some() { Some(Request::CommitCompletion) } else { Some(Request::NewLine) } },
            _ => Some(Request::InsertChar(key)),
        }
    }

    fn handle_replace_mode(&self, state: &mut VimState, key: char) -> Option<Request> {
        match key {
            '\x1B' => Some(Request::SetMode(Mode::Normal)),
            '\x7F' | '\x08' => {
                let cur = state.current_window().cursor();
                if cur.col > 0 { Some(Request::CursorMove { row: cur.row, col: cur.col - 1 }) } else { None }
            }
            _ => Some(Request::ReplaceCharUnderCursor(key)),
        }
    }

    fn handle_visual_mode(&self, state: &mut VimState, _vmode: VisualMode, key: char) -> Option<Request> {
        match (state.pending_key, key) {
            (None, '\x1B') => { state.visual_anchor = None; Some(Request::SetMode(Mode::Normal)) },
            (None, 'j') | (None, 'k') | (None, 'h') | (None, 'l') => {
                let cur = state.current_window().cursor();
                match key {
                    'j' => Some(Request::CursorMove { row: cur.row + 1, col: cur.col }),
                    'k' => Some(Request::CursorMove { row: cur.row.saturating_sub(1).max(1), col: cur.col }),
                    'h' => Some(Request::CursorMove { row: cur.row, col: cur.col.saturating_sub(1) }),
                    'l' => Some(Request::CursorMove { row: cur.row, col: cur.col + 1 }),
                    _ => None,
                }
            },
            (None, '(') => Some(Request::OpMotion { op: Operator::None, motion: Motion::SentenceBackward }),
            (None, ')') => Some(Request::OpMotion { op: Operator::None, motion: Motion::SentenceForward }),
            (None, '{') => Some(Request::OpMotion { op: Operator::None, motion: Motion::ParagraphBackward }),
            (None, '}') => Some(Request::OpMotion { op: Operator::None, motion: Motion::ParagraphForward }),
            (None, 'd') | (None, 'x') => Some(Request::OpVisualSelection(Operator::Delete)),
            (None, 'y') => Some(Request::OpVisualSelection(Operator::Yank)),
            (None, 'c') => Some(Request::OpVisualSelection(Operator::Change)),
            (None, 'I') => Some(Request::BlockInsert { append: false }),
            (None, 'A') => Some(Request::BlockInsert { append: true }),
            (None, 'u') => Some(Request::OpVisualSelection(Operator::ToLower)),
            (None, 'U') => Some(Request::OpVisualSelection(Operator::ToUpper)),
            (None, '~') => Some(Request::OpVisualSelection(Operator::SwapCase)),
            (None, '>') => Some(Request::OpVisualSelection(Operator::Indent)),
            (None, '<') => Some(Request::OpVisualSelection(Operator::Outdent)),
            (None, 'o') | (None, 'O') => {
                let cur = state.current_window().cursor();
                let anchor = state.visual_anchor.unwrap_or(cur);
                state.visual_anchor = Some(cur);
                state.current_window_mut().set_cursor(anchor.row, anchor.col);
                None
            }
            (None, 'i') | (None, 'a') | (None, '[') | (None, ']') => { state.pending_key = Some(key); None }
            (Some('['), '[') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionBackward }) }
            (Some(']'), ']') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionForward }) }
            (Some('['), ']') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionEndBackward }) }
            (Some(']'), '[') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::SectionEndForward }) }
            (Some('['), '(') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::ParenBackward }) }
            (Some(']'), ')') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::ParenForward }) }
            (Some('['), '{') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::BraceBackward }) }
            (Some(']'), '}') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::BraceForward }) }
            (Some('['), 'd') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::DiagnosticBackward }) }
            (Some(']'), 'd') => { state.pending_key = None; Some(Request::OpMotion { op: Operator::None, motion: Motion::DiagnosticForward }) }

            (Some('i'), key) => {
                let obj = match key {
                    'w' => Some(TextObject::Word), 'W' => Some(TextObject::BigWord),
                    '(' | ')' | 'b' => Some(TextObject::Paren), '[' | ']' => Some(TextObject::Bracket),
                    '{' | '}' | 'B' => Some(TextObject::Brace), '<' | '>' => Some(TextObject::AngleBracket),
                    '\'' => Some(TextObject::Quote), '"' => Some(TextObject::DoubleQuote), '`' => Some(TextObject::Backtick),
                    's' => Some(TextObject::Sentence), 'p' => Some(TextObject::Paragraph),
                    _ => None,

                };
                if let Some(obj) = obj { state.pending_key = None; Some(Request::OpTextObject { op: Operator::None, inner: true, obj }) } else { None }
            }
            (Some('a'), key) => {
                let obj = match key {
                    'w' => Some(TextObject::Word), 'W' => Some(TextObject::BigWord),
                    '(' | ')' | 'b' => Some(TextObject::Paren), '[' | ']' => Some(TextObject::Bracket),
                    '{' | '}' | 'B' => Some(TextObject::Brace), '<' | '>' => Some(TextObject::AngleBracket),
                    '\'' => Some(TextObject::Quote), '"' => Some(TextObject::DoubleQuote), '`' => Some(TextObject::Backtick),
                    's' => Some(TextObject::Sentence), 'p' => Some(TextObject::Paragraph),
                    _ => None,

                };
                if let Some(obj) = obj { state.pending_key = None; Some(Request::OpTextObject { op: Operator::None, inner: false, obj }) } else { None }
            }
            _ => None,
        }
    }

    fn handle_cmdline_mode(&self, state: &mut VimState, key: char) -> Option<Request> {
        match key {
            '\x1B' => Some(Request::SetMode(Mode::Normal)),
            '\x7F' | '\x08' => Some(Request::CmdLineBackspace),
            '\x17' => Some(Request::CmdLineDeleteWord),
            '\x15' => Some(Request::CmdLineDeleteLine),
            '\x12' => { state.set_mode(Mode::CommandLineWaitRegister); None }
            '\x02' => Some(Request::CmdLineMoveCursorTo { pos: 0 }),
            '\x05' => Some(Request::CmdLineMoveCursorTo { pos: 9999 }),
            '\x01' => Some(Request::CmdLineMoveCursorTo { pos: 0 }),
            '\x10' => Some(Request::CmdLineHistory { forward: false }),
            '\x0E' => Some(Request::CmdLineHistory { forward: true }),
            '\t' => Some(Request::CmdLineTab),
            '\r' | '\n' => {
                if state.mode() == Mode::CommandLine { Some(Request::ExecuteCommand) } else { Some(Request::ExecuteSearch) }
            },
            _ => Some(Request::CmdLineChar(key)),
        }
    }
}

fn digraph_lookup(_c1: char, _c2: char) -> char {
    // Placeholder
    '?'
}
