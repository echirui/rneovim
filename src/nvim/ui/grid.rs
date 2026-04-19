use unicode_width::UnicodeWidthChar;
use unicode_bidi::{BidiInfo, Level};

/// Standard ANSI colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Color {
    pub fn to_fg_code(self) -> &'static str {
        match self {
            Color::Default => "39",
            Color::Black => "30",
            Color::Red => "31",
            Color::Green => "32",
            Color::Yellow => "33",
            Color::Blue => "34",
            Color::Magenta => "35",
            Color::Cyan => "36",
            Color::White => "37",
        }
    }

    pub fn to_bg_code(self) -> &'static str {
        match self {
            Color::Default => "49",
            Color::Black => "40",
            Color::Red => "41",
            Color::Green => "42",
            Color::Yellow => "43",
            Color::Blue => "44",
            Color::Magenta => "45",
            Color::Cyan => "46",
            Color::White => "47",
        }
    }
}

/// Represents a single cell in the grid with color attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub char: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub is_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            char: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            is_continuation: false,
        }
    }
}

/// A 2D grid of cells that represents the screen buffer.
pub struct Grid {
    pub width: usize,
    pub height: usize,
    cells: Vec<Cell>,
    old_cells: Vec<Cell>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            cells: vec![Cell::default(); size],
            old_cells: vec![Cell::default(); size],
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        let size = width * height;
        self.width = width;
        self.height = height;
        self.cells = vec![Cell::default(); size];
        self.old_cells = vec![Cell::default(); size];
    }

    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell::default();
        }
    }

    pub fn put_char(&mut self, row: usize, col: usize, c: char, fg: Color, bg: Color, bold: bool) {
        if row < self.height && col < self.width {
            let width = c.width().unwrap_or(1);
            let idx = row * self.width + col;
            // 制御文字や不正な文字をスペースに置き換え
            let safe_c = if c < ' ' && c != '\t' { ' ' } else { c };
            self.cells[idx] = Cell { char: safe_c, fg, bg, bold, is_continuation: false };
            
            if width == 2 && col + 1 < self.width {
                self.cells[idx + 1] = Cell { char: ' ', fg, bg, bold, is_continuation: true };
            }
        }
    }

    pub fn get_char(&self, row: usize, col: usize) -> char {
        if row < self.height && col < self.width {
            let idx = row * self.width + col;
            self.cells[idx].char
        } else {
            ' '
        }
    }

    pub fn get_fg(&self, row: usize, col: usize) -> Color {
        if row < self.height && col < self.width {
            let idx = row * self.width + col;
            self.cells[idx].fg
        } else {
            Color::Default
        }
    }

    pub fn put_str(&mut self, row: usize, col: usize, s: &str, fg: Color, bg: Color, bold: bool) {
        let mut current_col = col;
        for c in s.chars() {
            let width = c.width().unwrap_or(1);
            if current_col + width > self.width { break; }
            self.put_char(row, current_col, c, fg, bg, bold);
            current_col += width;
        }
    }

    /// RTL対応版の文字列描画
    pub fn put_str_bidi(&mut self, row: usize, col: usize, s: &str, fg: Color, bg: Color, bold: bool) {
        let bidi_info = BidiInfo::new(s, Some(Level::ltr()));
        if bidi_info.paragraphs.is_empty() { 
            self.put_str(row, col, s, fg, bg, bold);
            return; 
        }
        
        let para = &bidi_info.paragraphs[0];
        let line = para.range.clone();
        let display_text = bidi_info.reorder_line(para, line);
        
        let mut current_col = col;
        for c in display_text.chars() {
            let width = c.width().unwrap_or(1);
            if current_col + width > self.width { break; }
            self.put_char(row, current_col, c, fg, bg, bold);
            current_col += width;
        }
    }

    pub fn flush(&mut self) {
        use std::io::{Write, stdout};
        let mut out = stdout();

        let mut last_fg = Color::Default;
        let mut last_bg = Color::Default;
        let mut last_bold = false;

        // 初期化エスケープ
        write!(out, "\x1B[0m").unwrap();

        for r in 0..self.height {
            for c in 0..self.width {
                let idx = r * self.width + c;
                if self.cells[idx] != self.old_cells[idx] {
                    let cell = &self.cells[idx];
                    if cell.is_continuation {
                        self.old_cells[idx] = *cell;
                        continue;
                    }
                    
                    // カーソル移動
                    write!(out, "\x1B[{};{}H", r + 1, c + 1).unwrap();

                    // 属性の変化を適用
                    if cell.fg != last_fg || cell.bg != last_bg || cell.bold != last_bold {
                        write!(out, "\x1B[0;{};{};{}m", 
                            if cell.bold { "1" } else { "0" },
                            cell.fg.to_fg_code(),
                            cell.bg.to_bg_code()
                        ).unwrap();
                        last_fg = cell.fg;
                        last_bg = cell.bg;
                        last_bold = cell.bold;
                    }

                    write!(out, "{}", cell.char).unwrap();
                    self.old_cells[idx] = *cell;
                }
            }
        }
        // リセット
        write!(out, "\x1B[0m").unwrap();
        let _ = out.flush();
    }

    pub fn force_redraw(&mut self) {
        for cell in self.old_cells.iter_mut() {
            cell.char = '\0';
        }
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_double_width() {
        let mut grid = Grid::new(10, 5);
        // 「あ」 is width 2
        grid.put_char(0, 0, 'あ', Color::Default, Color::Default, false);
        
        // Check first cell
        let cell1 = grid.cells[0];
        assert_eq!(cell1.char, 'あ');
        assert!(!cell1.is_continuation);
        
        // Check second cell (continuation)
        let cell2 = grid.cells[1];
        assert!(cell2.is_continuation);
        
        // Overwrite part of double width
        grid.put_char(0, 1, 'x', Color::Default, Color::Default, false);
        let cell_x = grid.cells[1];
        assert_eq!(cell_x.char, 'x');
        assert!(!cell_x.is_continuation);
    }

    #[test]
    fn test_grid_put_str_bidi() {
        let mut grid = Grid::new(20, 5);
        // "ABC" (LTR)
        grid.put_str_bidi(0, 0, "ABC", Color::Default, Color::Default, false);
        assert_eq!(grid.get_char(0, 0), 'A');
        assert_eq!(grid.get_char(0, 1), 'B');
        assert_eq!(grid.get_char(0, 2), 'C');

        // Arabic "مرحبا" (RTL) - should be reordered
        // Basic check that it doesn't crash and puts something
        grid.put_str_bidi(1, 0, "مرحبا", Color::Default, Color::Default, false);
        assert_ne!(grid.get_char(1, 0), ' ');
    }
}
