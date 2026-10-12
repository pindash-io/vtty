// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Display cell width and emoji-sequence handling.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use vtty_emoji::{cell_width, cell_widths, cell_advance};
//!
//! cell_width('中');                    // Some(2)
//! cell_width('\u{1F3FB}');             // Some(0) — skin-tone modifier
//! cell_widths("⚠\u{FE0F}");            // [2, 0] — VS16 widens the base
//! cell_widths("👨\u{200D}👩\u{200D}👧");    // [2, 0, 0, 0, 0] — ZWJ family
//! cell_advance(1.6, 10.0);             // 20.0 — snap a fractional advance to 2 cells
//! ```
//!
//! Also: `cell_col_byte_widths`, `Properties` (packed emoji flags + width), the
//! `emoji` segmentation DFA (`scan`/`classify`), and `width_icu` (`icu` feature).
//!
//! ## Features
//!
//! `simd` + `baked` (default) | `icu` (runtime ICU4X lookups).
//!
//! ## Credits
//!
//! The emoji DFA is ported from [`chansen/c-emoji`](https://github.com/chansen/c-emoji) (MIT).

#![warn(missing_debug_implementations)]
#![deny(missing_docs)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "Terminal cell/pixel math truncates widths and coordinates to fixed-size types where the range is known-safe; gated behind the `icu` feature"
)]
#[cfg(feature = "baked")]
#[rustfmt::skip]
pub mod baked;
#[cfg(feature = "icu")]
mod icu;
/// Re-export the `parley_emoji` DFA + categories so callers don't depend
/// on `parley_emoji` directly.
pub use parley_emoji::{EmojiDFA, EmojiPresentationStyle, EmojiSegmentationCategory};

#[cfg(feature = "icu")]
pub use icu::width_icu;

/// Whether a zero-width mark may attach to `base` (UAX #29 GB9 + UTS #51):
/// a ZWJ needs an emoji ZWJ target, VS15/VS16 need an emoji base, and a
/// combining mark accepts any non-space base. The caller still checks that
/// the base is a real visible glyph (non-space, width ≥ 1).
#[inline]
pub fn mark_attaches(mark: char, base: char) -> bool {
    match mark {
        '\u{200D}' => is_zwj_target(base),
        '\u{FE0F}' | '\u{FE0E}' => is_vs_target(base),
        _ => true,
    }
}

/// Whether a char is a valid VS15/VS16 target — Ghostty's
/// `grapheme_break == .extended_pictographic`. Keycap bases (`0-9`, `#`, `*`)
/// carry the `Emoji` property but are NOT pictographs: a VS16 there is a
/// stray unless the full keycap sequence (`U+20E3`) follows.
#[inline]
pub fn is_vs_target(c: char) -> bool {
    Properties::get(c).is_emoji() && emoji_category(c) != EmojiSegmentationCategory::KeycapBase
}

/// Whether a zero-width `mark` is a STRAY for `base` — the inverse of
/// [`mark_attaches`] for the ZWJ/VS family. A combining mark is never stray
/// by this test (it just attaches, or is dropped for lack of a base).
#[inline]
pub fn mark_is_stray(mark: char, base: char) -> bool {
    matches!(mark, '\u{200D}' | '\u{FE0F}' | '\u{FE0E}') && !mark_attaches(mark, base)
}

/// Whether a char can be the TARGET of a preceding ZWJ (a ZWJ-sequence
/// continuation): the DFA's `Zwj` state advances on `Emoji`,
/// `Emoji_Presentation` or `Emoji_Modifier_Base` — a keycap base (`9`/`#`/`*`)
/// carries the `Emoji` property but REJECTS, so it is NOT a target.
#[inline]
pub fn is_zwj_target(c: char) -> bool {
    matches!(
        emoji_category(c),
        EmojiSegmentationCategory::Emoji
            | EmojiSegmentationCategory::EmojiPresentation
            | EmojiSegmentationCategory::EmojiModifierBase
    )
}

