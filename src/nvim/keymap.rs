use std::collections::HashMap;

pub struct Mapping {
    pub from: String,
    pub to: String,
    pub recursive: bool,
}

pub struct KeymapEngine {
    mappings: HashMap<String, Mapping>,
}

impl KeymapEngine {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, from: &str, to: &str, recursive: bool) {
        self.mappings.insert(from.to_string(), Mapping {
            from: from.to_string(),
            to: to.to_string(),
            recursive,
        });
    }

    /// 入力文字列からマッピングを解決する
    /// 戻り値: (解決後の文字列, 消費した文字数)
    pub fn resolve(&self, input: &str) -> (Option<String>, usize) {
        let mut longest_match = None;
        let mut longest_len = 0;
        let mut is_prefix = false;

        for (from, mapping) in &self.mappings {
            if from == input {
                if from.len() > longest_len {
                    longest_len = from.len();
                    longest_match = Some(mapping.to.clone());
                }
            } else if from.starts_with(input) {
                is_prefix = true;
            } else if input.starts_with(from) {
                if from.len() > longest_len {
                    longest_len = from.len();
                    longest_match = Some(mapping.to.clone());
                }
            }
        }

        if is_prefix {
            return (None, 0); // Need more characters to disambiguate
        }

        (longest_match, longest_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keymap_resolution() {
        let mut engine = KeymapEngine::new();
        engine.add_mapping("jj", "\x1B", false);
        
        let (resolved, consumed) = engine.resolve("jj");
        assert_eq!(resolved.unwrap(), "\x1B");
        assert_eq!(consumed, 2);

        // Partial match
        let (resolved, consumed) = engine.resolve("j");
        assert!(resolved.is_none());
        assert_eq!(consumed, 0);

        // Ambiguous match (longer match exists)
        engine.add_mapping("abc", "XYZ", false);
        engine.add_mapping("abcd", "123", false);
        
        // "abc" could be a prefix of "abcd"
        let (resolved, consumed) = engine.resolve("abc");
        assert!(resolved.is_none()); // Should wait for more input
        assert_eq!(consumed, 0);
        
        let (resolved, consumed) = engine.resolve("abcd");
        assert_eq!(resolved.unwrap(), "123");
        assert_eq!(consumed, 4);
    }
}
