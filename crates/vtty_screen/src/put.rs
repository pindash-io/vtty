// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! put commands for [`Screen`](crate::Screen).

use crate::cell::{BufferFlags, CONTINUATION, Cell, ScreenFlags};
use vtty_width::{cell_width, cell_width_sum};

use crate::Screen;

impl Screen {
    /// Write one glyph, handling double-width cells and autowrap.
    pub(crate) fn put_char(&mut self, c: char) {
        let width = cell_width(c).unwrap_or(1) as u32;
        if width == 0 {
            self.attach_mark(c);
            return;
        }
        self.put_base(c, width);
    }

    /// Write a base glyph (width 1 or 2) at the cursor, handling autowrap.
    pub(crate) fn put_base(&mut self, c: char, mut width: u32) {
        if self.flags.contains(BufferFlags::WRAP_PENDING) {
            self.advance_line();
        }
        if self.cursor_col >= self.cols {
            self.cursor_col = self.cols - 1;
        }
        // A double-width glyph needs two cells; wrap if only one remains.
        if width == 2 && self.cursor_col == self.cols - 1 {
            if self.modes.contains(ScreenFlags::WRAP) {
                self.advance_line();
            } else {
                width = 1;
            }
        }
        let cell = Cell {
            ch: c,
            sgr: self.sgr,
            fg: self.fg,
            bg: self.bg,
            combining: ['\0'; 8],
            combining_len: 0,
        };
        let idx = (self.cursor_row * self.cols + self.cursor_col) as usize;
        self.cells[idx] = cell;
        self.cursor_col += width;
        if width == 2 {
            self.cells[idx + 1] = Cell {
                ch: CONTINUATION,
                ..cell
            };
        }
        if self.cursor_col >= self.cols {
            self.cursor_col = self.cols - 1;
            self.flags.set(
                BufferFlags::WRAP_PENDING,
                self.modes.contains(ScreenFlags::WRAP),
            );
        }
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
