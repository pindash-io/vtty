// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The screen cell and the per-buffer/global mode flags.

use alloc::vec::Vec;

use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

bitflags::bitflags! {
    /// Per-buffer screen state, swapped wholesale with the alternate screen.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct BufferFlags: u8 {
        /// A wide glyph was written at the last column; the next print wraps.
        const WRAP_PENDING = 1 << 0;
        /// DECOM — origin mode (cursor addressing is margin-relative).
        const ORIGIN = 1 << 1;
    }

    /// Global screen modes, shared across the main and alternate buffers.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ScreenFlags: u8 {
        /// DECAWM — autowrap.
        const WRAP = 1 << 0;
        /// DECTCEM — cursor visible.
        const CURSOR_VISIBLE = 1 << 1;
        /// The alternate screen buffer is active (DEC 47/1046/1047/1049).
        const ALT_ACTIVE = 1 << 2;
    }
}

impl ScreenFlags {
    /// Power-on modes: DECAWM autowrap + DECTCEM cursor visible.
    pub const DEFAULT: Self = Self::WRAP.union(Self::CURSOR_VISIBLE);
}

/// Marker stored in the trailing cell of a double-width glyph. Render skips
/// it (the lead cell already accounts for both columns).
pub(crate) const CONTINUATION: char = '\0';

/// A single screen cell: glyph + SGR attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The base glyph. `' '` is blank; `CONTINUATION` marks the trailing
    /// cell of a double-width glyph.
    pub ch: char,
    /// SGR attributes (bold, italic, underline, …).
    pub sgr: SgrFlags,
    /// Foreground color (indexed or a `>= 258` truecolor palette id).
    pub fg: ColorId,
    /// Background color.
    pub bg: ColorId,
    /// Trailing zero-width or ZWJ-joined glyphs (combining marks, variation
    /// selectors, skin tones, emoji family members), inline up to eight.
    pub combining: [char; 8],
    /// Number of entries used in [`Cell::combining`].
    pub combining_len: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            sgr: SgrFlags::DEFAULT,
            fg: DEFAULT_FG_ID,
            bg: DEFAULT_BG_ID,
            combining: ['\0'; 8],
            combining_len: 0,
        }
    }
}

impl Cell {
    /// True when this cell is the trailing half of a double-width glyph.
    #[inline]
    pub const fn is_continuation(&self) -> bool {
        self.ch == CONTINUATION
    }

    /// The trailing combining marks / zero-width glyphs.
    #[inline]
    pub fn marks(&self) -> &[char] {
        &self.combining[..self.combining_len as usize]
    }

    /// True when the cell is blank (a space with default attributes).
    #[inline]
    pub const fn is_blank(&self) -> bool {
        self.ch == ' '
            && self.combining_len == 0
            && self.sgr.is_empty()
            && self.fg.0 == DEFAULT_FG_ID.0
            && self.bg.0 == DEFAULT_BG_ID.0
    }
}

/// Default DEC tab stops: every 8th column, starting at column 8 (0-based).
pub(crate) fn default_tabs(cols: u32) -> Vec<bool> {
    (0..cols).map(|c| c != 0 && c % 8 == 0).collect()
}

/// The classic xterm 16-color palette (the default theme).
pub(crate) fn default_palette() -> [u32; 16] {
    core::array::from_fn(|i| vtty_core::indexed_rgb(i as u8))
}
