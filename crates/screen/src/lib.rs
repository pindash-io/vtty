// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A minimal cell-grid terminal screen for `vtty`.
//!
//! `Screen` is a `Perform` sink: feed it bytes through `vtty::Parser` and it
//! maintains a `rows × cols` grid of styled cells, a cursor, DECSTBM scroll
//! margins, and the common DEC cursor/erase/insert/delete commands. Render
//! with [`Screen::render_line`]/[`Screen::lines`] (plain text) or
//! [`Screen::render_ansi`] (SGR-styled text).
//!
//! This is the MVP — cursor addressing, scrolling, SGR, DECSC/DECRC, DECALN,
//! the alternate screen buffer (DEC 47/1046/1047/1049), scrollback (with
//! resize reflow), tab stops (HTS/TBC), resize reflow, and the VT editing
//! commands. Combining marks and ZWJ emoji families (up to eight trailing
//! glyphs) attach to their base cell.
//!
//! Sixel, `ReGIS`, and downloadable soft fonts are out of scope. This is a
//! text cell grid, not a pixel emulator.
//!
//! The cursor, erase, insert/delete and scroll commands follow EK-VT520-RM
//! (ANSI Control Functions).

#![forbid(unsafe_code)]
#![no_std]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_debug_implementations)]
#![deny(missing_docs)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "terminal cell/pixel math truncates to fixed-size types where the range is known-safe"
)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

mod cell;
mod cursor;
mod edit;
mod erase;
mod perform;
mod put;
mod rect;
mod reflow;
mod render;
mod scroll;
mod scrollback;
mod sgr;

pub use cell::{BufferFlags, Cell, ScreenFlags};
use cell::{default_palette, default_tabs};
use reflow::{fit_grid, reflow_rows, rows_from};
use render::{push_rgb_sgr, push_sgr, render_cells, trailing_blanks};
use scrollback::Scrollback;

/// `OSC 9;4` progress-bar state, as used by modern terminal emulators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Progress {
    /// No progress bar.
    #[default]
    Hidden,
    /// OSC 9;4;0, begin indeterminate.
    Indeterminate,
    /// OSC 9;4;1, determinate percent.
    Percent(u8),
    /// OSC 9;4;2, completed.
    Done,
    /// OSC 9;4;3, errored.
    Error,
}

/// A cursor position: logical row and column. The column may be one past the
/// last column to encode a pending autowrap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Position {
    row: u32,
    col: u32,
}

/// DECSTBM scroll margins (inclusive, 0-based); `top <= bottom < rows`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Margins {
    top: u32,
    bottom: u32,
}

/// One screen buffer: the cell grid plus the per-buffer cursor, style and
/// scroll state swapped in and out by the alternate-screen mode.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Buffer {
    cells: Vec<Cell>,
    /// Ring-buffer offset: logical row `0` lives at physical row `scroll_top`.
    scroll_top: u32,
    cursor: Position,
    saved: Position,
    sgr: SgrFlags,
    fg: ColorId,
    bg: ColorId,
    /// DECSCA protect attribute applied to subsequently written cells.
    protect: bool,
    /// Per-buffer flags: DECOM origin.
    flags: BufferFlags,
    margins: Margins,
    /// Soft-wrap flags: `continuation[r]` is true when row `r` continues the
    /// logical line of row `r-1` (autowrap) rather than starting after a hard
    /// newline. Drives reflow on resize.
    continuation: Vec<bool>,
}

impl Buffer {
    /// A blank buffer of `cols × rows` with default cursor/style/scroll state.
    fn blank(cols: u32, rows: u32) -> Self {
        Self {
            cells: vec![Cell::default(); (cols as usize) * (rows as usize)],
            scroll_top: 0,
            cursor: Position { row: 0, col: 0 },
            saved: Position { row: 0, col: 0 },
            sgr: SgrFlags::DEFAULT,
            fg: DEFAULT_FG_ID,
            bg: DEFAULT_BG_ID,
            protect: false,
            flags: BufferFlags::empty(),
            margins: Margins {
                top: 0,
                bottom: rows - 1,
            },
            continuation: vec![false; rows as usize],
        }
    }
}

