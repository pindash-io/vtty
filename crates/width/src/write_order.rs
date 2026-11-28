// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Layer 3 of the width model: cell width derived from write order. A
//! control resets the whole-text [`crate::cell_widths`] DFA's scope, so the
//! same text can carry different widths. See §3.2/§3.3 of the
//! [width model](crate::_documentation::width_model).

/// Write-order boundary data for the split DFA (§3.3): the restart flags `B`
/// (`fb`) and the flushed-base offsets `F` (`zwj_flushes`).
#[derive(Debug, Default)]
pub struct SplitBoundaries {
    /// Sparse `(byte_offset, flags)` entries (bit 0 = fresh base, bit 1 =
    /// re-open); absent bytes are implicitly zero.
    pub fb: Vec<(usize, u8)>,
    /// Byte offset just past each REP/direct-ZWJ flushed base (native width). `F`.
    pub zwj_flushes: Vec<usize>,
}

/// The live write-order emoji/ZWJ automaton (§3.2/§3.3); records the
/// [`SplitBoundaries`] that [`split_widths`] consumes.
#[derive(Debug, Default)]
pub struct EmojiTrace {
    /// A grapheme is open: the previous print was its base, so a following
    /// VS16/VS15 or mark attaches there, not at the cursor.
    pub grapheme_open: bool,
    /// The previous printed char was a width-2 emoji.
    pub prev_emoji: bool,
    /// The previous printed char was a ZWJ that followed a width-2 emoji (a
    /// width-2 emoji after it is a ZWJ-sequence continuation, 0 cells).
    pub zwj: bool,
    /// A wide ZWJ base was dropped at the right margin, so its continuations
    /// have no base and are dropped too.
    pub drop_next_zwj: bool,
    /// A control closed an open ZWJ: the next width-consuming char is fresh
    /// (its byte offset is recorded when it is written).
    pub fresh_pending: bool,
    /// Byte offset just past the open trailing ZWJ. Transient (set and read
    /// within one print run, before any control/scroll can intervene).
    pub zwj_tail: usize,
    /// The recorded write-order boundary data (fresh bases / re-opens /
    /// flushed bases): `(B, F)` in the spec.
    pub boundaries: SplitBoundaries,
}

impl EmojiTrace {
    // `fb`: bit 0 = fresh base (DFA restarts), bit 1 = re-open boundary
    // (print time). A marker past the end is a future marker.
    const FB_FRESH: u8 = 0x01;
    const FB_REOPEN: u8 = 0x02;
    const FB_ICH: u8 = 0x04;

    /// Record a fresh base at byte offset `abs` (bit 0 of the sparse trace).
    #[inline]
    pub fn mark_fresh(&mut self, abs: usize) {
        self.mark_bit(abs, Self::FB_FRESH);
    }

    /// Move a fresh-base flag from `pos` to `pos + diff`: an ICH shifts the content, so
    /// the flag moves past the inserted blanks.
    pub fn shift_fresh_over_insert(&mut self, pos: usize, diff: usize) {
        if diff == 0 {
            return;
        }
        let fb = &mut self.boundaries.fb;
        let Some(i) = fb.binary_search_by_key(&pos, |&(a, _)| a).ok() else {
            return;
        };
        let fresh = fb[i].1 & Self::FB_FRESH;
        if fresh == 0 {
            return;
        }
        fb[i].1 &= !fresh;
        if fb[i].1 == 0 {
            fb.remove(i);
        }
        self.mark_fresh(pos + diff);
        self.mark_bit(pos + diff, Self::FB_ICH);
    }

    /// Record a ZWJ re-open boundary at byte offset `abs` (bit 1 of the
    /// sparse trace, print-time only).
    #[inline]
    pub fn mark_reopen(&mut self, abs: usize) {
        self.mark_bit(abs, Self::FB_REOPEN);
    }

    /// Set `bit` at `abs`; a byte past the current end is a future marker.
    #[inline]
    pub fn mark_bit(&mut self, abs: usize, bit: u8) {
        let fb = &mut self.boundaries.fb;
        match fb.binary_search_by_key(&abs, |&(a, _)| a) {
            Ok(i) => fb[i].1 |= bit,
            Err(i) => fb.insert(i, (abs, bit)),
        }
    }

