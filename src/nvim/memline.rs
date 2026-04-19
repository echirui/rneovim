/// 1つのノードに保持する最大要素数。
#[cfg(not(test))]
const MAX_CAPACITY: usize = 128;
#[cfg(test)]
const MAX_CAPACITY: usize = 4;

#[derive(Debug, Clone)]
pub enum Node {
    Leaf(Vec<String>),
    Internal {
        children: Vec<Node>,
        line_counts: Vec<usize>,
    },
}

impl Node {
    pub fn new_leaf() -> Self {
        Node::Leaf(Vec::new())
    }

    pub fn line_count(&self) -> usize {
        match self {
            Node::Leaf(lines) => lines.len(),
            Node::Internal { line_counts, .. } => line_counts.iter().sum(),
        }
    }

    pub fn get_line(&self, lnum: usize) -> Option<&str> {
        match self {
            Node::Leaf(lines) => {
                if lnum == 0 || lnum > lines.len() {
                    None
                } else {
                    Some(lines[lnum - 1].as_str())
                }
            }
            Node::Internal { children, line_counts } => {
                let mut remaining = lnum;
                for (i, count) in line_counts.iter().enumerate() {
                    if remaining <= *count {
                        return children[i].get_line(remaining);
                    }
                    remaining -= count;
                }
                None
            }
        }
    }

