// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared style types (cursor shape, underline style, SGR flags).
//!
//! SGR (Select Graphic Rendition) follows ECMA-48, as documented in the DEC
//! manuals (EK-VT520-RM §5).

bitflags::bitflags! {
    /// SGR attribute state: bold/faint/italic/underline/strikethrough/
    /// overline/inverse/invisible/blink. The nine stored attributes occupy
    /// bits 0..8 in SGR-code order; [`Self::BLINK_RAPID`] and [`Self::RESET`]
    /// are parse-only marks above them.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SgrFlags: u16 {
        /// SGR 1 bold.
        const BOLD = 1 << 0;
        /// SGR 2 faint.
        const FAINT = 1 << 1;
        /// SGR 3 italic.
        const ITALIC = 1 << 2;
        /// SGR 4 underline.
        const UNDERLINE = 1 << 3;
        /// SGR 5 slow blink.
        const BLINK = 1 << 4;
        /// SGR 7 inverse video.
        const INVERSE = 1 << 5;
        /// SGR 8 invisible.
        const INVISIBLE = 1 << 6;
        /// SGR 9 strikethrough.
        const STRIKETHROUGH = 1 << 7;
        /// SGR 53 overline.
        const OVERLINE = 1 << 8;
        /// SGR 6 rapid blink (parse-only; screens store it as [`Self::BLINK`]).
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
    pub fn decorations(self) -> StyleFlags {
        let mut d = StyleFlags::empty();
        if self.contains(Self::BOLD) {
            d.insert(StyleFlags::BOLD);
        }
        if self.contains(Self::ITALIC) {
            d.insert(StyleFlags::ITALIC);
        }
        if self.contains(Self::UNDERLINE) {
            d.insert(StyleFlags::UNDERLINE);
        }
        if self.contains(Self::STRIKETHROUGH) {
            d.insert(StyleFlags::STRIKETHROUGH);
        }
        if self.contains(Self::OVERLINE) {
            d.insert(StyleFlags::OVERLINE);
        }
        d
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
    /// The five SGR decorations a screen stores per span/cell (bold /
    /// italic / underline / strikethrough / overline), bits ordered by SGR
    /// code.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StyleFlags: u8 {
        /// SGR 1 bold.
        const BOLD = 1 << 0;
        /// SGR 3 italic.
        const ITALIC = 1 << 1;
        /// SGR 4 underline.
        const UNDERLINE = 1 << 2;
        /// SGR 9 strikethrough.
        const STRIKETHROUGH = 1 << 3;
        /// SGR 53 overline.
        const OVERLINE = 1 << 4;
    }
}

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

impl UnderlineStyle {
    /// Decode a STORED underline-style code (0 = off, 1..=5 = a style) into a
    /// style, treating any out-of-range code as [`Self::None`]. The
    /// stored-field counterpart of the SGR sub-parameter decoder ([`From`]):
    /// a stored field holds only 0..=5 in practice, so an unknown code is the
    /// "no underline" fallback, not the SGR "plain `4` = single" default.
    #[inline]
    pub const fn from_stored(v: u8) -> Self {
        match v {
            1 => Self::Single,
            2 => Self::Double,
            3 => Self::Curly,
            4 => Self::Dotted,
            5 => Self::Dashed,
            _ => Self::None,
        }
    }
}

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

    /// Map a DECSCUSR parameter to a shape; unsupported codes → [`Self::None`]
    /// (the caller leaves the cursor style unchanged).
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

    /// Encode a cursor shape + blink flag into its DECSCUSR code — the
    /// inverse of [`Self::from_decscusr`]. The odd codes (1/3/5) blink, the
    /// even codes (2/4/6) are steady, and 0 is the default (steady block).
    #[inline]
    pub const fn to_decscusr(self, blink: bool) -> u8 {
        match (self, blink) {
            (Self::Block, true) => 1,
            (Self::Block, false) => 2,
            (Self::Underline, true) => 3,
            (Self::Underline, false) => 4,
            (Self::Bar, true) => 5,
            (Self::Bar, false) => 6,
            _ => 0,
        }
    }

    /// The DECSCUSR blink flag implied by a cursor-shape parameter: odd codes
    /// (1/3/5) blink, even codes (2/4/6) are steady, 0 is the default (steady),
    /// and unsupported codes (≥7) leave the blink state unchanged (`None`).
    #[inline]
    pub const fn decscusr_blink(param: i32) -> Option<bool> {
        match param {
            1 | 3 | 5 => Some(true),
            0 | 2 | 4 | 6 => Some(false),
            _ => None,
        }
    }
}

