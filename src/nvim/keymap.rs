use std::collections::HashMap;

pub enum MappingTarget {
    String(String),
    Lua(i32), // callback_id
}

impl Clone for MappingTarget {
    fn clone(&self) -> Self {
        match self {
            MappingTarget::String(s) => MappingTarget::String(s.clone()),
            MappingTarget::Lua(id) => MappingTarget::Lua(*id),
        }
    }
}

pub struct Mapping {
    pub from: String,
    pub target: MappingTarget,
    pub recursive: bool,
    pub silent: bool,
    pub expr: bool,
}

impl Clone for Mapping {
    fn clone(&self) -> Self {
        Self {
            from: self.from.clone(),
            target: self.target.clone(),
            recursive: self.recursive,
            silent: self.silent,
            expr: self.expr,
        }
    }
}

pub struct KeymapEngine {
    /// mode -> (from -> Mapping)
    /// Mode strings: "n", "i", "v", "x", "s", "o", "c", "t", "" (all)
    mappings: HashMap<String, HashMap<String, Mapping>>,
}

impl KeymapEngine {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, mode: &str, from: &str, target: MappingTarget, recursive: bool, silent: bool, expr: bool) {
        let mode_mappings = self.mappings.entry(mode.to_string()).or_insert_with(HashMap::new);
        mode_mappings.insert(from.to_string(), Mapping {
            from: from.to_string(),
            target,
            recursive,
            silent,
            expr,
        });
    }

    pub fn del_mapping(&mut self, mode: &str, from: &str) {
        if let Some(mode_mappings) = self.mappings.get_mut(mode) {
            mode_mappings.remove(from);
        }
    }

    /// 入力文字列からマッピングを解決する
    pub fn resolve(&self, mode: &str, input: &str) -> (Option<Mapping>, usize) {
        // First try the specific mode, then fall back to "" (all modes)
        if let Some(res) = self.resolve_in_mode(mode, input) {
            return res;
        }
        self.resolve_in_mode("", input).unwrap_or((None, 0))
    }

    fn resolve_in_mode(&self, mode: &str, input: &str) -> Option<(Option<Mapping>, usize)> {
        let mode_mappings = self.mappings.get(mode)?;
        
        let mut longest_match = None;
        let mut longest_len = 0;
        let mut is_prefix = false;

        for (from, mapping) in mode_mappings {
            if from == input {
                if from.len() > longest_len {
                    longest_len = from.len();
                    longest_match = Some(mapping.clone());
                }
            } else if from.starts_with(input) {
                is_prefix = true;
            } else if input.starts_with(from) {
                if from.len() > longest_len {
                    longest_len = from.len();
                    longest_match = Some(mapping.clone());
                }
            }
        }

        if is_prefix && longest_match.is_none() {
            return Some((None, 0)); // Need more characters
        }

        if longest_match.is_some() {
            return Some((longest_match, longest_len));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keymap_resolution() {
        let mut engine = KeymapEngine::new();
        engine.add_mapping("n", "jj", MappingTarget::String("\x1B".to_string()), false, false, false);
        
        let (resolved, consumed) = engine.resolve("n", "jj");
        match resolved {
            Some(m) => {
                match m.target {
                    MappingTarget::String(s) => assert_eq!(s, "\x1B"),
                    _ => panic!("Expected string mapping"),
                }
            }
            _ => panic!("Expected mapping"),
        }
        assert_eq!(consumed, 2);

        // Partial match
        let (resolved, consumed) = engine.resolve("n", "j");
        assert!(resolved.is_none());
        assert_eq!(consumed, 0);
    }
}
