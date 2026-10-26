// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! scroll commands for [`Screen`](crate::Screen).

use crate::cell::Cell;

use crate::Screen;

impl Screen {
    /// LF: down one line (scrolling within the margins), keeping the column.
    /// Marks the target row as a hard break.
    pub(crate) fn linefeed(&mut self) {
        self.line_advance(false);
    }

    /// Down one line and home to column 0 (the autowrap advance). Marks the
    /// target row as a soft-wrap continuation.
    pub(crate) fn advance_line(&mut self) {
        self.line_advance(true);
    }

    /// Down one line (scrolling within the margins). `soft_wrap` homes to
    /// column 0 and marks the target row as a continuation of the previous.
    pub(crate) fn line_advance(&mut self, soft_wrap: bool) {
        self.clear_wrap();
        self.move_down();
        if soft_wrap {
            self.cursor_col = 0;
        }
        self.continuation[self.cursor_row as usize] = soft_wrap;
    }

    /// Move the cursor down one line, scrolling within the margins if at the
    /// bottom edge. Shared by LF (hard break) and autowrap (soft wrap).
    pub(crate) fn move_down(&mut self) {
        self.move_vertical(true);
    }

    /// RI: up one line (scrolling within the margins), keeping the column.
    pub(crate) fn reverse_index(&mut self) {
        self.clear_wrap();
        self.move_vertical(false);
    }

    /// Move the cursor one line, scrolling within the margins at the edge.
    pub(crate) fn move_vertical(&mut self, down: bool) {
        if down {
            if self.cursor_row >= self.bottom_margin {
                self.scroll_up();
            } else {
                self.cursor_row += 1;
            }
        } else if self.cursor_row <= self.top_margin {
            self.scroll_down();
        } else {
            self.cursor_row -= 1;
        }
    }

    pub(crate) fn scroll_up(&mut self) {
        self.scroll(false);
    }

    pub(crate) fn push_scrollback(&mut self) {
        if self.max_scrollback == 0 {
            return;
        }
        if self.scrollback.len() >= self.max_scrollback {
            self.scrollback.pop_front();
        }
        let row = self.cells[..self.cols as usize].to_vec();
        let cont = self.continuation[0];
        self.scrollback.push_back((row, cont));
    }

    pub(crate) fn scroll_down(&mut self) {
        self.scroll(true);
    }

    /// Scroll the region one line. `down` shifts content down (RI / SD) and
    /// inserts a blank row at the top; otherwise content shifts up (LF / SU),
    /// and a full-screen scroll pushes the top row into scrollback.
    pub(crate) fn scroll(&mut self, down: bool) {
        let width = self.cols as usize;
        let top = self.top_margin as usize;
        let bottom = self.bottom_margin as usize;
        let start = top * width;
        let end = (bottom + 1) * width;
        if down {
            self.cells.copy_within(start..end - width, start + width);
            self.cells[start..start + width].fill(Cell::default());
            self.continuation.copy_within(top..bottom, top + 1);
            self.continuation[top] = false;
        } else {
            // Full-screen scrolls push the top row into scrollback (editing
            // operations such as IL/DL scroll in place and do not).
            if top == 0 && bottom == self.rows as usize - 1 {
                self.push_scrollback();
            }
            self.cells.copy_within(start + width..end, start);
            self.cells[end - width..end].fill(Cell::default());
            // Shift the soft-wrap flags up with the rows; the new blank
            // bottom row starts a fresh logical line.
            self.continuation.copy_within(top + 1..=bottom, top);
            self.continuation[bottom] = false;
        }
    }
}
