// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! sgr commands for [`Screen`](crate::Screen).

use alloc::vec::Vec;

use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

use crate::Screen;
use crate::cell::{BufferFlags, Cell, ScreenFlags};

/// Parse an SGR parameter list into `(sgr, fg, bg)` in place. Truecolor specs
/// are appended to `rgb` and referenced by id, so the same sequence applies
/// to one cell (the current style) or many (DECCARA/DECRARA).
pub(crate) fn apply_sgr_to(
    params: &[i32],
    sgr: &mut SgrFlags,
    fg: &mut ColorId,
    bg: &mut ColorId,
    rgb: &mut Vec<u32>,
) {
    if params.is_empty() {
        *sgr = SgrFlags::DEFAULT;
        *fg = DEFAULT_FG_ID;
        *bg = DEFAULT_BG_ID;
        return;
    }
    let mut i = 0;
    while i < params.len() {
        let code = params[i];
        match code {
            0 => {
                *sgr = SgrFlags::DEFAULT;
                *fg = DEFAULT_FG_ID;
                *bg = DEFAULT_BG_ID;
            }
            30..=37 => *fg = ColorId((code - 30) as u16),
            90..=97 => *fg = ColorId((code - 90 + 8) as u16),
            40..=47 => *bg = ColorId((code - 40) as u16),
            100..=107 => *bg = ColorId((code - 100 + 8) as u16),
            39 => *fg = DEFAULT_FG_ID,
            49 => *bg = DEFAULT_BG_ID,
            38 | 48 => {
                let is_fg = code == 38;
                match params.get(i + 1).copied() {
                    Some(5) => {
                        if let Some(&n) = params.get(i + 2) {
                            let id = ColorId::from(n);
                            if is_fg {
                                *fg = id;
                            } else {
                                *bg = id;
                            }
                            i += 2;
                        }
                    }
                    Some(2) => {
                        // Skip an empty colorspace (`38:2::r:g:b` → -1).
                        let mut j = i + 2;
                        if params.get(j) == Some(&-1) {
                            j += 1;
                        }
                        if j + 2 < params.len() {
                            let rgb_value = vtty_core::rgb(params[j], params[j + 1], params[j + 2]);
                            let id = ColorId(258 + rgb.len() as u16);
                            rgb.push(rgb_value);
                            if is_fg {
                                *fg = id;
                            } else {
                                *bg = id;
                            }
                            i = j + 2;
                        }
                    }
                    _ => {}
                }
            }
            58 => {
                // Underline color: skipped in the MVP.
            }
            _ => {
                let flags = SgrFlags::from(code);
                if flags.contains(SgrFlags::RESET) {
                    sgr.remove(flags.difference(SgrFlags::RESET));
                } else if !flags.is_empty() {
                    sgr.insert(flags);
                }
            }
        }
        i += 1;
    }
}

impl Screen {
    pub(crate) fn sgr(&mut self, params: &[i32]) {
        apply_sgr_to(
            params,
            &mut self.sgr,
            &mut self.fg,
            &mut self.bg,
            &mut self.rgb,
        );
    }

    pub(crate) const fn save_cursor(&mut self) {
        self.saved_row = self.cursor_row;
        self.saved_col = self.cursor_col;
    }

    pub(crate) fn restore_cursor(&mut self) {
        self.cursor_row = self.saved_row;
        self.cursor_col = self.saved_col;
        self.clear_wrap();
    }

    pub(crate) fn decaln(&mut self) {
        self.cells.fill(Cell {
            ch: 'E',
            ..Cell::default()
        });
        self.reset_cursor_style_margins();
        self.continuation.fill(false);
    }

    pub(crate) fn set_mode(&mut self, params: &[i32]) {
        self.apply_modes(params, true);
    }

    pub(crate) fn reset_mode(&mut self, params: &[i32]) {
        self.apply_modes(params, false);
    }

    /// Apply DEC private modes: set (`set = true`) or reset (`false`).
    pub(crate) fn apply_modes(&mut self, params: &[i32], set: bool) {
        for &p in params {
            match p {
                5 => self.modes.set(ScreenFlags::REVERSE, set), // DECSCNM
                6 => self.flags.set(BufferFlags::ORIGIN, set),  // DECOM
                7 => self.modes.set(ScreenFlags::WRAP, set),    // DECAWM
                25 => self.modes.set(ScreenFlags::CURSOR_VISIBLE, set), // DECTCEM
                47 | 1046 | 1047 | 1049 => {
                    if set {
                        self.enter_alt(p != 47); // DEC 1046/1047/1049 clear; DEC 47 keeps content
                    } else {
                        self.leave_alt();
                    }
                }
                1048 => {
                    if set {
                        self.save_cursor(); // DEC 1048
                    } else {
                        self.restore_cursor();
                    }
                }
                _ => {}
            }
        }
    }

    /// Apply ANSI modes (`CSI h` / `CSI l`, no `?`): currently IRM insert
    /// mode (4) only.
    pub(crate) fn apply_ansi_modes(&mut self, params: &[i32], set: bool) {
        for &p in params {
            if p == 4 {
                self.modes.set(ScreenFlags::INSERT, set); // IRM
            }
        }
    }

    /// DECSCA (`CSI Ps " q`): protect attribute for subsequently written
    /// cells. `1` marks them protected (DECSED/DECSEL skip them); `0`/`2`
    /// (or an absent parameter) leave them erasable.
    pub(crate) fn decsca(&mut self, params: &[i32]) {
        self.protect = params.first().copied().unwrap_or(0) == 1;
    }
}