/// A minimal cell-grid terminal screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    /// The displayed buffer.
    pub(crate) active: Buffer,
    /// The other buffer: the alternate screen when `ALT_ACTIVE` is set.
    pub(crate) inactive: Buffer,
    /// Global modes: DECAWM autowrap, DECTCEM cursor, alt-screen active.
    pub(crate) modes: ScreenFlags,
    /// Truecolor palette for `ColorId >= 258`.
    pub(crate) rgb: Vec<u32>,
    /// The 16 ANSI colors (theme).
    pub(crate) palette: [u32; 16],
    /// Default foreground RGB.
    pub(crate) default_fg: u32,
    /// Default background RGB.
    pub(crate) default_bg: u32,
    /// `OSC 0/1/2` window title.
    pub(crate) title: String,
    /// `OSC 9;4` progress-bar state.
    pub(crate) progress: Progress,
    /// Scrolled-off full-screen lines (oldest first).
    pub(crate) scrollback: Scrollback,
    /// Tab stops (column → has stop). Default: every 8th column.
    pub(crate) tab_stops: Vec<bool>,
}

impl Screen {
    /// A blank screen of `cols × rows`. Zero dimensions clamp to 1.
    #[inline]
    pub fn new(cols: u32, rows: u32) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            active: Buffer::blank(cols, rows),
            inactive: Buffer::blank(cols, rows),
            modes: ScreenFlags::DEFAULT,
            rgb: Vec::new(),
            palette: default_palette(),
            default_fg: 0xffff_ffff,
            default_bg: 0x0000_00ff,
            title: String::new(),
            progress: Progress::default(),
            scrollback: Scrollback::new(1000),
            tab_stops: default_tabs(cols),
        }
    }

    /// Home the cursor, clear SGR state, and reset the scroll margins (the
    /// per-buffer part of `reset`, DECALN, and alt-screen entry).
    fn reset_cursor_style_margins(&mut self) {
        self.active.cursor.row = 0;
        self.active.cursor.col = 0;
        self.active.sgr = SgrFlags::DEFAULT;
        self.active.fg = DEFAULT_FG_ID;
        self.active.bg = DEFAULT_BG_ID;
        self.active.margins.top = 0;
        self.active.margins.bottom = self.rows - 1;
    }

    /// Reset to a blank grid, default styles, margins and cursor (RIS).
    pub fn reset(&mut self) {
        self.active.cells.fill(Cell::default());
        self.active.scroll_top = 0;
        self.reset_cursor_style_margins();
        self.active.saved.row = 0;
        self.active.saved.col = 0;
        self.modes = ScreenFlags::DEFAULT;
        self.active.protect = false;
        self.active.flags.remove(BufferFlags::ORIGIN);
        self.rgb.clear();
        self.title.clear();
        self.progress = Progress::default();
        // Reset the alternate buffer too.
        self.inactive.cells.fill(Cell::default());
        self.inactive.scroll_top = 0;
        self.inactive.cursor.row = 0;
        self.inactive.cursor.col = 0;
        self.inactive.saved.row = 0;
        self.inactive.saved.col = 0;
        self.inactive.sgr = SgrFlags::DEFAULT;
        self.inactive.fg = DEFAULT_FG_ID;
        self.inactive.bg = DEFAULT_BG_ID;
        self.inactive.protect = false;
        self.inactive.flags = BufferFlags::empty();
        self.inactive.margins.top = 0;
        self.inactive.margins.bottom = self.rows - 1;
        self.scrollback.clear();
        self.tab_stops = default_tabs(self.cols);
        self.active.continuation.fill(false);
        self.inactive.continuation.fill(false);
    }

    /// DECSTR (`CSI ! p`): soft terminal reset. Resets cursor, styles,
    /// margins, modes, and tab stops while keeping the screen contents and
    /// scrollback.
    pub(crate) fn soft_reset(&mut self) {
        self.reset_cursor_style_margins();
        self.active.flags.remove(BufferFlags::ORIGIN);
        self.active.protect = false;
        // DECAWM on, DECTCEM on, IRM off. `ALT_ACTIVE` is untouched: DECSTR
        // resets modes but does not leave the alternate screen.
        self.modes.set(ScreenFlags::WRAP, true);
        self.modes.set(ScreenFlags::CURSOR_VISIBLE, true);
        self.modes.set(ScreenFlags::INSERT, false);
        self.tab_stops = default_tabs(self.cols);
    }

    /// Handle an `OSC 9;4` progress-bar command, ignoring other `OSC 9`
    /// sub-commands (notification, etc.).
    pub(crate) fn osc_progress(&mut self, params: &[&[u8]]) {
        if !matches!(params.get(1).copied(), Some(b"4")) {
            return;
        }
        match params.get(2).copied() {
            Some(b"0") => self.progress = Progress::Indeterminate,
            Some(b"1") => {
                let percent = params
                    .get(3)
                    .and_then(|v| core::str::from_utf8(v).ok())
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(0)
                    .min(100);
                self.progress = Progress::Percent(percent);
            }
            Some(b"2") => self.progress = Progress::Done,
            Some(b"3") => self.progress = Progress::Error,
            _ => {}
        }
    }

    /// Whether the alternate screen buffer is active.
    #[inline]
    pub const fn is_alt_screen(&self) -> bool {
        self.modes.contains(ScreenFlags::ALT_ACTIVE)
    }

    /// Enter the alternate screen buffer, swapping the active and saved per-buffer state.
    /// `clear` also blanks the new alt grid, homes its cursor and resets its margins
    /// and styles (DEC 1047/1049); DEC 47 passes `false` to keep the previous content.
    fn enter_alt(&mut self, clear: bool) {
        if self.modes.contains(ScreenFlags::ALT_ACTIVE) {
            return;
        }
        self.modes.insert(ScreenFlags::ALT_ACTIVE);
        self.swap_buffers();
        if clear {
            self.active.cells.fill(Cell::default());
            self.reset_cursor_style_margins();
            self.active.flags.remove(BufferFlags::ORIGIN);
        }
    }

    /// Leave the alternate screen buffer, swapping back to the main screen
    /// (restoring the cursor that was saved on entry).
    fn leave_alt(&mut self) {
        if !self.modes.contains(ScreenFlags::ALT_ACTIVE) {
            return;
        }
        self.modes.remove(ScreenFlags::ALT_ACTIVE);
        self.swap_buffers();
    }

    const fn swap_buffers(&mut self) {
        core::mem::swap(&mut self.active, &mut self.inactive);
    }

    /// Column count.
    #[inline]
    pub const fn cols(&self) -> u32 {
        self.cols
    }

    /// Row count.
    #[inline]
    pub const fn rows(&self) -> u32 {
        self.rows
    }

    /// Cursor position `(row, col)`, 0-based. A pending wrap reports the
    /// last column, not one past it.
    #[inline]
    pub const fn cursor(&self) -> (u32, u32) {
        let col = if self.active.cursor.col >= self.cols {
            self.cols - 1
        } else {
            self.active.cursor.col
        };
        (self.active.cursor.row, col)
    }

    /// The `OSC 0/1/2` window title.
    #[inline]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The `OSC 9;4` progress-bar state.
    #[must_use]
    #[inline]
    pub const fn progress(&self) -> Progress {
        self.progress
    }

    /// The cell at `(row, col)`, or `None` out of range.
    #[inline]
    pub fn cell(&self, row: u32, col: u32) -> Option<&Cell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        Some(&self.active.cells[self.row_offset(row) + col as usize])
    }

    /// Whether column `col` has a tab stop.
    #[inline]
    pub fn tab_stop_at(&self, col: u32) -> bool {
        col < self.cols && self.tab_stops[col as usize]
    }

    /// Assert the screen's structural invariants: cursor, saved cursor and margins in
    /// bounds, ring-buffer offsets valid, per-buffer vectors sized to the grid. Panics on
    /// the first violation; test and fuzz only.
    pub fn check_invariants(&self) {
        assert!(
            self.active.cursor.row < self.rows,
            "cursor.row {} out of bounds",
            self.active.cursor.row
        );
        assert!(
            self.active.cursor.col <= self.cols,
            "cursor.col {} out of bounds",
            self.active.cursor.col
        );
        assert!(
            self.active.saved.row < self.rows,
            "saved.row {} out of bounds",
            self.active.saved.row
        );
        assert!(
            self.active.saved.col < self.cols,
            "saved.col {} out of bounds",
            self.active.saved.col
        );
        assert!(
            self.active.margins.top <= self.active.margins.bottom,
            "margins inverted: top {} bottom {}",
            self.active.margins.top,
            self.active.margins.bottom
        );
        assert!(
            self.active.margins.bottom < self.rows,
            "margins.bottom {} out of bounds",
            self.active.margins.bottom
        );
        assert!(
            self.active.scroll_top < self.rows,
            "scroll_top {} out of bounds",
            self.active.scroll_top
        );
        assert_eq!(
            self.active.continuation.len(),
            self.rows as usize,
            "continuation length"
        );
        assert_eq!(
            self.active.cells.len(),
            (self.cols as usize) * (self.rows as usize),
            "cells length"
        );
        assert_eq!(self.tab_stops.len(), self.cols as usize, "tab_stops length");

        assert!(
            self.inactive.cursor.row < self.rows,
            "inactive.cursor.row {} out of bounds",
            self.inactive.cursor.row
        );
        assert!(
            self.inactive.cursor.col <= self.cols,
            "inactive.cursor.col {} out of bounds",
            self.inactive.cursor.col
        );
        assert!(
            self.inactive.saved.row < self.rows,
            "inactive.saved.row {} out of bounds",
            self.inactive.saved.row
        );
        assert!(
            self.inactive.saved.col < self.cols,
            "inactive.saved.col {} out of bounds",
            self.inactive.saved.col
        );
        assert!(
            self.inactive.margins.top <= self.inactive.margins.bottom,
            "inactive margins inverted: top {} bottom {}",
            self.inactive.margins.top,
            self.inactive.margins.bottom
        );
        assert!(
            self.inactive.margins.bottom < self.rows,
            "inactive.margins.bottom {} out of bounds",
            self.inactive.margins.bottom
        );
        assert!(
            self.inactive.scroll_top < self.rows,
            "inactive.scroll_top {} out of bounds",
            self.inactive.scroll_top
        );
        assert_eq!(
            self.inactive.continuation.len(),
            self.rows as usize,
            "inactive.continuation length"
        );
        assert_eq!(
            self.inactive.cells.len(),
            (self.cols as usize) * (self.rows as usize),
            "inactive.cells length"
        );

        self.check_cell_continuations(&self.active.cells);
        self.check_cell_continuations(&self.inactive.cells);
    }

    /// Assert that no continuation cell is orphaned: every double-width continuation
    /// immediately follows a non-continuation lead cell. The lead's own width may be 1
    /// for a cluster that is 2 cells wide overall, so it is not checked.
    fn check_cell_continuations(&self, cells: &[Cell]) {
        for row in 0..self.rows as usize {
            let base = row * self.cols as usize;
            for col in 0..self.cols as usize {
                if cells[base + col].is_continuation() {
                    assert!(
                        col > 0 && !cells[base + col - 1].is_continuation(),
                        "orphan continuation at ({row}, {col})"
                    );
                }
            }
        }
    }

    /// Resolve a [`ColorId`] to packed `0xRRGGBBAA`. Default fg/bg use the configured
    /// defaults, `0..=15` the configured palette, `16..=255` the xterm table, and
    /// truecolor ids (`>= 258`) the screen's dynamic palette.
    #[must_use]
    pub fn color_rgb(&self, id: ColorId) -> Option<u32> {
        match id.0 {
            256 => Some(self.default_fg),
            257 => Some(self.default_bg),
            0..=15 => Some(self.palette[id.0 as usize]),
            16..=255 => Some(vtty_core::indexed_rgb(id.0 as u8)),
            _ => self.rgb.get((id.0 as usize).saturating_sub(258)).copied(),
        }
    }

    /// Set the 16 ANSI colors (the theme's palette).
    pub const fn set_palette(&mut self, palette: [u32; 16]) {
        self.palette = palette;
    }

    /// Set the default foreground and background colors.
    pub const fn set_default_colors(&mut self, fg: u32, bg: u32) {
        self.default_fg = fg;
        self.default_bg = bg;
    }

    /// The physical flat offset of logical `row`'s first cell, accounting for
    /// the ring-buffer scroll offset. The ring wraps at most once, so a
    /// branch replaces the `%` (integer division) on this hot path.
    #[inline]
    pub(crate) const fn row_offset(&self, row: u32) -> usize {
        let sum = self.active.scroll_top as usize + row as usize;
        let phys = if sum >= self.rows as usize {
            sum - self.rows as usize
        } else {
            sum
        };
        phys * self.cols as usize
    }

    /// The physical row index of logical `row`.
    #[inline]
    pub(crate) const fn phys_row(&self, row: u32) -> usize {
        let sum = self.active.scroll_top as usize + row as usize;
        if sum >= self.rows as usize {
            sum - self.rows as usize
        } else {
            sum
        }
    }

    /// Rotate the active grid so logical row `0` sits at physical offset `0`.
    /// Region operations memmove contiguous physical ranges, so they call this
    /// first; full-screen scrolls rotate the ring in place instead.
    pub(crate) fn materialize(&mut self) {
        if self.active.scroll_top == 0 {
            return;
        }
        self.active
            .cells
            .rotate_left(self.active.scroll_top as usize * self.cols as usize);
        self.active
            .continuation
            .rotate_left(self.active.scroll_top as usize);
        self.active.scroll_top = 0;
    }

    /// [`Self::materialize`] for the alternate screen buffer.
    pub(crate) fn materialize_alt(&mut self) {
        if self.inactive.scroll_top == 0 {
            return;
        }
        self.inactive
            .cells
            .rotate_left(self.inactive.scroll_top as usize * self.cols as usize);
        self.inactive
            .continuation
            .rotate_left(self.inactive.scroll_top as usize);
        self.inactive.scroll_top = 0;
    }

    /// The cell index range of `row`.
    const fn row_range(&self, row: u32) -> Range<usize> {
        let start = self.row_offset(row);
        start..start + self.cols as usize
    }

    /// Render one row to plain text, trimming trailing spaces and skipping
    /// double-width continuation cells. `None` for an out-of-range row.
    #[must_use]
    pub fn render_line(&self, row: u32) -> Option<String> {
        if row >= self.rows {
            return None;
        }
        Some(render_cells(&self.active.cells[self.row_range(row)]))
    }

    /// Render every row to text, one `String` per row.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        (0..self.rows)
            .map(|r| self.render_line(r).unwrap_or_default())
            .collect()
    }

    /// Render every row to ANSI-styled text, for snapshots and tests: rows
    /// `\r\n`-terminated, trailing blanks trimmed, each line closed with an SGR reset.
    #[must_use]
    pub fn render_ansi(&self) -> String {
        let mut out = String::new();
        for row in 0..self.rows {
            out.push_str(&self.render_ansi_line(row));
            out.push_str("\r\n");
        }
        out
    }

    /// Render one row to ANSI-styled text (no trailing newline).
    #[must_use]
    pub fn render_ansi_line(&self, row: u32) -> String {
        let mut out = String::new();
        if row >= self.rows {
            return out;
        }
        let cells = &self.active.cells[self.row_range(row)];
        let last = cells.len() - trailing_blanks(cells);
        let reverse = self.modes.contains(ScreenFlags::REVERSE);
        // `current` is the emitted style; `None` means "default, no SGR
        // emitted yet". Default-styled cells add no escape sequences (unless
        // reverse video forces explicit colors).
        let mut current: Option<(SgrFlags, ColorId, ColorId)> = None;
        let mut current_rgb: Option<(SgrFlags, u32, u32)> = None;
        for cell in &cells[..last] {
            if cell.is_continuation() {
                continue;
            }
            if reverse {
                // Reverse video swaps foreground/background. Default colors
                // are resolved to RGB so the swap is visible (a snapshot is
                // rendered on a non-reversed terminal).
                let fg = self.color_rgb(cell.bg).unwrap_or(self.default_fg);
                let bg = self.color_rgb(cell.fg).unwrap_or(self.default_bg);
                let state = (cell.sgr, fg, bg);
                if current_rgb != Some(state) {
                    push_rgb_sgr(&mut out, cell.sgr, fg, bg);
                    current_rgb = Some(state);
                }
            } else {
                let is_default =
                    cell.sgr.is_empty() && cell.fg == DEFAULT_FG_ID && cell.bg == DEFAULT_BG_ID;
                if is_default {
                    if current.is_some() {
                        out.push_str("\x1b[0m");
                        current = None;
                    }
                } else {
                    let state = (cell.sgr, cell.fg, cell.bg);
                    if current != Some(state) {
                        push_sgr(&mut out, cell.sgr, cell.fg, cell.bg, &self.rgb);
                        current = Some(state);
                    }
                }
            }
            out.push(cell.ch);
            out.extend(cell.marks().iter().copied());
        }
        if reverse {
            if current_rgb.is_some() {
                out.push_str("\x1b[0m");
            }
        } else if current.is_some() {
            out.push_str("\x1b[0m");
        }
        out
    }

    /// The number of scrollback lines (oldest first).
    #[must_use]
    #[inline]
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// The maximum scrollback capacity (`0` disables).
    #[inline]
    pub const fn max_scrollback(&self) -> usize {
        self.scrollback.max()
    }

    /// Set the scrollback capacity, truncating the oldest lines if shrinking.
    /// `0` disables and clears.
    pub fn set_max_scrollback(&mut self, n: usize) {
        self.scrollback.set_max(n);
    }

    /// Resize to `cols x rows`, reflowing soft-wrapped lines in the screen and its
    /// scrollback. The cursor homes; zero dimensions clamp to 1.
    pub fn resize(&mut self, cols: u32, rows: u32) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let new_cols = cols as usize;
        let new_rows = rows as usize;

        // Restore logical layout so reflow reads rows in display order.
        self.materialize();
        self.materialize_alt();

        // Reflow scrollback + active buffer together as one logical stream.
        let active = rows_from(&self.active.cells, &self.active.continuation, self.cols);
        let mut all_rows = Vec::with_capacity(self.scrollback.len() + active.len());
        all_rows.extend(self.scrollback.iter().cloned());
        all_rows.extend(active);
        let reflowed = reflow_rows(&all_rows, new_cols);

        // The last `rows` rows become the visible screen; the rest return to
        // scrollback (capped at capacity).
        let take = reflowed.len().min(new_rows);
        let skip = reflowed.len() - take;
        let keep = skip.min(self.scrollback.max());
        let new_scrollback = reflowed[skip - keep..skip].to_vec();
        let (new_cells, new_cont) = fit_grid(&reflowed, new_cols, new_rows);

        // Reflow the inactive buffer (it has no scrollback of its own).
        let inactive = rows_from(&self.inactive.cells, &self.inactive.continuation, self.cols);
        let inactive_reflowed = reflow_rows(&inactive, new_cols);
        let (new_inactive_cells, new_inactive_cont) =
            fit_grid(&inactive_reflowed, new_cols, new_rows);

        self.cols = cols;
        self.rows = rows;
        self.active.cells = new_cells;
        self.active.continuation = new_cont;
        self.inactive.cells = new_inactive_cells;
        self.inactive.continuation = new_inactive_cont;
        self.scrollback.replace(new_scrollback);
        self.active.cursor.row = 0;
        self.active.cursor.col = 0;
        self.active.saved.row = 0;
        self.active.saved.col = 0;
        self.active.margins.top = 0;
        self.active.margins.bottom = rows - 1;
        self.inactive.margins.top = 0;
        self.inactive.margins.bottom = rows - 1;
        self.tab_stops = default_tabs(cols);
    }

    /// The raw cells of scrollback line `i` (`0` = oldest), or `None`.
    #[inline]
    pub fn scrollback_line(&self, i: usize) -> Option<&[Cell]> {
        self.scrollback.line(i)
    }

    /// Render scrollback line `i` to text, or `None` out of range.
    #[must_use]
    pub fn scrollback_text(&self, i: usize) -> Option<String> {
        self.scrollback.line(i).map(render_cells)
    }

    /// Attach a zero-width glyph (combining mark, variation selector, skin tone) to the
    /// base cell left of the cursor, skipping a double-width continuation. Marks past
    /// eight, and a leading mark with no base, are dropped.
    fn attach_mark(&mut self, c: char) {
        if self.active.cursor.col == 0 {
            return;
        }
        let mut col = self.active.cursor.col - 1;
        let base = self.row_offset(self.active.cursor.row);
        let idx = |col: u32| base + col as usize;
        if self.active.cells[idx(col)].is_continuation() {
            col = col.saturating_sub(1);
        }
        let cell = &mut self.active.cells[idx(col)];
        if cell.ch == ' ' || cell.is_continuation() {
            return;
        }
        if (cell.combining_len as usize) < 8 {
            cell.combining[cell.combining_len as usize] = c;
            cell.combining_len += 1;
        }
    }
}

#[cfg(test)]
mod tests;
