// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! String-level cell width — the width model built on [`crate::emoji`].
//!
//! A private `Widths` iterator is the single streaming source of truth: it
//! walks a string, merges emoji sequences through the UTS #51 DFA, and yields
//! one cell width per char in order. [`cell_widths`], [`cell_width_sum`] and
//! [`cell_col_byte_widths`] are thin views over it; [`cell_advance`] snaps a
//! font glyph advance to the same grid.

use crate::emoji::{Run, Selector, finalize_lead, run_continues};
use crate::properties::{cell_width, emoji_category, first_non_ascii};

/// The streaming width model — the single source of truth for
/// [`cell_widths`], [`cell_width_sum`] and [`cell_col_byte_widths`].
///
/// Yields one cell width per `char`, in char order. A multi-char emoji
/// sequence yields its lead's width (1 or 2) once, then `0` for each
/// continuation. The all-ASCII fast path yields `1` per byte without touching
/// the property table.
struct Widths<'a> {
    /// Remaining bytes of the all-ASCII fast path (each width 1).
    ascii: usize,
    /// The char stream (empty while `ascii > 0`).
    chars: core::str::Chars<'a>,
    /// A base glyph precedes the current position (lone-mark attach rule).
    has_base: bool,
    /// The last width>0 base was a SPACE (a space is not a VISIBLE base).
    space_base: bool,
    /// The open emoji run whose lead width is not yet finalized.
    run: Option<Run>,
    /// Pending lead width from a just-closed run.
    flush_lead: Option<usize>,
    /// Pending zero-width continuations from a just-closed run.
    flush_zeros: usize,
}

impl<'a> Widths<'a> {
    fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let all_ascii = first_non_ascii(bytes) == bytes.len();
        Self {
            ascii: if all_ascii { bytes.len() } else { 0 },
            chars: if all_ascii { "".chars() } else { text.chars() },
            has_base: false,
            space_base: false,
            run: None,
            flush_lead: None,
            flush_zeros: 0,
        }
    }
}

impl Iterator for Widths<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            // All-ASCII fast path: every byte is one cell.
            if self.ascii > 0 {
                self.ascii -= 1;
                return Some(1);
            }
            // Flush a just-closed run: its lead width, then its zero-width
            // continuations.
            if let Some(w) = self.flush_lead.take() {
                return Some(w);
            }
            if self.flush_zeros > 0 {
                self.flush_zeros -= 1;
                return Some(0);
            }

            let Some(c) = self.chars.next() else {
                // End of text: close the final open run and flush it.
                if let Some(r) = self.run.take() {
                    self.flush_lead = Some(finalize_lead(
                        &r,
                        &mut self.has_base,
                        &mut self.space_base,
                        false,
                    ));
                    self.flush_zeros = r.len() - 1;
                    continue;
                }
                return None;
            };

            let cat = emoji_category(c);
            if let Some(r) = &mut self.run
                && run_continues(r, c, cat)
            {
                r.push(cat);
                continue;
            }

            // `c` starts a new run: close the previous one, then open the new.
            // A mark followed by ANOTHER combining mark must be standalone
            // (its own cell) so the following mark has a visible base.
            let followed_by_mark = Selector::from_char(c).is_none() && cell_width(c) == Some(0);
            if let Some(r) = self.run.take() {
                self.flush_lead = Some(finalize_lead(
                    &r,
                    &mut self.has_base,
                    &mut self.space_base,
                    followed_by_mark,
                ));
                self.flush_zeros = r.len() - 1;
            }
            self.run = Some(Run::start(c, cat));
        }
    }
}

/// Per-char display cell widths, with emoji sequences merged by the UTS #51
/// DFA ([`crate::EmojiDFA`]) — the single structural source of truth
/// (keycap, modifier, RI flag, ZWJ, tag, VS16/VS15):
///
/// - A lone char keeps its own width (`©` = 1, `👍` = 2); a lone width-0 mark
///   (skin tone / combining) occupies one cell.
/// - A lone ZWJ (`U+200D`) is ignored — always zero cells.
/// - A multi-char sequence is one glyph: 2 cells, except text presentation
///   (VS15), a bare keycap without VS16, and a ZWJ sequence with a
///   text-presentation leading char stay 1.
/// - Every continuation char in a sequence is 0.
/// - The first variation selector locks the width (`▶+VS16+VS15` = 2,
///   `😐+VS15+VS16` = 1).
///
/// Returns one width per `char`, in char order, so callers can zip it with
/// `char_indices()`; [`crate::cell_width`] is single-char and cannot see a
/// following VS16/ZWJ.
///
/// Spec: Layer 2 `w₂` in `docs/width-model-spec.md`.
pub fn cell_widths(text: &str) -> Vec<usize> {
    let mut widths = Vec::with_capacity(text.len());
    widths.extend(Widths::new(text));
    widths
}

/// The total cell width of `text` in one allocation-free pass — the
/// streaming counterpart of `cell_widths(text).iter().sum()`.
///
/// Spec: `Σ w₂` in `docs/width-model-spec.md`.
pub fn cell_width_sum(text: &str) -> usize {
    // All-ASCII fast path: every byte is one cell, so the total is the byte
    // length — no per-char work (the streaming `Widths` would sum 1 per byte).
    if first_non_ascii(text.as_bytes()) == text.len() {
        return text.len();
    }
    Widths::new(text).sum()
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

/// The char INDEX (into `widths`) of the glyph that STARTS at (or contains)
/// 0-based cell column `col`, stepping over zero-width continuations.
/// Returns `widths.len()` past the last cell. The FLOOR boundary: a column on
/// a wide glyph's second cell maps back to that glyph's lead char.
#[inline]
pub fn cell_col_char_from_widths(widths: &[usize], col: usize) -> usize {
    let mut cell = 0;
    for (i, &w) in widths.iter().enumerate() {
        if w > 0 && col < cell + w {
            return i;
        }
        cell += w;
    }
    widths.len()
}

/// The char INDEX (into `widths`) of the first glyph AT or past 0-based cell
/// column `col` (the CEIL boundary) — a zero-width char sitting exactly at
/// the boundary IS included. Returns `widths.len()` past the last cell.
#[inline]
pub fn cell_col_char_ceil_from_widths(widths: &[usize], col: usize) -> usize {
    let mut cell = 0;
    for (i, &w) in widths.iter().enumerate() {
        if cell >= col {
            return i;
        }
        cell += w;
    }
    widths.len()
}

/// The char INDEX (into `widths`) ONE PAST the glyph that ENDS at 0-based
/// cell column `col`, including its trailing zero-width continuations.
/// Returns `widths.len()` past the last cell.
#[inline]
pub fn cell_col_char_end_from_widths(widths: &[usize], col: usize) -> usize {
    let mut cell = 0;
    for (i, &w) in widths.iter().enumerate() {
        if w > 0 && cell + w >= col {
            let mut end = i + 1;
            while end < widths.len() && widths[end] == 0 {
                end += 1;
            }
            return end;
        }
        cell += w;
    }
    widths.len()
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
    let mut cell = 0_usize;
    for ((i, _c), w) in text.char_indices().zip(Widths::new(text)) {
        // Return the char that STARTS at (or contains) `cell_col`. A column
        // landing in the SECOND cell of a wide char (cell_col == cell + 1)
        // must map to that same wide char, not skip past it to `text.len()`.
        if w > 0 && cell_col >= cell && cell_col < cell + w {
            return i;
        }
        cell += w;
    }
    text.len()
}