/// An unresolved SGR color (before palette interning).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorSpec {
    /// 39/49 — restore the default fg/bg.
    Default,
    /// 30..=37 / 90..=97 / 40..=47 / 100..=107 — the direct ANSI codes.
    /// The line screen maps 30..=37 to the bright half under bold (legacy).
    Ansi(u16),
    /// `38;5;n` / `48;5;n` / `58;5;n` — the extended indexed color (already
    /// clamped to 0..=255).
    Indexed(u16),
    /// `38;2;r;g;b` etc. — truecolor, packed 0xRRGGBB.
    Rgb(u32),
}

impl ColorSpec {
    /// Pack a truecolor triplet into an RGB color (`0x00RRGGBB`).
    #[inline]
    pub const fn rgb(r: i32, g: i32, b: i32) -> Self {
        Self::Rgb(crate::rgb(r, g, b))
    }

    /// Clamp an indexed color to the 256-color table and wrap it.
    #[inline]
    pub fn indexed(n: i32) -> Self {
        Self::Indexed(crate::ColorId::from(n).index())
    }
}

/// A single parsed SGR effect, in sequence order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SgrEffect {
    /// 0 — reset all attributes and colors.
    Reset,
    /// Turn attribute flags ON (bold/faint/…).
    Set(SgrFlags),
    /// Turn attribute flags OFF.
    Clear(SgrFlags),
    /// 4 / 4:n / 21 — set the underline style (`None` clears it).
    Underline(UnderlineStyle),
    /// 30..=37 / 39 / 38;… / 90..=97 — foreground.
    Fg(ColorSpec),
    /// 40..=47 / 49 / 48;… / 100..=107 — background.
    Bg(ColorSpec),
    /// 58;… — underline color.
    UnderlineColor(ColorSpec),
    /// 59 — clear the underline color.
    ClearUnderlineColor,
}

use alloc::string::ToString;

use smallvec::SmallVec;

use crate::event::param_colon_sep;

/// Parse an SGR sequence into its effects, appending to `out` (which the
/// caller reuses to avoid a per-sequence allocation). Colors stay unresolved
/// ([`ColorSpec`]) so each consumer interns them against its own palette.
pub fn parse_effects(params: &[i32], sep: u16, out: &mut SmallVec<SgrEffect, 8>) {
    if params.is_empty() {
        out.push(SgrEffect::Reset);
        return;
    }
    let mut iter = params.iter().enumerate();
    while let Some((i, &code)) = iter.next() {
        match code {
            // `4:0..5` is the colon-separated underline style; plain `4` is
            // single underline.
            4 => {
                let style = if param_colon_sep(sep, i) {
                    iter.next().map(|(_, &s)| UnderlineStyle::from(s as u8))
                } else {
                    None
                };
                out.push(SgrEffect::Underline(
                    style.unwrap_or(UnderlineStyle::Single),
                ));
            }
            // The extended colors (38/48/58) — the multi-param forms.
            38 | 48 | 58 => {
                let Some((c, skip)) = parse_extended(params, sep, i) else {
                    continue;
                };
                out.push(extended_effect(code, c));
                for _ in 0..skip {
                    iter.next();
                }
            }
            // Everything else — the single-code mapping.
            _ => out.extend(single_code(code)),
        }
    }
}

