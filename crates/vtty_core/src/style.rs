// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared style types (cursor shape, underline style, SGR flags).
//!
//! SGR (Select Graphic Rendition) follows ECMA-48, as documented in the DEC
//! manuals (EK-VT520-RM §5).

bitflags::bitflags! {
    /// SGR attribute state: bold/faint/italic/underline/strikethrough/
    /// overline/inverse/invisible/blink.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SgrFlags: u16 {
        /// SGR 4 underline.
        const UNDERLINE = 1 << 0;
        /// SGR 9 strikethrough.
        const STRIKETHROUGH = 1 << 1;
        /// SGR 53 overline.
        const OVERLINE = 1 << 2;
        /// SGR 1 bold.
        const BOLD = 1 << 3;
        /// SGR 3 italic.
        const ITALIC = 1 << 4;
        /// SGR 2 faint.
        const FAINT = 1 << 5;
        /// SGR 7 inverse video.
        const INVERSE = 1 << 6;
        /// SGR 8 invisible.
        const INVISIBLE = 1 << 7;
        /// SGR 5 slow blink.
        const BLINK = 1 << 8;
        /// SGR 6 rapid blink.
        const BLINK_RAPID = 1 << 9;
        /// Parse-time mark: an SGR "reset" code clears the carried flags
        /// rather than setting them (stripped before the flags reach state).
        const RESET = 1 << 10;
    }
}

impl SgrFlags {
    /// No attributes set — the default SGR state.
    pub const DEFAULT: Self = Self::empty();

    /// The decoration subset carried on a span/cell ([`StyleFlags`]).
    #[inline]
    pub const fn decorations(self) -> StyleFlags {
        // `as u8` drops the high bits (including the RESET mark), then
        // `from_bits_truncate` keeps the five decoration bits.
        StyleFlags::from_bits_truncate(self.bits() as u8)
    }
}

/// The flags an SGR code affects. "Reset" codes (22/23/24/25/27/28/29/55)
/// carry the [`SgrFlags::RESET`] mark so the caller can tell clear from set.
impl From<i32> for SgrFlags {
    fn from(code: i32) -> Self {
        match code {
            1 => Self::BOLD,
            2 => Self::FAINT,
            3 => Self::ITALIC,
            4 | 21 => Self::UNDERLINE,
            5 => Self::BLINK,
            6 => Self::BLINK_RAPID,
            7 => Self::INVERSE,
            8 => Self::INVISIBLE,
            9 => Self::STRIKETHROUGH,
            53 => Self::OVERLINE,
            // Reset codes: the flags to clear, marked with RESET.
            22 => Self::BOLD | Self::FAINT | Self::RESET,
            23 => Self::ITALIC | Self::RESET,
            24 => Self::UNDERLINE | Self::RESET,
            25 => Self::BLINK | Self::BLINK_RAPID | Self::RESET,
            27 => Self::INVERSE | Self::RESET,
            28 => Self::INVISIBLE | Self::RESET,
            29 => Self::STRIKETHROUGH | Self::RESET,
            55 => Self::OVERLINE | Self::RESET,
            _ => Self::DEFAULT,
        }
    }
}

bitflags::bitflags! {
    /// The five SGR decorations a screen stores per span/cell (underline /
    /// strikethrough / overline / bold / italic). The bit layout is the low
    /// five bits of [`SgrFlags`], so [`SgrFlags::decorations`] extracts them
    /// with one truncating cast.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StyleFlags: u8 {
        /// SGR 4 underline.
        const UNDERLINE = 1 << 0;
        /// SGR 9 strikethrough.
        const STRIKETHROUGH = 1 << 1;
        /// SGR 53 overline.
        const OVERLINE = 1 << 2;
        /// SGR 1 bold.
        const BOLD = 1 << 3;
        /// SGR 3 italic.
        const ITALIC = 1 << 4;
    }
}

/// Compile-time guard: `StyleFlags` is the low five bits of [`SgrFlags`], so
/// [`SgrFlags::decorations`] can extract it with a single truncating cast.
const _: () = {
    assert!(
        StyleFlags::UNDERLINE.bits() == SgrFlags::UNDERLINE.bits() as u8,
        "StyleFlags::UNDERLINE must mirror SgrFlags::UNDERLINE"
    );
    assert!(
        StyleFlags::STRIKETHROUGH.bits() == SgrFlags::STRIKETHROUGH.bits() as u8,
        "StyleFlags::STRIKETHROUGH must mirror SgrFlags::STRIKETHROUGH"
    );
    assert!(
        StyleFlags::OVERLINE.bits() == SgrFlags::OVERLINE.bits() as u8,
        "StyleFlags::OVERLINE must mirror SgrFlags::OVERLINE"
    );
    assert!(
        StyleFlags::BOLD.bits() == SgrFlags::BOLD.bits() as u8,
        "StyleFlags::BOLD must mirror SgrFlags::BOLD"
    );
    assert!(
        StyleFlags::ITALIC.bits() == SgrFlags::ITALIC.bits() as u8,
        "StyleFlags::ITALIC must mirror SgrFlags::ITALIC"
    );
};

