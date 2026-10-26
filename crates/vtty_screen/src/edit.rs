// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! edit commands for [`Screen`](crate::Screen).

use crate::cell::Cell;

use crate::Screen;

impl Screen {
    pub(crate) fn insert_lines(&mut self, n: u32) {
        self.shift_lines(n, true);
    }

    pub(crate) fn delete_lines(&mut self, n: u32) {
        self.shift_lines(n, false);
    }

    /// Insert (`insert = true`) or delete (`false`) blank lines at the cursor
    /// within the scroll region, shifting the soft-wrap flags with the rows.
    pub(crate) fn shift_lines(&mut self, n: u32, insert: bool) {
        for _ in 0..n {
            if self.cursor_row < self.top_margin || self.cursor_row > self.bottom_margin {
                break;
            }
            let width = self.cols as usize;
            let start = self.cursor_row as usize * width;
            let end = (self.bottom_margin as usize + 1) * width;
            let c = self.cursor_row as usize;
            let b = self.bottom_margin as usize;
            if insert {
                self.cells.copy_within(start..end - width, start + width);
                self.cells[start..start + width].fill(Cell::default());
                self.continuation.copy_within(c..b, c + 1);
                self.continuation[c] = false;
            } else {
                self.cells.copy_within(start + width..end, start);
                self.cells[end - width..end].fill(Cell::default());
                self.continuation.copy_within(c + 1..=b, c);
                self.continuation[b] = false;
            }
        }
    }

    pub(crate) fn insert_chars(&mut self, n: u32) {
        self.shift_chars(n, true);
    }

    pub(crate) fn delete_chars(&mut self, n: u32) {
        self.shift_chars(n, false);
    }

    /// Insert (`insert = true`) or delete (`false`) blank cells at the cursor
    /// on the current line.
    pub(crate) fn shift_chars(&mut self, n: u32, insert: bool) {
        let n = n.min(self.cols - self.cursor_col) as usize;
        let start = (self.cursor_row * self.cols + self.cursor_col) as usize;
        let end = ((self.cursor_row + 1) * self.cols) as usize;
        if insert {
            self.cells.copy_within(start..end - n, start + n);
            self.cells[start..start + n].fill(Cell::default());
        } else {
            self.cells.copy_within(start + n..end, start);
            self.cells[end - n..end].fill(Cell::default());
        }
    }
}
