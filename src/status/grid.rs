//! 80×26 character grid with attribute byte per cell.
//!
//! Attribute byte:
//!   bit 0  bright (use #FFD000 instead of #FFB000)
//!   bit 1  dim    (use #663D00)
//!   bit 2  inverse (swap fg/bg)
//!   bits 3-7 reserved

pub const COLS: usize = 80;
pub const ROWS: usize = 26;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub attr: u8,
}

pub const ATTR_NORMAL: u8 = 0;
pub const ATTR_BRIGHT: u8 = 1 << 0;
pub const ATTR_DIM: u8 = 1 << 1;
pub const ATTR_INVERSE: u8 = 1 << 2;

impl Cell {
    pub const BLANK: Cell = Cell {
        ch: ' ',
        attr: ATTR_NORMAL,
    };
    pub fn new(ch: char, attr: u8) -> Self {
        Self { ch, attr }
    }
}

#[derive(Debug, Clone)]
pub struct TextScreen {
    pub cells: Vec<Cell>, // ROWS * COLS, row-major
}

impl Default for TextScreen {
    fn default() -> Self {
        Self {
            cells: vec![Cell::BLANK; ROWS * COLS],
        }
    }
}

impl TextScreen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn at(&self, row: usize, col: usize) -> Cell {
        debug_assert!(row < ROWS && col < COLS);
        self.cells[row * COLS + col]
    }

    pub fn set(&mut self, row: usize, col: usize, cell: Cell) {
        debug_assert!(row < ROWS && col < COLS);
        self.cells[row * COLS + col] = cell;
    }

    /// Write a string starting at (row, col) with `attr`. Truncates at row end.
    pub fn write(&mut self, row: usize, col: usize, attr: u8, s: &str) {
        let mut c = col;
        for ch in s.chars() {
            if c >= COLS {
                break;
            }
            self.set(row, c, Cell::new(ch, attr));
            c += 1;
        }
    }

    /// Fill a horizontal run of cells with the same char + attr.
    pub fn fill(&mut self, row: usize, col_lo: usize, col_hi_excl: usize, ch: char, attr: u8) {
        for c in col_lo..col_hi_excl.min(COLS) {
            self.set(row, c, Cell::new(ch, attr));
        }
    }

    /// Find runs of changed cells against `prev`. Returns `(row, col_start, col_end_excl)`.
    pub fn diff_runs(&self, prev: &Self) -> Vec<(usize, usize, usize)> {
        let mut runs = Vec::new();
        for row in 0..ROWS {
            let mut col = 0;
            while col < COLS {
                if self.at(row, col) != prev.at(row, col) {
                    let start = col;
                    while col < COLS && self.at(row, col) != prev.at(row, col) {
                        col += 1;
                    }
                    runs.push((row, start, col));
                } else {
                    col += 1;
                }
            }
        }
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_screen_is_all_blank() {
        let s = TextScreen::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                assert_eq!(s.at(row, col), Cell::BLANK);
            }
        }
    }

    #[test]
    fn write_sets_cells() {
        let mut s = TextScreen::new();
        s.write(2, 5, ATTR_BRIGHT, "MODE");
        assert_eq!(s.at(2, 5), Cell::new('M', ATTR_BRIGHT));
        assert_eq!(s.at(2, 8), Cell::new('E', ATTR_BRIGHT));
        assert_eq!(s.at(2, 9), Cell::BLANK);
    }

    #[test]
    fn write_truncates_at_row_end() {
        let mut s = TextScreen::new();
        s.write(0, 78, ATTR_NORMAL, "ABCDE");
        assert_eq!(s.at(0, 78), Cell::new('A', ATTR_NORMAL));
        assert_eq!(s.at(0, 79), Cell::new('B', ATTR_NORMAL));
        // C, D, E truncated
    }

    #[test]
    fn fill_runs_a_horizontal_span() {
        let mut s = TextScreen::new();
        s.fill(0, 5, 10, '─', ATTR_DIM);
        for c in 5..10 {
            assert_eq!(s.at(0, c).ch, '─');
        }
        assert_eq!(s.at(0, 4), Cell::BLANK);
        assert_eq!(s.at(0, 10), Cell::BLANK);
    }

    #[test]
    fn diff_runs_detects_only_changed_cells() {
        let a = TextScreen::new();
        let mut b = a.clone();
        b.write(3, 10, ATTR_BRIGHT, "HELLO");
        let runs = b.diff_runs(&a);
        assert_eq!(runs, vec![(3, 10, 15)]);
    }

    #[test]
    fn diff_runs_handles_two_separate_changes_in_same_row() {
        let a = TextScreen::new();
        let mut b = a.clone();
        b.write(0, 0, ATTR_NORMAL, "AA");
        b.write(0, 10, ATTR_NORMAL, "BB");
        let runs = b.diff_runs(&a);
        assert_eq!(runs, vec![(0, 0, 2), (0, 10, 12)]);
    }
}
