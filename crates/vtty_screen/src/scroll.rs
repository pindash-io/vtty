// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! scroll commands for [`Screen`](crate::Screen).

use crate::Screen;
use crate::cell::Cell;

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
        let row = self.phys_row(self.cursor_row);
        self.continuation[row] = soft_wrap;
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
        let width = self.cols as usize;
        let start = (self.scroll_top as usize) * width;
        let cont = self.continuation[self.scroll_top as usize];
        if self.scrollback.len() < self.max_scrollback {
            let row = self.cells[start..start + width].to_vec();
            self.scrollback.push_back((row, cont));
            return;
        }
        // At capacity, recycle the oldest line's `Vec<Cell>` allocation
        // instead of freeing it and allocating a fresh one for the incoming
        // line. `len >= max_scrollback >= 1`, so the queue is non-empty; the
        // `if let` keeps the panic-free guarantee the crate promises.
        if let Some(mut oldest) = self.scrollback.pop_front() {
            oldest.0.copy_from_slice(&self.cells[start..start + width]);
            oldest.1 = cont;
            self.scrollback.push_back(oldest);
        }
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

        // Full-screen scrolls rotate the ring in place — O(cols) per line
        // instead of memmoving the whole grid.
        if top == 0 && bottom == self.rows as usize - 1 {
            if down {
                // RI / SD: rotate down; the new top row is blank.
                self.scroll_top = if self.scroll_top == 0 {
                    self.rows - 1
                } else {
                    self.scroll_top - 1
                };
                let start = (self.scroll_top as usize) * width;
                self.cells[start..start + width].fill(Cell::default());
                self.continuation[self.scroll_top as usize] = false;
            } else {
                // LF / SU: rotate up; the scrolled-out top row goes to
                // scrollback and its physical slot becomes the new bottom row.
                let top_row = self.scroll_top as usize;
                self.push_scrollback();
                self.scroll_top = (self.scroll_top + 1) % self.rows;
                let start = top_row * width;
                self.cells[start..start + width].fill(Cell::default());
                self.continuation[top_row] = false;
            }
            return;
        }

        // Region scroll (DECSTBM): restore the logical layout, then memmove.
        self.materialize();
        let start = top * width;
        let end = (bottom + 1) * width;
        if down {
            self.cells.copy_within(start..end - width, start + width);
            self.cells[start..start + width].fill(Cell::default());
            self.continuation.copy_within(top..bottom, top + 1);
            self.continuation[top] = false;
        } else {
            self.cells.copy_within(start + width..end, start);
            self.cells[end - width..end].fill(Cell::default());
            // Shift the soft-wrap flags up with the rows; the new blank
            // bottom row starts a fresh logical line.
            self.continuation.copy_within(top + 1..=bottom, top);
            self.continuation[bottom] = false;
        }
    }

    /// Scroll the region `n` lines in one call. Full-screen scrolls rotate
    /// the ring `n` times, each O(cols), so a loop over [`Self::scroll`] is
    /// already optimal there. Region scrolls memmove `n` lines in one pass
    /// instead of `n` passes.
    pub(crate) fn scroll_n(&mut self, down: bool, n: u32) {
        let top = self.top_margin as usize;
        let bottom = self.bottom_margin as usize;

        if top == 0 && bottom == self.rows as usize - 1 {
            for _ in 0..n {
                self.scroll(down);
            }
            return;
        }

        // Region scroll (DECSTBM): restore the logical layout once, then
        // shift `n` rows in one pass.
        self.materialize();
        let width = self.cols as usize;
        let n = (n as usize).min(bottom - top + 1);
        if n == 0 {
            return;
        }
        let start = top * width;
        let end = (bottom + 1) * width;

        // Content and soft-wrap flags shift one way by `n` rows; the vacated
        // `n` rows go blank. The source and destination overlap, and
        // `copy_within` moves them in the right order.
        let (src, dst, blank, c_src, c_dst, c_blank) = if down {
            (
                start..end - n * width,
                start + n * width,
                start..start + n * width,
                top..bottom + 1 - n,
                top + n,
                top..top + n,
            )
        } else {
            (
                start + n * width..end,
                start,
                end - n * width..end,
                top + n..bottom + 1,
                top,
                bottom + 1 - n..bottom + 1,
            )
        };

        self.cells.copy_within(src, dst);
        self.cells[blank].fill(Cell::default());
        self.continuation.copy_within(c_src, c_dst);
        self.continuation[c_blank].fill(false);
    }
}