/// SGR underline style (`4:0..5` / SGR 21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum UnderlineStyle {
    /// No underline (`SGR 24` / `4:0`).
    #[default]
    None = 0,
    /// A single underline (`SGR 4`).
    Single = 1,
    /// A double underline (`SGR 4:2` / `21`).
    Double = 2,
    /// A curly underline (`SGR 4:3`).
    Curly = 3,
    /// A dotted underline (`SGR 4:4`).
    Dotted = 4,
    /// A dashed underline (`SGR 4:5`).
    Dashed = 5,
}

impl From<u8> for UnderlineStyle {
    /// Map an SGR `4:<n>` sub-parameter to a style: `0` → [`Self::None`],
    /// `2..=5` → the specific style, anything else → [`Self::Single`] (the
    /// plain-`4` default).
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            2 => Self::Double,
            3 => Self::Curly,
            4 => Self::Dotted,
            5 => Self::Dashed,
            _ => Self::Single,
        }
    }
}

// ── Cursor style (DECSCUSR) ────────────────────────────────────────────

/// DECSCUSR cursor shape (`CSI Ps SP q`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
#[non_exhaustive]
pub enum CursorStyle {
    /// No cursor drawn (DECTCEM `CSI ?25l`, or an invalid DECSCUSR code).
    #[default]
    None = 0,
    /// Solid block covering the cell.
    Block = 1,
    /// Horizontal underline at the bottom of the cell.
    Underline = 2,
    /// Vertical bar (I-beam) at the left of the cell.
    Bar = 3,
}