    pub fn set_line(&mut self, lnum: usize, text: &str) -> Result<(), &'static str> {
        match self {
            Node::Leaf(lines) => {
                if lnum == 0 || lnum > lines.len() {
                    return Err("Invalid line number");
                }
                lines[lnum - 1] = text.to_string();
                Ok(())
            }
            Node::Internal { children, line_counts } => {
                let mut remaining = lnum;
                for (i, count) in line_counts.iter().enumerate() {
                    if remaining <= *count {
                        return children[i].set_line(remaining, text);
                    }
                    remaining -= count;
                }
                Err("Invalid line number")
            }
        }
    }

    /// 行を挿入し、ノードが分割された場合は新しいノードを返す
    pub fn insert_line(&mut self, lnum: usize, text: &str) -> Result<Option<Node>, &'static str> {
        match self {
            Node::Leaf(lines) => {
                if lnum == 0 || lnum > lines.len() + 1 {
                    return Err("Invalid line number");
                }
                lines.insert(lnum - 1, text.to_string());
                if lines.len() > MAX_CAPACITY {
                    let mid = lines.len() / 2;
                    let new_lines = lines.split_off(mid);
                    Ok(Some(Node::Leaf(new_lines)))
                } else {
                    Ok(None)
                }
            }
            Node::Internal { children, line_counts } => {
                let mut remaining = lnum;
                let mut target_idx = children.len() - 1;
                
                let mut sum = 0;
                for (i, count) in line_counts.iter().enumerate() {
                    sum += count;
                    if remaining <= *count || (remaining == *count + 1 && i == children.len() - 1) {
                        target_idx = i;
                        break;
                    }
                    remaining -= count;
                }
                
                if target_idx == children.len() - 1 && remaining > line_counts[target_idx] + 1 {
                    return Err("Invalid line number");
                }

                let new_child_opt = children[target_idx].insert_line(remaining, text)?;
                line_counts[target_idx] += 1;
                
                if let Some(new_child) = new_child_opt {
                    let new_child_count = new_child.line_count();
                    line_counts[target_idx] -= new_child_count;
                    
                    children.insert(target_idx + 1, new_child);
                    line_counts.insert(target_idx + 1, new_child_count);
                    
                    if children.len() > MAX_CAPACITY {
                        let mid = children.len() / 2;
                        let new_children = children.split_off(mid);
                        let new_counts = line_counts.split_off(mid);
                        Ok(Some(Node::Internal {
                            children: new_children,
                            line_counts: new_counts,
                        }))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// 行を削除し、ノードが空になった場合は true を返す
    pub fn delete_line(&mut self, lnum: usize) -> Result<bool, &'static str> {
        match self {
            Node::Leaf(lines) => {
                if lnum == 0 || lnum > lines.len() {
                    return Err("Invalid line number");
                }
                lines.remove(lnum - 1);
                Ok(lines.is_empty())
            }
            Node::Internal { children, line_counts } => {
                let mut remaining = lnum;
                let mut target_idx = children.len();
                
                for (i, count) in line_counts.iter().enumerate() {
                    if remaining <= *count {
                        target_idx = i;
                        break;
                    }
                    remaining -= count;
                }
                
                if target_idx == children.len() {
                    return Err("Invalid line number");
                }
                
                let should_remove = children[target_idx].delete_line(remaining)?;
                line_counts[target_idx] -= 1;
                
                if should_remove {
                    children.remove(target_idx);
                    line_counts.remove(target_idx);
                }
                
                Ok(children.is_empty())
            }
        }
    }

    pub fn replace_line(&mut self, lnum: usize, text: &str) -> Result<(), &'static str> {
        match self {
            Node::Leaf(lines) => {
                if lnum == 0 || lnum > lines.len() {
                    return Err("Invalid line number");
                }
                lines[lnum - 1] = text.to_string();
                Ok(())
            }
            Node::Internal { children, line_counts } => {
                let mut remaining = lnum;
                for (i, count) in line_counts.iter().enumerate() {
                    if remaining <= *count {
                        return children[i].replace_line(remaining, text);
                    }
                    remaining -= count;
                }
                Err("Invalid line number")
            }
        }
    }
}

/// B-Treeベースの行管理構造。
pub struct MemLine {
    root: Node,
}

impl MemLine {
    pub fn new() -> Self {
        Self {
            root: Node::new_leaf(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.root.line_count()
    }

    pub fn get_line(&self, lnum: usize) -> Option<&str> {
        self.root.get_line(lnum)
    }

    pub fn append_line(&mut self, text: &str) {
        let count = self.line_count();
        let _ = self.insert_line(count + 1, text);
    }

    pub fn insert_line(&mut self, lnum: usize, text: &str) -> Result<(), &str> {
        if let Some(new_node) = self.root.insert_line(lnum, text)? {
            // ルートが分割された場合は新しい Internal ノードを作成して高さを1段階増やす
            let old_root = std::mem::replace(&mut self.root, Node::new_leaf());
            let c1_count = old_root.line_count();
            let c2_count = new_node.line_count();
            self.root = Node::Internal {
                children: vec![old_root, new_node],
                line_counts: vec![c1_count, c2_count],
            };
        }
        Ok(())
    }

    pub fn delete_line(&mut self, lnum: usize) -> Result<(), &str> {
        let empty = self.root.delete_line(lnum)?;
        if empty {
            self.root = Node::new_leaf();
        } else {
            // Internalノードの子が1つだけになった場合、高さを1段階減らす
            if let Node::Internal { children, .. } = &mut self.root {
                if children.len() == 1 {
                    self.root = children.pop().unwrap();
                }
            }
        }
        Ok(())
    }

    pub fn replace_line(&mut self, lnum: usize, text: &str) -> Result<(), &str> {
        self.root.replace_line(lnum, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btree_memline_extensive() {
        let mut ml = MemLine::new();
        let count = 100; // MAX_CAPACITY=4 なので、深さ3〜4の木になるはず
        
        for i in 1..=count {
            ml.append_line(&format!("Line {}", i));
        }
        assert_eq!(ml.line_count(), count);

        assert_eq!(ml.get_line(1), Some("Line 1"));
        let last_line_expected = format!("Line {}", count);
        assert_eq!(ml.get_line(count), Some(last_line_expected.as_str()));

        // 中間挿入
        ml.insert_line(50, "Split Middle").unwrap();
        assert_eq!(ml.get_line(50), Some("Split Middle"));
        assert_eq!(ml.line_count(), count + 1);

        // 削除
        for _ in 0..50 {
            ml.delete_line(1).unwrap();
        }
        assert_eq!(ml.line_count(), count + 1 - 50);
    }

    #[test]
    fn test_memline_empty_ops() {
        let mut ml = MemLine::new();
        assert_eq!(ml.line_count(), 0);
        assert_eq!(ml.get_line(1), None);
        assert!(ml.delete_line(1).is_err());
        
        ml.insert_line(1, "first").unwrap();
        assert_eq!(ml.line_count(), 1);
        assert_eq!(ml.get_line(1), Some("first"));
    }

    #[test]
    fn test_memline_set_line() {
        let mut ml = MemLine::new();
        ml.insert_line(1, "line 1").unwrap();
        ml.replace_line(1, "updated").unwrap();
        assert_eq!(ml.get_line(1), Some("updated"));
        
        // Out of bounds
        assert!(ml.replace_line(2, "fail").is_err());
    }

    #[test]
    fn test_memline_out_of_bounds() {
        let ml = MemLine::new();
        assert_eq!(ml.get_line(0), None);
        assert_eq!(ml.get_line(1), None);
    }
}
