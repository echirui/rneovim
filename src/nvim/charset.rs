/// Neovim's charset utilities re-implemented in idiomatic Rust.

/// Returns true if 'c' is a white space character: space or tab.
/// Equivalent to Neovim's vim_iswhite.
pub fn vim_iswhite(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Returns true if 'c' is a digit.
pub fn vim_isdigit(c: char) -> bool {
    c.is_ascii_digit()
}

/// Returns true if 'c' is an identifier character.
pub fn vim_isident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Returns true if 'c' is a hexadecimal digit.
pub fn vim_isxdigit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_iswhite() {
        assert!(vim_iswhite(' '));
        assert!(vim_iswhite('\t'));
        assert!(!vim_iswhite('a'));
        assert!(!vim_iswhite('\n'));
        assert!(!vim_iswhite('\r'));
    }

    #[test]
    fn test_vim_isdigit() {
        for c in '0'..='9' {
            assert!(vim_isdigit(c));
        }
        assert!(!vim_isdigit('a'));
        assert!(!vim_isdigit(' '));
    }

    #[test]
    fn test_vim_isident_ascii() {
        assert!(vim_isident('a'));
    }

    #[test]
    fn test_vim_isident_digit() {
        assert!(vim_isident('1'));
    }

    #[test]
    fn test_vim_isident_underscore() {
        assert!(vim_isident('_'));
    }

    #[test]
    fn test_vim_isident_mbyte() {
        assert!(vim_isident('あ')); // Japanese
        assert!(vim_isident('漢')); // Kanji
    }

    #[test]
    fn test_vim_isident_false() {
        assert!(!vim_isident(' '));
        assert!(!vim_isident('!'));
    }

    #[test]
    fn test_vim_isxdigit() {
        for c in '0'..='9' {
            assert!(vim_isxdigit(c));
        }
        for c in 'a'..='f' {
            assert!(vim_isxdigit(c));
        }
        for c in 'A'..='F' {
            assert!(vim_isxdigit(c));
        }
        assert!(!vim_isxdigit('g'));
        assert!(!vim_isxdigit('G'));
    }
}
