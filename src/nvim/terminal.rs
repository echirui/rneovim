#[derive(Debug, Clone, PartialEq)]
pub enum TermCommand {
    PutChar(char),
    SetFG(u8),
    SetBG(u8),
    ResetAttrs,
    ClearScreen,
    MoveCursor(usize, usize),
}

pub struct TerminalState {
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub fg: u8,
    pub bg: u8,
}

impl TerminalState {
    pub fn new() -> Self {
        Self {
            cursor_row: 0,
            cursor_col: 0,
            fg: 7, // White
            bg: 0, // Black
        }
    }

    pub fn parse(&mut self, input: &str) -> Vec<TermCommand> {
        let mut commands = Vec::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1B' {
                if let Some('[') = chars.next() {
                    // CSI シーケンスの簡易解析
                    let mut params = String::new();
                    while let Some(&next) = chars.peek() {
                        if next.is_ascii_digit() || next == ';' {
                            params.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if let Some(cmd_char) = chars.next() {
                        match cmd_char {
                            'm' => { // SGR (Select Graphic Rendition)
                                if params == "0" || params.is_empty() {
                                    commands.push(TermCommand::ResetAttrs);
                                } else if params.starts_with("3") {
                                    if let Ok(color) = params[1..].parse::<u8>() {
                                        commands.push(TermCommand::SetFG(color));
                                    }
                                }
                            }
                            'J' if params == "2" => {
                                commands.push(TermCommand::ClearScreen);
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                commands.push(TermCommand::PutChar(c));
            }
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_ansi_parsing() {
        let mut term = TerminalState::new();
        let input = "\x1B[31mHi\x1B[0m";
        let commands = term.parse(input);
        
        assert!(commands.iter().any(|c| matches!(c, TermCommand::SetFG(1))));
        assert!(commands.iter().any(|c| matches!(c, TermCommand::PutChar('H'))));
        assert!(commands.iter().any(|c| matches!(c, TermCommand::ResetAttrs)));
    }

    #[test]
    fn test_terminal_clear_screen() {
        let mut term = TerminalState::new();
        let commands = term.parse("\x1B[2J");
        assert!(commands.iter().any(|c| matches!(c, TermCommand::ClearScreen)));
    }
}
