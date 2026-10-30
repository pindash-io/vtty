// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Per-codepoint properties and single-char width — the lookup layer the
//! emoji segmentation ([`crate::emoji`]) and the width model
//! ([`crate::width`]) build on.
//!
//! [`Properties`] packs the parley emoji flags plus the display width into a
//! single byte (baked table or ICU4X runtime); [`cell_width`], [`vs_width`]
//! and [`is_zero_width`] are the single-char width predicates.

use parley_emoji::EmojiSegmentationCategory;

/// Composite per-codepoint properties (parley-style): a single packed value
/// holding the emoji flags and the display width, so one lookup serves the
/// whole width/emoji pipeline.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Properties(u16);

impl Properties {
    const EMOJI: u16 = 1 << 0;
    const EMOJI_PRESENTATION: u16 = 1 << 1;
    const EMOJI_MODIFIER: u16 = 1 << 2;
    const MODIFIER_BASE: u16 = 1 << 3;
    const REGIONAL_INDICATOR: u16 = 1 << 4;
    const EXTENDED_PICTOGRAPHIC: u16 = 1 << 7;
    const AMBIGUOUS: u16 = 1 << 8;
    const WIDTH_SHIFT: u16 = 5;
    const WIDTH_MASK: u16 = 0b11;

    /// Resolve the composite properties for a char — the baked table when
    /// `baked` is on, ICU4X runtime otherwise, degenerate width 1 when no
    /// backend is compiled in.
    #[inline]
    pub fn get(ch: char) -> Self {
        #[cfg(feature = "baked")]
        {
            Self(crate::baked::composite_get(ch as u32))
        }
        #[cfg(all(not(feature = "baked"), feature = "icu"))]
        {
            Self::from_icu(ch)
        }
        #[cfg(not(any(feature = "baked", feature = "icu")))]
        {
            let _ = ch;
            Self(1 << Self::WIDTH_SHIFT)
        }
    }

    /// The `Emoji` binary property.
    #[inline]
    pub const fn is_emoji(self) -> bool {
        self.0 & Self::EMOJI != 0
    }

    /// The `Emoji_Modifier_Base` binary property.
    #[inline]
    pub const fn is_modifier_base(self) -> bool {
        self.0 & Self::MODIFIER_BASE != 0
    }

    /// The `Emoji_Presentation` binary property.
    #[inline]
    pub const fn is_emoji_presentation(self) -> bool {
        self.0 & Self::EMOJI_PRESENTATION != 0
    }

    /// The `Emoji_Modifier` binary property (Fitzpatrick skin tone).
    #[inline]
    pub const fn is_emoji_modifier(self) -> bool {
        self.0 & Self::EMOJI_MODIFIER != 0
    }

    /// The `Regional_Indicator` binary property.
    #[inline]
    pub const fn is_regional_indicator(self) -> bool {
        self.0 & Self::REGIONAL_INDICATOR != 0
    }

    /// The `Extended_Pictographic` binary property (includes the reserved
    /// emoji ranges — the VS15/VS16 base predicate).
    #[inline]
    pub const fn is_extended_pictographic(self) -> bool {
        self.0 & Self::EXTENDED_PICTOGRAPHIC != 0
    }

    /// The display width (0/1/2).
    #[inline]
    pub const fn width(self) -> u8 {
        ((self.0 >> Self::WIDTH_SHIFT) & Self::WIDTH_MASK) as u8
    }

    /// East Asian Width "Ambiguous" (A) — 1 cell by default, 2 cells under
    /// CJK width mode.
    #[inline]
    pub const fn is_ambiguous(self) -> bool {
        self.0 & Self::AMBIGUOUS != 0
    }

