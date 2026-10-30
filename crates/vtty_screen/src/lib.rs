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
mod reflow;
mod render;
mod scroll;
mod sgr;

pub use cell::{BufferFlags, Cell, ScreenFlags};
use cell::{default_palette, default_tabs};
use reflow::{fit_grid, reflow_rows, rows_from};
use render::{push_sgr, render_cells, trailing_blanks};

/// A minimal cell-grid terminal screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    pub(crate) cells: Vec<Cell>,
    pub(crate) cursor_row: u32,
    pub(crate) cursor_col: u32,
    pub(crate) saved_row: u32,
    pub(crate) saved_col: u32,
    pub(crate) sgr: SgrFlags,
    pub(crate) fg: ColorId,
    pub(crate) bg: ColorId,
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
    // The alternate screen buffer (DEC 47/1046/1047/1049): the same
    // per-buffer state as the main screen, swapped in/out wholesale.
    pub(crate) alt_cells: Vec<Cell>,
    pub(crate) alt_cursor_row: u32,
    pub(crate) alt_cursor_col: u32,
    pub(crate) alt_saved_row: u32,
    pub(crate) alt_saved_col: u32,
    pub(crate) alt_sgr: SgrFlags,
    pub(crate) alt_fg: ColorId,
    pub(crate) alt_bg: ColorId,
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
    pub fn new(cols: u32, rows: u32) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); (cols * rows) as usize],
            cursor_row: 0,
            cursor_col: 0,
            saved_row: 0,
            saved_col: 0,
            sgr: SgrFlags::DEFAULT,
            fg: DEFAULT_FG_ID,
            bg: DEFAULT_BG_ID,
            modes: ScreenFlags::DEFAULT,
            flags: BufferFlags::empty(),
            top_margin: 0,
            bottom_margin: rows - 1,
            rgb: Vec::new(),
            palette: default_palette(),
            default_fg: 0xffffff,
            default_bg: 0x000000,
            title: String::new(),
            alt_cells: vec![Cell::default(); (cols * rows) as usize],
            alt_cursor_row: 0,
            alt_cursor_col: 0,
            alt_saved_row: 0,
            alt_saved_col: 0,
            alt_sgr: SgrFlags::DEFAULT,
            alt_fg: DEFAULT_FG_ID,
            alt_bg: DEFAULT_BG_ID,
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
        self.reset_cursor_style_margins();
        self.saved_row = 0;
        self.saved_col = 0;
        self.modes = ScreenFlags::DEFAULT;
        self.flags.remove(BufferFlags::ORIGIN);
        self.rgb.clear();
        self.title.clear();
        // Reset the alternate buffer too.
        self.alt_cells.fill(Cell::default());
        self.alt_cursor_row = 0;
        self.alt_cursor_col = 0;
        self.alt_saved_row = 0;
        self.alt_saved_col = 0;
        self.alt_sgr = SgrFlags::DEFAULT;
        self.alt_fg = DEFAULT_FG_ID;
        self.alt_bg = DEFAULT_BG_ID;
        self.alt_flags = BufferFlags::empty();
        self.alt_top_margin = 0;
        self.alt_bottom_margin = self.rows - 1;
        self.scrollback.clear();
        self.tab_stops = default_tabs(self.cols);
        self.continuation.fill(false);
        self.alt_continuation.fill(false);
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

    fn swap_buffers(&mut self) {
        core::mem::swap(&mut self.cells, &mut self.alt_cells);
        core::mem::swap(&mut self.cursor_row, &mut self.alt_cursor_row);
        core::mem::swap(&mut self.cursor_col, &mut self.alt_cursor_col);
        core::mem::swap(&mut self.saved_row, &mut self.alt_saved_row);
        core::mem::swap(&mut self.saved_col, &mut self.alt_saved_col);
        core::mem::swap(&mut self.sgr, &mut self.alt_sgr);
        core::mem::swap(&mut self.fg, &mut self.alt_fg);
        core::mem::swap(&mut self.bg, &mut self.alt_bg);
        core::mem::swap(&mut self.flags, &mut self.alt_flags);
        core::mem::swap(&mut self.top_margin, &mut self.alt_top_margin);
        core::mem::swap(&mut self.bottom_margin, &mut self.alt_bottom_margin);
        core::mem::swap(&mut self.continuation, &mut self.alt_continuation);
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

    /// The cell at `(row, col)`, or `None` out of range.
    #[inline]
    pub fn cell(&self, row: u32, col: u32) -> Option<&Cell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        Some(&self.cells[(row * self.cols + col) as usize])
    }

    /// Whether column `col` has a tab stop.
    #[inline]
    pub fn tab_stop_at(&self, col: u32) -> bool {
        col < self.cols && self.tab_stops[col as usize]
    }

    /// Resolve a [`ColorId`] to packed `0xRRGGBB`. The default fg/bg ids use
    /// the configured default colors; the 16 ANSI colors use the configured
    /// palette; `16..=255` use the xterm table; truecolor ids (`>= 258`) look
    /// up the screen's dynamic palette.
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
    pub fn set_palette(&mut self, palette: [u32; 16]) {
        self.palette = palette;
    }

    /// Set the default foreground and background colors.
    pub fn set_default_colors(&mut self, fg: u32, bg: u32) {
        self.default_fg = fg;
        self.default_bg = bg;
    }

    /// The cell index range of `row`.
    fn row_range(&self, row: u32) -> Range<usize> {
        let start = (row * self.cols) as usize;
        start..start + self.cols as usize
    }

    /// Render one row to plain text, trimming trailing spaces and skipping
    /// double-width continuation cells.
    pub fn render_line(&self, row: u32) -> String {
        if row >= self.rows {
            return String::new();
        }
        render_cells(&self.cells[self.row_range(row)])
    }

    /// Render every row to text, one `String` per row.
    pub fn lines(&self) -> Vec<String> {
        (0..self.rows).map(|r| self.render_line(r)).collect()
    }

    /// Render every row to ANSI-styled text (for snapshots/tests): each row
    /// `\r\n`-terminated, cells carrying their SGR attributes. Trailing blank
    /// cells are trimmed; each line ends with an SGR reset.
    pub fn render_ansi(&self) -> String {
        let mut out = String::new();
        for row in 0..self.rows {
            out.push_str(&self.render_ansi_line(row));
            out.push_str("\r\n");
        }
        out
    }

    /// Render one row to ANSI-styled text (no trailing newline).
    pub fn render_ansi_line(&self, row: u32) -> String {
        let mut out = String::new();
        if row >= self.rows {
            return out;
        }
        let cells = &self.cells[self.row_range(row)];
        // Trim trailing blank cells.
        let last = cells.len() - trailing_blanks(cells);
        // `current` is the emitted style; `None` means "default, no SGR
        // emitted yet". Default-styled cells add no escape sequences.
        let mut current: Option<(SgrFlags, ColorId, ColorId)> = None;
        for cell in &cells[..last] {
            if cell.is_continuation() {
                continue;
            }
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
            out.push(cell.ch);
            out.extend(cell.marks().iter().copied());
        }
        if current.is_some() {
            out.push_str("\x1b[0m");
        }
        out
    }

    /// The number of scrollback lines (oldest first).
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
    pub fn scrollback_line(&self, i: usize) -> Option<&[Cell]> {
        self.scrollback.get(i).map(|(row, _)| row.as_slice())
    }

    /// Render scrollback line `i` to text, or `None` out of range.
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
        let idx = |col: u32| (self.cursor_row * self.cols + col) as usize;
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
        match (private, f) {
            (false, b'A') => self.cursor_up(n(0)),      // CUU
            (false, b'B') => self.cursor_down(n(0)),    // CUD
            (false, b'C') => self.cursor_forward(n(0)), // CUF
            (false, b'D') => self.cursor_back(n(0)),    // CUB
            (false, b'E') => {
                // CNL
                self.cursor_down(n(0));
                self.cursor_col = 0;
            }
            (false, b'F') => {
                // CPL
                self.cursor_up(n(0));
                self.cursor_col = 0;
            }
            (false, b'G') => self.cha(n(0)), // CHA
            (false, b'd') => self.vpa(n(0)), // VPA
            (false, b'H') | (false, b'f') => self.cup(n(0), n(1)), // CUP / HVP
            (false, b'J') => self.erase_in_display(params.first().copied().unwrap_or(0)), // ED
            (false, b'K') => self.erase_in_line(params.first().copied().unwrap_or(0)), // EL
            (false, b'L') => self.insert_lines(n(0)), // IL
            (false, b'M') => self.delete_lines(n(0)), // DL
            (false, b'P') => self.delete_chars(n(0)), // DCH
            (false, b'@') => self.insert_chars(n(0)), // ICH
            (false, b'X') => self.erase_chars(n(0)), // ECH
            (false, b'S') => {
                // SU
                for _ in 0..n(0) {
                    self.scroll_up();
                }
            }
            (false, b'T') => {
                // SD
                for _ in 0..n(0) {
                    self.scroll_down();
                }
            }
            (false, b'm') => self.sgr(params),         // SGR
            (false, b'g') => self.tbc(params),         // TBC
            (false, b'r') => self.set_margins(params), // DECSTBM
            (true, b'r') => self.reset_margins(),      // DECSTBM restore
            (true, b'h') => self.set_mode(params),     // DECSET
            (true, b'l') => self.reset_mode(params),   // DECRST
            _ => {}
        }
    }
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn print_bytes(&mut self, s: &[u8]) {
        for &b in s {
            self.put_char(b as char);
        }
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
        let private = intermediates.first() == Some(&b'?');
        self.csi(params, private, f);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        match (intermediates, byte) {
            (&[b'#'], b'8') => self.decaln(),     // DECALN
            (&[], b'7') => self.save_cursor(),    // DECSC
            (&[], b'8') => self.restore_cursor(), // DECRC
            (&[], b'D') => self.linefeed(),       // IND
            (&[], b'H') => self.hts(),            // HTS
            (&[], b'M') => self.reverse_index(),  // RI
            (&[], b'E') => {
                // NEL
                self.cursor_col = 0;
                self.linefeed();
            }
            (&[], b'c') => self.reset(), // RIS
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
        assert_eq!(s.render_line(0), "hi");
        assert_eq!(s.render_line(1), "");
    }

    #[test]
    fn autowrap_at_last_column() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"abc");
        assert_eq!(s.cursor(), (0, 2));
        run(&mut s, b"d");
        // `d` wraps to the next line.
        assert_eq!(s.cursor(), (1, 1));
        assert_eq!(s.render_line(0), "abc");
        assert_eq!(s.render_line(1), "d");
    }

    #[test]
    fn double_width_glyph() {
        let mut s = Screen::new(5, 2);
        run(&mut s, "a中b".as_bytes());
        assert_eq!(s.render_line(0), "a中b");
        assert_eq!(s.cell(0, 2).map(Cell::is_continuation), Some(true));
    }

    #[test]
    fn combining_mark_attaches() {
        let mut s = Screen::new(4, 1);
        run(&mut s, "e\u{301}x".as_bytes());
        assert_eq!(s.render_line(0), "e\u{301}x");
        assert_eq!(s.cell(0, 0).map(|c| c.marks()), Some(&['\u{301}'][..]));
    }

    #[test]
    fn skin_tone_attaches_to_wide() {
        let mut s = Screen::new(4, 1);
        run(&mut s, "👋\u{1F3FF}".as_bytes());
        assert_eq!(s.render_line(0), "👋\u{1F3FF}");
        assert_eq!(s.cell(0, 0).map(|c| c.marks()), Some(&['\u{1F3FF}'][..]));
        assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
    }

    #[test]
    fn zwj_emoji_family_is_one_cell() {
        let mut s = Screen::new(8, 1);
        run(&mut s, "👨\u{200D}👩\u{200D}👧x".as_bytes());
        // The whole family occupies two cells (one glyph); `x` lands after.
        assert_eq!(s.render_line(0), "👨\u{200D}👩\u{200D}👧x");
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
        assert_eq!(s.render_line(0), "");
        assert_eq!(s.render_line(4), "");
        assert_eq!(s.render_line(1), "b");
        assert_eq!(s.render_line(2), "c");
        assert_eq!(s.render_line(3), "d");
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
        assert_eq!(s.render_line(0), "EEE");
        assert_eq!(s.render_line(1), "EEE");
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn insert_and_delete_lines() {
        let mut s = Screen::new(3, 3);
        run(&mut s, b"aaa\r\nbbb\r\nccc");
        run(&mut s, b"\x1b[2;1H\x1b[L"); // insert line at row 2 (1-based)
        assert_eq!(s.render_line(0), "aaa");
        assert_eq!(s.render_line(1), "");
        assert_eq!(s.render_line(2), "bbb");
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
        assert_eq!(s.render_line(0), "main");
        let main_cursor = s.cursor();

        // Enter the alt screen: blank it and save the main cursor.
        run(&mut s, b"\x1b[?1049h");
        assert!(s.is_alt_screen());
        assert_eq!(s.render_line(0), "");
        run(&mut s, b"alt");
        assert_eq!(s.render_line(0), "alt");

        // Leave: main content and cursor are restored.
        run(&mut s, b"\x1b[?1049l");
        assert!(!s.is_alt_screen());
        assert_eq!(s.render_line(0), "main");
        assert_eq!(s.cursor(), main_cursor);
    }

    #[test]
    fn alternate_screen_47_persists() {
        let mut s = Screen::new(5, 2);
        run(&mut s, b"main");
        run(&mut s, b"\x1b[?47h");
        assert!(s.is_alt_screen());
        // 47 does not clear, but the alt buffer starts blank.
        assert_eq!(s.render_line(0), "");
        run(&mut s, b"alt");
        run(&mut s, b"\x1b[?47l");
        assert_eq!(s.render_line(0), "main");
        // Re-enter: the alt content persists (no clear).
        run(&mut s, b"\x1b[?47h");
        assert_eq!(s.render_line(0), "alt");
        run(&mut s, b"\x1b[?47l");
    }

    #[test]
    fn scrollback_captures_lines() {
        let mut s = Screen::new(3, 2);
        run(&mut s, b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.scrollback_len(), 2);
        assert_eq!(s.render_line(0), "c");
        assert_eq!(s.render_line(1), "d");
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
        assert_eq!(s.render_line(0), "b");
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
        assert_eq!(s.render_line(0), "a");
        assert_eq!(s.render_line(1), "b");
        assert_eq!(s.render_line(2), "c");
        assert_eq!(s.render_line(3), "d");
        assert_eq!(s.render_line(4), "");
    }

    #[test]
    fn resize_does_not_multiply_blank_lines() {
        // One intentional blank line must stay one blank line when the width
        // shrinks (a blank row stores `cols` blank cells, which would
        // otherwise re-wrap into `ceil(old/new)` rows).
        let mut s = Screen::new(20, 4);
        run(&mut s, b"a\r\n\r\nb");
        assert_eq!(s.render_line(0), "a");
        assert_eq!(s.render_line(1), "");
        assert_eq!(s.render_line(2), "b");
        s.resize(10, 4);
        assert_eq!(s.render_line(0), "a");
        assert_eq!(s.render_line(1), "");
        assert_eq!(s.render_line(2), "b");
        assert_eq!(s.render_line(3), "");
    }

    #[test]
    fn resize_preserves_wrapped_space() {
        // "hello world" wraps as "hello " + "world"; the trailing space on
        // the soft-wrapped row is content and must survive reflow.
        let mut s = Screen::new(6, 2);
        run(&mut s, b"hello world");
        assert_eq!(s.cell(0, 5).map(|c| c.ch), Some(' '));
        assert_eq!(s.render_line(1), "world");
        s.resize(11, 2);
        assert_eq!(s.render_line(0), "hello world");
        assert_eq!(s.render_line(1), "");
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
        assert_eq!(s.render_line(0), "ccccc");
        assert_eq!(s.render_line(1), "ddddd");
    }

    #[test]
    fn resize_rejoins_scrollback_soft_wrap() {
        let mut s = Screen::new(5, 2);
        // "helloworld" soft-wraps to two rows; a following line pushes the
        // first half into scrollback.
        run(&mut s, b"helloworld\r\nxy");
        assert_eq!(s.scrollback_text(0).as_deref(), Some("hello"));
        assert_eq!(s.render_line(0), "world");
        assert_eq!(s.render_line(1), "xy");
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
}
