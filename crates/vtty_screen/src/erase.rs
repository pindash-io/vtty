// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! erase commands for [`Screen`](crate::Screen).

use crate::cell::Cell;

use crate::Screen;

impl Screen {
    pub(crate) fn erase_in_display(&mut self, mode: i32) {
        match mode {
            0 => {
                // Cursor to end.
                self.erase_from_cursor_to_end();
            }
            1 => {
                // Start to cursor.
                let (r, c) = (self.cursor_row, self.cursor_col);
                self.erase_region(0, 0, r, c);
            }
            _ => {
                // 2 (and 3, scrollback — same in the MVP): whole screen.
                self.cells.fill(Cell::default());
            }
        }
    }

    /// Erase columns `[c0, c1)` of row `row` (0-based, `c1` exclusive).
    #[inline]
    pub(crate) fn erase_row_range(&mut self, row: u32, c0: u32, c1: u32) {
        let base = row * self.cols;
        self.cells[(base + c0) as usize..(base + c1) as usize].fill(Cell::default());
    }

    pub(crate) fn erase_from_cursor_to_end(&mut self) {
        self.erase_to_line_end();
        for r in self.cursor_row + 1..self.rows {
            self.erase_row_range(r, 0, self.cols);
        }
    }

    pub(crate) fn erase_to_line_end(&mut self) {
        self.erase_row_range(self.cursor_row, self.cursor_col, self.cols);
    }

    pub(crate) fn erase_region(&mut self, r0: u32, c0: u32, r1: u32, c1: u32) {
        for r in r0..=r1 {
            let end = if r == r1 { c1 + 1 } else { self.cols };
            self.erase_row_range(r, c0, end);
        }
    }

    pub(crate) fn erase_in_line(&mut self, mode: i32) {
        match mode {
            0 => self.erase_to_line_end(),
            1 => self.erase_row_range(self.cursor_row, 0, self.cursor_col + 1),
            _ => self.erase_row_range(self.cursor_row, 0, self.cols),
        }
    }

    pub(crate) fn erase_chars(&mut self, n: u32) {
        let n = n.min(self.cols - self.cursor_col);
        let start = (self.cursor_row * self.cols + self.cursor_col) as usize;
        self.cells[start..start + n as usize].fill(Cell::default());
    }
}
