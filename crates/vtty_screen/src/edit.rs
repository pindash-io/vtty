// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! edit commands for [`Screen`](crate::Screen).

use crate::Screen;
use crate::cell::Cell;

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
        // Region memmoves need logical == physical layout.
        self.materialize();
        if self.cursor_row < self.top_margin || self.cursor_row > self.bottom_margin {
            return;
        }

        let width = self.cols as usize;
        let c = self.cursor_row as usize;
        let b = self.bottom_margin as usize;
        let n = (n as usize).min(b - c + 1);
        if n == 0 {
            return;
        }
        let start = c * width;
        let end = (b + 1) * width;

        // Content and soft-wrap flags shift one way by `n` rows in one pass;
        // the vacated `n` rows go blank. The source and destination overlap,
        // and `copy_within` moves them in the right order.
        let (src, dst, blank, c_src, c_dst, c_blank) = if insert {
            (
                start..end - n * width,
                start + n * width,
                start..start + n * width,
                c..b + 1 - n,
                c + n,
                c..c + n,
            )
        } else {
            (
                start + n * width..end,
                start,
                end - n * width..end,
                c + n..b + 1,
                c,
                b + 1 - n..b + 1,
            )
        };

        self.cells.copy_within(src, dst);
        self.cells[blank].fill(Cell::default());
        self.continuation.copy_within(c_src, c_dst);
        self.continuation[c_blank].fill(false);
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
        let row = self.row_offset(self.cursor_row);
        let start = row + self.cursor_col as usize;
        let end = row + self.cols as usize;

        // Content shifts one way; the vacated `n` cells go blank. The source
        // and destination overlap, and `copy_within` moves them in the right
        // order.
        let (src, dst, blank) = if insert {
            (start..end - n, start + n, start..start + n)
        } else {
            (start + n..end, start, end - n..end)
        };
        self.cells.copy_within(src, dst);
        self.cells[blank].fill(Cell::default());
    }
}
