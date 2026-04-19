/// Neovim's string utilities re-implemented in idiomatic Rust.

pub struct StringBuilder {
    inner: String,
}

impl StringBuilder {
    pub fn new() -> Self {
        Self { inner: String::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { inner: String::with_capacity(capacity) }
    }

    pub fn append(&mut self, s: &str) {
        self.inner.push_str(s);
    }

    pub fn append_char(&mut self, c: char) {
        self.inner.push(c);
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn to_string(self) -> String {
        self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Precedes by a backslash all characters from `chars` in string `s`.
/// Equivalent to Neovim's vim_strsave_escaped.
pub fn vim_strsave_escaped(s: &str, chars: &str) -> String {
    let mut res = String::with_capacity(s.len());
    for c in s.chars() {
        if chars.contains(c) {
            res.push('\\');
        }
        res.push(c);
    }
    res
}

/// Removes quotes from a string and performs unescaping.
/// Equivalent to Neovim's vim_strnsave_unquoted.
pub fn vim_strnsave_unquoted(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let mut in_quotes = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == '\\' && in_quotes {
            if let Some(&next) = chars.peek() {
                if next == '"' || next == '\\' {
                    res.push(chars.next().unwrap());
                } else {
                    res.push(c);
                }
            } else {
                res.push(c);
            }
        } else {
            res.push(c);
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_builder() {
        let mut sb = StringBuilder::new();
        sb.append("Hello");
        sb.append(", ");
        sb.append("World!");
        assert_eq!(sb.as_str(), "Hello, World!");
    }

    #[test]
    fn test_vim_strsave_escaped() {
        // From strings_spec.lua
        assert_eq!(vim_strsave_escaped("abcd", "abcd"), r"\a\b\c\d");
        assert_eq!(vim_strsave_escaped("abcd", "ab"), r"\a\bcd");
        assert_eq!(vim_strsave_escaped("text \n text", ""), "text \n text");
        assert_eq!(vim_strsave_escaped("", "\r"), "");
        assert_eq!(vim_strsave_escaped("some text", "a"), "some text");
        
        // Additional cases
        assert_eq!(vim_strsave_escaped("foo", "o"), r"f\o\o");
        assert_eq!(vim_strsave_escaped("bar", "z"), "bar");
    }

    #[test]
    fn test_vim_strnsave_unquoted() {
        // From strings_spec.lua assertions
        // itp('copies unquoted strings as-is')
        assert_eq!(vim_strnsave_unquoted("-c"), "-c");
        assert_eq!(vim_strnsave_unquoted(""), "");

        // itp('unquotes fully quoted word')
        assert_eq!(vim_strnsave_unquoted("\"/bin/sh\""), "/bin/sh");

        // itp('unquotes partially quoted word')
        assert_eq!(vim_strnsave_unquoted("/Program\" \"Files/sh"), "/Program Files/sh");

        // itp('removes ""')
        assert_eq!(vim_strnsave_unquoted("/\"\"Program\" \"Files/sh"), "/Program Files/sh");

        // itp('performs unescaping of "')
        assert_eq!(vim_strnsave_unquoted("/\"\\\"\"Program Files\"\\\"\"/sh"), "/\"Program Files\"/sh");

        // itp('performs unescaping of \\')
        assert_eq!(vim_strnsave_unquoted("/\"\\\\\"Program Files\"\\\\foo\"/sh"), "/\\Program Files\\foo/sh");

        // itp('strips quote when there is no pair to it')
        assert_eq!(vim_strnsave_unquoted("/Program\" Files/sh"), "/Program Files/sh");
        assert_eq!(vim_strnsave_unquoted("\""), "");

        // itp('allows string to end with one backslash unescaped')
        assert_eq!(vim_strnsave_unquoted("/Program\" Files/sh\\"), "/Program Files/sh\\");

        // itp('does not perform unescaping out of quotes')
        assert_eq!(vim_strnsave_unquoted("/Program\\ Files/sh\\"), "/Program\\ Files/sh\\");

        // itp('does not unescape \\n')
        assert_eq!(vim_strnsave_unquoted("/Program\"\\n\"Files/sh"), "/Program\\nFiles/sh");
        
        // Multi-byte test (UTF-8)
        assert_eq!(vim_strnsave_unquoted("\"こんにちは\""), "こんにちは");
    }
}
