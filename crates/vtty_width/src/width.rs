// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! String-level cell width, built on [`crate::emoji`]. A private `Widths`
//! iterator merges emoji sequences through the UTS #51 DFA and yields one cell
//! width per char.

use crate::emoji::{
    Run, Selector, finalize_lead, fresh_emoji_continues_from, is_emoji_starter_from, run_continues,
};
use crate::properties::{
    Gcb, Properties, category_from, cell_width_from, cjk_fast_width,
    first_non_printable_ascii,
};

/// Number of leading printable-ASCII bytes that are NOT keycap bases
/// (`#`, `*`, `0-9`). Keycap bases can start a keycap emoji sequence
/// (e.g. `1 VS16 U+20E3`), so they must go through the DFA; every other
/// printable-ASCII char is always a standalone width-1 base.
#[inline]
fn plain_ascii_run(bytes: &[u8]) -> usize {
    let mut n = 0;
    while let Some(&b) = bytes.get(n) {
        if !(0x20..=0x7e).contains(&b) || matches!(b, b'#' | b'*' | b'0'..=b'9') {
            break;
        }
        n += 1;
    }
    n
}

/// Decode one UTF-8 char at `pos` into `(char, byte_len)`. The input is a
/// valid `&str`, so the manual decode needs no validation.
#[inline]
fn decode_at(bytes: &[u8], pos: usize) -> Option<(char, usize)> {
    let b0 = *bytes.get(pos)?;
    let (len, cp) = if b0 < 0x80 {
        (1, b0 as u32)
    } else if b0 < 0xe0 {
        let b1 = *bytes.get(pos + 1)?;
        (2, ((b0 as u32 & 0x1f) << 6) | (b1 as u32 & 0x3f))
    } else if b0 < 0xf0 {
        let b1 = *bytes.get(pos + 1)?;
        let b2 = *bytes.get(pos + 2)?;
        (
            3,
            ((b0 as u32 & 0x0f) << 12) | ((b1 as u32 & 0x3f) << 6) | (b2 as u32 & 0x3f),
        )
    } else {
        let b1 = *bytes.get(pos + 1)?;
        let b2 = *bytes.get(pos + 2)?;
        let b3 = *bytes.get(pos + 3)?;
        (
            4,
            ((b0 as u32 & 0x07) << 18)
                | ((b1 as u32 & 0x3f) << 12)
                | ((b2 as u32 & 0x3f) << 6)
                | (b3 as u32 & 0x3f),
        )
    };
    Some((char::from_u32(cp)?, len))
}

/// A char source for the width state machine: UTF-8 bytes (decoded on the
/// fly) or raw codepoints (indexed directly).
trait Source {
    /// Total length in source units (bytes or codepoints).
    fn len(&self) -> usize;
    /// Decode the char at `pos`, returning `(char, unit_count)` — `1` for a
    /// codepoint, `1..=4` for a UTF-8 byte sequence.
    fn char_at(&self, pos: usize) -> Option<(char, usize)>;
    /// Count of leading printable-ASCII non-keycap chars from `pos`.
    fn ascii_run(&self, pos: usize) -> usize;
}

struct Bytes<'a>(&'a [u8]);

impl Source for Bytes<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
    #[inline]
    fn char_at(&self, pos: usize) -> Option<(char, usize)> {
        decode_at(self.0, pos)
    }
    #[inline]
    fn ascii_run(&self, pos: usize) -> usize {
        plain_ascii_run(&self.0[pos..])
    }
}

struct Codepoints<'a>(&'a [u32]);

impl Source for Codepoints<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
    #[inline]
    fn char_at(&self, pos: usize) -> Option<(char, usize)> {
        self.0
            .get(pos)
            .and_then(|&cp| char::from_u32(cp))
            .map(|c| (c, 1))
    }
    #[inline]
    fn ascii_run(&self, pos: usize) -> usize {
        let mut n = 0;
        while let Some(&cp) = self.0.get(pos + n) {
            if (0x20..=0x7e).contains(&cp) && !matches!(cp, 0x30..=0x39 | 0x23 | 0x2A) {
                n += 1;
            } else {
                break;
            }
        }
        n
    }
}