    /// Clear the re-open boundary at byte offset `abs`, dropping the sparse
    /// entry when it becomes empty.
    #[inline]
    pub fn clear_reopen(&mut self, abs: usize) {
        let fb = &mut self.boundaries.fb;
        if let Ok(i) = fb.binary_search_by_key(&abs, |&(a, _)| a) {
            fb[i].1 &= !Self::FB_REOPEN;
            // Drop a fully-cleared entry so the sparse trace stays minimal.
            if fb[i].1 == 0 {
                fb.remove(i);
            }
        }
    }

    /// Clear the fresh bits in `[start, end)`; an erase removes the chars, so their
    /// fresh-base markers must not survive the `splice_fb` boundary rule.
    #[inline]
    pub fn clear_fresh_range(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let fb = &mut self.boundaries.fb;
        for entry in fb.iter_mut() {
            if entry.0 >= start && entry.0 < end {
                entry.1 &= !(Self::FB_FRESH | Self::FB_ICH);
            }
        }
        fb.retain(|&(_, flags)| flags != 0);
    }

    /// Splice the trace through a `[pos, pos+old_len)` edit, keeping a boundary
    /// marker and clearing the shifted copy of a pure insert.
    #[inline]
    pub fn splice_fb(&mut self, pos: usize, old_len: usize, diff: isize) {
        let fb = &mut self.boundaries.fb;
        // Clamp to the trace's span: a test that direct-assigns `text`
        // leaves `fb` empty, so a later edit can carry a `pos` past it.
        let max = fb.last().map_or(0, |&(a, _)| a);
        let pos = pos.min(max + 1);
        let saved = fb
            .binary_search_by_key(&pos, |&(a, _)| a)
            .ok()
            .map(|i| fb[i].1)
            .unwrap_or(0);
        let end = pos + old_len;
        let mut new_fb: Vec<(usize, u8)> = Vec::with_capacity(fb.len());
        for &(abs, flags) in fb.iter() {
            let Some(na) = splice_offset(abs, pos, end, diff) else {
                continue;
            };
            // A pure insert clears the shifted copy of the flag at `pos`.
            let flags = if old_len == 0 && diff > 0 && na == (pos.cast_signed() + diff) as usize {
                flags & !saved
            } else {
                flags
            };
            if flags != 0 {
                new_fb.push((na, flags));
            }
        }
        if saved != 0 {
            match new_fb.binary_search_by_key(&pos, |&(a, _)| a) {
                Ok(i) => new_fb[i].1 |= saved,
                Err(i) => new_fb.insert(i, (pos, saved)),
            }
        }
        *fb = new_fb;
    }
}

/// Relative byte offsets where the DFA restarts: markers strictly inside the
/// slice that split a ZWJ from a following non-zero-width char, or a fresh
/// base from what precedes it. `fb` is the sparse flag set; `start` is the
/// logical offset of `text`.
pub(crate) fn split_offsets(text: &str, fb: &[(usize, u8)], start: usize) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }
    let end = start + text.len();
    let mut offs: Vec<usize> = Vec::new();
    // Sparse trace: iterate only the recorded (non-zero) entries and keep
    // those inside `[start, end)`. The bytes not present are implicitly zero.
    for &(abs, flags) in fb {
        if flags == 0 || abs < start || abs >= end {
            continue;
        }
        let rel = abs - start;
        if !text.is_char_boundary(rel) {
            continue;
        }
        if rel > 0
            && text[..rel].ends_with('\u{200D}')
            && text[rel..]
                .chars()
                .next()
                .is_some_and(|c| !crate::is_zero_width(c))
        {
            offs.push(rel);
            continue;
        }
        // A fresh-base mark restarts the DFA so it keeps its 1-cell glyph
        // instead of re-attaching to a preceding base; selectors never are.
        if flags & EmojiTrace::FB_FRESH != 0
            && rel > 0
            && text[rel..].chars().next().is_some_and(|c| {
                crate::cell_width(c) == Some(0) && crate::Selector::from_char(c).is_none()
            })
        {
            offs.push(rel);
        }
    }
    offs
}

