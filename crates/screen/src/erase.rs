// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! erase commands for [`Screen`](crate::Screen).

use crate::Screen;
use crate::cell::Cell;

impl Screen {
    pub(crate) fn erase_in_display(&mut self, mode: i32, selective: bool) {
        match mode {
            0 => {
                // Cursor to end.
                self.erase_from_cursor_to_end(selective);
            }
            1 => {
                // Start to cursor.
                let (r, c) = (self.cursor_row, self.cursor_col);
                self.erase_region(0, 0, r, c, selective);
            }
            _ => {
                // 2 (and 3, scrollback — same in the MVP): whole screen.
                if selective {
                    self.erase_region(0, 0, self.rows - 1, self.cols - 1, true);
                } else {
                    self.cells.fill(Cell::default());
                }
            }
        }
    }

    /// Erase columns `[c0, c1)` of row `row` (0-based, `c1` exclusive).
    /// Selective erase skips protected cells.
    #[inline]
    pub(crate) fn erase_row_range(&mut self, row: u32, c0: u32, c1: u32, selective: bool) {
        let base = self.row_offset(row);
        // Extend the range to whole-glyph boundaries so erasing never splits
        // or strands a double-width glyph.
        let mut c0 = c0;
        let mut c1 = c1;
        if c0 > 0 && c0 < self.cols && self.cells[base + c0 as usize].is_continuation() {
            c0 -= 1;
        }
        if c1 > 0 && c1 < self.cols && self.cells[base + c1 as usize].is_continuation() {
            c1 += 1;
        }
        let range = &mut self.cells[base + c0 as usize..base + c1 as usize];
        if selective {
            for cell in range {
                if !cell.protected {
                    *cell = Cell::default();
                }
            }
        } else {
            range.fill(Cell::default());
        }
    }

    pub(crate) fn erase_from_cursor_to_end(&mut self, selective: bool) {
        self.erase_to_line_end(selective);
        for r in self.cursor_row + 1..self.rows {
            self.erase_row_range(r, 0, self.cols, selective);
        }
    }

    pub(crate) fn erase_to_line_end(&mut self, selective: bool) {
        self.erase_row_range(self.cursor_row, self.cursor_col, self.cols, selective);
    }

    pub(crate) fn erase_region(&mut self, r0: u32, c0: u32, r1: u32, c1: u32, selective: bool) {
        for r in r0..=r1 {
            let end = if r == r1 { c1 + 1 } else { self.cols };
            self.erase_row_range(r, c0, end, selective);
        }
    }

    pub(crate) fn erase_in_line(&mut self, mode: i32, selective: bool) {
        match mode {
            0 => self.erase_to_line_end(selective),
            1 => self.erase_row_range(self.cursor_row, 0, self.cursor_col + 1, selective),
            _ => self.erase_row_range(self.cursor_row, 0, self.cols, selective),
        }
    }

    pub(crate) fn erase_chars(&mut self, n: u32) {
        let cols = self.cols as usize;
        let row = self.row_offset(self.cursor_row);
        let mut start = self.cursor_col as usize;
        if start > 0 && self.cells[row + start].is_continuation() {
            start -= 1;
        }
        // Erase `n` logical glyphs (a double-width glyph is one glyph, two
        // cells) so a wide glyph is never left half-erased.
        let mut count = 0_usize;
        let mut glyphs = 0_usize;
        let mut col = start;
        while col < cols && glyphs < n as usize {
            let wide = col + 1 < cols && self.cells[row + col + 1].is_continuation();
            let width = if wide { 2 } else { 1 };
            count += width;
            glyphs += 1;
            col += width;
        }
        if count == 0 {
            return;
        }
        self.cells[row + start..row + start + count].fill(Cell::default());
    }
}