    /// Build the composite from ICU4X runtime data (feature `icu`) — the
    /// runtime counterpart of the baked [`Properties::get`].
    #[cfg(feature = "icu")]
    pub fn from_icu(ch: char) -> Self {
        use icu_properties::CodePointSetData;
        use icu_properties::CodePointSetDataBorrowed;
        use icu_properties::props::{
            Emoji, EmojiModifier, EmojiModifierBase, EmojiPresentation, ExtendedPictographic,
            RegionalIndicator,
        };
        thread_local! {
            static EMOJI: CodePointSetDataBorrowed<'static> =
                const { CodePointSetData::new::<Emoji>() };
            static PRES: CodePointSetDataBorrowed<'static> =
                const { CodePointSetData::new::<EmojiPresentation>() };
            static MOD: CodePointSetDataBorrowed<'static> =
                const { CodePointSetData::new::<EmojiModifier>() };
            static MOD_BASE: CodePointSetDataBorrowed<'static> =
                const { CodePointSetData::new::<EmojiModifierBase>() };
            static RI: CodePointSetDataBorrowed<'static> =
                const { CodePointSetData::new::<RegionalIndicator>() };
            static EXT_PICT: CodePointSetDataBorrowed<'static> =
                const { CodePointSetData::new::<ExtendedPictographic>() };
        }
        let mut b = 0_u16;
        macro_rules! set_flag {
            ($set:ident => $flag:ident) => {
                if $set.with(|p| p.contains(ch)) {
                    b |= Self::$flag;
                }
            };
        }
        set_flag!(EMOJI => EMOJI);
        set_flag!(PRES => EMOJI_PRESENTATION);
        set_flag!(MOD => EMOJI_MODIFIER);
        set_flag!(MOD_BASE => MODIFIER_BASE);
        set_flag!(RI => REGIONAL_INDICATOR);
        set_flag!(EXT_PICT => EXTENDED_PICTOGRAPHIC);
        if crate::icu::ambiguous_icu(ch) {
            b |= Self::AMBIGUOUS;
        }
        b |= (crate::icu::width_icu(ch) as u16) << Self::WIDTH_SHIFT;
        Self(b)
    }
}

/// The parley `EmojiSegmentationCategory` for a char, from the baked (or
/// ICU4X) properties — the input to `parley_emoji`'s UTS #51 DFA.
#[inline]
pub(crate) fn emoji_category(c: char) -> EmojiSegmentationCategory {
    let p = Properties::get(c);
    EmojiSegmentationCategory::from_codepoint(
        c as u32,
        p.is_emoji(),
        p.is_emoji_presentation(),
        p.is_emoji_modifier(),
        p.is_modifier_base(),
        p.is_regional_indicator(),
    )
}

/// Grapheme cluster break property (UAX #29), reduced to the symbols the
/// classic GB rules need, with `Extended_Pictographic` folded in as its own
/// symbol for GB11.
///
/// The emoji-specific GCB values (`E_Base`, `E_Base_GAZ`, `E_Modifier`,
/// `Glue_After_Zwj`) are folded into [`Gcb::Other`] — emoji sequences are
/// handled by [`crate::emoji`]'s DFA.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Gcb {
    /// Other (including the emoji-specific GCB values).
    Other = 0,
    /// `Control` (includes newlines other than CR/LF).
    Control = 1,
    /// Carriage return.
    CR = 2,
    /// Line feed.
    LF = 3,
    /// `Extend` (combining marks, variation selectors, ZWJ-adjacent).
    Extend = 4,
    /// Zero-width joiner.
    ZWJ = 5,
    /// Regional indicator (flag emoji half).
    RegionalIndicator = 6,
    /// `Prepend`.
    Prepend = 7,
    /// `SpacingMark`.
    SpacingMark = 8,
    /// Hangul leading jamo `L`.
    L = 9,
    /// Hangul vowel jamo `V`.
    V = 10,
    /// Hangul trailing jamo `T`.
    T = 11,
    /// Hangul syllable `LV`.
    LV = 12,
    /// Hangul syllable `LVT`.
    LVT = 13,
    /// `Extended_Pictographic` (GB11).
    ExtPict = 14,
}

impl Gcb {
    /// The grapheme cluster break property of `c`.
    #[inline]
    pub fn get(c: char) -> Self {
        #[cfg(feature = "baked")]
        {
            Self::from_u8(crate::baked::gcb_get(c as u32))
        }
        #[cfg(all(not(feature = "baked"), feature = "icu"))]
        {
            crate::icu::gcb_icu(c)
        }
        #[cfg(not(any(feature = "baked", feature = "icu")))]
        {
            let _ = c;
            Self::Other
        }
    }

    /// Whether this symbol is a C0 control, CR, or LF (GB4/GB5).
    #[inline]
    pub const fn is_control(self) -> bool {
        matches!(self, Self::Control | Self::CR | Self::LF)
    }

