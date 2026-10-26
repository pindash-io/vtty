// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! cursor commands for [`Screen`](crate::Screen).

use crate::cell::BufferFlags;

use crate::Screen;

impl Screen {
    #[inline]
    pub(crate) fn clear_wrap(&mut self) {
        self.flags.remove(BufferFlags::WRAP_PENDING);
    }

    /// The row boundary for cursor movement: the scroll-region edge in origin
    /// mode, otherwise the screen edge (`down` = bottom).
    pub(crate) fn row_bound(&self, down: bool) -> u32 {
        if self.flags.contains(BufferFlags::ORIGIN) {
            if down {
                self.bottom_margin
            } else {
                self.top_margin
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
        self.cursor_row = if down {
            (self.cursor_row + n).min(self.row_bound(true))
        } else {
            self.cursor_row.saturating_sub(n).max(self.row_bound(false))
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
        self.cursor_col = if backward {
            self.cursor_col.saturating_sub(n)
        } else {
            (self.cursor_col + n).min(self.cols - 1)
        };
    }

    /// Set the row from a 1-based row number (margin-relative in origin mode).
    pub(crate) fn set_row_1based(&mut self, row: u32) {
        let row = row.saturating_sub(1);
        if self.flags.contains(BufferFlags::ORIGIN) {
            self.cursor_row = self.top_margin + row.min(self.bottom_margin - self.top_margin);
        } else {
            self.cursor_row = row.min(self.rows - 1);
        }
    }

    /// Set the column from a 1-based column number.
    pub(crate) fn set_col_1based(&mut self, col: u32) {
        self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
    }

    /// CUP/HVP — 1-based `(row, col)`, origin-mode aware.
    pub(crate) fn cup(&mut self, row: u32, col: u32) {
        self.clear_wrap();
        self.set_row_1based(row);
        self.set_col_1based(col);
    }

    /// The next tab stop strictly after the cursor, or the last column.
    pub(crate) fn next_tab_stop(&self) -> u32 {
        for c in (self.cursor_col + 1)..self.cols {
            if self.tab_stops[c as usize] {
                return c;
            }
        }
        self.cols - 1
    }

    /// HTS — set a tab stop at the cursor column.
    pub(crate) fn hts(&mut self) {
        self.clear_wrap();
        self.tab_stops[self.cursor_col as usize] = true;
    }

    /// TBC — clear a tab stop: `0` (default) at the cursor, `3` all.
    pub(crate) fn tbc(&mut self, params: &[i32]) {
        match params.first().copied().unwrap_or(0) {
            0 => self.tab_stops[self.cursor_col as usize] = false,
            3 => self.tab_stops.fill(false),
            _ => {}
        }
    }

    /// CHA — 1-based column, same row.
    pub(crate) fn cha(&mut self, col: u32) {
        self.clear_wrap();
        self.set_col_1based(col);
    }

    /// VPA — 1-based row, same column.
    pub(crate) fn vpa(&mut self, row: u32) {
        self.clear_wrap();
        self.set_row_1based(row);
    }

    pub(crate) fn set_margins(&mut self, params: &[i32]) {
        let top = params.first().copied().unwrap_or(1).max(1) as u32 - 1;
        let bottom = params.get(1).copied().unwrap_or(self.rows as i32) as u32 - 1;
        // DECSTBM requires top < bottom; otherwise the sequence is ignored.
        if top >= bottom || bottom >= self.rows {
            return;
        }
        self.apply_margins(top, bottom);
    }

    pub(crate) fn reset_margins(&mut self) {
        self.apply_margins(0, self.rows - 1);
    }

    /// Set the scroll margins to `[top, bottom]` and home the cursor.
    pub(crate) fn apply_margins(&mut self, top: u32, bottom: u32) {
        self.top_margin = top;
        self.bottom_margin = bottom;
        self.cursor_row = self.top_margin;
        self.cursor_col = 0;
    }
}
