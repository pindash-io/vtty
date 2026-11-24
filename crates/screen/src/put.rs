// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Put commands for [`Screen`](crate::Screen).

use vtty_width::{cell_width, cell_width_sum};

use crate::Screen;
use crate::cell::{CONTINUATION, Cell, ScreenFlags};

impl Screen {
    /// A base cell carrying the current SGR/colors for glyph `ch`, with no
    /// combining marks yet.
    pub(crate) const fn base_cell(&self, ch: char) -> Cell {
        Cell {
            ch,
            sgr: self.active.sgr,
            fg: self.active.fg,
            bg: self.active.bg,
            protected: self.active.protect,
            combining: ['\0'; 8],
            combining_len: 0,
        }
    }

    /// Write one glyph, handling double-width cells and autowrap.
    pub(crate) fn put_char(&mut self, c: char) {
        let width = cell_width(c).unwrap_or(1) as u32;
        if width == 0 {
            self.attach_mark(c);
            return;
        }
        self.put_base(c, width);
    }

    /// Write a run of printable ASCII bytes as width-1 cells. `print_bytes`
    /// only ever receives `0x20..=0x7e` (the parser bulk-dispatches printable
    /// ASCII runs), so the per-byte width lookup and double-width branch of
    /// [`Self::put_base`] are dead work here. This is the bulk text hot path.
    pub(crate) fn put_ascii(&mut self, s: &[u8]) {
        // Insert mode writes one cell at a time so each glyph shifts the row
        // right; the bulk path below is the non-insert fast path.
        if self.modes.contains(ScreenFlags::INSERT) {
            for &b in s {
                self.put_base(b as char, 1);
            }
            return;
        }
        let wrap = self.modes.contains(ScreenFlags::WRAP);
        // The per-cell attributes are constant across the run; build the
        // template once and only vary `ch` per byte.
        let template = self.base_cell(' ');
        let mut col = self.active.cursor.col;

        let mut i = 0;
        while i < s.len() {
            // A pending wrap leaves the cursor one past the last column;
            // move to the next line before writing.
            if col == self.cols {
                self.advance_line();
                col = 0;
            }
            // A cursor on a double-width continuation acts on the glyph's lead.
            let base = self.row_offset(self.active.cursor.row);
            if col > 0 && self.active.cells[base + col as usize].is_continuation() {
                col -= 1;
            }
            while i < s.len() && col < self.cols {
                let idx = base + col as usize;
                self.active.cells[idx] = Cell {
                    ch: s[i] as char,
                    ..template
                };
                i += 1;
                col += 1;
                // Overwriting a wide glyph's lead orphans its continuation.
                if col < self.cols && self.active.cells[idx + 1].is_continuation() {
                    self.active.cells[idx + 1] = Cell::default();
                }
            }
            if col >= self.cols {
                col = if wrap { self.cols } else { self.cols - 1 };
            }
        }
        self.active.cursor.col = col;
    }

    /// Write a base glyph (width 1 or 2) at the cursor, handling autowrap.
    pub(crate) fn put_base(&mut self, c: char, mut width: u32) {
        // A double-width glyph needs two cells; a one-column grid can't hold
        // it, so clamp to the grid width (otherwise the `cells[idx + 1]`
        // continuation write goes out of bounds).
        if width > self.cols {
            width = self.cols;
        }
        // A pending wrap leaves the cursor one past the last column; move to
        // the next line before writing.
        if self.active.cursor.col == self.cols {
            self.advance_line();
        }
        // A cursor on a double-width continuation acts on the glyph's lead,
        // so a write replaces/shifts the whole glyph, never half of it.
        if self.active.cursor.col > 0 {
            let idx = self.row_offset(self.active.cursor.row) + self.active.cursor.col as usize;
            if self.active.cells[idx].is_continuation() {
                self.active.cursor.col -= 1;
            }
        }
        // A double-width glyph needs two cells; wrap if only one remains.
        if width == 2 && self.active.cursor.col == self.cols - 1 {
            if self.modes.contains(ScreenFlags::WRAP) {
                self.advance_line();
            } else {
                width = 1;
            }
        }
        if self.modes.contains(ScreenFlags::INSERT) {
            self.insert_shift(width);
        }
        let row_start = self.row_offset(self.active.cursor.row);
        let idx = row_start + self.active.cursor.col as usize;
        let row_end = row_start + self.cols as usize;
        let cell = self.base_cell(c);
        self.active.cells[idx] = cell;
        self.active.cursor.col += width;
        if width == 2 {
            self.active.cells[idx + 1] = Cell {
                ch: CONTINUATION,
                ..cell
            };
        } else if idx + 1 < row_end && self.active.cells[idx + 1].is_continuation() {
            // Overwrote a double-width glyph's lead: clear the now-orphaned
            // continuation cell.
            self.active.cells[idx + 1] = Cell::default();
        }
        if self.active.cursor.col >= self.cols {
            self.active.cursor.col = if self.modes.contains(ScreenFlags::WRAP) {
                self.cols
            } else {
                self.cols - 1
            };
        }
    }

    /// Shift the rest of the current row right by `width` cells (IRM insert
    /// mode); cells shifted past the last column are dropped.
    fn insert_shift(&mut self, width: u32) {
        let cols = self.cols as usize;
        let col = self.active.cursor.col as usize;
        let shift = (width as usize).min(cols - col);
        if shift == 0 {
            return;
        }
        let start = self.row_offset(self.active.cursor.row);
        self.active.cells[start..start + cols].copy_within(col..cols - shift, col + shift);
    }

    /// Write a grapheme cluster as one cell: the first glyph is the base, the
    /// rest are trailing marks. This is what makes a ZWJ emoji family
    /// (`👨‍👩‍👧`) occupy one cell instead of separate glyphs.
    pub(crate) fn put_cluster(&mut self, cluster: &str) {
        let mut chars = cluster.chars();
        let Some(base) = chars.next() else {
            return;
        };
        // A lone combining mark (no base in this run) attaches to the cell
        // left of the cursor, matching `put_char`'s per-glyph behavior.
        if cell_width(base).unwrap_or(1) == 0 {
            for c in cluster.chars() {
                self.attach_mark(c);
            }
            return;
        }
        self.put_base(base, cell_width_sum(cluster) as u32);
        for c in chars {
            self.attach_mark(c);
        }
    }
}