/// The streaming width model — the single source of truth for
/// [`cell_widths`], [`cell_width_sum`] and [`cell_col_byte_widths`].
///
/// Yields one cell width per `char`, in char order. A multi-char emoji
/// sequence yields its lead's width (1 or 2) once, then `0` for each
/// continuation. The printable-ASCII fast path yields `1` per byte without
/// touching the property table.
struct Widths<S> {
    /// The char source (UTF-8 bytes or raw codepoints).
    src: S,
    /// Source-unit offset of the next unread char.
    pos: usize,
    /// Remaining printable-ASCII fast-path chars (each width 1).
    ascii: usize,
    /// A base glyph precedes the current position (lone-mark attach rule).
    has_base: bool,
    /// Hangul continuation bitmask: bit 0 = V may follow (after L/LV/V),
    /// bit 1 = T may follow (after LV/V/LVT/T) — UAX #29 GB6/GB7/GB8.
    hangul: u8,
    /// The last width>0 base was a SPACE (a space is not a VISIBLE base).
    space_base: bool,
    /// The open emoji run whose lead width is not yet finalized.
    run: Option<Run>,
    /// Pending lead width from a just-closed run.
    flush_lead: Option<usize>,
    /// Pending zero-width continuations from a just-closed run.
    flush_zeros: usize,
    /// Pending width of a plain (non-emoji, non-mark, non-Hangul) char.
    plain: Option<usize>,
}

impl<S: Source> Widths<S> {
    fn new(src: S) -> Self {
        Self {
            src,
            pos: 0,
            ascii: 0,
            has_base: false,
            hangul: 0,
            space_base: false,
            run: None,
            flush_lead: None,
            flush_zeros: 0,
            plain: None,
        }
    }

    /// Decode the next char and advance `pos` past it. Only reached for
    /// non-ASCII or control chars — printable-ASCII runs take the fast path.
    #[inline]
    fn next_char(&mut self) -> Option<char> {
        let (c, n) = self.src.char_at(self.pos)?;
        self.pos += n;
        Some(c)
    }

    /// Decode the next char WITHOUT advancing `pos` — the standalone-emoji
    /// lookahead.
    #[inline]
    fn peek_char(&self) -> Option<char> {
        self.src.char_at(self.pos).map(|(c, _)| c)
    }

    /// Fill `out` with the remaining widths. Printable-ASCII runs are
    /// bulk-filled with `1` (a `resize`/memset) instead of one `next()` +
    /// `push` per byte; standalone-base runs (plain non-ASCII + standalone
    /// emoji) are bulk-filled too. Only sequence chars go through
    /// [`Iterator::next`].
    pub(crate) fn fill(&mut self, out: &mut Vec<usize>) {
        loop {
            if self.ascii > 0 {
                let n = self.ascii;
                self.ascii = 0;
                self.pos += n;
                out.resize(out.len() + n, 1);
                continue;
            }
            // In the clean state (no open emoji run, no pending flush), a
            // run of standalone bases needs no DFA state machine — bulk-fill
            // it directly.
            if self.run.is_none()
                && self.flush_lead.is_none()
                && self.flush_zeros == 0
                && self.plain.is_none()
                && self.fill_plain_run(out)
            {
                continue;
            }
            let Some(w) = self.next() else { break };
            out.push(w);
        }
    }

    /// Bulk-fill consecutive standalone bases from the current position,
    /// returning `true` if any were emitted. Only valid in the clean state
    /// (no open emoji run, no pending flush) — the caller guarantees it.
    fn fill_plain_run(&mut self, out: &mut Vec<usize>) -> bool {
        let mut n = 0;
        while self.pos < self.src.len() {
            let Some((c, len)) = self.src.char_at(self.pos) else {
                break;
            };
            // ASCII (and keycap bases/controls) take the ASCII fast path or
            // the DFA, not this bulk loop.
            if c.is_ascii() {
                break;
            }
            let p = Properties::get(c);
            let w1 = cell_width_from(p, c);
            let starter = is_emoji_starter_from(p, c);
            let w = if matches!(w1, Some(1) | Some(2)) && !starter && p.gcb() == Gcb::Other {
                // Plain non-ASCII: a standalone width-1/2 base.
                w1.unwrap()
            } else if matches!(w1, Some(1) | Some(2)) && starter {
                // Standalone emoji: an emoji starter NOT followed by a
                // continuation (VS15/VS16/ZWJ/modifier/RI/tag).
                let standalone = match self.src.char_at(self.pos + len) {
                    None => true,
                    Some((c2, _)) => {
                        let p2 = Properties::get(c2);
                        !fresh_emoji_continues_from(p, c, p2, c2, p2.gcb())
                    }
                };
                if standalone {
                    w1.unwrap()
                } else {
                    break;
                }
            } else {
                break;
            };
            out.push(w);
            self.pos += len;
            n += 1;
        }
        if n == 0 {
            return false;
        }
        // The last char is a visible base (never a space — spaces are ASCII).
        self.has_base = true;
        self.hangul = 0;
        self.space_base = false;
        true
    }

