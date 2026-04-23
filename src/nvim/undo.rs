use serde::{Serialize, Deserialize};

/// Represents a single atomic change to the buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    InsertLine { lnum: usize, col: usize, text: String },
    DeleteLine { lnum: usize, col: usize, text: String },
    ReplaceLine { lnum: usize, col: usize, old_text: String, new_text: String },
    Group(Vec<Action>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoNode {
    pub action: Option<Action>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub timestamp: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoTree {
    pub nodes: Vec<UndoNode>,
    pub current_idx: usize,
    pub pending_group: Option<Vec<Action>>,
    pub group_depth: usize,
}

impl UndoTree {
    pub fn new() -> Self {
        let root = UndoNode {
            action: None,
            parent: None,
            children: Vec::new(),
            timestamp: std::time::SystemTime::now(),
        };
        Self {
            nodes: vec![root],
            current_idx: 0,
            pending_group: None,
            group_depth: 0,
        }
    }

    fn log(&self, msg: &str) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("rneovim.log") 
        {
            let _ = writeln!(file, "[{}] UNDO: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), msg);
        }
    }

    pub fn start_group(&mut self) {
        self.log(&format!("start_group (depth={}, cur={})", self.group_depth, self.current_idx));
        if self.group_depth == 0 && self.pending_group.is_none() {
            self.pending_group = Some(Vec::new());
        }
        self.group_depth += 1;
    }

    pub fn end_group(&mut self) {
        self.log(&format!("end_group (depth={}, cur={})", self.group_depth, self.current_idx));
        if self.group_depth > 0 {
            self.group_depth -= 1;
            if self.group_depth == 0 {
                self.close_atom();
            }
        }
    }

    pub fn push(&mut self, action: Action) {
        self.log(&format!("push (depth={}, cur={})", self.group_depth, self.current_idx));
        if let Some(ref mut group) = self.pending_group {
            group.push(action);
        } else {
            let new_idx = self.nodes.len();
            let new_node = UndoNode {
                action: Some(action),
                parent: Some(self.current_idx),
                children: Vec::new(),
                timestamp: std::time::SystemTime::now(),
            };
            self.nodes.push(new_node);
            self.nodes[self.current_idx].children.push(new_idx);
            self.current_idx = new_idx;
        }
    }

    pub fn close_atom(&mut self) {
        self.log(&format!("close_atom (depth={}, cur={})", self.group_depth, self.current_idx));
        if self.group_depth > 0 { return; }
        
        if let Some(actions) = self.pending_group.take() {
            if actions.is_empty() { return; }
            
            let action = if actions.len() == 1 {
                actions[0].clone()
            } else {
                Action::Group(actions)
            };

            let new_idx = self.nodes.len();
            let new_node = UndoNode {
                action: Some(action),
                parent: Some(self.current_idx),
                children: Vec::new(),
                timestamp: std::time::SystemTime::now(),
            };
            self.nodes.push(new_node);
            self.nodes[self.current_idx].children.push(new_idx);
            self.current_idx = new_idx;
        }
    }

    pub fn pop_undo(&mut self) -> Option<Action> {
        self.log(&format!("pop_undo (depth={}, cur={})", self.group_depth, self.current_idx));
        if self.pending_group.is_some() { self.close_atom(); }
        if self.current_idx == 0 { return None; }

        let current = &self.nodes[self.current_idx];
        let action = current.action.clone();
        if let Some(parent_idx) = current.parent {
            self.current_idx = parent_idx;
        }
        action
    }

    pub fn pop_redo(&mut self) -> Option<Action> {
        self.log(&format!("pop_redo (depth={}, cur={})", self.group_depth, self.current_idx));
        let child_idx = *self.nodes[self.current_idx].children.last()?;
        self.current_idx = child_idx;
        self.nodes[child_idx].action.clone()
    }

    pub fn earlier(&mut self) -> Option<Action> {
        if self.current_idx > 0 {
            self.current_idx -= 1;
            return self.nodes[self.current_idx + 1].action.clone();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_grouping() {
        let mut tree = UndoTree::new();
        tree.start_group();
        tree.push(Action::InsertLine { lnum: 1, col: 0, text: "H".to_string() });
        tree.push(Action::InsertLine { lnum: 1, col: 1, text: "e".to_string() });
        tree.start_group(); // Nested
        tree.push(Action::InsertLine { lnum: 1, col: 2, text: "l".to_string() });
        tree.end_group();
        tree.end_group();

        assert_eq!(tree.nodes.len(), 2);
        let u = tree.pop_undo().unwrap();
        if let Action::Group(actions) = u {
            assert_eq!(actions.len(), 3);
        } else {
            panic!("Expected Group action");
        }
    }
}