    #[cfg(feature = "baked")]
    #[inline]
    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Control,
            2 => Self::CR,
            3 => Self::LF,
            4 => Self::Extend,
            5 => Self::ZWJ,
            6 => Self::RegionalIndicator,
            7 => Self::Prepend,
            8 => Self::SpacingMark,
            9 => Self::L,
            10 => Self::V,
            11 => Self::T,
            12 => Self::LV,
            13 => Self::LVT,
            14 => Self::ExtPict,
            _ => Self::Other,
        }
    }
}

/// Index of the first byte >= 0x80 (non-ASCII), or `bytes.len()` when the
/// whole slice is ASCII. With the `simd` feature this is a 16-byte-at-a-time
/// vectorized scan; otherwise a scalar fallback.
#[inline(always)]
pub fn first_non_ascii(bytes: &[u8]) -> usize {
    #[cfg(feature = "simd")]
    {
        simd::first_non_ascii(bytes)
    }
    #[cfg(not(feature = "simd"))]
    {
        bytes.iter().position(|&b| b >= 0x80).unwrap_or(bytes.len())
    }
}

/// Index of the first byte that is NOT printable ASCII (`0x20..=0x7e`), or
/// `bytes.len()` when the whole slice is printable ASCII. This is the
/// all-ASCII fast-path predicate for grapheme clustering: printable ASCII is
/// the only input where every byte is its own cluster.
#[inline(always)]
pub fn first_non_printable_ascii(bytes: &[u8]) -> usize {
    #[cfg(feature = "simd")]
    {
        simd::first_non_printable_ascii(bytes)
    }
    #[cfg(not(feature = "simd"))]
    {
        bytes
            .iter()
            .position(|&b| !(0x20..=0x7e).contains(&b))
            .unwrap_or(bytes.len())
    }
}

#[cfg(feature = "simd")]
mod simd {
    use std::sync::OnceLock;

    use fearless_simd::{Level, Simd, dispatch, prelude::*, u8x16};

    static LEVEL: OnceLock<Level> = OnceLock::new();

    #[inline(always)]
    pub(crate) fn first_non_ascii(bytes: &[u8]) -> usize {
        let level = *LEVEL.get_or_init(Level::new);
        dispatch!(level, simd => scan(simd, bytes))
    }

    #[inline(always)]
    pub(crate) fn first_non_printable_ascii(bytes: &[u8]) -> usize {
        let level = *LEVEL.get_or_init(Level::new);
        dispatch!(level, simd => scan_printable(simd, bytes))
    }

    /// Shared SIMD scan: 16-byte chunks via `stop_mask(simd, v) -> u64`, then
    /// a scalar `is_stop` tail. Returns the first stop index, or `bytes.len()`.
    #[inline(always)]
    fn scan_first<S: Simd>(
        simd: S,
        bytes: &[u8],
        stop_mask: impl Fn(S, u8x16<S>) -> u64,
        is_stop: impl Fn(u8) -> bool,
    ) -> usize {
        let (chunks, tail) = bytes.as_chunks::<16>();
        for (i, chunk) in chunks.iter().enumerate() {
            let v = u8x16::from_slice(simd, chunk.as_slice());
            let mask = stop_mask(simd, v);
            if mask != 0 {
                return i * 16 + mask.trailing_zeros() as usize;
            }
        }
        // Scalar tail (< 16 bytes).
        let off = bytes.len() - tail.len();
        tail.iter()
            .position(|&b| is_stop(b))
            .map_or_else(|| bytes.len(), |p| off + p)
    }

    /// Non-ASCII ⇔ b >= 0x80 (the high bit is set).
    #[inline(always)]
    fn scan<S: Simd>(simd: S, bytes: &[u8]) -> usize {
        scan_first(
            simd,
            bytes,
            |simd, v| {
                let high = simd.simd_gt_u8x16(v, simd.splat_u8x16(0x7f));
                simd.to_bitmask_mask8x16(high)
            },
            |b| b >= 0x80,
        )
    }

    /// Non-printable ASCII ⇔ b < 0x20 || b >= 0x7f.
    #[inline(always)]
    fn scan_printable<S: Simd>(simd: S, bytes: &[u8]) -> usize {
        scan_first(
            simd,
            bytes,
            |simd, v| {
                let low = simd.simd_lt_u8x16(v, simd.splat_u8x16(0x20));
                let high = simd.simd_ge_u8x16(v, simd.splat_u8x16(0x7f));
                simd.to_bitmask_mask8x16(low | high)
            },
            |b| !(0x20..=0x7e).contains(&b),
        )
    }
}