/// The effect of the common single-color / single-attribute SGR forms,
/// avoiding the effects `SmallVec` + iteration on the per-cell redraw hot
/// path. Returns `None` for anything else (colon forms, multi-param
/// sequences) — the caller falls back to [`parse_effects`].
#[inline]
pub fn single_effect(params: &[i32], sep: u16) -> Option<SgrEffect> {
    if sep != 0 {
        return None;
    }
    match params {
        [] => Some(SgrEffect::Reset),
        [code @ (38 | 48 | 58), 2, r, g, b] => {
            Some(extended_effect(*code, ColorSpec::rgb(*r, *g, *b)))
        }
        [code @ (38 | 48 | 58), 5, n] => Some(extended_effect(*code, ColorSpec::indexed(*n))),
        [code] => single_code(*code),
        _ => None,
    }
}

/// The common two-effect SGR form — one attribute code followed by one
/// extended color (`1;38;2;r;g;b`, `4;48;5;n`) — avoiding the effects
/// `SmallVec` + iteration on the bold+truecolor TUI hot path. Returns `None`
/// for anything else; the caller falls back to [`parse_effects`].
#[inline]
pub fn two_effects(params: &[i32], sep: u16) -> Option<(SgrEffect, SgrEffect)> {
    if sep != 0 {
        return None;
    }
    match params {
        [attr, code @ (38 | 48 | 58), 2, r, g, b] => Some((
            single_code(*attr)?,
            extended_effect(*code, ColorSpec::rgb(*r, *g, *b)),
        )),
        [attr, code @ (38 | 48 | 58), 5, n] => Some((
            single_code(*attr)?,
            extended_effect(*code, ColorSpec::indexed(*n)),
        )),
        _ => None,
    }
}

/// The SGR effect for an extended color code (`38` fg / `48` bg / `58`
/// underline), shared by the single-effect fast path and `parse_effects`.
#[inline]
const fn extended_effect(code: i32, spec: ColorSpec) -> SgrEffect {
    match code {
        38 => SgrEffect::Fg(spec),
        48 => SgrEffect::Bg(spec),
        _ => SgrEffect::UnderlineColor(spec),
    }
}

/// The effect of a single SGR attribute code.
#[inline]
pub fn single_code(code: i32) -> Option<SgrEffect> {
    match code {
        // `-2` is the parser's `;`-separated empty sentinel; in SGR an empty
        // (or `0`) parameter is a reset. `-1` (trailing/colon empty) is NOT a
        // reset — it is ignored by the callers.
        0 | -2 => Some(SgrEffect::Reset),
        4 => Some(SgrEffect::Underline(UnderlineStyle::Single)),
        21 => Some(SgrEffect::Underline(UnderlineStyle::Double)),
        30..=37 => Some(SgrEffect::Fg(ColorSpec::Ansi((code - 30) as u16))),
        39 => Some(SgrEffect::Fg(ColorSpec::Default)),
        40..=47 => Some(SgrEffect::Bg(ColorSpec::Ansi((code - 40) as u16))),
        49 => Some(SgrEffect::Bg(ColorSpec::Default)),
        59 => Some(SgrEffect::ClearUnderlineColor),
        90..=97 => Some(SgrEffect::Fg(ColorSpec::Ansi((code - 90 + 8) as u16))),
        100..=107 => Some(SgrEffect::Bg(ColorSpec::Ansi((code - 100 + 8) as u16))),
        _ => {
            let mut flags = SgrFlags::from(code);
            if flags.is_empty() {
                return None;
            }
            if flags.contains(SgrFlags::RESET) {
                flags.remove(SgrFlags::RESET);
                Some(SgrEffect::Clear(flags))
            } else {
                Some(SgrEffect::Set(flags))
            }
        }
    }
}

