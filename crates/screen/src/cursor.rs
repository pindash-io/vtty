// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cursor commands for [`Screen`](crate::Screen).

use crate::Screen;
use crate::cell::BufferFlags;

impl Screen {
    /// Consume a pending wrap (the cursor one past the last column) by
    /// clamping the cursor back to the last column.
    #[inline]
    pub(crate) fn clear_wrap(&mut self) {
        if self.active.cursor.col == self.cols {
            self.active.cursor.col = self.cols - 1;
        }
    }

    /// The row boundary for cursor movement: the scroll-region edge in origin
    /// mode, otherwise the screen edge (`down` = bottom).
    pub(crate) const fn row_bound(&self, down: bool) -> u32 {
        if self.active.flags.contains(BufferFlags::ORIGIN) {
            if down {
                self.active.margins.bottom
            } else {
                self.active.margins.top
            }
        } else if down {
            self.rows - 1
        } else {
            0
        }
    }

    pub(crate) fn cursor_up(&mut self, n: u32) {
        self.cursor_vertical(n, false);
    }

    pub(crate) fn cursor_down(&mut self, n: u32) {
        self.cursor_vertical(n, true);
    }

    /// Move the cursor vertically, clamping to the margin (or screen) bounds.
    pub(crate) fn cursor_vertical(&mut self, n: u32, down: bool) {
        self.clear_wrap();
        self.active.cursor.row = if down {
            (self.active.cursor.row + n).min(self.row_bound(true))
        } else {
            self.active
                .cursor
                .row
                .saturating_sub(n)
                .max(self.row_bound(false))
        };
    }

    pub(crate) fn cursor_forward(&mut self, n: u32) {
        self.cursor_horizontal(n, false);
    }

    pub(crate) fn cursor_back(&mut self, n: u32) {
        self.cursor_horizontal(n, true);
    }

    /// Move the cursor horizontally, clamping to the row bounds.
    pub(crate) fn cursor_horizontal(&mut self, n: u32, backward: bool) {
        self.clear_wrap();
        self.active.cursor.col = if backward {
            self.active.cursor.col.saturating_sub(n)
        } else {
            self.active.cursor.col.saturating_add(n).min(self.cols - 1)
        };
    }

    /// Set the row from a 1-based row number (margin-relative in origin mode).
    pub(crate) fn set_row_1based(&mut self, row: u32) {
        let row = row.saturating_sub(1);
        if self.active.flags.contains(BufferFlags::ORIGIN) {
            self.active.cursor.row = self.active.margins.top
                + row.min(self.active.margins.bottom - self.active.margins.top);
        } else {
            self.active.cursor.row = row.min(self.rows - 1);
        }
    }

    /// Set the column from a 1-based column number.
    pub(crate) fn set_col_1based(&mut self, col: u32) {
        self.active.cursor.col = col.saturating_sub(1).min(self.cols - 1);
    }

    /// CUP/HVP: 1-based `(row, col)`, origin-mode aware.
    pub(crate) fn cup(&mut self, row: u32, col: u32) {
        self.clear_wrap();
        self.set_row_1based(row);
        self.set_col_1based(col);
    }

    /// The next tab stop strictly after the cursor, or the last column.
    pub(crate) fn next_tab_stop(&self) -> u32 {
        for c in (self.active.cursor.col + 1)..self.cols {
            if self.tab_stops[c as usize] {
                return c;
            }
        }
        self.cols - 1
    }

    /// HTS: set a tab stop at the cursor column.
    pub(crate) fn hts(&mut self) {
        self.clear_wrap();
        self.tab_stops[self.active.cursor.col as usize] = true;
    }

    /// TBC: clear a tab stop at the cursor (`0`, the default) or all of them (`3`).
    pub(crate) fn tbc(&mut self, params: &[i32]) {
        self.clear_wrap();
        match params.first().copied().unwrap_or(0) {
            0 => self.tab_stops[self.active.cursor.col as usize] = false,
            3 => self.tab_stops.fill(false),
            _ => {}
        }
    }

    /// CHA: 1-based column, same row.
    pub(crate) fn cha(&mut self, col: u32) {
        self.clear_wrap();
        self.set_col_1based(col);
    }

    /// VPA: 1-based row, same column.
    pub(crate) fn vpa(&mut self, row: u32) {
        self.clear_wrap();
        self.set_row_1based(row);
    }

    pub(crate) fn set_margins(&mut self, params: &[i32]) {
        // DECSTBM: 0 or negative means the default row, and must not reach the
        // `- 1` below (a `0` param would underflow).
        let top = params.first().copied().filter(|&p| p > 0).unwrap_or(1) as u32 - 1;
        let bottom = params
            .get(1)
            .copied()
            .filter(|&p| p > 0)
            .unwrap_or(self.rows as i32) as u32
            - 1;
        // DECSTBM requires top < bottom; otherwise the sequence is ignored.
        if top >= bottom || bottom >= self.rows {
            return;
        }
        self.apply_margins(top, bottom);
    }

    /// Set the scroll margins to `[top, bottom]` and home the cursor.
    pub(crate) fn apply_margins(&mut self, top: u32, bottom: u32) {
        self.clear_wrap();
        self.active.margins.top = top;
        self.active.margins.bottom = bottom;
        self.active.cursor.row = self.active.margins.top;
        self.active.cursor.col = 0;
    }
}