/// Whether a char is a zero-width grapheme continuation in the terminal
/// grid: a combining mark, VS15/VS16, ZWJ, skin-tone modifier, or tag char.
/// These occupy no cell of their own (they extend the preceding cluster).
/// A control char is also zero-width (`cell_width` is `None`), so this is the
/// single "does it take a cell" predicate.
#[inline]
pub fn is_zero_width(c: char) -> bool {
    !matches!(cell_width(c), Some(1) | Some(2))
}

/// Whether `base` + `mark` would be composed by the shaper into a precomposed
/// glyph (canonical NFC composition), so the renderer must insert a grapheme
/// break (ZWSP) to keep the mark's separate glyph.
///
/// Scoped to the combining diacritical marks U+0300..=U+036F — the block
/// where the `ccmp` precomposed composition happens (e + U+0301 → é). Emoji
/// selectors, skin tones, and Hangul Jamo are excluded: they compose through
/// other shaping features (VS/ZWJ/algorithmic Hangul), not `ccmp`.
#[inline]
pub fn composes_precomposed(base: char, mark: char) -> bool {
    ('\u{0300}'..='\u{036F}').contains(&mark)
        && icu_normalizer::properties::CanonicalComposition::new()
            .compose(base, mark)
            .is_some()
}

/// The display width a VS15/VS16 selector forces onto an emoji base: 2
/// (emoji presentation, VS16), 1 (text presentation, VS15), or 0 (not a
/// variation selector). Mirrors [`crate::EmojiDFA::presentation_style`]'s
/// selector arms — the single source of truth for which selector means which
/// presentation.
#[inline]
pub fn vs_width(selector: char) -> usize {
    match selector {
        '\u{FE0F}' => 2,
        '\u{FE0E}' => 1,
        _ => 0,
    }
}

/// Display cell width of a char: `None` (control), `Some(0)` (combining),
/// `Some(1)`/`Some(2)` (printable). Resolves ASCII, terminal overrides
/// (skin tones, halfwidth katakana, tags), the baked composite width, then
/// ICU4X `Emoji_Presentation` (width-1 emoji → 2). The broader `Emoji`
/// property is deliberately unused (it covers text-default symbols the mono
/// font owns at one cell).
#[inline]
pub fn cell_width(c: char) -> Option<usize> {
    // ASCII fast path: printable = 1 cell, control = no width.
    if c.is_ascii() {
        return if c.is_ascii_control() { None } else { Some(1) };
    }

    // Terminal width overrides — where the Unicode tables disagree with the
    // terminal cell grid:
    match c {
        // Skin-tone modifiers (U+1F3FB..=U+1F3FF): zero-width Extend chars
        // that combine with the preceding emoji; Unicode reports them wide.
        '\u{1F3FB}'..='\u{1F3FF}' => return Some(0),
        // Halfwidth katakana dakuten/handakuten (U+FF9E/U+FF9F): independent
        // halfwidth chars (1 cell); Grapheme_Extend makes them zero in
        // Unicode.
        '\u{FF9E}' | '\u{FF9F}' => return Some(1),
        // Tag chars + cancel tag (U+E0020..=U+E007F): only appear inside a
        // tag-emoji sequence, so they contribute no cells.
        '\u{E0020}'..='\u{E007F}' => return Some(0),
        _ => {}
    }

    let p = Properties::get(c);
    let w = p.width() as usize;
    // Emoji_Presentation: a width-1 emoji-presentation char becomes 2 cells.
    // Text-default emoji like ❤ U+2764 (Emoji=Yes, Emoji_Presentation=No)
    // stay ONE cell; only VS16 or a ZWJ-family membership widens them (the
    // DFA's `presentation_style(Emoji)` handles that via `lead_ep`).
    if w <= 1 && p.is_emoji_presentation() {
        return Some(2);
    }
    // CJK width mode: East Asian Width "Ambiguous" (A) becomes 2 cells,
    // matching wcwidth under a CJK locale / Ghostty's `cjk_width`.
    if w == 1 && p.is_ambiguous() && crate::cjk_width() {
        return Some(2);
    }
    Some(w)
}
