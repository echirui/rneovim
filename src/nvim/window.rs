use crate::nvim::buffer::Buffer;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, Ordering};

/// Window ID generator
static NEXT_WINDOW_ID: AtomicI32 = AtomicI32::new(1);

use serde::{Serialize, Deserialize};

/// Represents a cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    id: i32,
    pub buffer: Rc<RefCell<Buffer>>,
    cursor: Cursor,
    pub curswant: usize,
    topline: usize,
    height: usize,
    width: usize,
    pub config: Option<WinConfig>,
    pub folds: Vec<(usize, usize)>,
}

impl Window {
    pub fn new(buffer: Rc<RefCell<Buffer>>) -> Self {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::SeqCst);
        Self {
            id,
            buffer,
            cursor: Cursor { row: 1, col: 0 },
            curswant: 0,
            topline: 1,
            height: 24,
            width: 80,
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
            curswant: 0,
            topline: 1,
            height: config.height,
            width: config.width,
            config: Some(config),
            folds: Vec::new(),
        }
    }

    pub fn id(&self) -> i32 { self.id }
    pub fn cursor(&self) -> Cursor { self.cursor }
    pub fn topline(&self) -> usize { self.topline }
    pub fn height(&self) -> usize { self.height }
    pub fn set_height(&mut self, h: usize) { self.height = h; }
    pub fn set_width(&mut self, w: usize) { self.width = w; }
    pub fn buffer(&self) -> Rc<RefCell<Buffer>> { Rc::clone(&self.buffer) }
    pub fn config(&self) -> Option<&WinConfig> { self.config.as_ref() }
    pub fn is_floating(&self) -> bool { self.config.is_some() }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor = Cursor { row, col };
        self.curswant = col;
        self.adjust_topline();
    }

    pub fn set_curswant(&mut self, col: usize) {
        self.curswant = col;
    }

    pub fn set_cursor_vertical(&mut self, row: usize) {
        let c = if let Ok(buf) = self.buffer.try_borrow() {
            let chars_count = buf.get_line(row).map(|l| l.chars().count()).unwrap_or(0);
            if chars_count == 0 { 0 } 
            else if self.curswant == usize::MAX { chars_count.saturating_sub(1) }
            else { self.curswant.min(chars_count.saturating_sub(1)) }
        } else {
            self.curswant.min(1000) // Fallback
        };
        self.cursor = Cursor { row, col: c };
        self.adjust_topline();
    }

    fn adjust_topline(&mut self) {
        if self.cursor.row < self.topline {
            self.topline = self.cursor.row;
        } else if self.cursor.row >= self.topline + self.height {
            self.topline = self.cursor.row - self.height + 1;
        }
    }

    pub fn center_cursor(&mut self) {
        let half = self.height / 2;
        self.topline = self.cursor.row.saturating_sub(half).max(1);
    }

    pub fn cursor_to_top(&mut self) { self.topline = self.cursor.row; }
    pub fn cursor_to_bottom(&mut self) { self.topline = self.cursor.row.saturating_sub(self.height.saturating_sub(1)).max(1); }

    pub fn scroll(&mut self, lines: i32) {
        let line_count = if let Ok(b) = self.buffer.try_borrow() { b.line_count() } else { 1000 };
        self.topline = (self.topline as i32 + lines).clamp(1, line_count as i32) as usize;
    }
}

#[derive(Clone)]
pub struct TabPage {
    pub windows: Vec<Window>,
    pub current_win_idx: usize,
}

impl TabPage {
    pub fn new(win: Window) -> Self {
        Self {
            windows: vec![win],
            current_win_idx: 0,
        }
    }
}