/// The parley `EmojiSegmentationCategory` for a char, from the baked (or ICU4X)
/// properties — the input to `parley_emoji`'s UTS #51 DFA.
#[inline]
fn emoji_category(c: char) -> EmojiSegmentationCategory {
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

/// Composite per-codepoint properties (parley-style): a single packed value
/// holding the emoji flags and the display width, so one lookup serves the
/// whole width/emoji pipeline.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Properties(u8);

impl Properties {
    const EMOJI: u8 = 1 << 0;
    const EMOJI_PRESENTATION: u8 = 1 << 1;
    const EMOJI_MODIFIER: u8 = 1 << 2;
    const MODIFIER_BASE: u8 = 1 << 3;
    const REGIONAL_INDICATOR: u8 = 1 << 4;
    const WIDTH_SHIFT: u8 = 5;
    const WIDTH_MASK: u8 = 0b11;

    /// Resolve the composite properties for a char — the baked table when
    /// `baked` is on, ICU4X runtime otherwise, degenerate width 1 when no
    /// backend is compiled in.
    #[inline]
    pub fn get(ch: char) -> Self {
        #[cfg(feature = "baked")]
        {
            Self(baked::composite_get(ch as u32))
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

    /// The display width (0/1/2).
    #[inline]
    pub const fn width(self) -> u8 {
        (self.0 >> Self::WIDTH_SHIFT) & Self::WIDTH_MASK
    }

    /// Build the composite from ICU4X runtime data (feature `icu`) — the
    /// runtime counterpart of the baked [`Properties::get`].
    #[cfg(feature = "icu")]
    pub fn from_icu(ch: char) -> Self {
        use icu_properties::CodePointSetData;
        use icu_properties::CodePointSetDataBorrowed;
        use icu_properties::props::{
            Emoji, EmojiModifier, EmojiModifierBase, EmojiPresentation, RegionalIndicator,
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
        }
        let mut b = 0_u8;
        if EMOJI.with(|p| p.contains(ch)) {
            b |= Self::EMOJI;
        }
        if PRES.with(|p| p.contains(ch)) {
            b |= Self::EMOJI_PRESENTATION;
        }
        if MOD.with(|p| p.contains(ch)) {
            b |= Self::EMOJI_MODIFIER;
        }
        if MOD_BASE.with(|p| p.contains(ch)) {
            b |= Self::MODIFIER_BASE;
        }
        if RI.with(|p| p.contains(ch)) {
            b |= Self::REGIONAL_INDICATOR;
        }
        b |= (width_icu(ch).unwrap_or(0) as u8) << Self::WIDTH_SHIFT;
        Self(b)
    }
}

/// Index of the first byte >= 0x80 (non-ASCII), or `bytes.len()` when the
/// whole slice is ASCII. With the `simd` feature this is a 16-byte-at-a-time
/// vectorized scan; otherwise a scalar fallback.
#[inline(always)]
fn first_non_ascii(bytes: &[u8]) -> usize {
    #[cfg(feature = "simd")]
    {
        simd::first_non_ascii(bytes)
    }
    #[cfg(not(feature = "simd"))]
    {
        bytes.iter().position(|&b| b >= 0x80).unwrap_or(bytes.len())
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
    fn scan<S: Simd>(simd: S, bytes: &[u8]) -> usize {
        let (chunks, tail) = bytes.as_chunks::<16>();
        for (i, chunk) in chunks.iter().enumerate() {
            let v = u8x16::from_slice(simd, chunk.as_slice());
            // Non-ASCII ⇔ b >= 0x80 (the high bit is set).
            let high = simd.simd_gt_u8x16(v, simd.splat_u8x16(0x7f));
            let mask = simd.to_bitmask_mask8x16(high);
            if mask != 0 {
                return i * 16 + mask.trailing_zeros() as usize;
            }
        }
        // Scalar tail (< 16 bytes).
        let off = bytes.len() - tail.len();
        tail.iter()
            .position(|&b| b >= 0x80)
            .map_or_else(|| bytes.len(), |p| off + p)
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

/// The display width a VS15/VS16 selector forces onto an emoji base: 2
/// (emoji presentation, VS16) or 1 (text presentation, VS15). Mirrors
/// [`EmojiDFA::presentation_style`]'s selector arms — the single source of
/// truth for which selector means which presentation.
#[inline]
pub fn vs_width(selector: char) -> Option<usize> {
    match selector {
        '\u{FE0F}' => Some(2),
        '\u{FE0E}' => Some(1),
        _ => None,
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
    if w <= 1 && p.is_emoji_presentation() {
        return Some(2);
    }
    Some(w)
}

/// A category that continues the current emoji run (zero cursor cells).
#[inline]
fn is_continuation(cat: EmojiSegmentationCategory) -> bool {
    use EmojiSegmentationCategory as C;
    matches!(
        cat,
        C::Vs15 | C::Vs16 | C::Zwj | C::KeycapEnd | C::TagSpec | C::TagEnd | C::EmojiModifier
    )
}

/// Whether a category can START an emoji sequence (a real base glyph).
#[inline]
fn is_emoji_starter(cat: EmojiSegmentationCategory) -> bool {
    use EmojiSegmentationCategory as C;
    matches!(
        cat,
        C::Emoji
            | C::EmojiPresentation
            | C::EmojiModifierBase
            | C::KeycapBase
            | C::TagBase
            | C::RegionalIndicator
    )
}

/// The terminal cell width of a run's leading char (1 or 2), from the
/// `parley_emoji` presentation — one column narrower for a bare keycap: a
/// keycap base is 2 cells only in the COMPLETE `base + VS16 + U+20E3`
/// sequence (the no-VS16 `1 U+20E3` and the incomplete `1 VS16` both stay
/// text-default 1 — Ghostty drops a VS16 after a keycap base because it is
/// not `Extended_Pictographic`).
#[inline]
fn run_width(dfa: &EmojiDFA, lead: char, lead_ep: bool, keycap_end: bool) -> usize {
    if emoji_category(lead) == EmojiSegmentationCategory::KeycapBase {
        return if dfa.has_vs() && keycap_end { 2 } else { 1 };
    }
    match dfa.presentation_style(lead_ep) {
        EmojiPresentationStyle::Emoji => 2,
        _ => 1,
    }
}

/// Per-char display cell widths, with emoji sequences handled by the UTS #51
/// [`EmojiDFA`]:
///
/// - a lone char keeps its width; a lone width-0 mark occupies one cell;
/// - a lone ZWJ is ignored (zero cells);
/// - a multi-char sequence is one glyph (2 cells), except text presentation
///   (VS15) and bare keycap stay 1; continuation chars are 0;
/// - the first variation selector locks the width.
///
/// Returns one entry per `char`, so callers can zip with `char_indices()`.
pub fn cell_widths(text: &str) -> Vec<usize> {
    // ASCII fast path: one cell per byte (no emoji sequence is pure ASCII).
    let ascii_len = first_non_ascii(text.as_bytes());
    if ascii_len == text.len() {
        return vec![1_usize; text.len()];
    }

    // Per-char base widths (the overrides in `cell_width` — skin tone = 0,
    // halfwidth dakuten = 1, tag chars = 0, emoji-presentation = 2).
    let chars: Vec<char> = text.chars().collect();
    let mut widths: Vec<usize> = chars.iter().map(|&c| cell_width(c).unwrap_or(0)).collect();

    // Greedy emoji runs: a base char + its zero-width continuations (VS / ZWJ /
    // keycap-end / tag / modifier) + a ZWJ's target. Each run is classified by
    // `parley_emoji`'s DFA and the leading char takes the presentation width.
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let lead = chars[i];
        let lead_cat = emoji_category(lead);
        let lead_is_emoji = is_emoji_starter(lead_cat);
        let lead_ep = Properties::get(lead).is_emoji_presentation();
        let mut dfa = EmojiDFA::new();
        let mut j = i;
        let mut prev_zwj = false;
        let mut ri_count = 0_u8;
        while j < n {
            let cat = emoji_category(chars[j]);
            if j > i {
                // Continuations only extend a run whose LEAD is an emoji
                // starter. A ZWJ/VS/modifier after a non-emoji is a STRAY and
                // must not swallow the next char (the DFA rejects it too).
                let cont = lead_is_emoji
                    && (is_continuation(cat)
                        || (prev_zwj && is_zwj_target(chars[j]))
                        // A flag is exactly TWO regional indicators.
                        || (cat == EmojiSegmentationCategory::RegionalIndicator
                            && ri_count == 1)
                        // A combining mark (Grapheme_Extend) is a grapheme
                        // continuation — the DFA's `Other` is a no-op, so a
                        // keycap/VS16 sequence resolves even with marks between
                        // the base and selector.
                        // the cell screen's `set_cell`, which widens the base
                        // regardless of intervening combining marks.
                        || cell_width(chars[j]) == Some(0));
                if !cont {
                    break;
                }
            }
            dfa.step_record(cat);
            if cat == EmojiSegmentationCategory::RegionalIndicator {
                ri_count += 1;
            } else {
                ri_count = 0;
            }
            prev_zwj = cat == EmojiSegmentationCategory::Zwj;
            j += 1;
        }
        // A lone char keeps its base `cell_width` (CJK 2, text emoji 1, …);
        // only a multi-char run with a real emoji base overrides it.
        if j > i + 1 && lead_is_emoji {
            let keycap_end = chars[i..j]
                .iter()
                .any(|&c| emoji_category(c) == EmojiSegmentationCategory::KeycapEnd);
            widths[i] = run_width(&dfa, lead, lead_ep, keycap_end);
        }
        widths[i + 1..j].fill(0);
        i = j;
    }

    // A lone width-0 mark with no base occupies its own cell; a lone ZWJ/VS15/
    // VS16 is invisible (no base to attach to), so it stays zero cells.
    let mut has_base = false;
    for (&c, w) in chars.iter().zip(widths.iter_mut()) {
        if *w == 0 && !has_base && !matches!(c, '\u{200D}' | '\u{FE0E}' | '\u{FE0F}') {
            *w = 1;
        }
        if *w > 0 {
            has_base = true;
        }
    }
    widths
}

/// Snap a font glyph advance to the terminal cell grid: wide glyphs
/// (`advance > 1.5` cells) occupy two cells, non-zero glyphs exactly one,
/// and zero-width glyphs (combining marks) none. The renderer and the screen
/// crates share this so glyph positions and cell backgrounds stay on the same
/// grid even when a proportional fallback font advances at a fractional cell.
#[inline]
pub const fn cell_advance(advance: f32, cell_w: f32) -> f32 {
    if advance > cell_w * 1.5 {
        2.0 * cell_w
    } else if advance > 0.0 {
        cell_w
    } else {
        0.0
    }
}

/// Byte offset (into `text`) of the char that STARTS at the given 0-based
/// CELL column, stepping over wide chars by their [`cell_widths`] widths —
/// including VS16 and ZWJ emoji merged to a single 2-cell unit. Zero-width
/// continuations (the VS16/ZWJ components) are skipped. Clamped to `text.len()`
/// when the column is past the last cell.
pub fn cell_col_byte_widths(text: &str, cell_col: usize) -> usize {
    // ASCII fast path: one cell per byte, no width table needed.
    if text.is_ascii() {
        return cell_col.min(text.len());
    }
    let widths = cell_widths(text);
    let mut cell = 0_usize;
    for ((i, _c), w) in text.char_indices().zip(widths.iter()) {
        // Return the char that STARTS at (or contains) `cell_col`. A column
        // landing in the SECOND cell of a wide char (cell_col == cell + 1)
        // must map to that same wide char, not skip past it to `text.len()`.
        if *w > 0 && cell_col >= cell && cell_col < cell + w {
            return i;
        }
        cell += w;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vs_width_forces_emoji_or_text_presentation() {
        // VS16 forces emoji (2 cells), VS15 forces text (1 cell).
        assert_eq!(vs_width('\u{FE0F}'), Some(2));
        assert_eq!(vs_width('\u{FE0E}'), Some(1));
        assert_eq!(vs_width('x'), None);
        assert_eq!(vs_width('\u{200D}'), None);
    }
    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn is_zero_width_covers_continuations_and_controls() {
        // Combining, VS15/VS16, ZWJ, skin tone, tags — grapheme continuations.
        assert!(is_zero_width('\u{301}'));
        assert!(is_zero_width('\u{FE0F}'));
        assert!(is_zero_width('\u{FE0E}'));
        assert!(is_zero_width('\u{200D}'));
        assert!(is_zero_width('\u{1F3FB}'));
        assert!(is_zero_width('\u{E0020}'));
        // Controls are zero-width too.
        assert!(is_zero_width('\n'));
        assert!(is_zero_width('\x1b'));
        // Narrow and wide glyphs are NOT zero-width.
        assert!(!is_zero_width('a'));
        assert!(!is_zero_width('中'));
        assert!(!is_zero_width('👍'));
    }
    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn cell_widths_merges_vs16_and_zwj_emoji() {
        // VS16 widens a width-1 text-default base to 2 and stays zero-width.
        assert_eq!(cell_widths("⚠\u{FE0F}x"), vec![2, 0, 1]);
        assert_eq!(cell_widths("❤\u{FE0F}x"), vec![2, 0, 1]);
        // VS16 after a plain LETTER is a defective sequence — the DFA rejects
        // the selector (W is not an emoji base), so it stays zero-width.
        assert_eq!(cell_widths("W\u{FE0F}"), vec![1, 0]);
        assert_eq!(cell_widths("W\u{FE0F}x"), vec![1, 0, 1]);
        // VS15 narrows a width-2 emoji to 1; the FIRST selector locks the width
        // (kitty: `▶+VS16+VS15` stays 2, `😐+VS15+VS16` stays 1).
        assert_eq!(cell_widths("😐\u{FE0E}x"), vec![1, 0, 1]);
        assert_eq!(cell_widths("▶\u{FE0F}\u{FE0E}"), vec![2, 0, 0]);
        assert_eq!(cell_widths("😐\u{FE0E}\u{FE0F}"), vec![1, 0, 0]);
        // A ZWJ family emoji is one 2-cell unit; the components contribute 0.
        assert_eq!(
            cell_widths("👨\u{200D}👩\u{200D}👧\u{200D}👦x"),
            vec![2, 0, 0, 0, 0, 0, 0, 1]
        );
        // Plain CJK is still per-char 2, ASCII per-char 1.
        assert_eq!(cell_widths("中ab"), vec![2, 1, 1]);
        // A skin-tone modifier stays zero-width, the base emoji stays 2.
        assert_eq!(cell_widths("👍🏽x"), vec![2, 0, 1]);
        // A stray ZWJ outside an emoji sequence must NOT widen the base.
        assert_eq!(cell_widths("a\u{200D}b"), vec![1, 0, 1]);
        // Regional-indicator pairs form ONE 2-cell flag; odd trailing RIs stay
        // lone (kitty: "🇮🇳" is one cell, "a🇮b" advances 4 cells).
        assert_eq!(cell_widths("🇮🇳"), vec![2, 0]);
        assert_eq!(cell_widths("🇮🇳🇺"), vec![2, 0, 2]);
        assert_eq!(cell_widths("a🇮b"), vec![1, 2, 1]);
        // ZWJ sequences with VS16 on one or both sides stay one 2-cell unit
        // (ratatui: "🐻❄️" and "👁️🗨️" are single 2-wide emoji).
        assert_eq!(cell_widths("🐻\u{200D}❄\u{FE0F}x"), vec![2, 0, 0, 0, 1]);
        assert_eq!(
            cell_widths("👁\u{FE0F}\u{200D}🗨\u{FE0F}x"),
            vec![2, 0, 0, 0, 0, 1]
        );
        // ASCII prefix + non-ASCII tail (exercises the SIMD ASCII split).
        assert_eq!(cell_widths("ab中😀c"), vec![1, 1, 2, 2, 1]);
        // Keycap sequence crossing the ASCII boundary: `1` + VS16 + U+20E3 is
        // one 2-cell keycap emoji (the no-VS16 form stays text-default 1; an
        // incomplete `1 VS16` without U+20E3 also stays 1 — the VS16 is a
        // stray on a keycap base).
        assert_eq!(cell_widths("1\u{FE0F}\u{20E3}"), vec![2, 0, 0]);
        assert_eq!(cell_widths("x1\u{FE0F}\u{20E3}y"), vec![1, 2, 0, 0, 1]);
        assert_eq!(cell_widths("1\u{20E3}"), vec![1, 0]);
        assert_eq!(cell_widths("1\u{FE0F}"), vec![1, 0]);
        // Tag characters and cancel tag contribute no cells.
        assert_eq!(
            cell_widths("🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}x"),
            vec![2, 0, 0, 0, 0, 0, 0, 1]
        );
    }
    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn lone_zwj_is_ignored() {
        // A standalone ZWJ is a degenerate joiner — zero cells (the cell grid
        // drops it), unlike a lone combining mark which occupies its own cell.
        assert_eq!(cell_widths("\u{200D}"), vec![0]);
        assert_eq!(cell_widths("\u{200D}\u{200D}"), vec![0, 0]);
        assert_eq!(cell_widths(" \u{200D}"), vec![1, 0]);
        // A lone VS15/VS16 is a presentation selector with no base — invisible
        // (zero cells), just like a lone ZWJ.
        assert_eq!(cell_widths("\u{FE0F}"), vec![0]);
        assert_eq!(cell_widths("\u{FE0E}"), vec![0]);
        assert_eq!(cell_widths("x\u{FE0F}"), vec![1, 0]);
        assert_eq!(cell_widths("x\u{FE0E}"), vec![1, 0]);
        // A lone combining mark still occupies its own cell.
        assert_eq!(cell_widths("\u{0301}"), vec![1]);
    }

    #[cfg(feature = "icu")]
    #[test]
    fn width_icu_agrees_with_unicode_width() {
        use unicode_width::UnicodeWidthChar;
        // Curated common cases must agree exactly.
        for c in [
            'a', '中', 'ｱ', 'ア', '👍', '\u{0301}', '\u{200D}', '\u{AC00}', '¡',
        ] {
            assert_eq!(
                width_icu(c),
                UnicodeWidthChar::width(c),
                "U+{:04X}",
                c as u32
            );
        }
        // The derivation matches `unicode-width` on every scalar except the 5
        // skin-tone modifiers (intentional: treated as combining, width 0) and
        // U+17D8 Khmer Sign Beyyal (width 3, unrepresentable in 0/1/2).
        let mut diffs = 0_u32;
        for cp in 0..=0x10FFFF_u32 {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            if width_icu(c) != UnicodeWidthChar::width(c) {
                diffs += 1;
            }
        }
        assert!(
            diffs <= 6,
            "ICU4X width derivation diverges from unicode-width for {diffs} codepoints"
        );
    }

    #[cfg(all(feature = "baked", feature = "icu"))]
    #[test]
    fn baked_width_agrees_with_width_icu() {
        // The baked composite width is the ICU4X (EAW + GC) derivation computed
        // at generation time — it must match `width_icu` exactly.
        for cp in 0..=0x10FFFF_u32 {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            assert_eq!(
                Properties::get(c).width(),
                width_icu(c).unwrap_or(0) as u8,
                "U+{cp:04X}"
            );
        }
    }

    #[test]
    fn cell_advance_snaps_to_grid() {
        let cw = 19.2;
        assert_eq!(cell_advance(0.0, cw), 0.0);
        assert_eq!(cell_advance(19.2, cw), cw);
        assert_eq!(cell_advance(25.0, cw), cw);
        assert_eq!(cell_advance(28.8, cw), cw);
        assert_eq!(cell_advance(28.9, cw), 2.0 * cw);
        assert_eq!(cell_advance(32.0, cw), 2.0 * cw);
    }
    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn cell_col_byte_widths_steps_over_merged_emoji() {
        // 中 is 2 cells, so cell 2 is the byte after it (3 bytes).
        assert_eq!(cell_col_byte_widths("中ab", 0), 0);
        assert_eq!(cell_col_byte_widths("中ab", 2), 3);
        // VS16 emoji merges to 2 cells: ⚠️ then 'x' — cell 2 is 'x' (byte 6).
        assert_eq!(cell_col_byte_widths("⚠\u{FE0F}x", 2), 6);
        // ZWJ family merges to 2 cells; cell 2 is 'x'.
        assert_eq!(
            cell_col_byte_widths("👨\u{200D}👩\u{200D}👧\u{200D}👦x", 2),
            "👨\u{200D}👩\u{200D}👧\u{200D}👦".len(),
        );
        // A column landing in the SECOND cell of a wide char maps back to
        // that wide char (not to the char after it).
        assert_eq!(cell_col_byte_widths("中ab", 1), 0);
        assert_eq!(cell_col_byte_widths("a中b", 2), 1);
        assert_eq!(cell_col_byte_widths("⚠\u{FE0F}x", 1), 0);
    }
    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn halfwidth_katakana_sound_marks_are_one_cell() {
        // U+FF9E/U+FF9F are Grapheme_Extend (unicode-width says 0), but
        // terminals render them as independent halfwidth chars (1 cell).
        // The combining variants U+3099/U+309A stay combining (0 cells).
        assert_eq!(cell_width('\u{FF9E}'), Some(1));
        assert_eq!(cell_width('\u{FF9F}'), Some(1));
        assert_eq!(cell_width('\u{3099}'), Some(0));
        assert_eq!(cell_width('\u{309A}'), Some(0));
        // Halfwidth katakana + dakuten renders as two halfwidth cells
        // (ratatui: "ｶﾞ" == 2), while fullwidth ka + combining dakuten
        // "が" stays one wide cell.
        assert_eq!(cell_widths("ｶﾞ"), vec![1, 1]);
        assert_eq!(cell_widths("か\u{3099}"), vec![2, 0]);
    }
}

#[test]
fn tui_symbols_are_single_cell() {
    // The box-drawing / braille / half-block / shade / arrow / bullet
    // symbols ratatui and every TUI app draws are all width 1.
    for c in [
        '─', '│', '┌', '┐', '└', '┘', '╭', '╮', '╰', '╯', '▀', '▄', '█', '▌', '▐', '░', '▒', '▓',
        '⠿', '⣿', '•', '●', '○', '←', '→', '↑', '↓',
    ] {
        assert_eq!(
            cell_width(c),
            Some(1),
            "U+{:04X} should be 1 cell",
            c as u32
        );
    }
    // Keycap / key symbols gitui uses for its keybinding hints.
    for c in ['⌥', '⌦', '⌫', '⎋', '⏎', '⇥', '⇧', '⇞', '▸', '▾', '✓', '…'] {
        assert_eq!(
            cell_width(c),
            Some(1),
            "U+{:04X} should be 1 cell",
            c as u32
        );
    }
}
#[cfg(any(feature = "baked", feature = "icu"))]
#[test]
fn starship_glyph_widths() {
    // The starship default-prompt glyph vocabulary: width-2 emoji and
    // width-1 symbols/text must measure correctly (drives prompt width).
    for (c, w) in [
        ('~', 1),
        ('❯', 1),
        ('❌', 2),
        ('🔒', 2),
        ('🔍', 2),
        ('🚫', 2),
        ('🧱', 2),
        ('⚡', 2),
        ('⇡', 1),
        ('⇣', 1),
        ('⇕', 1),
        ('✘', 1),
        ('»', 1),
        ('!', 1),
        ('+', 1),
        ('?', 1),
        ('=', 1),
        ('$', 1),
        ('…', 1),
        ('↕', 1),
        ('✦', 1),
        ('🐧', 2),
        ('🍎', 2),
        ('🐱', 2),
        ('λ', 1),
        ('∙', 1),
    ] {
        assert_eq!(cell_width(c), Some(w), "U+{:04X} {c:?}", c as u32);
    }
    // `↕` + VS16 (emoji presentation) widens to 2; alone it stays 1.
    assert_eq!(cell_widths("\u{2195}\u{FE0F}"), vec![2, 0]);
}
#[cfg(any(feature = "baked", feature = "icu"))]
#[test]
fn stray_zwj_after_non_emoji_does_not_swallow_next() {
    // A ZWJ after a CJK char is a STRAY joiner (DFA Start rejects it): the
    // emoji after it is a fresh 2-cell glyph, not a 0-width continuation.
    assert_eq!(cell_widths("侞\u{200D}\u{200D}👍"), vec![2, 0, 0, 2]);
    assert_eq!(
        cell_widths("👍侞\u{200D}\u{200D}👍侞"),
        vec![2, 2, 0, 0, 2, 2]
    );
    // A stray ZWJ after a plain ASCII letter behaves the same.
    assert_eq!(cell_widths("a\u{200D}👍"), vec![1, 0, 2]);
}
