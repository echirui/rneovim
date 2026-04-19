use std::collections::HashSet;

pub struct SpellChecker {
    dictionary: HashSet<String>,
}

impl SpellChecker {
    pub fn new() -> Self {
        Self {
            dictionary: HashSet::new(),
        }
    }

    pub fn add_word(&mut self, word: &str) {
        self.dictionary.insert(word.to_lowercase());
    }

    pub fn is_misspelled(&self, word: &str) -> bool {
        if word.is_empty() || word.chars().all(|c| !c.is_alphabetic()) {
            return false;
        }
        !self.dictionary.contains(&word.to_lowercase())
    }

    pub fn check_line(&self, line: &str) -> Vec<(usize, usize)> {
        let mut misspellings = Vec::new();
        let mut start = None;

        for (i, c) in line.char_indices() {
            if c.is_alphabetic() {
                if start.is_none() {
                    start = Some(i);
                }
            } else {
                if let Some(s) = start {
                    let word = &line[s..i];
                    if self.is_misspelled(word) {
                        misspellings.push((s, i));
                    }
                    start = None;
                }
            }
        }

        if let Some(s) = start {
            let word = &line[s..];
            if self.is_misspelled(word) {
                misspellings.push((s, line.len()));
            }
        }

        misspellings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spell_check_extensive() {
        let mut checker = SpellChecker::new();
        checker.add_word("I");
        checker.add_word("like");
        checker.add_word("Rust");
        
        // Correct sentence
        assert_eq!(checker.check_line("I like Rust").len(), 0);
        
        // One misspelling
        let errors = checker.check_line("I like Ruzt");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], (7, 11)); // "Ruzt"
        
        // Punctuation and cases
        assert_eq!(checker.check_line("I like Rust!").len(), 0);
        assert_eq!(checker.check_line("I LIKE RUST").len(), 0);
        
        // Numbers should be ignored
        assert_eq!(checker.check_line("Rust 1.68").len(), 0);
    }
}