/// Parse an extended color (38/48/58): `X;5;n` / `X;2;r;g;b` and the colon
/// forms `X:5:n`, `X:2:r:g:b`, `X:2:cs:r:g:b` / `X:2::r:g:b` (the colorspace
/// is skipped — xterm reads the triplet as RGB, no CMY conversion).
/// Returns `(spec, skip)` where `skip` is the number of params AFTER `i`
/// consumed (so the caller does `i += skip`).
#[inline]
pub fn parse_extended(params: &[i32], sep: u16, i: usize) -> Option<(ColorSpec, usize)> {
    let len = params.len();
    let colon = param_colon_sep(sep, i);
    // A colon after the `g` slot (i + 4) marks the colorspace form.
    let cs = colon && param_colon_sep(sep, i + 4);
    match *params.get(i + 1)? {
        5 if len > i + 2 => {
            // Indexed colors clamp to the 256-color table (xterm).
            Some((ColorSpec::indexed(params[i + 2]), 2))
        }
        2 => {
            // `cs` skips the colorspace param at `i+2`, shifting the triplet
            // (and the required length) by one.
            let off = cs as usize;
            let need = i + 4 + off;
            if len <= need {
                return None;
            }
            let (r, g, b) = (
                params[i + 2 + off],
                params[i + 3 + off],
                params[i + 4 + off],
            );
            Some((ColorSpec::rgb(r, g, b), 4 + off))
        }
        _ => None,
    }
}

/// A raw SGR (`CSI … m`) attribute sequence: the parameter values plus the
/// colon-separator bitfield (bit `i` = parameter `i` followed by `:`).
///
/// Params are inline-allocated — the common 1–8 param SGR sequence never
/// touches the heap.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sgr {
    /// The SGR parameter values.
    pub params: SmallVec<i32, 8>,
    /// Bit `i` is set when parameter `i` was followed by a colon.
    pub sep: u16,
}

impl Sgr {
    /// Build an SGR sequence from raw CSI dispatch parts.
    pub const fn new(params: SmallVec<i32, 8>, sep: u16) -> Self {
        Self { params, sep }
    }

    /// Append the canonical `CSI … m` sequence to `out`, honouring the
    /// colon-separator bitfield.
    pub fn write_bytes<const N: usize>(&self, out: &mut SmallVec<u8, N>) {
        out.extend_from_slice_copy(b"\x1b[");
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 {
                let colon = i - 1 < 16 && self.sep & (1_u16 << (i - 1)) != 0;
                out.push(if colon { b':' } else { b';' });
            }
            out.extend_from_slice_copy(p.to_string().as_bytes());
        }
        out.push(b'm');
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