impl CursorStyle {
    /// Parse a cursor-style name ("block"/"bar"/"underline"/"none"). Unknown → `Bar`.
    pub fn parse(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "block" => Self::Block,
            "bar" | "beam" | "vertical" => Self::Bar,
            "underline" | "underscore" => Self::Underline,
            "none" | "off" | "hidden" => Self::None,
            _ => Self::Bar,
        }
    }

    /// Map a DECSCUSR parameter to a shape; unsupported codes →
    /// [`Self::None`] (the caller leaves the cursor style unchanged —
    /// Ghostty ignores invalid values instead of hiding the cursor).
    #[inline]
    pub const fn from_decscusr(param: i32) -> Self {
        match param {
            0..=2 => Self::Block,
            3 | 4 => Self::Underline,
            5 | 6 => Self::Bar,
            _ => Self::None,
        }
    }

    /// Map an `OSC 50` `CursorShape=N` parameter (0 = block, 1 = beam,
    /// 2 = underline); unsupported codes → `None`.
    #[inline]
    pub const fn from_osc(param: i32) -> Self {
        match param {
            0 => Self::Block,
            1 => Self::Bar,
            2 => Self::Underline,
            _ => Self::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_set_code_maps_to_flag() {
        assert_eq!(SgrFlags::from(1), SgrFlags::BOLD);
        assert_eq!(SgrFlags::from(2), SgrFlags::FAINT);
        assert_eq!(SgrFlags::from(3), SgrFlags::ITALIC);
        assert_eq!(SgrFlags::from(4), SgrFlags::UNDERLINE);
        assert_eq!(SgrFlags::from(5), SgrFlags::BLINK);
        assert_eq!(SgrFlags::from(6), SgrFlags::BLINK_RAPID);
        assert_eq!(SgrFlags::from(7), SgrFlags::INVERSE);
        assert_eq!(SgrFlags::from(8), SgrFlags::INVISIBLE);
        assert_eq!(SgrFlags::from(9), SgrFlags::STRIKETHROUGH);
        assert_eq!(SgrFlags::from(21), SgrFlags::UNDERLINE);
        assert_eq!(SgrFlags::from(53), SgrFlags::OVERLINE);
        assert_eq!(SgrFlags::from(99), SgrFlags::DEFAULT);
    }

    #[test]
    fn sgr_reset_code_maps_to_flags() {
        // Reset codes carry the RESET mark so the caller can tell clear from set.
        assert_eq!(
            SgrFlags::from(22),
            SgrFlags::BOLD | SgrFlags::FAINT | SgrFlags::RESET
        );
        assert_eq!(SgrFlags::from(23), SgrFlags::ITALIC | SgrFlags::RESET);
        assert_eq!(SgrFlags::from(24), SgrFlags::UNDERLINE | SgrFlags::RESET);
        assert_eq!(
            SgrFlags::from(25),
            SgrFlags::BLINK | SgrFlags::BLINK_RAPID | SgrFlags::RESET
        );
        assert_eq!(SgrFlags::from(27), SgrFlags::INVERSE | SgrFlags::RESET);
        assert_eq!(SgrFlags::from(28), SgrFlags::INVISIBLE | SgrFlags::RESET);
        assert_eq!(
            SgrFlags::from(29),
            SgrFlags::STRIKETHROUGH | SgrFlags::RESET
        );
        assert_eq!(SgrFlags::from(55), SgrFlags::OVERLINE | SgrFlags::RESET);
        assert_eq!(SgrFlags::from(99), SgrFlags::DEFAULT);
    }

    #[test]
    fn decorations_extract_low_five_bits() {
        let all = SgrFlags::BOLD | SgrFlags::ITALIC | SgrFlags::UNDERLINE;
        assert_eq!(
            all.decorations(),
            StyleFlags::BOLD | StyleFlags::ITALIC | StyleFlags::UNDERLINE
        );

        // BLINK lives in bit 8 and must not leak into the u8 decoration subset.
        assert_eq!((SgrFlags::BLINK).decorations(), StyleFlags::empty());
    }

    #[test]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "decoration bits fit u8 by definition"
    )]
    fn style_flag_bits_mirror_sgr() {
        assert_eq!(
            StyleFlags::UNDERLINE.bits(),
            SgrFlags::UNDERLINE.bits() as u8
        );
        assert_eq!(
            StyleFlags::STRIKETHROUGH.bits(),
            SgrFlags::STRIKETHROUGH.bits() as u8
        );
        assert_eq!(StyleFlags::OVERLINE.bits(), SgrFlags::OVERLINE.bits() as u8);
        assert_eq!(StyleFlags::BOLD.bits(), SgrFlags::BOLD.bits() as u8);
        assert_eq!(StyleFlags::ITALIC.bits(), SgrFlags::ITALIC.bits() as u8);
    }

    #[test]
    fn cursor_style_from_decscusr() {
        assert_eq!(CursorStyle::from_decscusr(0), CursorStyle::Block);
        assert_eq!(CursorStyle::from_decscusr(2), CursorStyle::Block);
        assert_eq!(CursorStyle::from_decscusr(3), CursorStyle::Underline);
        assert_eq!(CursorStyle::from_decscusr(4), CursorStyle::Underline);
        assert_eq!(CursorStyle::from_decscusr(5), CursorStyle::Bar);
        assert_eq!(CursorStyle::from_decscusr(6), CursorStyle::Bar);
        // Unsupported codes are ignored by the caller (Ghostty leaves the
        // style unchanged), so the mapper returns `None`.
        assert_eq!(CursorStyle::from_decscusr(7), CursorStyle::None);
    }

    #[test]
    fn cursor_style_parse() {
        assert_eq!(CursorStyle::parse("block"), CursorStyle::Block);
        assert_eq!(CursorStyle::parse("BAR"), CursorStyle::Bar);
        assert_eq!(CursorStyle::parse("beam"), CursorStyle::Bar);
        assert_eq!(CursorStyle::parse("underline"), CursorStyle::Underline);
        assert_eq!(CursorStyle::parse("none"), CursorStyle::None);
        assert_eq!(CursorStyle::parse("hidden"), CursorStyle::None);
        assert_eq!(CursorStyle::parse("garbage"), CursorStyle::Bar);
    }

    #[test]
    fn underline_style_from_u8() {
        assert_eq!(UnderlineStyle::from(0), UnderlineStyle::None);
        assert_eq!(UnderlineStyle::from(1), UnderlineStyle::Single);
        assert_eq!(UnderlineStyle::from(2), UnderlineStyle::Double);
        assert_eq!(UnderlineStyle::from(3), UnderlineStyle::Curly);
        assert_eq!(UnderlineStyle::from(4), UnderlineStyle::Dotted);
        assert_eq!(UnderlineStyle::from(5), UnderlineStyle::Dashed);
        // Unknown sub-params default to single (plain `4`).
        assert_eq!(UnderlineStyle::from(9), UnderlineStyle::Single);
    }
}