/// `cell_widths` per DFA segment, split at `offs` (relative to the text start).
pub(crate) fn segmented_widths(text: &str, offs: &[usize]) -> Vec<usize> {
    let mut w = Vec::new();
    let mut prev = 0;
    for &b in offs {
        w.extend(crate::cell_widths(&text[prev..b]));
        prev = b;
    }
    w.extend(crate::cell_widths(&text[prev..]));
    w
}

/// Un-widen a REP/direct-ZWJ flushed base that the whole-text DFA widened
/// (e.g. `❤ ZWJ` → 2 cells). `zwj_flushes` holds absolute byte offsets.
pub(crate) fn override_flush_width<T: Copy + Into<usize>>(
    text: &str,
    zwj_flushes: &[T],
    start: usize,
    w: &mut [usize],
) {
    for &abs in zwj_flushes {
        let flush = abs.into().saturating_sub(start);
        if flush >= text.len() || !text.is_char_boundary(flush) {
            continue;
        }
        let mut marks = text[flush..].chars();
        if matches!(marks.next(), Some('\u{FE0F}' | '\u{FE0E}')) {
            continue;
        }
        // A VS16/VS15 in the trailing zero-width run re-widens or narrows the
        // base, so the flushed-native override must not apply.
        let mut has_zwj = false;
        let mut has_vs = false;
        for c in text[flush..]
            .chars()
            .take_while(|&c| crate::is_zero_width(c))
        {
            if c == '\u{200D}' {
                has_zwj = true;
            }
            if matches!(c, '\u{FE0F}' | '\u{FE0E}') {
                has_vs = true;
            }
        }
        if !has_zwj || has_vs {
            continue;
        }
        for ((i, c), wi) in text.char_indices().zip(w.iter_mut()) {
            if i + c.len_utf8() == flush {
                let native = crate::cell_width(c).unwrap_or(0);
                if native > 0 && *wi > native {
                    *wi = native.max(1);
                }
                break;
            }
        }
    }
}

/// Splice the sorted offsets through a `[pos, end) -> diff` byte edit: keep `t < pos`,
/// drop `[pos, end)`, shift the rest by `diff`.
#[inline]
pub fn splice_offset(t: usize, pos: usize, end: usize, diff: isize) -> Option<usize> {
    if t < pos {
        return Some(t);
    }
    if t >= end {
        return Some((t.cast_signed() + diff).max(0) as usize);
    }
    None
}

/// The Layer-3 derived width: `correct_F(seg(split(text)))`. Pure in the
/// text and boundary data, so the spec's non-compositionality cases are
/// testable.
pub fn split_widths(text: &str, b: &SplitBoundaries, start: usize) -> Vec<usize> {
    if crate::first_non_ascii(text.as_bytes()) == text.len() {
        return vec![1; text.len()];
    }
    let offs = split_offsets(text, &b.fb, start);
    let mut w = segmented_widths(text, &offs);
    override_flush_width(text, &b.zwj_flushes, start, &mut w);
    w
}

/// The cursor advance of one printed char. `dfa_w` is `Some` for a buffered
/// char (part of the segmented DFA), `None` for a direct mark. §3.2.
#[inline]
pub const fn classify_cell(
    zwj: bool,
    is_target: bool,
    wrap_pending: bool,
    raw: usize,
    dfa_w: Option<usize>,
    followed_by_zwj: bool,
    keycap: bool,
) -> usize {
    match dfa_w {
        Some(w) => {
            if (zwj && is_target) || (raw > 0 && w == 0) {
                return 0;
            }
            if (followed_by_zwj || keycap) && w > raw {
                return w;
            }
            if raw == 0 {
                // A mark is 1 cell lone, 0 attached; trust the DFA, since `raw` is
                // always 0 for marks.
                return w;
            }
            raw
        }
        None => {
            if zwj && is_target && !wrap_pending {
                0
            } else {
                raw
            }
        }
    }
}