    /// Emit a standalone width-1/2 base: set the base state, then either
    /// return the width (no pending emoji run) or buffer it behind the
    /// pending run's flush. `None` means "buffered — continue the loop".
    #[inline]
    fn emit_base(&mut self, w: usize) -> Option<usize> {
        match self.run.take() {
            None => {
                self.has_base = true;
                self.hangul = 0;
                self.space_base = false;
                Some(w)
            }
            Some(r) => {
                self.flush_lead = Some(finalize_lead(
                    &r,
                    &mut self.has_base,
                    &mut self.hangul,
                    &mut self.space_base,
                    false,
                ));
                self.flush_zeros = r.len() - 1;
                self.has_base = true;
                self.hangul = 0;
                self.space_base = false;
                self.plain = Some(w);
                None
            }
        }
    }
}

impl<S: Source> Iterator for Widths<S> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            // Printable-ASCII fast path: every char is one cell.
            if self.ascii > 0 {
                self.ascii -= 1;
                self.pos += 1;
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
            if let Some(w) = self.plain.take() {
                return Some(w);
            }

            // End of input: close the final open run and flush it.
            if self.pos >= self.src.len() {
                if let Some(r) = self.run.take() {
                    self.flush_lead = Some(finalize_lead(
                        &r,
                        &mut self.has_base,
                        &mut self.hangul,
                        &mut self.space_base,
                        false,
                    ));
                    self.flush_zeros = r.len() - 1;
                    continue;
                }
                return None;
            }

            // A printable-ASCII run anywhere in the stream: each char is
            // width 1 and never merges, so skip the emoji DFA entirely. The
            // state a following combining mark sees is just the last char (a
            // base, or a space).
            let ascii_run = self.src.ascii_run(self.pos);
            if ascii_run > 0 {
                if let Some(r) = self.run.take() {
                    self.flush_lead = Some(finalize_lead(
                        &r,
                        &mut self.has_base,
                        &mut self.hangul,
                        &mut self.space_base,
                        false, // an ASCII char is never a mark
                    ));
                    self.flush_zeros = r.len() - 1;
                    continue; // drain the flush, then take the ASCII run
                }
                let space = matches!(
                    self.src.char_at(self.pos + ascii_run - 1),
                    Some((c, _)) if c == ' '
                );
                self.ascii = ascii_run;
                self.has_base = true;
                self.hangul = 0;
                self.space_base = space;
                continue;
            }

            // Non-ASCII or control char.
            let c = self.next_char()?;
            let p = Properties::get(c);

            // A pending emoji run: check continuation (the rare DFA path).
            if self.run.is_some() {
                let cat = category_from(p, c);
                if let Some(r) = &mut self.run
                    && run_continues(r, c, cat, p.gcb())
                {
                    r.push(cat);
                    continue;
                }
            }

            let w1 = cell_width_from(p, c);
            let starter = is_emoji_starter_from(p, c);
            // Plain non-ASCII char: a standalone width-1/2 base that never
            // merges (not an emoji starter, not a mark/selector/control, not
            // a Hangul jamo). Skip the DFA — only the base state a following
            // combining mark sees matters.
            if matches!(w1, Some(1) | Some(2)) && !starter && p.gcb() == Gcb::Other {
                match self.emit_base(w1.unwrap()) {
                    Some(w) => return Some(w),
                    None => continue,
                }
            }

            // Standalone emoji: an emoji starter NOT followed by a
            // continuation (VS15/VS16/ZWJ/modifier/RI/tag) is a fixed-width
            // base — skip the DFA and emit `w1` directly.
            if matches!(w1, Some(1) | Some(2)) && starter {
                let standalone = match self.peek_char() {
                    None => true,
                    Some(c2) => {
                        let p2 = Properties::get(c2);
                        !fresh_emoji_continues_from(p, c, p2, c2, p2.gcb())
                    }
                };
                if standalone {
                    match self.emit_base(w1.unwrap()) {
                        Some(w) => return Some(w),
                        None => continue,
                    }
                }
            }

            // `c` starts a new run: close the previous one, then open the new.
            // A mark followed by ANOTHER combining mark must be standalone
            // (its own cell) so the following mark has a visible base.
            let cat = category_from(p, c);
            let followed_by_mark = Selector::from_char(c).is_none() && w1 == Some(0);
            if let Some(r) = self.run.take() {
                self.flush_lead = Some(finalize_lead(
                    &r,
                    &mut self.has_base,
                    &mut self.hangul,
                    &mut self.space_base,
                    followed_by_mark,
                ));
                self.flush_zeros = r.len() - 1;
            }
            self.run = Some(Run::start(
                c,
                cat,
                w1.unwrap_or(0),
                p.is_emoji_presentation(),
                p.gcb(),
            ));
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
#[must_use]
pub fn cell_widths(text: &str) -> Vec<usize> {
    let mut widths = Vec::with_capacity(text.len());
    Widths::new(Bytes(text.as_bytes())).fill(&mut widths);
    widths
}

/// [`cell_widths`] from already-decoded codepoints — the no-re-decode path
/// for callers that hold `u32` code points (the SIMD-transcoded parser path).
#[must_use]
pub fn cell_widths_from_codepoints(cps: &[u32]) -> Vec<usize> {
    let mut widths = Vec::with_capacity(cps.len());
    Widths::new(Codepoints(cps)).fill(&mut widths);
    widths
}

/// The single-lookup print-handoff path: returns `(cell_width(c), standalone)`
/// from one [`Properties`] lookup. `standalone` is true when `c`'s DFA width
/// equals its single-char width given the next char `next` (the standalone-
/// emoji lookahead). Callers that already hold the codepoint use this to
/// avoid a second `Properties` lookup when detecting DFA-simple runs.
#[must_use]
#[inline]
pub fn width_and_standalone(c: char, next: Option<u32>) -> (Option<usize>, bool) {
    if c.is_ascii() {
        return (
            if c.is_ascii_control() { None } else { Some(1) },
            !c.is_ascii_control(),
        );
    }
    // CJK/kana/fullwidth blocks are always width-2 standalone bases: skip the
    // composite lookup AND the emoji lookahead (the dominant hot path).
    if let Some(w) = cjk_fast_width(c) {
        return (Some(w), true);
    }
    let p = Properties::get(c);
    let next_info = next.and_then(|cp| {
        if cp < 0x80 {
            None // ASCII never continues an emoji sequence
        } else {
            let c2 = char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER);
            Some((cp, Properties::get(c2)))
        }
    });
    width_and_standalone_with(c, p, next_info)
}

