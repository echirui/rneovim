use serde::{Serialize, Deserialize};

/// Represents a single atomic change to the buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    InsertLine { lnum: usize, text: String },
    DeleteLine { lnum: usize, text: String },
    ReplaceLine { lnum: usize, old_text: String, new_text: String },
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
}

impl UndoTree {
    pub fn new() -> Self {
        // Root node (no action)
        let root = UndoNode {
            action: None,
            parent: None,
            children: Vec::new(),
            timestamp: std::time::SystemTime::now(),
        };
        Self {
            nodes: vec![root],
            current_idx: 0,
        }
    }

    pub fn push(&mut self, action: Action) {
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

    pub fn pop_undo(&mut self) -> Option<Action> {
        let current = &self.nodes[self.current_idx];
        let action = current.action.clone()?;
        if let Some(parent_idx) = current.parent {
            self.current_idx = parent_idx;
        }
        Some(action)
    }

    pub fn pop_redo(&mut self) -> Option<Action> {
        // Redo は常に最新の子ノードを辿る（簡易実装）
        let child_idx = *self.nodes[self.current_idx].children.last()?;
        self.current_idx = child_idx;
        self.nodes[child_idx].action.clone()
    }

    /// 以前の枝（履歴）に戻るための機能 (g- 相当)
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
    fn test_undo_branching() {
        let mut tree = UndoTree::new();
        
        // 1. A を入力
        tree.push(Action::InsertLine { lnum: 1, text: "A".to_string() });
        // 2. Undo
        tree.pop_undo();
        // 3. B を入力 (ここで枝分かれが発生)
        tree.push(Action::InsertLine { lnum: 1, text: "B".to_string() });
        
        // 最新の状態は B
        assert_eq!(tree.nodes.len(), 3); // Root, A, B
        
        // Undo すると Root に戻る
        let u1 = tree.pop_undo().unwrap();
        if let Action::InsertLine { text, .. } = u1 { assert_eq!(text, "B"); }
        assert_eq!(tree.current_idx, 0);
    }

    #[test]
    fn test_undo_redo_complex() {
        let mut tree = UndoTree::new();
        
        tree.push(Action::InsertLine { lnum: 1, text: "x".to_string() });
        tree.push(Action::InsertLine { lnum: 1, text: "y".to_string() });
        
        assert_eq!(tree.nodes.len(), 3);
        
        // Undo 'y'
        tree.pop_undo();
        // Undo 'x'
        tree.pop_undo();
        
        // Redo 'x'
        let r1 = tree.pop_redo().unwrap();
        if let Action::InsertLine { text, .. } = r1 { assert_eq!(text, "x"); }
        
        // Push 'z' instead of 'y' (Branching)
        tree.push(Action::InsertLine { lnum: 1, text: "z".to_string() });
        
        // Cannot redo 'y' anymore
        assert!(tree.pop_redo().is_none());
    }
}
