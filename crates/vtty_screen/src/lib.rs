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

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, Perform, SgrFlags};

mod cell;
mod cursor;
mod edit;
mod erase;
mod put;
mod rect;
mod reflow;
mod render;
mod scroll;
mod sgr;

pub use cell::{BufferFlags, Cell, ScreenFlags};
use cell::{default_palette, default_tabs};
use reflow::{fit_grid, reflow_rows, rows_from};
use render::{push_rgb_sgr, push_sgr, render_cells, trailing_blanks};

/// `OSC 9;4` progress-bar state, as used by modern terminal emulators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Progress {
    /// No progress bar is shown.
    #[default]
    Hidden,
    /// `OSC 9;4;0` — begin, indeterminate; the terminal animates it.
    Indeterminate,
    /// `OSC 9;4;1;<n>` — determinate progress, percent `0..=100`.
    Percent(u8),
    /// `OSC 9;4;2` — completed.
    Done,
    /// `OSC 9;4;3` — errored.
    Error,
}

/// A minimal cell-grid terminal screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    pub(crate) cells: Vec<Cell>,
    /// Ring-buffer offset: logical row `0` lives at physical row `scroll_top`.
    pub(crate) scroll_top: u32,
    pub(crate) cursor_row: u32,
    pub(crate) cursor_col: u32,
    pub(crate) saved_row: u32,
    pub(crate) saved_col: u32,
    pub(crate) sgr: SgrFlags,
    pub(crate) fg: ColorId,
    pub(crate) bg: ColorId,
    /// DECSCA protect attribute applied to subsequently written cells.
    pub(crate) protect: bool,
    /// Global modes: DECAWM autowrap, DECTCEM cursor, alt-screen active.
    pub(crate) modes: ScreenFlags,
    /// Per-buffer flags: autowrap-pending and DECOM origin.
    pub(crate) flags: BufferFlags,
    /// DECSTBM top margin (inclusive, 0-based).
    pub(crate) top_margin: u32,
    /// DECSTBM bottom margin (inclusive, 0-based).
    pub(crate) bottom_margin: u32,
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
    // The alternate screen buffer (DEC 47/1046/1047/1049): the same
    // per-buffer state as the main screen, swapped in/out wholesale.
    pub(crate) alt_cells: Vec<Cell>,
    /// Ring-buffer offset for the alternate screen buffer.
    pub(crate) alt_scroll_top: u32,
    pub(crate) alt_cursor_row: u32,
    pub(crate) alt_cursor_col: u32,
    pub(crate) alt_saved_row: u32,
    pub(crate) alt_saved_col: u32,
    pub(crate) alt_sgr: SgrFlags,
    pub(crate) alt_fg: ColorId,
    pub(crate) alt_bg: ColorId,
    pub(crate) alt_protect: bool,
    pub(crate) alt_flags: BufferFlags,
    pub(crate) alt_top_margin: u32,
    pub(crate) alt_bottom_margin: u32,
    pub(crate) alt_continuation: Vec<bool>,
    /// Scrolled-off full-screen lines (oldest first), each with its soft-wrap
    /// continuation flag so resize can reflow history too.
    pub(crate) scrollback: VecDeque<(Vec<Cell>, bool)>,
    /// Maximum scrollback lines; `0` disables.
    pub(crate) max_scrollback: usize,
    /// Tab stops (column → has stop). Default: every 8th column.
    pub(crate) tab_stops: Vec<bool>,
    /// Soft-wrap flags: `continuation[r]` is true when row `r` continues the
    /// logical line of row `r-1` (autowrap) rather than starting after a hard
    /// newline. Drives reflow on resize.
    pub(crate) continuation: Vec<bool>,
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
            cells: vec![Cell::default(); (cols as usize) * (rows as usize)],
            scroll_top: 0,
            cursor_row: 0,
            cursor_col: 0,
            saved_row: 0,
            saved_col: 0,
            sgr: SgrFlags::DEFAULT,
            fg: DEFAULT_FG_ID,
            bg: DEFAULT_BG_ID,
            protect: false,
            modes: ScreenFlags::DEFAULT,
            flags: BufferFlags::empty(),
            top_margin: 0,
            bottom_margin: rows - 1,
            rgb: Vec::new(),
            palette: default_palette(),
            default_fg: 0xffffff,
            default_bg: 0x000000,
            title: String::new(),
            progress: Progress::default(),
            alt_cells: vec![Cell::default(); (cols as usize) * (rows as usize)],
            alt_scroll_top: 0,
            alt_cursor_row: 0,
            alt_cursor_col: 0,
            alt_saved_row: 0,
            alt_saved_col: 0,
            alt_sgr: SgrFlags::DEFAULT,
            alt_fg: DEFAULT_FG_ID,
            alt_bg: DEFAULT_BG_ID,
            alt_protect: false,
            alt_flags: BufferFlags::empty(),
            alt_top_margin: 0,
            alt_bottom_margin: rows - 1,
            alt_continuation: vec![false; rows as usize],
            scrollback: VecDeque::new(),
            max_scrollback: 1000,
            tab_stops: default_tabs(cols),
            continuation: vec![false; rows as usize],
        }
    }

    /// Home the cursor, clear SGR state, and reset the scroll margins (the
    /// per-buffer part shared by `reset`, DECALN, and alt-screen entry).
    fn reset_cursor_style_margins(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.sgr = SgrFlags::DEFAULT;
        self.fg = DEFAULT_FG_ID;
        self.bg = DEFAULT_BG_ID;
        self.flags.remove(BufferFlags::WRAP_PENDING);
        self.top_margin = 0;
        self.bottom_margin = self.rows - 1;
    }

    /// Reset to a blank grid, default styles, margins and cursor (RIS).
    pub fn reset(&mut self) {
        self.cells.fill(Cell::default());
        self.scroll_top = 0;
        self.reset_cursor_style_margins();
        self.saved_row = 0;
        self.saved_col = 0;
        self.modes = ScreenFlags::DEFAULT;
        self.protect = false;
        self.flags.remove(BufferFlags::ORIGIN);
        self.rgb.clear();
        self.title.clear();
        self.progress = Progress::default();
        // Reset the alternate buffer too.
        self.alt_cells.fill(Cell::default());
        self.alt_scroll_top = 0;
        self.alt_cursor_row = 0;
        self.alt_cursor_col = 0;
        self.alt_saved_row = 0;
        self.alt_saved_col = 0;
        self.alt_sgr = SgrFlags::DEFAULT;
        self.alt_fg = DEFAULT_FG_ID;
        self.alt_bg = DEFAULT_BG_ID;
        self.alt_protect = false;
        self.alt_flags = BufferFlags::empty();
        self.alt_top_margin = 0;
        self.alt_bottom_margin = self.rows - 1;
        self.scrollback.clear();
        self.tab_stops = default_tabs(self.cols);
        self.continuation.fill(false);
        self.alt_continuation.fill(false);
    }

    /// DECSTR (`CSI ! p`): soft terminal reset. Resets cursor, styles,
    /// margins, modes, and tab stops while keeping the screen contents and
    /// scrollback.
    pub(crate) fn soft_reset(&mut self) {
        self.reset_cursor_style_margins();
        self.flags.remove(BufferFlags::ORIGIN);
        self.protect = false;
        // DECAWM on, DECTCEM on, IRM off. `ALT_ACTIVE` is untouched: DECSTR
        // resets modes but does not leave the alternate screen.
        self.modes.set(ScreenFlags::WRAP, true);
        self.modes.set(ScreenFlags::CURSOR_VISIBLE, true);
        self.modes.set(ScreenFlags::INSERT, false);
        self.tab_stops = default_tabs(self.cols);
    }

    /// Handle an `OSC 9;4` progress-bar command, ignoring other `OSC 9`
    /// sub-commands (notification, etc.).
    fn osc_progress(&mut self, params: &[&[u8]]) {
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

    /// Enter the alternate screen buffer, swapping the active and saved
    /// per-buffer state. `clear` blanks the (newly active) alt grid, homes its
    /// cursor, and resets its margins and styles (DEC 1047/1049); DEC 47
    /// passes `false` to keep the previous alt content.
    fn enter_alt(&mut self, clear: bool) {
        if self.modes.contains(ScreenFlags::ALT_ACTIVE) {
            return;
        }
        self.modes.insert(ScreenFlags::ALT_ACTIVE);
        self.swap_buffers();
        if clear {
            self.cells.fill(Cell::default());
            self.reset_cursor_style_margins();
            self.flags.remove(BufferFlags::ORIGIN);
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
        core::mem::swap(&mut self.cells, &mut self.alt_cells);
        core::mem::swap(&mut self.cursor_row, &mut self.alt_cursor_row);
        core::mem::swap(&mut self.cursor_col, &mut self.alt_cursor_col);
        core::mem::swap(&mut self.saved_row, &mut self.alt_saved_row);
        core::mem::swap(&mut self.saved_col, &mut self.alt_saved_col);
        core::mem::swap(&mut self.sgr, &mut self.alt_sgr);
        core::mem::swap(&mut self.fg, &mut self.alt_fg);
        core::mem::swap(&mut self.bg, &mut self.alt_bg);
        core::mem::swap(&mut self.protect, &mut self.alt_protect);
        core::mem::swap(&mut self.flags, &mut self.alt_flags);
        core::mem::swap(&mut self.top_margin, &mut self.alt_top_margin);
        core::mem::swap(&mut self.bottom_margin, &mut self.alt_bottom_margin);
        core::mem::swap(&mut self.continuation, &mut self.alt_continuation);
        core::mem::swap(&mut self.scroll_top, &mut self.alt_scroll_top);
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

    /// Cursor position `(row, col)`, 0-based.
    #[inline]
    pub const fn cursor(&self) -> (u32, u32) {
        (self.cursor_row, self.cursor_col)
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
        Some(&self.cells[self.row_offset(row) + col as usize])
    }

    /// Whether column `col` has a tab stop.
    #[inline]
    pub fn tab_stop_at(&self, col: u32) -> bool {
        col < self.cols && self.tab_stops[col as usize]
    }

    /// Assert the screen's structural invariants: cursor, saved cursor and
    /// margins in bounds, ring-buffer offsets valid, `WRAP_PENDING` consistent
    /// with the cursor column, and every per-buffer vector sized to the grid.
    /// Tests and fuzzing call this after each mutation to catch drift in the
    /// implicit state machines (autowrap, origin, alt-screen). Panics on the
    /// first violation; production code never calls it.
    pub fn check_invariants(&self) {
        assert!(
            self.cursor_row < self.rows,
            "cursor_row {} out of bounds",
            self.cursor_row
        );
        assert!(
            self.cursor_col < self.cols,
            "cursor_col {} out of bounds",
            self.cursor_col
        );
        assert!(
            self.saved_row < self.rows,
            "saved_row {} out of bounds",
            self.saved_row
        );
        assert!(
            self.saved_col < self.cols,
            "saved_col {} out of bounds",
            self.saved_col
        );
        assert!(
            self.top_margin <= self.bottom_margin,
            "margins inverted: top {} bottom {}",
            self.top_margin,
            self.bottom_margin
        );
        assert!(
            self.bottom_margin < self.rows,
            "bottom_margin {} out of bounds",
            self.bottom_margin
        );
        assert!(
            self.scroll_top < self.rows,
            "scroll_top {} out of bounds",
            self.scroll_top
        );
        assert!(
            !self.flags.contains(BufferFlags::WRAP_PENDING) || self.cursor_col == self.cols - 1,
            "WRAP_PENDING with cursor_col {} (cols {})",
            self.cursor_col,
            self.cols
        );
        assert_eq!(
            self.continuation.len(),
            self.rows as usize,
            "continuation length"
        );
        assert_eq!(
            self.cells.len(),
            (self.cols as usize) * (self.rows as usize),
            "cells length"
        );
        assert_eq!(self.tab_stops.len(), self.cols as usize, "tab_stops length");

        assert!(
            self.alt_cursor_row < self.rows,
            "alt_cursor_row {} out of bounds",
            self.alt_cursor_row
        );
        assert!(
            self.alt_cursor_col < self.cols,
            "alt_cursor_col {} out of bounds",
            self.alt_cursor_col
        );
        assert!(
            self.alt_saved_row < self.rows,
            "alt_saved_row {} out of bounds",
            self.alt_saved_row
        );
        assert!(
            self.alt_saved_col < self.cols,
            "alt_saved_col {} out of bounds",
            self.alt_saved_col
        );
        assert!(
            self.alt_top_margin <= self.alt_bottom_margin,
            "alt margins inverted: top {} bottom {}",
            self.alt_top_margin,
            self.alt_bottom_margin
        );
        assert!(
            self.alt_bottom_margin < self.rows,
            "alt_bottom_margin {} out of bounds",
            self.alt_bottom_margin
        );
        assert!(
            self.alt_scroll_top < self.rows,
            "alt_scroll_top {} out of bounds",
            self.alt_scroll_top
        );
        assert!(
            !self.alt_flags.contains(BufferFlags::WRAP_PENDING)
                || self.alt_cursor_col == self.cols - 1,
            "alt WRAP_PENDING with alt_cursor_col {} (cols {})",
            self.alt_cursor_col,
            self.cols
        );
        assert_eq!(
            self.alt_continuation.len(),
            self.rows as usize,
            "alt_continuation length"
        );
        assert_eq!(
            self.alt_cells.len(),
            (self.cols as usize) * (self.rows as usize),
            "alt_cells length"
        );

        self.check_cell_continuations(&self.cells);
        self.check_cell_continuations(&self.alt_cells);
    }

    /// Assert that no continuation cell is orphaned: every double-width
    /// continuation immediately follows a non-continuation lead cell. (The
    /// lead's single-char width can be 1 for a grapheme cluster whose total
    /// width is 2, so it is not checked here.)
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

    /// Resolve a [`ColorId`] to packed `0xRRGGBB`. The default fg/bg ids use
    /// the configured default colors; the 16 ANSI colors use the configured
    /// palette; `16..=255` use the xterm table; truecolor ids (`>= 258`) look
    /// up the screen's dynamic palette.
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
    /// the ring-buffer scroll offset.
    #[inline]
    pub(crate) const fn row_offset(&self, row: u32) -> usize {
        let phys = (self.scroll_top as usize + row as usize) % self.rows as usize;
        phys * self.cols as usize
    }

    /// The physical row index of logical `row`.
    #[inline]
    pub(crate) const fn phys_row(&self, row: u32) -> usize {
        (self.scroll_top as usize + row as usize) % self.rows as usize
    }

    /// Rotate the active grid so logical row `0` sits at physical offset `0`.
    /// Region operations memmove contiguous physical ranges, so they call this
    /// first; full-screen scrolls rotate the ring in place instead.
    pub(crate) fn materialize(&mut self) {
        if self.scroll_top == 0 {
            return;
        }
        self.cells
            .rotate_left(self.scroll_top as usize * self.cols as usize);
        self.continuation.rotate_left(self.scroll_top as usize);
        self.scroll_top = 0;
    }

    /// [`Self::materialize`] for the alternate screen buffer.
    pub(crate) fn materialize_alt(&mut self) {
        if self.alt_scroll_top == 0 {
            return;
        }
        self.alt_cells
            .rotate_left(self.alt_scroll_top as usize * self.cols as usize);
        self.alt_continuation
            .rotate_left(self.alt_scroll_top as usize);
        self.alt_scroll_top = 0;
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
        Some(render_cells(&self.cells[self.row_range(row)]))
    }

    /// Render every row to text, one `String` per row.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        (0..self.rows)
            .map(|r| self.render_line(r).unwrap_or_default())
            .collect()
    }

    /// Render every row to ANSI-styled text (for snapshots/tests): each row
    /// `\r\n`-terminated, cells carrying their SGR attributes. Trailing blank
    /// cells are trimmed; each line ends with an SGR reset.
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
        let cells = &self.cells[self.row_range(row)];
        // Trim trailing blank cells.
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
        self.max_scrollback
    }

    /// Set the scrollback capacity, truncating the oldest lines if shrinking.
    /// `0` disables and clears.
    pub fn set_max_scrollback(&mut self, n: usize) {
        self.max_scrollback = n;
        if n == 0 {
            self.scrollback.clear();
        } else {
            while self.scrollback.len() > n {
                self.scrollback.pop_front();
            }
        }
    }

    /// Resize to `cols × rows`, reflowing soft-wrapped lines (in the screen
    /// and its scrollback) at the new width. The cursor is reset to home.
    /// Zero dimensions clamp to 1.
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
        let active = rows_from(&self.cells, &self.continuation, self.cols);
        let mut all_rows = Vec::with_capacity(self.scrollback.len() + active.len());
        all_rows.extend(self.scrollback.iter().cloned());
        all_rows.extend(active);
        let reflowed = reflow_rows(&all_rows, new_cols);

        // The last `rows` rows become the visible screen; the rest return to
        // scrollback (capped at `max_scrollback`).
        let take = reflowed.len().min(new_rows);
        let skip = reflowed.len() - take;
        let keep = skip.min(self.max_scrollback);
        let mut new_scrollback = VecDeque::new();
        for row in &reflowed[skip - keep..skip] {
            new_scrollback.push_back(row.clone());
        }
        let (new_cells, new_cont) = fit_grid(&reflowed, new_cols, new_rows);

        // Reflow the alternate buffer (it has no scrollback of its own).
        let alt = rows_from(&self.alt_cells, &self.alt_continuation, self.cols);
        let alt_reflowed = reflow_rows(&alt, new_cols);
        let (new_alt_cells, new_alt_cont) = fit_grid(&alt_reflowed, new_cols, new_rows);

        self.cols = cols;
        self.rows = rows;
        self.cells = new_cells;
        self.continuation = new_cont;
        self.alt_cells = new_alt_cells;
        self.alt_continuation = new_alt_cont;
        self.scrollback = new_scrollback;
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.saved_row = 0;
        self.saved_col = 0;
        self.flags.remove(BufferFlags::WRAP_PENDING);
        self.top_margin = 0;
        self.bottom_margin = rows - 1;
        self.alt_top_margin = 0;
        self.alt_bottom_margin = rows - 1;
        self.tab_stops = default_tabs(cols);
    }

    /// The raw cells of scrollback line `i` (`0` = oldest), or `None`.
    #[inline]
    pub fn scrollback_line(&self, i: usize) -> Option<&[Cell]> {
        self.scrollback.get(i).map(|(row, _)| row.as_slice())
    }

    /// Render scrollback line `i` to text, or `None` out of range.
    #[must_use]
    pub fn scrollback_text(&self, i: usize) -> Option<String> {
        self.scrollback.get(i).map(|(row, _)| render_cells(row))
    }

    /// Attach a zero-width glyph (combining mark, variation selector, skin
    /// tone) to the base cell immediately left of the cursor, skipping a
    /// double-width continuation cell. Marks beyond four are dropped; a
    /// leading mark with no base cell is dropped.
    fn attach_mark(&mut self, c: char) {
        if self.cursor_col == 0 {
            return;
        }
        let mut col = self.cursor_col - 1;
        let base = self.row_offset(self.cursor_row);
        let idx = |col: u32| base + col as usize;
        if self.cells[idx(col)].is_continuation() {
            col = col.saturating_sub(1);
        }
        let cell = &mut self.cells[idx(col)];
        if cell.ch == ' ' || cell.is_continuation() {
            return;
        }
        if (cell.combining_len as usize) < 8 {
            cell.combining[cell.combining_len as usize] = c;
            cell.combining_len += 1;
        }
    }

    fn csi(&mut self, params: &[i32], private: bool, f: u8) {
        let n = |i: usize| params.get(i).copied().unwrap_or(1).max(1) as u32;
        if private {
            // DEC private sequences. `CSI ? r` is DECPCTERM, not a DECSTBM
            // form; the screen does not implement it, so it is ignored.
            match f {
                b'h' => self.set_mode(params),   // DECSET
                b'l' => self.reset_mode(params), // DECRST
                b'J' => self.erase_in_display(params.first().copied().unwrap_or(0), true), // DECSED
                b'K' => self.erase_in_line(params.first().copied().unwrap_or(0), true), // DECSEL
                _ => {}
            }
            return;
        }
        match f {
            b'A' => self.cursor_up(n(0)),      // CUU
            b'B' => self.cursor_down(n(0)),    // CUD
            b'C' => self.cursor_forward(n(0)), // CUF
            b'D' => self.cursor_back(n(0)),    // CUB
            b'E' => {
                // CNL
                self.cursor_down(n(0));
                self.cursor_col = 0;
            }
            b'F' => {
                // CPL
                self.cursor_up(n(0));
                self.cursor_col = 0;
            }
            b'G' => self.cha(n(0)),              // CHA
            b'd' => self.vpa(n(0)),              // VPA
            b'H' | b'f' => self.cup(n(0), n(1)), // CUP / HVP
            b'J' => self.erase_in_display(params.first().copied().unwrap_or(0), false), // ED
            b'K' => self.erase_in_line(params.first().copied().unwrap_or(0), false), // EL
            b'L' => self.insert_lines(n(0)),     // IL
            b'M' => self.delete_lines(n(0)),     // DL
            b'P' => self.delete_chars(n(0)),     // DCH
            b'@' => self.insert_chars(n(0)),     // ICH
            b'X' => self.erase_chars(n(0)),      // ECH
            b'S' => self.scroll_n(false, n(0)),  // SU
            b'T' => self.scroll_n(true, n(0)),   // SD
            b'h' => self.apply_ansi_modes(params, true), // SM
            b'l' => self.apply_ansi_modes(params, false), // RM
            b'm' => self.sgr(params),            // SGR
            b'g' => self.tbc(params),            // TBC
            b'r' => self.set_margins(params),    // DECSTBM
            _ => {}
        }
    }
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn print_bytes(&mut self, s: &[u8]) {
        self.put_ascii(s);
    }

    fn print_codepoints(&mut self, codepoints: &[u32]) {
        // Segment the run into grapheme clusters so ZWJ emoji families write
        // as one cell instead of separate glyphs.
        let text: String = codepoints
            .iter()
            .filter_map(|&cp| char::from_u32(cp))
            .collect();
        for grapheme in vtty_grapheme::graphemes(&text) {
            self.put_cluster(grapheme.as_str(&text));
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => self.linefeed(), // LF / VT / FF
            b'\r' => {
                // CR
                self.clear_wrap();
                self.cursor_col = 0;
            }
            b'\t' => {
                // HT
                self.clear_wrap();
                self.cursor_col = self.next_tab_stop();
            }
            0x08 => {
                // BS
                self.clear_wrap();
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &[i32], _sep: u16, intermediates: &[u8], f: u8) {
        // DECSTR (`CSI ! p`): soft reset. The `!` intermediate distinguishes
        // it from the DEC private (`?`) sequences handled by `csi`.
        if intermediates.first() == Some(&b'!') && f == b'p' {
            self.soft_reset();
            return;
        }
        // DECSCA (`CSI Ps " q`): protect attribute for subsequent cells.
        if intermediates.first() == Some(&b'"') && f == b'q' {
            self.decsca(params);
            return;
        }
        // VT400 rectangular-area operations (`CSI … $ x/v/z/{/r/t`).
        if intermediates.first() == Some(&b'$') {
            match f {
                b'x' => self.decfra(params),  // DECFRA
                b'v' => self.deccra(params),  // DECCRA
                b'z' => self.decera(params),  // DECERA
                b'{' => self.decsera(params), // DECSERA
                b'r' => self.deccara(params), // DECCARA
                b't' => self.decrara(params), // DECRARA
                _ => {}
            }
            return;
        }
        let private = intermediates.first() == Some(&b'?');
        self.csi(params, private, f);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        // DECALN is the only escape sequence the screen acts on that carries
        // an intermediate byte; everything else uses a bare final byte.
        if matches!(intermediates, [b'#']) && byte == b'8' {
            self.decaln();
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.save_cursor(),    // DECSC
            b'8' => self.restore_cursor(), // DECRC
            b'D' => self.linefeed(),       // IND
            b'H' => self.hts(),            // HTS
            b'M' => self.reverse_index(),  // RI
            b'E' => {
                self.cursor_col = 0; // NEL
                self.linefeed();
            }
            b'c' => self.reset(), // RIS
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        match params.first().copied() {
            Some(b"0") | Some(b"1") | Some(b"2") => {
                if let Some(t) = params.get(1) {
                    self.title = String::from_utf8_lossy(t).into_owned();
                }
            }
            Some(b"9") => self.osc_progress(params),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use vtty::Parser;

    fn run(screen: &mut Screen, bytes: &[u8]) {
        Parser::new().advance(screen, bytes);
    }

    #[test]
    fn print_and_cursor() {
        let mut s = Screen::new(10, 3);
        run(&mut s, b"hi");
        assert_eq!(s.cursor(), (0, 2));
        assert_eq!(s.render_line(0).as_deref(), Some("hi"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
    }

    #[test]
    fn autowrap_at_last_column() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"abc");
        assert_eq!(s.cursor(), (0, 2));
        run(&mut s, b"d");
        // `d` wraps to the next line.
        assert_eq!(s.cursor(), (1, 1));
        assert_eq!(s.render_line(0).as_deref(), Some("abc"));
        assert_eq!(s.render_line(1).as_deref(), Some("d"));
    }

    #[test]
    fn double_width_glyph() {
        let mut s = Screen::new(5, 2);
        run(&mut s, "a中b".as_bytes());
        assert_eq!(s.render_line(0).as_deref(), Some("a中b"));
        assert_eq!(s.cell(0, 2).map(Cell::is_continuation), Some(true));
    }

    #[test]
    fn combining_mark_attaches() {
        let mut s = Screen::new(4, 1);
        run(&mut s, "e\u{301}x".as_bytes());
        assert_eq!(s.render_line(0).as_deref(), Some("e\u{301}x"));
        assert_eq!(s.cell(0, 0).map(|c| c.marks()), Some(&['\u{301}'][..]));
    }

    #[test]
    fn skin_tone_attaches_to_wide() {
        let mut s = Screen::new(4, 1);
        run(&mut s, "👋\u{1F3FF}".as_bytes());
        assert_eq!(s.render_line(0).as_deref(), Some("👋\u{1F3FF}"));
        assert_eq!(s.cell(0, 0).map(|c| c.marks()), Some(&['\u{1F3FF}'][..]));
        assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
    }

    #[test]
    fn zwj_emoji_family_is_one_cell() {
        let mut s = Screen::new(8, 1);
        run(&mut s, "👨\u{200D}👩\u{200D}👧x".as_bytes());
        // The whole family occupies two cells (one glyph); `x` lands after.
        assert_eq!(s.render_line(0).as_deref(), Some("👨\u{200D}👩\u{200D}👧x"));
        assert_eq!(s.cell(0, 0).map(|c| c.ch), Some('👨'));
        assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
        assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('x'));
    }

    #[test]
    fn render_ansi_bold_red() {
        let mut s = Screen::new(8, 1);
        run(&mut s, b"\x1b[1;31mhi");
        assert_eq!(s.render_ansi_line(0), "\x1b[0;1;38;5;1mhi\x1b[0m");
    }

    #[test]
    fn render_ansi_truecolor() {
        let mut s = Screen::new(8, 1);
        run(&mut s, b"\x1b[38;2;255;0;128mX");
        assert_eq!(s.render_ansi_line(0), "\x1b[0;38;2;255;0;128mX\x1b[0m");
    }

    #[test]
    fn render_ansi_style_change() {
        let mut s = Screen::new(8, 1);
        run(&mut s, b"\x1b[31mR\x1b[32mG");
        assert_eq!(
            s.render_ansi_line(0),
            "\x1b[0;38;5;1mR\x1b[0;38;5;2mG\x1b[0m"
        );
    }

    #[test]
    fn render_ansi_trims_trailing_blank() {
        let mut s = Screen::new(8, 1);
        run(&mut s, b"hi");
        // Plain text needs no escape sequences and no trailing blanks.
        assert_eq!(s.render_ansi_line(0), "hi");
    }

    #[test]
    fn lf_cr_tab_bs() {
        let mut s = Screen::new(16, 3);
        run(&mut s, b"ab\ncd");
        // LF keeps the column, so `cd` lands on row 1 at columns 2..4.
        assert_eq!(s.cursor(), (1, 4));
        run(&mut s, b"\r");
        assert_eq!(s.cursor(), (1, 0));
        run(&mut s, b"\t");
        assert_eq!(s.cursor(), (1, 8));
        run(&mut s, b"\x08");
        assert_eq!(s.cursor(), (1, 7));
    }

    #[test]
    fn cursor_movement() {
        let mut s = Screen::new(10, 5);
        run(&mut s, b"\x1b[3;4H");
        assert_eq!(s.cursor(), (2, 3));
        run(&mut s, b"\x1b[A");
        assert_eq!(s.cursor(), (1, 3));
        run(&mut s, b"\x1b[2C");
        assert_eq!(s.cursor(), (1, 5));
        run(&mut s, b"\x1b[2D");
        assert_eq!(s.cursor(), (1, 3));
    }

    #[test]
    fn erase_display_and_line() {
        let mut s = Screen::new(5, 3);
        run(&mut s, b"hello\r\nworld");
        run(&mut s, b"\x1b[2J");
        assert_eq!(s.lines(), vec!["", "", ""]);
    }

    #[test]
    fn sgr_flags_and_colors() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[1;31mX");
        assert_eq!(
            s.cell(0, 0).map(|c| c.sgr.contains(SgrFlags::BOLD)),
            Some(true)
        );
        assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(1)));
        run(&mut s, b"\x1b[0m");
        run(&mut s, b"Y");
        assert_eq!(s.cell(0, 1).map(|c| c.sgr), Some(SgrFlags::DEFAULT));
        assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(DEFAULT_FG_ID));
    }

    #[test]
    fn truecolor_sgr() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[38;2;255;0;128mX");
        assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(258)));
        assert_eq!(s.rgb, vec![vtty_core::rgb(255, 0, 128)]);
    }

    #[test]
    fn color_rgb_resolves_palette() {
        let mut s = Screen::new(4, 1);
        // Default fg/bg resolve to the configured defaults.
        assert_eq!(s.color_rgb(DEFAULT_FG_ID), Some(0xffffff));
        assert_eq!(s.color_rgb(DEFAULT_BG_ID), Some(0x000000));
        // Indexed colors: 0..=15 use the palette, 16..=255 the xterm table.
        assert_eq!(s.color_rgb(ColorId(1)), Some(0xcd0000));
        assert_eq!(s.color_rgb(ColorId(196)), Some(0xff0000));
        // Truecolor ids resolve through the dynamic palette.
        run(&mut s, b"\x1b[38;2;0;128;255mX");
        assert_eq!(s.color_rgb(ColorId(258)), Some(0x0080ff));
    }

    #[test]
    fn set_palette_changes_colors() {
        let mut s = Screen::new(4, 1);
        s.set_palette([0x123456; 16]);
        s.set_default_colors(0xf0f0f0, 0x101010);
        assert_eq!(s.color_rgb(ColorId(1)), Some(0x123456));
        assert_eq!(s.color_rgb(DEFAULT_FG_ID), Some(0xf0f0f0));
        assert_eq!(s.color_rgb(DEFAULT_BG_ID), Some(0x101010));
    }

    #[test]
    fn decstbm_scroll_region() {
        let mut s = Screen::new(3, 5);
        // Margins rows 2-4 (1-based) = 1..=3 (0-based).
        run(&mut s, b"\x1b[2;4r");
        // Setting margins homes the cursor to the top margin.
        assert_eq!(s.cursor(), (1, 0));
        // Four lines scroll the region (rows 1-3); `a` scrolls out.
        // CR before each LF so each line starts at column 0.
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        // Rows above and below the region are untouched.
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        assert_eq!(s.render_line(4).as_deref(), Some(""));
        assert_eq!(s.render_line(1).as_deref(), Some("b"));
        assert_eq!(s.render_line(2).as_deref(), Some("c"));
        assert_eq!(s.render_line(3).as_deref(), Some("d"));
    }

    #[test]
    fn save_restore_cursor() {
        let mut s = Screen::new(8, 3);
        run(&mut s, b"\x1b[2;3H");
        run(&mut s, b"\x1b7"); // DECSC
        run(&mut s, b"\x1b[1;1H");
        run(&mut s, b"\x1b8"); // DECRC
        assert_eq!(s.cursor(), (1, 2));
    }

    #[test]
    fn decaln_fills_e() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"abc\x1b#8");
        assert_eq!(s.render_line(0).as_deref(), Some("EEE"));
        assert_eq!(s.render_line(1).as_deref(), Some("EEE"));
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn decstr_preserves_content_and_homes_cursor() {
        let mut s = Screen::new(4, 2);
        run(&mut s, b"ab\x1b[2;3H"); // content, cursor to (1, 2)
        run(&mut s, b"\x1b[!p");
        assert_eq!(s.render_line(0).as_deref(), Some("ab"));
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn decstr_resets_style() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[1;31m"); // bold red active
        run(&mut s, b"\x1b[!p");
        run(&mut s, b"X");
        assert_eq!(s.render_ansi_line(0), "X"); // default, no SGR escapes
    }

    #[test]
    fn decstr_clears_insert_mode() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"12\x1b[1G\x1b[4h"); // IRM on at column 0
        run(&mut s, b"\x1b[!p");
        run(&mut s, b"AB"); // replace, not shift
        assert_eq!(s.render_line(0).as_deref(), Some("AB"));
    }

    #[test]
    fn selective_erase_in_line_skips_protected() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde\x1b[1;3H\x1b[1\"qc"); // protect "c"
        run(&mut s, b"\x1b[?2K"); // DECSEL 2: erase whole line, keep protected
        assert_eq!(s.render_line(0).as_deref(), Some("  c"));
        assert_eq!(s.cell(0, 2).map(|c| c.protected), Some(true));
        assert_eq!(s.cell(0, 0).map(|c| c.ch), Some(' '));
    }

    #[test]
    fn selective_erase_in_display_skips_protected() {
        let mut s = Screen::new(4, 2);
        run(&mut s, b"abcd\r\nEFGH");
        run(&mut s, b"\x1b[1;2H\x1b[1\"qbc"); // protect "bc" on row 0
        run(&mut s, b"\x1b[?2J"); // DECSED 2: erase display, keep protected
        assert_eq!(s.render_line(0).as_deref(), Some(" bc"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
    }

    #[test]
    fn plain_erase_ignores_protection() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde\x1b[1;3H\x1b[1\"qc"); // protect "c"
        run(&mut s, b"\x1b[2J"); // ED 2 (non-selective) erases everything
        assert_eq!(s.render_line(0).as_deref(), Some(""));
    }

    #[test]
    fn decsca_zero_clears_protection() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde\x1b[1;3H\x1b[1\"qc"); // protect "c"
        run(&mut s, b"\x1b[1;4H\x1b[0\"qd"); // DECSCA 0: "d" is erasable
        run(&mut s, b"\x1b[?2K"); // selective erase line
        assert_eq!(s.render_line(0).as_deref(), Some("  c"));
    }

    #[test]
    fn reverse_video_swaps_default_colors() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"X\x1b[?5h"); // DECSCNM on
        // Default fg/bg swap to explicit black-on-white.
        assert_eq!(
            s.render_ansi_line(0),
            "\x1b[0;38;2;0;0;0;48;2;255;255;255mX\x1b[0m"
        );
    }

    #[test]
    fn reverse_video_swaps_fg_bg() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[31mX"); // red foreground
        run(&mut s, b"\x1b[?5h"); // DECSCNM on
        // Red fg becomes red bg; the default bg becomes the foreground.
        assert_eq!(
            s.render_ansi_line(0),
            "\x1b[0;38;2;0;0;0;48;2;205;0;0mX\x1b[0m"
        );
    }

    #[test]
    fn reverse_video_off_restores_render() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"X\x1b[?5h\x1b[?5l"); // DECSCNM on, then off
        assert_eq!(s.render_ansi_line(0), "X");
    }

    #[test]
    fn decfra_fills_rectangle() {
        let mut s = Screen::new(4, 3);
        run(&mut s, b"\x1b[65;2;3;1;2$x"); // fill cols 2-3, rows 1-2 with 'A'
        assert_eq!(s.render_line(0).as_deref(), Some(" AA"));
        assert_eq!(s.render_line(1).as_deref(), Some(" AA"));
        assert_eq!(s.render_line(2).as_deref(), Some(""));
    }

    #[test]
    fn decera_erases_rectangle() {
        let mut s = Screen::new(4, 2);
        run(&mut s, b"abcdefgh");
        run(&mut s, b"\x1b[2;3;1;2$z"); // erase cols 2-3, rows 1-2
        assert_eq!(s.render_line(0).as_deref(), Some("a  d"));
        assert_eq!(s.render_line(1).as_deref(), Some("e  h"));
    }

    #[test]
    fn decsera_skips_protected() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"abcdef");
        run(&mut s, b"\x1b[1;2H\x1b[1\"qb"); // protect "b"
        run(&mut s, b"\x1b[1;3;1;2${"); // DECSERA whole screen
        assert_eq!(s.render_line(0).as_deref(), Some(" b"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
    }

    #[test]
    fn deccra_copies_rectangle() {
        let mut s = Screen::new(4, 2);
        run(&mut s, b"abcd");
        // Copy row 1 cols 1-2 ("ab") to row 2 col 3.
        run(&mut s, b"\x1b[2;1;2;1;1;3$v");
        assert_eq!(s.render_line(0).as_deref(), Some("abcd"));
        assert_eq!(s.render_line(1).as_deref(), Some("  ab"));
    }

    #[test]
    fn deccara_applies_attrs() {
        let mut s = Screen::new(3, 1);
        run(&mut s, b"abc");
        // Bold + red on cols 2-3 of row 1 (cells 'b', 'c').
        run(&mut s, b"\x1b[1;2;1;3;1;31$r");
        assert_eq!(s.render_ansi_line(0), "a\x1b[0;1;38;5;1mbc\x1b[0m");
    }

    #[test]
    fn decrara_toggles_attrs() {
        let mut s = Screen::new(2, 1);
        run(&mut s, b"\x1b[1mab"); // "ab" bold
        // Reverse bold on col 1 (cell 'a').
        run(&mut s, b"\x1b[1;1;1;1;1$t");
        assert_eq!(s.render_ansi_line(0), "a\x1b[0;1mb\x1b[0m");
    }

    #[test]
    fn insert_and_delete_lines() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"aaa\r\nbbb\r\nccc");
        run(&mut s, b"\x1b[2;1H\x1b[L"); // insert line at row 2 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("aaa"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
        assert_eq!(s.render_line(2).as_deref(), Some("bbb"));
    }

    #[test]
    fn delete_lines_shifts_up_and_blanks_bottom() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"aaa\r\nbbb\r\nccc");
        run(&mut s, b"\x1b[1;1H\x1b[M"); // DL at row 1 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("bbb"));
        assert_eq!(s.render_line(1).as_deref(), Some("ccc"));
        assert_eq!(s.render_line(2).as_deref(), Some(""));
    }

    #[test]
    fn insert_chars_opens_a_gap() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde");
        run(&mut s, b"\x1b[1;2H\x1b[2@"); // ICH 2 at column 2 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("a  bc"));
        assert_eq!(s.cell(0, 1).map(|c| c.ch), Some(' '));
        assert_eq!(s.cell(0, 3).map(|c| c.ch), Some('b'));
        assert_eq!(s.cell(0, 4).map(|c| c.ch), Some('c'));
    }

    #[test]
    fn delete_chars_closes_the_gap() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde");
        run(&mut s, b"\x1b[1;2H\x1b[2P"); // DCH 2 at column 2 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("ade"));
        assert_eq!(s.cell(0, 1).map(|c| c.ch), Some('d'));
        assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('e'));
        assert_eq!(s.cell(0, 3).map(|c| c.ch), Some(' '));
    }

    #[test]
    fn insert_mode_shifts_row_right() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"12");
        run(&mut s, b"\x1b[1G\x1b[4h"); // IRM on at column 1
        run(&mut s, b"AB");
        assert_eq!(s.render_line(0).as_deref(), Some("AB12"));
    }

    #[test]
    fn insert_mode_reset_restores_replace() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"12\x1b[1G\x1b[4hAB");
        assert_eq!(s.render_line(0).as_deref(), Some("AB12"));
        // Reset IRM: writing replaces instead of shifting.
        run(&mut s, b"\x1b[1G\x1b[4lXY");
        assert_eq!(s.render_line(0).as_deref(), Some("XY12"));
    }

    #[test]
    fn insert_mode_at_last_column_drops_tail() {
        let mut s = Screen::new(3, 1);
        run(&mut s, b"abc");
        run(&mut s, b"\x1b[3G\x1b[4hX"); // IRM on at the last column
        assert_eq!(s.render_line(0).as_deref(), Some("abX"));
        assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('X'));
    }

    #[test]
    fn insert_mode_wide_glyph_shifts_two_cells() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"ab");
        run(&mut s, b"\x1b[1G\x1b[4h"); // IRM on at column 1
        run(&mut s, "中".as_bytes()); // width 2
        assert_eq!(s.render_line(0).as_deref(), Some("中ab"));
        assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
    }

    #[test]
    fn line_edit_ignores_cursor_outside_margins() {
        let mut s = Screen::new(3, 4);
        run(&mut s, b"aaa\r\nbbb\r\nccc\r\nddd");
        run(&mut s, b"\x1b[2;3r"); // margins rows 2-3 (1-based)
        // Cursor above the top margin: IL must not move anything.
        run(&mut s, b"\x1b[1;1H\x1b[L");
        assert_eq!(s.lines(), vec!["aaa", "bbb", "ccc", "ddd"]);
        // Cursor below the bottom margin: DL must not move anything.
        run(&mut s, b"\x1b[4;1H\x1b[M");
        assert_eq!(s.lines(), vec!["aaa", "bbb", "ccc", "ddd"]);
    }

    #[test]
    fn erase_in_display_from_cursor_to_end() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"abc\r\ndef\r\nghi");
        run(&mut s, b"\x1b[2;2H\x1b[J"); // ED 0 at row 2 col 2 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("abc"));
        assert_eq!(s.render_line(1).as_deref(), Some("d"));
        assert_eq!(s.render_line(2).as_deref(), Some(""));
    }

    #[test]
    fn erase_in_display_start_to_cursor() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"abc\r\ndef\r\nghi");
        run(&mut s, b"\x1b[2;2H\x1b[1J"); // ED 1
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        assert_eq!(s.render_line(1).as_deref(), Some("  f"));
        assert_eq!(s.render_line(2).as_deref(), Some("ghi"));
    }

    #[test]
    fn erase_in_display_whole_screen_keeps_cursor() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"abc\r\ndef\r\nghi");
        run(&mut s, b"\x1b[2;2H\x1b[2J"); // ED 2
        assert_eq!(s.lines(), vec!["", "", ""]);
        assert_eq!(s.cursor(), (1, 1));
    }

    #[test]
    fn erase_in_line_to_end() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde\x1b[1;3H\x1b[K"); // EL 0 at column 3 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("ab"));
        assert_eq!(s.cursor(), (0, 2));
    }

    #[test]
    fn erase_in_line_start_to_cursor() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde\x1b[1;3H\x1b[1K"); // EL 1
        assert_eq!(s.render_line(0).as_deref(), Some("   de"));
        assert_eq!(s.cell(0, 2).map(|c| c.ch), Some(' '));
        assert_eq!(s.cell(0, 3).map(|c| c.ch), Some('d'));
    }

    #[test]
    fn erase_in_line_whole_line() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde\x1b[1;3H\x1b[2K"); // EL 2
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        assert_eq!(s.cursor(), (0, 2));
    }

    #[test]
    fn erase_chars_clamps_to_line_end() {
        let mut s = Screen::new(5, 1);
        run(&mut s, b"abcde");
        run(&mut s, b"\x1b[1;2H\x1b[3X"); // ECH 3 at column 2 (1-based)
        assert_eq!(s.render_line(0).as_deref(), Some("a   e"));
        assert_eq!(s.cell(0, 1).map(|c| c.ch), Some(' '));
        assert_eq!(s.cell(0, 3).map(|c| c.ch), Some(' '));
        assert_eq!(s.cell(0, 4).map(|c| c.ch), Some('e'));

        // Oversized count clamps to the line end instead of overflowing.
        run(&mut s, b"\x1b[1;2H\x1b[99X");
        assert_eq!(s.render_line(0).as_deref(), Some("a"));
    }

    #[test]
    fn title() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b]0;hello\x07");
        assert_eq!(s.title(), "hello");
    }

    #[test]
    fn alternate_screen_1049() {
        let mut s = Screen::new(5, 2);
        run(&mut s, b"main");
        assert_eq!(s.render_line(0).as_deref(), Some("main"));
        let main_cursor = s.cursor();

        // Enter the alt screen: blank it and save the main cursor.
        run(&mut s, b"\x1b[?1049h");
        assert!(s.is_alt_screen());
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        run(&mut s, b"alt");
        assert_eq!(s.render_line(0).as_deref(), Some("alt"));

        // Leave: main content and cursor are restored.
        run(&mut s, b"\x1b[?1049l");
        assert!(!s.is_alt_screen());
        assert_eq!(s.render_line(0).as_deref(), Some("main"));
        assert_eq!(s.cursor(), main_cursor);
    }

    #[test]
    fn alternate_screen_47_persists() {
        let mut s = Screen::new(5, 2);
        run(&mut s, b"main");
        run(&mut s, b"\x1b[?47h");
        assert!(s.is_alt_screen());
        // 47 does not clear, but the alt buffer starts blank.
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        run(&mut s, b"alt");
        run(&mut s, b"\x1b[?47l");
        assert_eq!(s.render_line(0).as_deref(), Some("main"));
        // Re-enter: the alt content persists (no clear).
        run(&mut s, b"\x1b[?47h");
        assert_eq!(s.render_line(0).as_deref(), Some("alt"));
        run(&mut s, b"\x1b[?47l");
    }

    #[test]
    fn scrollback_captures_lines() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.scrollback_len(), 2);
        assert_eq!(s.render_line(0).as_deref(), Some("c"));
        assert_eq!(s.render_line(1).as_deref(), Some("d"));
        assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));
        assert_eq!(s.scrollback_text(1).as_deref(), Some("b"));
    }

    #[test]
    fn scrollback_cap_drops_oldest() {
        let mut s = Screen::new(3, 1);
        s.set_max_scrollback(2);
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.scrollback_len(), 2);
        assert_eq!(s.scrollback_text(0).as_deref(), Some("b"));
        assert_eq!(s.scrollback_text(1).as_deref(), Some("c"));
    }

    #[test]
    fn scrollback_disabled() {
        let mut s = Screen::new(3, 1);
        s.set_max_scrollback(0);
        run(&mut s, b"a\r\nb");
        assert_eq!(s.scrollback_len(), 0);
        assert_eq!(s.render_line(0).as_deref(), Some("b"));
    }

    #[test]
    fn scrollback_ignores_scroll_region() {
        let mut s = Screen::new(3, 3);
        // Margins rows 2-3 (1-based) = 1..=2 (0-based), not the full screen.
        run(&mut s, b"\x1b[2;3r");
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.scrollback_len(), 0);
    }

    #[test]
    fn default_tabs_every_8() {
        let mut s = Screen::new(20, 1);
        run(&mut s, b"\t");
        assert_eq!(s.cursor(), (0, 8));
        run(&mut s, b"\t");
        assert_eq!(s.cursor(), (0, 16));
    }

    #[test]
    fn hts_and_tbc() {
        let mut s = Screen::new(20, 1);
        run(&mut s, b"abc\x1bH"); // HTS: set a stop at column 3.
        assert!(s.tab_stop_at(3));
        run(&mut s, b"\r\t"); // tab from column 0 lands on 3.
        assert_eq!(s.cursor(), (0, 3));
        run(&mut s, b"\x1b[g"); // TBC 0: clear the stop at the cursor.
        assert!(!s.tab_stop_at(3));
        run(&mut s, b"\r\t"); // now skips 3 and lands on the default 8.
        assert_eq!(s.cursor(), (0, 8));
    }

    #[test]
    fn tbc_clear_all() {
        let mut s = Screen::new(20, 1);
        run(&mut s, b"\x1b[3g"); // TBC 3: clear every stop.
        assert!(!s.tab_stop_at(8));
        assert!(!s.tab_stop_at(16));
        run(&mut s, b"\t"); // no stops left → clamp to the last column.
        assert_eq!(s.cursor(), (0, 19));
    }

    #[test]
    fn resize_reflows_soft_wrap() {
        let mut s = Screen::new(5, 3);
        run(&mut s, b"helloworld");
        assert_eq!(s.lines(), vec!["hello", "world", ""]);
        // Widening re-joins the soft-wrapped logical line.
        s.resize(10, 2);
        assert_eq!(s.lines(), vec!["helloworld", ""]);
    }

    #[test]
    fn resize_preserves_hard_break() {
        let mut s = Screen::new(5, 3);
        run(&mut s, b"ab\r\ncd");
        s.resize(10, 3);
        assert_eq!(s.lines(), vec!["ab", "cd", ""]);
    }

    #[test]
    fn resize_rewraps_wide_char() {
        let mut s = Screen::new(3, 3);
        run(&mut s, "a中b".as_bytes());
        assert_eq!(s.lines(), vec!["a中", "b", ""]);
        s.resize(4, 3);
        assert_eq!(s.lines(), vec!["a中b", "", ""]);
    }

    #[test]
    fn resize_narrow_rewraps() {
        let mut s = Screen::new(6, 2);
        run(&mut s, b"foobar");
        assert_eq!(s.lines(), vec!["foobar", ""]);
        s.resize(3, 4);
        assert_eq!(s.lines(), vec!["foo", "bar", "", ""]);
    }

    #[test]
    fn resize_keeps_content_in_tall_screen() {
        // A tall viewport pads the grid with trailing blanks; shrinking must
        // keep the content, not those blanks.
        let mut s = Screen::new(20, 40);
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        s.resize(20, 24);
        assert_eq!(s.render_line(0).as_deref(), Some("a"));
        assert_eq!(s.render_line(1).as_deref(), Some("b"));
        assert_eq!(s.render_line(2).as_deref(), Some("c"));
        assert_eq!(s.render_line(3).as_deref(), Some("d"));
        assert_eq!(s.render_line(4).as_deref(), Some(""));
    }

    #[test]
    fn resize_does_not_multiply_blank_lines() {
        // One intentional blank line must stay one blank line when the width
        // shrinks (a blank row stores `cols` blank cells, which would
        // otherwise re-wrap into `ceil(old/new)` rows).
        let mut s = Screen::new(20, 4);
        run(&mut s, b"a\r\n\r\nb");
        assert_eq!(s.render_line(0).as_deref(), Some("a"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
        assert_eq!(s.render_line(2).as_deref(), Some("b"));
        s.resize(10, 4);
        assert_eq!(s.render_line(0).as_deref(), Some("a"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
        assert_eq!(s.render_line(2).as_deref(), Some("b"));
        assert_eq!(s.render_line(3).as_deref(), Some(""));
    }

    #[test]
    fn resize_preserves_wrapped_space() {
        // "hello world" wraps as "hello " + "world"; the trailing space on
        // the soft-wrapped row is content and must survive reflow.
        let mut s = Screen::new(6, 2);
        run(&mut s, b"hello world");
        assert_eq!(s.cell(0, 5).map(|c| c.ch), Some(' '));
        assert_eq!(s.render_line(1).as_deref(), Some("world"));
        s.resize(11, 2);
        assert_eq!(s.render_line(0).as_deref(), Some("hello world"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
    }

    #[test]
    fn resize_reflows_scrollback() {
        let mut s = Screen::new(5, 2);
        run(&mut s, b"aaaaa\r\nbbbbb\r\nccccc\r\nddddd");
        assert_eq!(s.scrollback_len(), 2);
        assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaaa"));
        assert_eq!(s.scrollback_text(1).as_deref(), Some("bbbbb"));
        s.resize(10, 2);
        // History survives the resize (now at the wider width).
        assert_eq!(s.scrollback_len(), 2);
        assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaaa"));
        assert_eq!(s.scrollback_text(1).as_deref(), Some("bbbbb"));
        assert_eq!(s.render_line(0).as_deref(), Some("ccccc"));
        assert_eq!(s.render_line(1).as_deref(), Some("ddddd"));
    }

    #[test]
    fn resize_rejoins_scrollback_soft_wrap() {
        let mut s = Screen::new(5, 2);
        // "helloworld" soft-wraps to two rows; a following line pushes the
        // first half into scrollback.
        run(&mut s, b"helloworld\r\nxy");
        assert_eq!(s.scrollback_text(0).as_deref(), Some("hello"));
        assert_eq!(s.render_line(0).as_deref(), Some("world"));
        assert_eq!(s.render_line(1).as_deref(), Some("xy"));
        // Widening re-joins the soft-wrapped history into the active screen.
        s.resize(10, 2);
        assert_eq!(s.scrollback_len(), 0);
        assert_eq!(s.lines(), vec!["helloworld", "xy"]);
    }

    #[test]
    fn zero_dimensions_clamp() {
        let s = Screen::new(0, 0);
        assert_eq!((s.cols(), s.rows()), (1, 1));
    }

    #[test]
    fn reverse_index_scrolls_down_at_top_margin() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"aaa\r\nbbb\r\nccc");
        run(&mut s, b"\x1b[1;1H\x1bM"); // RI at the top row
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        assert_eq!(s.render_line(1).as_deref(), Some("aaa"));
        assert_eq!(s.render_line(2).as_deref(), Some("bbb"));
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn reverse_index_mid_screen_moves_up() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"\x1b[2;1H\x1bM"); // RI at row 2, not the top
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn ind_and_nel_linefeed() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"\x1b[1;2H\x1bD"); // IND: down one line, keep column
        assert_eq!(s.cursor(), (1, 1));
        run(&mut s, b"\x1bE"); // NEL: home column, then down one line
        assert_eq!(s.cursor(), (2, 0));
    }

    #[test]
    fn origin_mode_clamps_to_margins() {
        let mut s = Screen::new(3, 5);
        run(&mut s, b"\x1b[2;4r\x1b[?6h"); // margins rows 2-4, DECOM on
        run(&mut s, b"\x1b[1;1H"); // CUP 1;1 is margin-relative
        assert_eq!(s.cursor(), (1, 0));
        run(&mut s, b"\x1b[5;1H"); // row 5 clamps to the bottom margin
        assert_eq!(s.cursor(), (3, 0));
        run(&mut s, b"\x1b[1;1H\x1b[A"); // CUU clamps to the top margin
        assert_eq!(s.cursor(), (1, 0));
        run(&mut s, b"\x1b[3;1H\x1b[B"); // CUD clamps to the bottom margin
        assert_eq!(s.cursor(), (3, 0));
        run(&mut s, b"\x1b[?6l\x1b[1;1H"); // DECOM off: CUP 1;1 is row 0
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn invalid_margins_are_ignored() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"\x1b[3;1H"); // move the cursor first
        run(&mut s, b"\x1b[3;1r"); // top >= bottom: ignored
        assert_eq!(s.cursor(), (2, 0));
        run(&mut s, b"\x1b[1;5r"); // bottom >= rows: ignored
        assert_eq!(s.cursor(), (2, 0));
    }

    #[test]
    fn set_margins_with_only_top() {
        let mut s = Screen::new(3, 5);
        run(&mut s, b"\x1b[2r"); // top row 2, bottom defaults to the last row
        assert_eq!(s.cursor(), (1, 0)); // homed to the top margin
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.lines(), vec!["", "a", "b", "c", "d"]);
    }

    #[test]
    fn cursor_movement_commands() {
        let mut s = Screen::new(5, 3);
        run(&mut s, b"\x1b[2;2H"); // CUP
        assert_eq!(s.cursor(), (1, 1));
        run(&mut s, b"\x1b[2A"); // CUU clamps to row 0
        assert_eq!(s.cursor(), (0, 1));
        run(&mut s, b"\x1b[3B"); // CUD clamps to row 2
        assert_eq!(s.cursor(), (2, 1));
        run(&mut s, b"\x1b[2C"); // CUF
        assert_eq!(s.cursor(), (2, 3));
        run(&mut s, b"\x1b[1D"); // CUB
        assert_eq!(s.cursor(), (2, 2));
        run(&mut s, b"\x1b[2G"); // CHA
        assert_eq!(s.cursor(), (2, 1));
        run(&mut s, b"\x1b[1d"); // VPA
        assert_eq!(s.cursor(), (0, 1));
    }

    #[test]
    fn tbc_unknown_mode_is_noop() {
        let mut s = Screen::new(20, 1);
        run(&mut s, b"\x1b[2g"); // unknown TBC mode: no-op
        assert!(s.tab_stop_at(8));
    }

    #[test]
    fn sgr_color_codes_set_fg_and_bg() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[31;44mX"); // red fg, blue bg
        assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(1)));
        assert_eq!(s.cell(0, 0).map(|c| c.bg), Some(ColorId(4)));
        run(&mut s, b"\x1b[92;103mY"); // bright green fg, bright yellow bg
        assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(ColorId(10)));
        assert_eq!(s.cell(0, 1).map(|c| c.bg), Some(ColorId(11)));
        run(&mut s, b"\x1b[39;49mZ"); // default fg and bg
        assert_eq!(s.cell(0, 2).map(|c| c.fg), Some(DEFAULT_FG_ID));
        assert_eq!(s.cell(0, 2).map(|c| c.bg), Some(DEFAULT_BG_ID));
    }

    #[test]
    fn sgr_indexed_and_rgb_bg_colors() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[38;5;196;48;5;21mX"); // indexed fg and bg
        assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(196)));
        assert_eq!(s.cell(0, 0).map(|c| c.bg), Some(ColorId(21)));
        run(&mut s, b"\x1b[48;2;1;2;3mY"); // RGB background
        assert_eq!(s.cell(0, 1).map(|c| c.bg), Some(ColorId(258)));
        assert_eq!(s.color_rgb(ColorId(258)), Some(0x010203));
    }

    #[test]
    fn sgr_empty_resets_and_underline_color_skipped() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[31mX\x1b[mY"); // bare CSI m resets
        assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(DEFAULT_FG_ID));
        run(&mut s, b"\x1b[58;5;196mZ"); // underline color is skipped
        assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('Z'));
        assert_eq!(s.cell(0, 2).map(|c| c.fg), Some(DEFAULT_FG_ID));
    }

    #[test]
    fn sgr_reset_flags_clear_attributes() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[1;3mX"); // bold + italic
        assert_eq!(
            s.cell(0, 0).map(|c| c.sgr.contains(SgrFlags::BOLD)),
            Some(true)
        );
        assert_eq!(
            s.cell(0, 0).map(|c| c.sgr.contains(SgrFlags::ITALIC)),
            Some(true)
        );
        run(&mut s, b"\x1b[22mY"); // 22 clears bold (and faint), italic stays
        assert_eq!(
            s.cell(0, 1).map(|c| c.sgr.contains(SgrFlags::BOLD)),
            Some(false)
        );
        assert_eq!(
            s.cell(0, 1).map(|c| c.sgr.contains(SgrFlags::ITALIC)),
            Some(true)
        );
    }

    #[test]
    fn ris_resets_screen() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"abc\r\n\x1b[31m");
        s.set_max_scrollback(5);
        run(&mut s, b"\x1bc"); // RIS
        assert_eq!(s.lines(), vec!["", "", ""]);
        assert_eq!(s.cursor(), (0, 0));
        assert_eq!(s.scrollback_len(), 0);
    }

    #[test]
    fn alt_screen_reenter_and_leave_are_noops() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"main");
        run(&mut s, b"\x1b[?1049h"); // enter alt
        assert!(s.is_alt_screen());
        run(&mut s, b"\x1b[?1049h"); // re-enter: no-op
        assert!(s.is_alt_screen());
        run(&mut s, b"\x1b[?1049l"); // leave
        assert!(!s.is_alt_screen());
        run(&mut s, b"\x1b[?1049l"); // leave again: no-op
        assert!(!s.is_alt_screen());
    }

    #[test]
    fn cell_and_render_line_out_of_range() {
        let s = Screen::new(3, 2);
        assert_eq!(s.cell(2, 0), None);
        assert_eq!(s.cell(0, 3), None);
        assert_eq!(s.render_line(2), None, "out-of-range row is None");
    }

    #[test]
    fn render_ansi_multiple_rows() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"a\r\n\x1b[1mb");
        let ansi = s.render_ansi();
        assert!(ansi.contains("a\r\n"));
        assert!(ansi.contains("b"));
        // Wide glyphs skip their continuation cell.
        run(&mut s, b"\x1b[2J\x1b[1;1H");
        run(&mut s, "中".as_bytes());
        assert!(s.render_ansi_line(0).contains("中"));
    }

    #[test]
    fn max_scrollback_getter_and_shrink() {
        let mut s = Screen::new(3, 2);
        s.set_max_scrollback(5);
        assert_eq!(s.max_scrollback(), 5);
        run(&mut s, b"a\r\nb\r\nc\r\nd\r\ne");
        assert_eq!(s.scrollback_len(), 3);
        s.set_max_scrollback(2); // shrinking truncates the oldest
        assert_eq!(s.scrollback_len(), 2);
        assert_eq!(s.scrollback_text(0).as_deref(), Some("b"));
    }

    #[test]
    fn resize_same_size_is_noop() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"abc");
        s.resize(3, 2);
        assert_eq!(s.render_line(0).as_deref(), Some("abc"));
    }

    #[test]
    fn scrollback_line_returns_raw_cells() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"a\r\nb\r\nc");
        assert_eq!(s.scrollback_line(0).map(|r| r.len()), Some(3));
        assert_eq!(s.scrollback_line(1), None);
    }

    #[test]
    fn cnl_and_cpl_move_and_home() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"\x1b[1;2H\x1b[E"); // CNL: down one, home column
        assert_eq!(s.cursor(), (1, 0));
        run(&mut s, b"\x1b[F"); // CPL: up one, home column
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn su_and_sd_scroll() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"aaa\r\nbbb\r\nccc");
        run(&mut s, b"\x1b[1;1H\x1b[T"); // SD: scroll down
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        assert_eq!(s.render_line(1).as_deref(), Some("aaa"));
        assert_eq!(s.render_line(2).as_deref(), Some("bbb"));
        run(&mut s, b"\x1b[1;1H\x1b[S"); // SU: scroll up
        assert_eq!(s.render_line(0).as_deref(), Some("aaa"));
        assert_eq!(s.render_line(1).as_deref(), Some("bbb"));
        assert_eq!(s.render_line(2).as_deref(), Some(""));
    }

    #[test]
    fn region_scroll_n_batches() {
        let mut s = Screen::new(3, 5);
        run(&mut s, b"\x1b[2;4r"); // margins rows 1..=3 (0-based)
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.render_line(1).as_deref(), Some("b"));
        assert_eq!(s.render_line(2).as_deref(), Some("c"));
        assert_eq!(s.render_line(3).as_deref(), Some("d"));

        // SU 2 scrolls the region up two lines in one pass.
        run(&mut s, b"\x1b[2S");
        assert_eq!(s.render_line(1).as_deref(), Some("d"));
        assert_eq!(s.render_line(2).as_deref(), Some(""));
        assert_eq!(s.render_line(3).as_deref(), Some(""));

        // SD 2 scrolls it back down two lines in one pass.
        run(&mut s, b"\x1b[2T");
        assert_eq!(s.render_line(1).as_deref(), Some(""));
        assert_eq!(s.render_line(2).as_deref(), Some(""));
        assert_eq!(s.render_line(3).as_deref(), Some("d"));
    }

    #[test]
    fn csi_question_r_is_decpcterm_and_ignored() {
        let mut s = Screen::new(3, 5);
        run(&mut s, b"\x1b[2;4r"); // DECSTBM: margins rows 2-4, cursor homes to row 1
        run(&mut s, b"\x1b[3;1H"); // move the cursor to row 2
        run(&mut s, b"\x1b[?r"); // DECPCTERM (PCTerm/Scancode mode): out of scope, ignored
        assert_eq!(s.cursor(), (2, 0)); // cursor unchanged — margins were not reset
    }

    #[test]
    fn hvp_matches_cup() {
        let mut s = Screen::new(5, 3);
        run(&mut s, b"\x1b[3;2f"); // HVP: same as CUP
        assert_eq!(s.cursor(), (2, 1));
    }

    #[test]
    fn vt_and_ff_linefeed() {
        let mut s = Screen::new(5, 3);
        run(&mut s, b"a\x0b"); // VT linefeeds, keeping the column
        assert_eq!(s.cursor(), (1, 1));
        run(&mut s, b"\x0c"); // FF linefeeds too
        assert_eq!(s.cursor(), (2, 1));
    }

    #[test]
    fn ignored_csi_finals_are_noop() {
        // The exact CSI finals the screen dispatches. Everything else in
        // `0x40..=0x7e` (the DEC/ANSI final range) must be a no-op, so a new
        // handler cannot slip in without updating this pin.
        let handled = b"@ABCDEFGHJKLMPSTXdfghlmr";
        for f in 0x40..=0x7e {
            if handled.contains(&f) {
                continue;
            }
            let mut s = Screen::new(4, 3);
            run(&mut s, &[b'\x1b', b'[', f]);
            assert_eq!(
                s,
                Screen::new(4, 3),
                "CSI {} (0x{f:02x}) must be a no-op",
                f as char
            );
        }
    }

    #[test]
    fn ignored_esc_finals_are_noop() {
        // ESC finals the screen dispatches, and the six sequence introducers
        // (`P` `X` `[` `]` `^` `_`) that are not finals at all.
        let handled = b"78DEHMc";
        let introducers = b"PX[]^_";
        for f in 0x30..=0x7e {
            if handled.contains(&f) || introducers.contains(&f) {
                continue;
            }
            let mut s = Screen::new(4, 3);
            run(&mut s, &[b'\x1b', f]);
            assert_eq!(
                s,
                Screen::new(4, 3),
                "ESC {} (0x{f:02x}) must be a no-op",
                f as char
            );
        }
    }

    #[test]
    fn tracked_but_unapplied_modes_are_noop() {
        // Modes `vtty_core::TermFlags` tracks but the screen does not apply.
        // Pinned so wiring one in (e.g. DECSCNM) is an explicit change.
        let seqs: &[&[u8]] = &[
            b"\x1b[?1h",
            b"\x1b[?1l",
            b"\x1b[?3h",
            b"\x1b[?3l",
            b"\x1b[?66h",
            b"\x1b[?66l",
            b"\x1b[?67h",
            b"\x1b[?67l",
            b"\x1b[?69h",
            b"\x1b[?69l",
            b"\x1b[?80h",
            b"\x1b[?80l",
        ];
        for seq in seqs {
            let mut s = Screen::new(4, 3);
            run(&mut s, seq);
            assert_eq!(s, Screen::new(4, 3), "{seq:?} must be a no-op");
        }
    }

    #[test]
    fn esc_with_intermediate_is_noop() {
        // Charset designations and other intermediate ESC sequences the
        // screen does not act on (it keeps no active charset).
        let seqs: &[&[u8]] = &[
            b"\x1b(0", // DEC Special Graphics designation
            b"\x1b(B", // US-ASCII
            b"\x1b)0", // G1 designation
            b"\x1b*0", // G2 designation
            b"\x1b+0", // G3 designation
            b"\x1b#3", // DECDHL, unsupported
            b"\x1b#7", // not DECALN
            b"\x1b(8", // intermediate + final 8, not DECRC
        ];
        for seq in seqs {
            let mut s = Screen::new(4, 3);
            run(&mut s, seq);
            assert_eq!(s, Screen::new(4, 3), "{seq:?} must be a no-op");
        }
    }

    #[test]
    fn reverse_index_scrolls_region_down() {
        let mut s = Screen::new(3, 5);
        run(&mut s, b"a\r\nb\r\nc\r\nd"); // rows 0..3
        run(&mut s, b"\x1b[2;4r"); // margins rows 1..=3 (0-based)
        run(&mut s, b"\x1b[2;1H\x1bM"); // RI at the region top scrolls it down
        assert_eq!(s.render_line(0).as_deref(), Some("a"));
        assert_eq!(s.render_line(1).as_deref(), Some(""));
        assert_eq!(s.render_line(2).as_deref(), Some("b"));
        assert_eq!(s.render_line(3).as_deref(), Some("c"));
    }

    #[test]
    fn osc_progress_unknown_mode_is_noop() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"\x1b]9;4;1;40\x07");
        run(&mut s, b"\x1b]9;4;99\x07"); // unknown mode: progress unchanged
        assert_eq!(s.progress(), Progress::Percent(40));
    }

    #[test]
    fn leading_mark_with_no_base_is_dropped() {
        let mut s = Screen::new(4, 1);
        run(&mut s, "\u{301}x".as_bytes()); // lone combining mark at column 0
        assert_eq!(s.render_line(0).as_deref(), Some("x"));
    }

    #[test]
    fn unhandled_sequences_fall_through() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[Z"); // unhandled CSI final (Cbt)
        run(&mut s, b"\x00"); // unhandled C0 execute (NUL)
        run(&mut s, b"\x1b]52;x\x07"); // unhandled OSC (52)
        run(&mut s, b"\x1b6"); // unhandled ESC final
        assert_eq!(s.render_line(0).as_deref(), Some(""));
    }

    #[test]
    fn chunked_multibyte_hits_print() {
        let mut s = Screen::new(4, 1);
        let mut p = Parser::new();
        p.advance(&mut s, &[0xe4]);
        p.advance(&mut s, &[0xb8, 0xad]); // 中
        assert_eq!(s.render_line(0).as_deref(), Some("中"));
    }

    #[test]
    fn invalid_utf8_prints_replacement_char() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\xffx"); // invalid lead byte -> U+FFFD
        assert_eq!(s.render_line(0).as_deref(), Some("\u{FFFD}x"));
    }

    #[test]
    fn dec_private_modes_set_and_reset() {
        let mut s = Screen::new(4, 4);
        run(&mut s, b"\x1b[?7l"); // DECAWM off
        run(&mut s, b"\x1b[?25l"); // DECTCEM off
        run(&mut s, b"\x1b[?9999h"); // unhandled mode: no-op
        // DEC 1048 saves and restores only the cursor.
        run(&mut s, b"\x1b[3;3H\x1b[?1048h");
        run(&mut s, b"\x1b[1;1H\x1b[?1048l");
        assert_eq!(s.cursor(), (2, 2));
    }

    #[test]
    fn sgr_empty_colorspace_and_unknown_color_form() {
        let mut s = Screen::new(4, 1);
        run(&mut s, b"\x1b[38:2::255:0:0mX"); // empty colorspace is skipped
        assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(258)));
        assert_eq!(s.color_rgb(ColorId(258)), Some(0xff0000));
        run(&mut s, b"\x1b[0m\x1b[38;3;255mY"); // unknown sub-param: ignored
        assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(DEFAULT_FG_ID));
    }

    #[test]
    fn put_char_combining_mark_attaches_to_base() {
        let mut s = Screen::new(4, 1);
        s.put_char('a');
        s.put_char('\u{301}'); // width 0 -> attach to 'a'
        assert_eq!(s.render_line(0).as_deref(), Some("a\u{301}"));
    }

    #[test]
    fn render_ansi_line_out_of_range_is_empty() {
        let s = Screen::new(4, 1);
        assert_eq!(s.render_ansi_line(99), "");
    }

    #[test]
    fn render_ansi_resets_after_styled_run() {
        let mut s = Screen::new(6, 1);
        run(&mut s, b"\x1b[1ma\x1b[0mb");
        let ansi = s.render_ansi_line(0);
        assert!(ansi.contains("\x1b[0;1m"));
        assert!(ansi.contains('a'));
        assert!(ansi.contains('b'));
    }

    #[test]
    fn mark_after_space_is_dropped() {
        let mut s = Screen::new(4, 1);
        s.put_char(' ');
        s.put_char('\u{301}'); // no visible base: dropped
        assert_eq!(s.cell(0, 0).map(|c| c.combining_len), Some(0));
    }

    #[test]
    fn put_cluster_empty_is_noop() {
        let mut s = Screen::new(4, 1);
        s.put_cluster("");
        assert_eq!(s.render_line(0).as_deref(), Some(""));
    }

    #[test]
    fn wide_char_without_wrap_clamps_to_one_cell() {
        let mut s = Screen::new(3, 1);
        run(&mut s, b"\x1b[?7l"); // DECAWM off
        run(&mut s, b"ab");
        run(&mut s, "中".as_bytes()); // width 2 at the last column -> width 1
        assert_eq!(s.render_line(0).as_deref(), Some("ab中"));
    }

    #[test]
    fn wide_char_at_last_column_wraps() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"ab"); // cursor at the last column
        run(&mut s, "中".as_bytes()); // width 2 -> wraps to the next row
        assert_eq!(s.render_line(0).as_deref(), Some("ab"));
        assert_eq!(s.render_line(1).as_deref(), Some("中"));
    }

    #[test]
    fn resize_reflow_clamps_and_wraps_wide() {
        // Width 1: a wide glyph clamps to one cell.
        let mut s = Screen::new(3, 1);
        run(&mut s, "中".as_bytes());
        s.resize(1, 1);
        assert_eq!(s.render_line(0).as_deref(), Some("中"));
        // Width 2: a wide glyph after a single cell wraps to the next row.
        let mut s = Screen::new(4, 1);
        run(&mut s, "a中".as_bytes());
        s.resize(2, 2);
        assert_eq!(s.render_line(0).as_deref(), Some("a"));
        assert_eq!(s.render_line(1).as_deref(), Some("中"));
    }

    #[test]
    fn region_scroll_after_full_screen_scroll() {
        // Full-screen scroll rotates the ring (scroll_top != 0); a later
        // DECSTBM region scroll must materialize the ring first.
        let mut s = Screen::new(3, 3);
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.render_line(0).as_deref(), Some("b"));
        assert_eq!(s.render_line(1).as_deref(), Some("c"));
        assert_eq!(s.render_line(2).as_deref(), Some("d"));
        assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));

        // Region rows 1-2 (1-based) = 0..=1 (0-based).
        run(&mut s, b"\x1b[1;2r");
        run(&mut s, b"e\r\nf\r\ng");
        assert_eq!(s.render_line(0).as_deref(), Some("f"));
        assert_eq!(s.render_line(1).as_deref(), Some("g"));
        assert_eq!(s.render_line(2).as_deref(), Some("d"));
        // Region scrolls still discard, so only `a` remains in scrollback.
        assert_eq!(s.scrollback_len(), 1);
        assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));
    }

    #[test]
    fn resize_after_full_screen_scroll_reflows() {
        // Reflow must materialize the ring before reading rows in order.
        let mut s = Screen::new(4, 2);
        run(&mut s, b"aaaa\r\nbbbb\r\ncccc");
        assert_eq!(s.render_line(0).as_deref(), Some("bbbb"));
        assert_eq!(s.render_line(1).as_deref(), Some("cccc"));
        assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaa"));

        s.resize(8, 2);
        assert_eq!(s.render_line(0).as_deref(), Some("bbbb"));
        assert_eq!(s.render_line(1).as_deref(), Some("cccc"));
        assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaa"));
    }

    #[test]
    fn reverse_index_after_full_screen_scroll() {
        // RI at the top margin rotates the ring back down and blanks row 0.
        let mut s = Screen::new(3, 2);
        run(&mut s, b"a\r\nb\r\nc");
        assert_eq!(s.render_line(0).as_deref(), Some("b"));
        assert_eq!(s.render_line(1).as_deref(), Some("c"));

        run(&mut s, b"\x1b[H\x1bM");
        assert_eq!(s.render_line(0).as_deref(), Some(""));
        assert_eq!(s.render_line(1).as_deref(), Some("b"));
        assert_eq!(s.scrollback_len(), 1);
        assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));
    }

    #[test]
    fn osc_progress_bar() {
        let mut s = Screen::new(3, 2);
        assert_eq!(s.progress(), Progress::Hidden);

        run(&mut s, b"\x1b]9;4;0\x07");
        assert_eq!(s.progress(), Progress::Indeterminate);

        run(&mut s, b"\x1b]9;4;1;40\x07");
        assert_eq!(s.progress(), Progress::Percent(40));

        run(&mut s, b"\x1b]9;4;1;150\x07");
        assert_eq!(s.progress(), Progress::Percent(100));

        run(&mut s, b"\x1b]9;4;2\x07");
        assert_eq!(s.progress(), Progress::Done);

        run(&mut s, b"\x1b]9;4;3\x07");
        assert_eq!(s.progress(), Progress::Error);

        // A non-progress OSC 9 sub-command is ignored.
        run(&mut s, b"\x1b]9;1;hello\x07");
        assert_eq!(s.progress(), Progress::Error);
    }

    #[test]
    fn cursor_horizontal_forward_does_not_overflow() {
        // The forward branch must saturate like the backward branch, so a
        // near-maximum column plus a large param cannot overflow.
        let mut s = Screen::new(3, 2);
        s.cursor_col = u32::MAX - 5;
        s.cursor_horizontal(65535, false);
        assert_eq!(s.cursor_col, s.cols - 1);
    }
}