/// [`width_and_standalone`] for a non-ASCII, non-CJK char with an
/// already-looked-up [`Properties`]. `next` carries the lookahead codepoint
/// AND its [`Properties`], so a codepoint loop can compute the lookahead once
/// and reuse it as the next iteration's current char (the emoji-lookahead
/// double lookup).
#[must_use]
#[inline]
pub fn width_and_standalone_with(
    c: char,
    p: Properties,
    next: Option<(u32, Properties)>,
) -> (Option<usize>, bool) {
    let w = cell_width_from(p, c);
    let standalone = matches!(w, Some(1) | Some(2))
        && (if is_emoji_starter_from(p, c) {
            // Emoji starter: standalone iff the next char does not continue
            // it (VS16/VS15/ZWJ/modifier/RI/tag).
            match next {
                None => true,
                Some((cp2, _)) if cp2 < 0x80 => true,
                Some((cp2, p2)) => {
                    let c2 =
                        char::from_u32(cp2).unwrap_or(char::REPLACEMENT_CHARACTER);
                    !fresh_emoji_continues_from(p, c, p2, c2, p2.gcb())
                }
            }
        } else {
            // Plain base: never merges only when its GCB is Other (not a
            // Jamo, mark, or other sequence category).
            p.gcb() == Gcb::Other
        });
    (w, standalone)
}

/// The total cell width of `text` in one allocation-free pass — the
/// streaming counterpart of `cell_widths(text).iter().sum()`.
///
/// Spec: `Σ w₂` in `docs/width-model-spec.md`.
#[must_use]
pub fn cell_width_sum(text: &str) -> usize {
    // Printable-ASCII fast path: every byte is one cell, so the total is the
    // byte length — no per-char work (the streaming `Widths` sums 1 per byte).
    // Controls are zero-width, so they take the general path.
    if first_non_printable_ascii(text.as_bytes()) == text.len() {
        return text.len();
    }
    Widths::new(Bytes(text.as_bytes())).sum()
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
#[must_use]
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
#[must_use]
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
#[must_use]
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
#[must_use]
pub fn cell_col_byte_widths(text: &str, cell_col: usize) -> usize {
    // Printable-ASCII fast path: one cell per byte, no width table needed.
    if first_non_printable_ascii(text.as_bytes()) == text.len() {
        return cell_col.min(text.len());
    }
    let mut cell = 0_usize;
    for ((i, _c), w) in text.char_indices().zip(Widths::new(Bytes(text.as_bytes()))) {
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
