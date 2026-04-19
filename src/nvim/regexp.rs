/// Vim 特有の正規表現を標準的な正規表現に変換する
pub fn convert_vim_regex(pattern: &str) -> String {
    let mut res = String::new();
    let mut chars = pattern.chars().peekable();
    let mut very_magic = false;

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(&next) = chars.peek() {
                    match next {
                        'v' => {
                            very_magic = true;
                            chars.next();
                        }
                        'z' => {
                            chars.next();
                            if let Some('s') | Some('e') = chars.peek() {
                                chars.next();
                            }
                        }
                        'd' => { res.push_str(r"\d"); chars.next(); }
                        'D' => { res.push_str(r"\D"); chars.next(); }
                        's' => { res.push_str(r"\s"); chars.next(); }
                        'S' => { res.push_str(r"\S"); chars.next(); }
                        'w' => { res.push_str(r"\w"); chars.next(); }
                        'W' => { res.push_str(r"\W"); chars.next(); }
                        '|' => { res.push('|'); chars.next(); }
                        _ => {
                            res.push('\\');
                            res.push(chars.next().unwrap());
                        }
                    }
                } else {
                    res.push('\\');
                }
            }
            '(' | ')' | '{' | '}' | '+' | '?' | '|' if !very_magic => {
                res.push('\\');
                res.push(c);
            }
            _ => res.push(c),
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_regex_conversion_advanced() {
        // Very Magic (\v)
        assert_eq!(convert_vim_regex(r"\v(a|b)+"), "(a|b)+");
        
        // Character classes
        assert_eq!(convert_vim_regex(r"\d+"), r"\d\+");
        assert_eq!(convert_vim_regex(r"\s*"), r"\s*");
        
        // Logical OR in normal magic
        assert_eq!(convert_vim_regex(r"a\|b"), "a|b");

        // Escape sequence in normal magic
        assert_eq!(convert_vim_regex(r"\."), r"\.");
    }
}