        // BLINK is not a decoration and must not leak into the subset.
        assert_eq!((SgrFlags::BLINK).decorations(), StyleFlags::empty());
    }

    #[test]
    fn decorations_map_each_flag_independently() {
        // The five decorations map to StyleFlags regardless of SgrFlags bit
        // positions (they are no longer a low-bit mirror).
        assert_eq!(SgrFlags::BOLD.decorations(), StyleFlags::BOLD);
        assert_eq!(SgrFlags::ITALIC.decorations(), StyleFlags::ITALIC);
        assert_eq!(SgrFlags::UNDERLINE.decorations(), StyleFlags::UNDERLINE);
        assert_eq!(
            SgrFlags::STRIKETHROUGH.decorations(),
            StyleFlags::STRIKETHROUGH
        );
        assert_eq!(SgrFlags::OVERLINE.decorations(), StyleFlags::OVERLINE);
        // Non-decoration flags map to empty.
        for f in [
            SgrFlags::FAINT,
            SgrFlags::BLINK,
            SgrFlags::BLINK_RAPID,
            SgrFlags::INVERSE,
            SgrFlags::INVISIBLE,
        ] {
            assert_eq!(f.decorations(), StyleFlags::empty());
        }
    }

    #[test]
    fn cursor_style_from_decscusr() {
        assert_eq!(CursorStyle::from_decscusr(0), CursorStyle::Block);
        assert_eq!(CursorStyle::from_decscusr(2), CursorStyle::Block);
        assert_eq!(CursorStyle::from_decscusr(3), CursorStyle::Underline);
        assert_eq!(CursorStyle::from_decscusr(4), CursorStyle::Underline);
        assert_eq!(CursorStyle::from_decscusr(5), CursorStyle::Bar);
        assert_eq!(CursorStyle::from_decscusr(6), CursorStyle::Bar);
        // Unsupported codes are ignored by the caller, so the mapper returns
        // `None`.
        assert_eq!(CursorStyle::from_decscusr(7), CursorStyle::None);
    }

    #[test]
    fn cursor_style_decscusr_round_trip_and_blink() {
        // For the explicit shape+blink codes (1..=6), decode then encode is
        // the identity. Code 0 is the "default" (a steady block, re-encoded
        // as 2) so it is excluded from the round trip.
        for code in 1..=6 {
            let shape = CursorStyle::from_decscusr(code);
            assert_eq!(
                CursorStyle::decscusr_blink(code).map(|blink| shape.to_decscusr(blink)),
                Some(code as u8),
            );
        }
        // Odd codes blink, even codes are steady, unsupported → None.
        assert_eq!(CursorStyle::decscusr_blink(1), Some(true));
        assert_eq!(CursorStyle::decscusr_blink(4), Some(false));
        assert_eq!(CursorStyle::decscusr_blink(7), None);
        // An unknown shape encodes to the default 0.
        assert_eq!(CursorStyle::None.to_decscusr(false), 0);
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

    #[test]
    fn underline_style_from_stored_differs_from_wire() {
        // The stored-field decoder treats an out-of-range code as None (the
        // stored field is never corrupt in practice), unlike the wire decoder
        // which defaults an unknown sub-param to Single.
        assert_eq!(UnderlineStyle::from_stored(9), UnderlineStyle::None);
        // The valid codes agree.
        for v in 0..=5 {
            assert_eq!(
                UnderlineStyle::from_stored(v),
                UnderlineStyle::from(v),
                "code {v}"
            );
        }
    }

    #[test]
    fn sgr_parse_effects_reset_and_attrs() {
        let mut out = SmallVec::new();
        parse_effects(&[], 0, &mut out);
        assert_eq!(out.as_ref(), &[SgrEffect::Reset]);

        out.clear();
        parse_effects(&[1, 4], 0, &mut out);
        assert_eq!(
            out.as_ref(),
            &[
                SgrEffect::Set(SgrFlags::BOLD),
                SgrEffect::Underline(UnderlineStyle::Single),
            ]
        );
    }

    #[test]
    fn sgr_parse_effects_extended_rgb() {
        let mut out = SmallVec::new();
        parse_effects(&[38, 2, 255, 0, 128], 0, &mut out);
        assert_eq!(out.as_ref(), &[SgrEffect::Fg(ColorSpec::rgb(255, 0, 128))]);
    }

    #[test]
    fn sgr_parse_effects_colon_underline_and_colorspace() {
        // `4:2` — double underline via the colon form.
        let mut out = SmallVec::new();
        parse_effects(&[4, 2], 0b01, &mut out);
        assert_eq!(
            out.as_ref(),
            &[SgrEffect::Underline(UnderlineStyle::Double)]
        );

        // `38:2::r:g:b` — the colorspace slot is skipped (`::`); the colon
        // after the `g` slot (bit 4) marks the colorspace form.
        out.clear();
        parse_effects(&[38, 2, 0, 1, 2, 3], 0b11111, &mut out);
        assert_eq!(out.as_ref(), &[SgrEffect::Fg(ColorSpec::rgb(1, 2, 3))]);
    }

    #[test]
    fn sgr_single_effect_hot_path() {
        assert_eq!(
            single_effect(&[38, 2, 1, 2, 3], 0),
            Some(SgrEffect::Fg(ColorSpec::rgb(1, 2, 3)))
        );
        assert_eq!(
            single_effect(&[48, 5, 196], 0),
            Some(SgrEffect::Bg(ColorSpec::indexed(196)))
        );
        assert_eq!(single_effect(&[1], 0), Some(SgrEffect::Set(SgrFlags::BOLD)));
        // Colon-separated forms fall back to `parse_effects`.
        assert_eq!(single_effect(&[4, 2], 0b01), None);
        assert_eq!(single_effect(&[1, 2], 0), None);
    }

    #[test]
    fn sgr_encode_round_trips_canonical_bytes() {
        let mut out = SmallVec::<u8, 16>::new();
        Sgr::new(SmallVec::from([1, 31]), 0).write_bytes(&mut out);
        assert_eq!(out.as_ref(), b"\x1b[1;31m");

        // Colon-separated form: `4:2` underlines double.
        out.clear();
        Sgr::new(SmallVec::from([4, 2]), 0b01).write_bytes(&mut out);
        assert_eq!(out.as_ref(), b"\x1b[4:2m");

        out.clear();
        Sgr::default().write_bytes(&mut out);
        assert_eq!(out.as_ref(), b"\x1b[m");
    }

    #[test]
    fn single_code_maps_all_sgr_codes() {
        assert_eq!(single_code(0), Some(SgrEffect::Reset));
        assert_eq!(
            single_code(4),
            Some(SgrEffect::Underline(UnderlineStyle::Single))
        );
        assert_eq!(
            single_code(21),
            Some(SgrEffect::Underline(UnderlineStyle::Double))
        );
        assert_eq!(single_code(31), Some(SgrEffect::Fg(ColorSpec::Ansi(1))));
        assert_eq!(single_code(39), Some(SgrEffect::Fg(ColorSpec::Default)));
        assert_eq!(single_code(44), Some(SgrEffect::Bg(ColorSpec::Ansi(4))));
        assert_eq!(single_code(49), Some(SgrEffect::Bg(ColorSpec::Default)));
        assert_eq!(single_code(59), Some(SgrEffect::ClearUnderlineColor));
        assert_eq!(single_code(92), Some(SgrEffect::Fg(ColorSpec::Ansi(10))));
        assert_eq!(single_code(103), Some(SgrEffect::Bg(ColorSpec::Ansi(11))));
        assert_eq!(single_code(1), Some(SgrEffect::Set(SgrFlags::BOLD)));
        assert_eq!(
            single_code(22),
            Some(SgrEffect::Clear(SgrFlags::BOLD | SgrFlags::FAINT))
        );
        assert_eq!(single_code(99), None);
    }

    #[test]
    fn cursor_style_from_osc() {
        assert_eq!(CursorStyle::from_osc(0), CursorStyle::Block);
        assert_eq!(CursorStyle::from_osc(1), CursorStyle::Bar);
        assert_eq!(CursorStyle::from_osc(2), CursorStyle::Underline);
        assert_eq!(CursorStyle::from_osc(9), CursorStyle::None);
    }

    #[test]
    fn parse_extended_and_effects_edges() {
        assert_eq!(
            parse_extended(&[38, 5, 196], 0, 0),
            Some((ColorSpec::Indexed(196), 2))
        );
        // RGB with too-few params is rejected.
        assert_eq!(parse_extended(&[38, 2, 255, 0], 0, 0), None);
        // An unknown sub-param is rejected.
        assert_eq!(parse_extended(&[38, 3, 255], 0, 0), None);

        // parse_effects skips a malformed extended color (38;2 with too few
        // params), then continues with the remaining codes.
        let mut out = SmallVec::<SgrEffect, 8>::new();
        parse_effects(&[38, 2, 255, 0], 0, &mut out);
        assert_eq!(
            out.as_ref(),
            &[SgrEffect::Set(SgrFlags::FAINT), SgrEffect::Reset]
        );

        // Underline color (58) maps to UnderlineColor.
        let mut out = SmallVec::<SgrEffect, 8>::new();
        parse_effects(&[58, 5, 196], 0, &mut out);
        assert_eq!(
            out.as_ref(),
            &[SgrEffect::UnderlineColor(ColorSpec::Indexed(196))]
        );
    }
}
