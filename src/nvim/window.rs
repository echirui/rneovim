use crate::nvim::buffer::Buffer;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, Ordering};

/// Window ID generator
static NEXT_WINDOW_ID: AtomicI32 = AtomicI32::new(1);

use serde::{Serialize, Deserialize};

/// Represents a cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cursor {
    pub row: usize, // 1-based
    pub col: usize, // 0-based
}

/// フローティングウィンドウの設定
#[derive(Debug, Clone)]
pub struct WinConfig {
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub height: usize,
    pub focusable: bool,
    pub external: bool,
}

/// Represents a Neovim window (win_T).
#[derive(Clone)]
pub struct Window {
    /// Unique identifier for the window
    id: i32,
    /// The buffer displayed in this window
    pub buffer: Rc<RefCell<Buffer>>,
    /// Cursor position in the buffer
    cursor: Cursor,
    /// The line number displayed at the top of the window (w_topline)
    topline: usize,
    /// Window height
    height: usize,
    /// Window width
    width: usize,
    /// フローティングウィンドウの設定 (None なら通常のウィンドウ)
    config: Option<WinConfig>,
    pub folds: Vec<(usize, usize)>,
}

impl Window {
    pub fn new(buffer: Rc<RefCell<Buffer>>) -> Self {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::SeqCst);
        Self {
            id,
            buffer,
            cursor: Cursor { row: 1, col: 0 },
            topline: 1,
            height: 24, // Default height
            width: 80,  // Default width
            config: None,
            folds: Vec::new(),
        }
    }

    pub fn new_floating(buffer: Rc<RefCell<Buffer>>, config: WinConfig) -> Self {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::SeqCst);
        Self {
            id,
            buffer,
            cursor: Cursor { row: 1, col: 0 },
            topline: 1,
            height: config.height,
            width: config.width,
            config: Some(config),
            folds: Vec::new(),
        }
    }

    pub fn is_floating(&self) -> bool {
        self.config.is_some()
    }

    pub fn add_fold(&mut self, start: usize, end: usize) {
        self.folds.push((start, end));
        // 単純化のため、追加後にソートしておく
        self.folds.sort_by_key(|f| f.0);
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn topline(&self) -> usize {
        self.topline
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn set_height(&mut self, height: usize) {
        self.height = height;
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn set_width(&mut self, width: usize) {
        self.width = width;
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        let buf = self.buffer.borrow();
        let line_count = buf.line_count();
        
        let r = row.clamp(1, if line_count == 0 { 1 } else { line_count });
        let c = if let Some(line) = buf.get_line(r) {
            col.min(line.chars().count())
        } else {
            0
        };
        
        self.cursor = Cursor { row: r, col: c };
        
        if self.cursor.row < self.topline {
            self.topline = self.cursor.row;
        } else if self.cursor.row >= self.topline + self.height {
            self.topline = self.cursor.row - self.height + 1;
        }
    }

    pub fn buffer(&self) -> Rc<RefCell<Buffer>> {
        Rc::clone(&self.buffer)
    }

    pub fn center_cursor(&mut self) {
        let half = self.height / 2;
        self.topline = self.cursor.row.saturating_sub(half).max(1);
    }

    pub fn cursor_to_top(&mut self) {
        self.topline = self.cursor.row;
    }

    pub fn cursor_to_bottom(&mut self) {
        self.topline = self.cursor.row.saturating_sub(self.height.saturating_sub(1)).max(1);
    }

    pub fn scroll(&mut self, lines: i32) {
        let buf = self.buffer.borrow();
        let line_count = buf.line_count().max(1);
        
        let new_topline = (self.topline as i32 + lines).clamp(1, line_count as i32) as usize;
        self.topline = new_topline;
        
        let new_row = (self.cursor.row as i32 + lines).clamp(1, line_count as i32) as usize;
        let mut r = new_row;
        
        // Ensure cursor is within the visible area.
        if r < self.topline {
            r = self.topline;
        } else if r >= self.topline + self.height {
            r = self.topline + self.height - 1;
        }
        r = r.min(line_count);
        
        let c = if let Some(line) = buf.get_line(r) {
            self.cursor.col.min(line.chars().count())
        } else {
            0
        };
        self.cursor = Cursor { row: r, col: c };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nvim::buffer::Buffer;

    #[test]
    fn test_window_cursor_move() {
        let buf = Rc::new(RefCell::new(Buffer::new()));
        buf.borrow_mut().append_line("L1").unwrap();
        buf.borrow_mut().append_line("L2").unwrap();
        
        let mut win = Window::new(Rc::clone(&buf));
        win.set_height(10);
        
        win.set_cursor(2, 0);
        assert_eq!(win.cursor().row, 2);
        win.set_cursor(100, 0);
        assert_eq!(win.cursor().row, 2);
        win.set_cursor(0, 0);
        assert_eq!(win.cursor().row, 1);
    }

    #[test]
    fn test_window_scrolling() {
        let buf = Rc::new(RefCell::new(Buffer::new()));
        for i in 1..20 {
            buf.borrow_mut().append_line(&format!("Line {}", i)).unwrap();
        }
        
        let mut win = Window::new(Rc::clone(&buf));
        win.set_height(5);
        
        assert_eq!(win.topline(), 1);
        win.set_cursor(5, 0);
        assert_eq!(win.topline(), 1);
        win.set_cursor(6, 0);
        assert_eq!(win.topline(), 2);
        win.set_cursor(15, 0);
        assert_eq!(win.topline(), 11);
        win.set_cursor(10, 0);
        assert_eq!(win.topline(), 10);
    }
}
