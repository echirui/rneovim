use crate::nvim::ui::grid::Color;
use regex::Regex;

#[derive(Debug, Clone)]
pub enum HighlightGroup {
    Keyword,
    Comment,
    String,
    Function,
    Type,
    Number,
    Operator,
    Default,
}

impl HighlightGroup {
    pub fn color(&self) -> Color {
        match self {
            HighlightGroup::Keyword => Color::Yellow,
            HighlightGroup::Comment => Color::Blue,
            HighlightGroup::String => Color::Green,
            HighlightGroup::Function => Color::Cyan,
            HighlightGroup::Type => Color::Magenta,
            HighlightGroup::Number => Color::Red,
            HighlightGroup::Operator => Color::White,
            HighlightGroup::Default => Color::Default,
        }
    }
}

pub struct SyntaxRule {
    pub regex: Regex,
    pub group: HighlightGroup,
}

pub struct SyntaxHighlighter {
    pub rules: Vec<SyntaxRule>,
}

impl SyntaxHighlighter {
    pub fn new_rust() -> Self {
        let keywords = vec!["fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "match", "if", "else", "for", "while", "return", "true", "false"];
        let mut rules = Vec::new();
        
        // キーワード (\b を使って単語境界を指定)
        for k in keywords {
            rules.push(SyntaxRule {
                regex: Regex::new(&format!(r"\b{}\b", k)).unwrap(),
                group: HighlightGroup::Keyword,
            });
        }
        
        // コメント
        rules.push(SyntaxRule {
            regex: Regex::new(r"//.*").unwrap(),
            group: HighlightGroup::Comment,
        });

        // 文字列
        rules.push(SyntaxRule {
            regex: Regex::new(r#""[^"]*""#).unwrap(),
            group: HighlightGroup::String,
        });

        Self { rules }
    }

    pub fn highlight_line(&self, line: &str) -> Vec<(usize, usize, HighlightGroup)> {
        let mut highlights = Vec::new();
        for rule in &self.rules {
            for m in rule.regex.find_iter(line) {
                highlights.push((m.start(), m.end(), rule.group.clone()));
            }
        }
        highlights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_highlighting() {
        let hl = SyntaxHighlighter::new_rust();
        
        // Keyword
        let h = hl.highlight_line("fn main()");
        assert!(h.iter().any(|(_, _, g)| matches!(g, HighlightGroup::Keyword)));

        // Comment
        let h = hl.highlight_line("let x = 1; // comment");
        assert!(h.iter().any(|(_, _, g)| matches!(g, HighlightGroup::Comment)));

        // String
        let h = hl.highlight_line("let s = \"hello\";");
        assert!(h.iter().any(|(_, _, g)| matches!(g, HighlightGroup::String)));
    }

    #[test]
    fn test_highlight_colors() {
        assert_eq!(HighlightGroup::Keyword.color(), Color::Yellow);
        assert_eq!(HighlightGroup::Comment.color(), Color::Blue);
    }
}
