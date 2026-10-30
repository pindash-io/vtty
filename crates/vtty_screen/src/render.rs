// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Plain-text and ANSI render helpers (pure functions over cell slices).

use alloc::string::String;
use core::fmt::Write as _;

use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

use crate::cell::Cell;

/// Emit an SGR sequence (`ESC [ 0 ; … m`) for a cell's attributes.
pub(crate) fn push_sgr(out: &mut String, sgr: SgrFlags, fg: ColorId, bg: ColorId, palette: &[u32]) {
    out.push_str("\x1b[0");
    for (flag, code) in [
        (SgrFlags::BOLD, ";1"),
        (SgrFlags::FAINT, ";2"),
        (SgrFlags::ITALIC, ";3"),
        (SgrFlags::UNDERLINE, ";4"),
        (SgrFlags::BLINK, ";5"),
        (SgrFlags::BLINK_RAPID, ";6"),
        (SgrFlags::INVERSE, ";7"),
        (SgrFlags::INVISIBLE, ";8"),
        (SgrFlags::STRIKETHROUGH, ";9"),
    ] {
        if sgr.contains(flag) {
            out.push_str(code);
        }
    }
    push_ansi_color(out, 3, fg, palette);
    push_ansi_color(out, 4, bg, palette);
    out.push('m');
}

/// Emit one SGR color (`;{base}8;5;n` indexed or `;{base}8;2;r;g;b`
/// truecolor resolved from `palette`). The default color emits nothing —
/// the leading `ESC [ 0` already selects it.
pub(crate) fn push_ansi_color(out: &mut String, base: u8, id: ColorId, palette: &[u32]) {
    let default = if base == 3 {
        DEFAULT_FG_ID
    } else {
        DEFAULT_BG_ID
    };
    if id != default && id.0 <= 255 {
        let _ = write!(out, ";{base}8;5;{}", id.0);
    } else if let Some(&rgb) = id.0.checked_sub(258).and_then(|i| palette.get(i as usize)) {
        let [r, g, b] = vtty_core::u32_to_rgb(rgb);
        let _ = write!(out, ";{base}8;2;{r};{g};{b}");
    }
}

/// Number of trailing blank cells in `cells`.
pub(crate) fn trailing_blanks(cells: &[Cell]) -> usize {
    let mut n = 0;
    while n < cells.len() && cells[cells.len() - 1 - n].is_blank() {
        n += 1;
    }
    n
}

/// Render a `Cell` slice to plain text (used by
/// [`Screen::render_line`](crate::Screen::render_line) and
/// [`Screen::scrollback_text`](crate::Screen::scrollback_text)).
pub(crate) fn render_cells(cells: &[Cell]) -> String {
    let mut s = String::new();
    for cell in cells {
        if !cell.is_continuation() {
            s.push(cell.ch);
            s.extend(cell.marks().iter().copied());
        }
    }
    while s.ends_with(' ') {
        s.pop();
    }
    s
}
