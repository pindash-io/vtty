// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! VT400 rectangular-area operations for [`Screen`](crate::Screen).

use alloc::vec::Vec;
use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

use crate::Screen;
use crate::cell::Cell;
use crate::sgr::apply_sgr_to;

/// A 1-based DEC coordinate parameter; `0`/absent mean 1.
#[inline]
fn coord(params: &[i32], i: usize) -> i32 {
    params.get(i).copied().unwrap_or(1).max(1)
}

/// DECRARA's "reverse": toggle SGR flags and swap foreground/background.
fn reverse_sgr_to(params: &[i32], sgr: &mut SgrFlags, fg: &mut ColorId, bg: &mut ColorId) {
    let mut i = 0;
    while i < params.len() {
        let code = params[i];
        match code {
            0 => {
                *sgr = SgrFlags::DEFAULT;
                *fg = DEFAULT_FG_ID;
                *bg = DEFAULT_BG_ID;
            }
            30..=37 | 90..=97 | 39 | 40..=47 | 100..=107 | 49 => {
                core::mem::swap(fg, bg);
            }
            38 | 48 => {
                core::mem::swap(fg, bg);
                // Skip the extended-color sub-parameters.
                match params.get(i + 1).copied() {
                    Some(5) => {
                        if params.get(i + 2).is_some() {
                            i += 2;
                        }
                    }
                    Some(2) => {
                        let mut j = i + 2;
                        if params.get(j) == Some(&-1) {
                            j += 1;
                        }
                        if j + 2 < params.len() {
                            i = j + 2;
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                let flags = SgrFlags::from(code);
                if flags.contains(SgrFlags::RESET) {
                    sgr.insert(flags.difference(SgrFlags::RESET));
                } else if !flags.is_empty() {
                    sgr.toggle(flags);
                }
            }
        }
        i += 1;
    }
}

impl Screen {
    /// Clamp 1-based DEC coordinates to a 0-based inclusive
    /// `(top, left, bottom, right)`, or `None` when the rectangle is empty.
    fn rect(&self, top: i32, left: i32, bottom: i32, right: i32) -> Option<(u32, u32, u32, u32)> {
        let top = (top - 1).clamp(0, self.rows as i32 - 1) as u32;
        let bottom = (bottom - 1).clamp(0, self.rows as i32 - 1) as u32;
        let left = (left - 1).clamp(0, self.cols as i32 - 1) as u32;
        let right = (right - 1).clamp(0, self.cols as i32 - 1) as u32;
        (top <= bottom && left <= right).then_some((top, left, bottom, right))
    }

    /// DECFRA (`CSI Pc;Pl;Pr;Pt;Pb $ x`): fill a rectangle with `ch`.
    pub(crate) fn decfra(&mut self, params: &[i32]) {
        let Some((r0, c0, r1, c1)) = self.rect(
            coord(params, 3),
            coord(params, 1),
            coord(params, 4),
            coord(params, 2),
        ) else {
            return;
        };
        let ch = params
            .first()
            .copied()
            .and_then(|c| char::from_u32(c as u32))
            .unwrap_or(' ');
        let cell = self.base_cell(ch);
        for r in r0..=r1 {
            let base = self.row_offset(r);
            self.cells[base + c0 as usize..=base + c1 as usize].fill(cell);
        }
    }

    /// DECERA (`CSI Pl;Pr;Pt;Pb $ z`): blank a rectangle.
    pub(crate) fn decera(&mut self, params: &[i32]) {
        let Some((r0, c0, r1, c1)) = self.rect(
            coord(params, 2),
            coord(params, 0),
            coord(params, 3),
            coord(params, 1),
        ) else {
            return;
        };
        for r in r0..=r1 {
            let base = self.row_offset(r);
            self.cells[base + c0 as usize..=base + c1 as usize].fill(Cell::default());
        }
    }

    /// DECSERA (`CSI Pl;Pr;Pt;Pb $ {`): blank non-protected cells.
    pub(crate) fn decsera(&mut self, params: &[i32]) {
        let Some((r0, c0, r1, c1)) = self.rect(
            coord(params, 2),
            coord(params, 0),
            coord(params, 3),
            coord(params, 1),
        ) else {
            return;
        };
        for r in r0..=r1 {
            let base = self.row_offset(r);
            for cell in &mut self.cells[base + c0 as usize..=base + c1 as usize] {
                if !cell.protected {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// DECCRA (`CSI Pts;Pl;Pr;Pt;Pb;Pp $ v`): copy a source rectangle to a
    /// destination top-left corner, clipping to the screen.
    pub(crate) fn deccra(&mut self, params: &[i32]) {
        let Some((r0, c0, r1, c1)) = self.rect(
            coord(params, 3),
            coord(params, 1),
            coord(params, 4),
            coord(params, 2),
        ) else {
            return;
        };
        let dst_top = (coord(params, 0) - 1).clamp(0, self.rows as i32 - 1) as u32;
        let dst_left = (coord(params, 5) - 1).clamp(0, self.cols as i32 - 1) as u32;
        let width = (c1 - c0 + 1) as usize;
        for dr in 0..=(r1 - r0) {
            let dst_row = dst_top + dr;
            if dst_row >= self.rows {
                break;
            }
            let src_base = self.row_offset(r0 + dr) + c0 as usize;
            let dst_base = self.row_offset(dst_row) + dst_left as usize;
            let len = width.min((self.cols - dst_left) as usize);
            // A per-row buffer keeps overlapping copies correct.
            let row: Vec<Cell> = self.cells[src_base..src_base + len].to_vec();
            self.cells[dst_base..dst_base + len].copy_from_slice(&row);
        }
    }

    /// DECCARA (`CSI Pt;Pl;Pb;Pr;Sps... $ r`): apply SGR attributes to a
    /// rectangle.
    pub(crate) fn deccara(&mut self, params: &[i32]) {
        self.rect_attrs(params, false);
    }

    /// DECRARA (`CSI Pt;Pl;Pb;Pr;Sps... $ t`): reverse SGR attributes in a
    /// rectangle.
    pub(crate) fn decrara(&mut self, params: &[i32]) {
        self.rect_attrs(params, true);
    }

    fn rect_attrs(&mut self, params: &[i32], reverse: bool) {
        let Some((r0, c0, r1, c1)) = self.rect(
            coord(params, 0),
            coord(params, 1),
            coord(params, 2),
            coord(params, 3),
        ) else {
            return;
        };
        let sgr_params = params.get(4..).unwrap_or(&[]);
        for r in r0..=r1 {
            let base = self.row_offset(r);
            for cell in &mut self.cells[base + c0 as usize..=base + c1 as usize] {
                let (mut sgr, mut fg, mut bg) = (cell.sgr, cell.fg, cell.bg);
                if reverse {
                    reverse_sgr_to(sgr_params, &mut sgr, &mut fg, &mut bg);
                } else {
                    apply_sgr_to(sgr_params, &mut sgr, &mut fg, &mut bg, &mut self.rgb);
                }
                cell.sgr = sgr;
                cell.fg = fg;
                cell.bg = bg;
            }
        }
    }
}