/// Row-wrap width of one printed char: `max(cell, dfa_w)`.
#[inline]
pub fn wrap_width(cell: usize, dfa_w: Option<usize>) -> usize {
    dfa_w.map_or(cell, |w| cell.max(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn split_widths_matches_theorem_1() {
        // E1 = P(👍) P(ZWJ) P(👍): no boundary → the second 👍 is a 0-cell
        // continuation of the same ZWJ family.
        let text = "👍\u{200D}👍";
        assert_eq!(
            split_widths(text, &SplitBoundaries::default(), 0),
            vec![2, 0, 0]
        );

        // E2 = P(👍) P(ZWJ) K P(👍): a fresh base at byte 7 (the second 👍)
        // restarts the DFA, so it is a fresh 2-cell base.
        let mut b = SplitBoundaries::default();
        b.fb.push((7, 0x01));
        assert_eq!(split_widths(text, &b, 0), vec![2, 0, 2]);
    }

    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn split_widths_breaks_a_zwj_family_on_vs15() {
        // Layer 3 agrees with the DFA: a VS15 on the last member splits the family
        // into the couple + a text member.
        let text = "\u{1F469}\u{200D}\u{1F468}\u{200D}\u{1F467}\u{FE0E}";
        assert_eq!(
            split_widths(text, &SplitBoundaries::default(), 0),
            vec![2, 0, 0, 0, 1, 0]
        );
        assert_eq!(crate::cell_widths(text), vec![2, 0, 0, 0, 1, 0]);
    }

    #[cfg(any(feature = "baked", feature = "icu"))]
    #[test]
    fn override_flush_width_unwidens_a_reped_base() {
        // ❤ is text-default (native width 1), but its trailing ZWJ widens it
        // to 2 in the whole-text DFA. A REP'd base must be un-widened back.
        let text = "❤\u{200D}";
        let mut w = crate::cell_widths(text);
        assert_eq!(w, vec![2, 0]);
        override_flush_width(text, &[3_usize], 0, &mut w);
        assert_eq!(w, vec![1, 0]);
    }

    #[test]
    fn classify_cell_matches_the_spec_cases() {
        // buffered: zwj + target → 0 (family continuation).
        assert_eq!(
            classify_cell(true, true, false, 2, Some(2), false, false),
            0
        );
        // buffered: raw>0 but DFA width 0 → 0.
        assert_eq!(
            classify_cell(false, false, false, 1, Some(0), false, false),
            0
        );
        // buffered: followed_by_zwj widens → DFA width.
        assert_eq!(
            classify_cell(false, false, false, 1, Some(2), true, false),
            2
        );
        // buffered: raw==0 (combining mark) → trust the DFA width.
        assert_eq!(
            classify_cell(false, false, false, 0, Some(1), false, false),
            1
        );
        // buffered: otherwise → native raw.
        assert_eq!(
            classify_cell(false, false, false, 1, Some(1), false, false),
            1
        );
        // direct: zwj + target + no wrap pending → 0.
        assert_eq!(classify_cell(true, true, false, 2, None, false, false), 0);
        // direct: wrap pending → raw (no swallow).
        assert_eq!(classify_cell(true, true, true, 2, None, false, false), 2);
        // direct: otherwise → raw.
        assert_eq!(classify_cell(false, false, false, 1, None, false, false), 1);
    }

    #[test]
    fn wrap_width_is_the_max_of_cell_and_dfa() {
        assert_eq!(wrap_width(0, Some(2)), 2);
        assert_eq!(wrap_width(2, Some(0)), 2);
        assert_eq!(wrap_width(1, None), 1);
    }

    #[test]
    fn splice_offset_keeps_drops_and_shifts() {
        assert_eq!(splice_offset(2, 5, 8, 3), Some(2)); // before pos: keep
        assert_eq!(splice_offset(6, 5, 8, 3), None); // inside [pos, end): drop
        assert_eq!(splice_offset(9, 5, 8, 3), Some(12)); // after end: shift
        assert_eq!(splice_offset(9, 5, 8, -4), Some(5)); // negative diff
        assert_eq!(splice_offset(9, 5, 8, -100), Some(0)); // clamp to 0
    }
}
