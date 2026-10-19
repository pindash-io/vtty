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
//! ## API
//!
//! - **Width** — `cell_width`, `cell_widths`, `cell_width_sum`,
//!   `cell_col_byte_widths`, `cell_advance`.
//! - **Predicates** — `vs_width`, `is_zero_width`.
//! - **Emoji segmentation** — `mark_attaches`, `mark_is_stray`, `is_vs_base`,
//!   `is_vs_target`, `is_zwj_target`.
//! - **`Properties`** — packed emoji flags + width in one lookup.
//!
//! ## Features
//!
//! `simd` + `baked` (default) | `icu` (runtime ICU4X lookups).
//!
//! ## Credits
//!
//! The emoji DFA is provided by [`parley_emoji`](https://github.com/linebender/parley) (MIT).

#![warn(missing_debug_implementations)]
#![deny(missing_docs)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "Terminal cell/pixel math truncates widths and coordinates to fixed-size types where the range is known-safe; gated behind the `icu` feature"
)]
#[cfg(feature = "baked")]
#[rustfmt::skip]
pub mod baked;
pub mod emoji;
#[cfg(feature = "icu")]
mod icu;
pub mod properties;
pub mod width;

/// Re-export the `parley_emoji` DFA + categories so callers don't depend
/// on `parley_emoji` directly.
pub use parley_emoji::{EmojiDFA, EmojiPresentationStyle, EmojiSegmentationCategory};

#[cfg(feature = "icu")]
pub use icu::width_icu;

pub use emoji::{
    Selector, is_keycap_base, is_vs_base, is_vs_target, is_zwj_target, mark_attaches, mark_is_stray,
};
pub use properties::{Properties, cell_width, first_non_ascii, is_zero_width, vs_width};
pub use width::{cell_advance, cell_col_byte_widths, cell_width_sum, cell_widths};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vs_width_forces_emoji_or_text_presentation() {
        // VS16 forces emoji (2 cells), VS15 forces text (1 cell).
        assert_eq!(vs_width('\u{FE0F}'), 2);
        assert_eq!(vs_width('\u{FE0E}'), 1);
        assert_eq!(vs_width('x'), 0);
        assert_eq!(vs_width('\u{200D}'), 0);
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
    fn text_default_emoji_stay_one_cell() {
        // U+2764 ❤ is Emoji=Yes but Emoji_Presentation=No: text-default, so
        // it is ONE cell alone. VS16 widens it to 2; a ZWJ-family membership
        // also widens it (the DFA's `presentation_style(Emoji)`).
        assert_eq!(cell_width('❤'), Some(1));
        assert_eq!(cell_width('↕'), Some(1));
        assert_eq!(cell_widths("❤"), vec![1]);
        assert_eq!(cell_widths("❤\u{FE0F}"), vec![2, 0]);
        assert_eq!(cell_widths("↕\u{FE0F}"), vec![2, 0]);
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
    // `↕` + VS16 (emoji presentation) widens to 2; alone it is ALREADY 2
    // (Emoji=Yes — parley renders it as a color emoji).
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
