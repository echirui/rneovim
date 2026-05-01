
/// Standard ANSI colors + RGB support.
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
    Rgb(u8, u8, u8),
}

impl Color {
    pub fn to_fg_code(self) -> String {
        match self {
            Color::Default => "39".to_string(),
            Color::Black => "30".to_string(),
            Color::Red => "31".to_string(),
            Color::Green => "32".to_string(),
            Color::Yellow => "33".to_string(),
            Color::Blue => "34".to_string(),
            Color::Magenta => "35".to_string(),
            Color::Cyan => "36".to_string(),
            Color::White => "37".to_string(),
            Color::Rgb(r, g, b) => format!("38;2;{};{};{}", r, g, b),
        }
    }

    pub fn to_bg_code(self) -> String {
        match self {
            Color::Default => "49".to_string(),
            Color::Black => "40".to_string(),
            Color::Red => "41".to_string(),
            Color::Green => "42".to_string(),
            Color::Yellow => "43".to_string(),
            Color::Blue => "44".to_string(),
            Color::Magenta => "45".to_string(),
            Color::Cyan => "46".to_string(),
            Color::White => "47".to_string(),
            Color::Rgb(r, g, b) => format!("48;2;{};{};{}", r, g, b),
        }
    }
    
    pub fn from_u32(val: u32) -> Self {
        let r = ((val >> 16) & 0xFF) as u8;
        let g = ((val >> 8) & 0xFF) as u8;
        let b = (val & 0xFF) as u8;
        Color::Rgb(r, g, b)
    }
}

/// Represents a single cell in the grid with color attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        let cells = vec![Cell { 
            c: ' ', 
            fg: Color::Default, 
            bg: Color::Default, 
            bold: false, 
            italic: false, 
            underline: false 
        }; width * height];
        Self { width, height, cells, cursor_row: 0, cursor_col: 0 }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cells = vec![Cell { 
            c: ' ', 
            fg: Color::Default, 
            bg: Color::Default, 
            bold: false, 
            italic: false, 
            underline: false 
        }; width * height];
    }

    pub fn put_char(&mut self, row: usize, col: usize, c: char, fg: Color, bg: Color, bold: bool) {
        if row < self.height && col < self.width {
            let idx = row * self.width + col;
            self.cells[idx] = Cell { c, fg, bg, bold, italic: false, underline: false };
        }
    }

    pub fn put_str(&mut self, row: usize, mut col: usize, s: &str, fg: Color, bg: Color, bold: bool) {
        for c in s.chars() {
            if col >= self.width { break; }
            self.put_char(row, col, c, fg, bg, bold);
            col += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        }
    }

    pub fn put_char_full(&mut self, row: usize, col: usize, c: char, fg: Color, bg: Color, bold: bool, italic: bool, underline: bool) {
        if row < self.height && col < self.width {
            let idx = row * self.width + col;
            self.cells[idx] = Cell { c, fg, bg, bold, italic, underline };
        }
    }

    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            cell.c = ' ';
            cell.fg = Color::Default;
            cell.bg = Color::Default;
            cell.bold = false;
        }
    }

    pub fn flush(&self) -> String {
        let mut res = String::new();
        let mut cur_fg = Color::Default;
        let mut cur_bg = Color::Default;
        let mut cur_bold = false;

        res.push_str("\x1b[H"); // Move to 0,0

        for row in 0..self.height {
            for col in 0..self.width {
                let cell = self.cells[row * self.width + col];
                
                let mut attr_changed = false;
                if cell.fg != cur_fg || cell.bg != cur_bg || cell.bold != cur_bold {
                    attr_changed = true;
                }

                if attr_changed {
                    res.push_str("\x1b[0;"); // Reset all
                    res.push_str(&cell.fg.to_fg_code());
                    res.push(';');
                    res.push_str(&cell.bg.to_bg_code());
                    if cell.bold { res.push_str(";1"); }
                    res.push('m');
                    
                    cur_fg = cell.fg;
                    cur_bg = cell.bg;
                    cur_bold = cell.bold;
                }
                res.push(cell.c);
            }
            if row < self.height - 1 {
                res.push_str("\r\n");
            }
        }
        res
    }
}
