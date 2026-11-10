//! Layer 3 of the width model: the write-order derived cell width.
//!
//! The whole-text [`crate::cell_widths`] DFA cannot see write order: a
//! control resets its scope, so the same text can carry different widths.
//! This module makes the write order explicit as pure functions of
//! `(text, B, F)` — the text slice, the split-boundary flags `B`, and the
//! flushed-base offsets `F` — plus the live [`EmojiTrace`] automaton that
//! records those boundaries at print time.
//!
//! Spec: §3.2/§3.3 of `docs/width-model-spec.md`.

/// `split(B, a, b)` — the relative byte offsets (inside `text`) where the
/// emoji DFA restarts: markers that fall strictly inside and split a ZWJ from
/// a following non-zero-width char. `fb` is the per-byte write-order flag
/// aligned to the LOGICAL text (bit 0 = fresh base, bit 1 = re-open); `start`
/// is the logical offset of `text`'s first byte.
///
/// Spec §3.3:
/// $$\mathsf{split}(B, a, b) = \{ x : x \in B,\ a < x < b,\ s\[x-1\] = \mathsf{ZWJ},\ w_1^0(s\[x\]) > 0 \}$$
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
        // A STANDALONE width-0 mark written as a fresh base (bit 0): the
        // write path gave it its own 1-cell glyph (no base to attach to),
        // but the whole-text DFA would re-attach it to a preceding base
        // (0 cells) — a lone mark after a SPACE or after a VS16-widened
        // base. Restart the DFA AT the mark so it stays 1 cell. A selector
        // (VS16/VS15/ZWJ) is never standalone, so it is excluded.
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

/// `seg` — concatenate `cell_widths` per DFA segment (split at `offs`,
/// relative to the text start).
///
/// Spec §3.3:
/// $$\mathsf{seg}(B, s[a..b)) = w_2(s[a..x_1)) \cdot w_2(s[x_1..x_2)) \cdot \dots \cdot w_2(s[x_k..b))$$
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

/// `correct_F` — a REP/direct-ZWJ flush writes its repeated base at NATIVE
/// width, but the whole-text DFA widens a text-default emoji whose trailing
/// ZWJ is a sequence element (e.g. `❤ ZWJ` → `Emoji_Presentation`, 2 cells).
/// Override the flushed base's width back to native — the DFA's family
/// continuations after it are untouched. `zwj_flushes` holds ABSOLUTE byte
/// offsets.
///
/// Spec §3.3: `correct_F` — for each `f ∈ F, f ≥ a` with
/// `overridable(s, f)`, set `w[idx(c)] ← max(w₁⁰(c), 1)` when
/// `w₁⁰(c) > 0 ∧ w[idx(c)] > w₁⁰(c)`.
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
        // A ZWJ in the trailing zero-width run re-opens the family (the DFA
        // widens the base to Emoji_Presentation), which `correct_F` undoes
        // back to the flushed native width. But a VS16/VS15 ANYWHERE in that
        // run EXPLICITLY re-widens/narrows the base, so the flushed-native
        // override must NOT apply — the selector's width is authoritative.
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

/// The sorted-offset splice map: keep `t < pos`, drop `[pos, end)`, and
/// shift `t >= end` by `diff` (clamped to 0). `None` drops the offset — the
/// pure arithmetic behind the byte-offset traces' edit splice (spec §3.3).
#[inline]
pub fn splice_offset(t: usize, pos: usize, end: usize, diff: isize) -> Option<usize> {
    if t < pos {
        return Some(t);
    }
    if t >= end {
        return Some((t as isize + diff).max(0) as usize);
    }
    None
}

/// The write-order boundary DATA for the split-DFA (spec §3.3): the per-byte
/// flags where the emoji DFA restarts (`B`) and the byte offsets where a
/// REP'd base was flushed (`F`). Pure data — the live automaton flags live in
/// [`EmojiTrace`], which OWNS a `SplitBoundaries` alongside them.
///
/// $$B = \{ \mathsf{bytes}(s, k) : fb\[k\] \neq 0 \}, \qquad F = \mathsf{zwj\_flushes}$$
#[derive(Debug, Default)]
pub struct SplitBoundaries {
    /// Sparse write-order flag set aligned to the LOGICAL text: a sorted
    /// `(byte_offset, flags)` list of the NON-ZERO entries (bit 0 = fresh
    /// base, bit 1 = re-open boundary). Bytes not present are implicitly
    /// zero. Sparse because the fresh bases are RARE (a few per screen) —
    /// the old dense `Vec<u8>` was one byte per character and doubled the
    /// memory of emoji-bearing scrollback.
    pub fb: Vec<(usize, u8)>,
    /// Byte PAST each REP/direct-ZWJ flushed base (native width). `F`.
    pub zwj_flushes: Vec<usize>,
}

/// `W(s[a..b), B, F)` — the Layer-3 derived width:
/// `correct_F(seg(B, s[a..b)))`. Pure function of the text slice and the
/// write-order boundary data, so the spec's non-compositionality
/// counterexamples are directly testable.
///
/// Spec §3.3:
/// $$W(s, a, b, B, F) = \mathsf{correct\_F}(\mathsf{seg}(B, s[a..b)), s, a, F)$$
pub fn split_widths(text: &str, b: &SplitBoundaries, start: usize) -> Vec<usize> {
    if crate::first_non_ascii(text.as_bytes()) == text.len() {
        return vec![1; text.len()];
    }
    let offs = split_offsets(text, &b.fb, start);
    let mut w = segmented_widths(text, &offs);
    override_flush_width(text, &b.zwj_flushes, start, &mut w);
    w
}

/// The live write-order emoji/ZWJ width automaton (spec §3.2/§3.3).
///
/// Tracks the open-ZWJ / fresh-base / flushed-base state and records the
/// per-byte [`SplitBoundaries`] the segmented DFA ([`split_widths`]) consumes.
/// Shared by terminal consumers so they use ONE width model.
///
/// State tuple (spec §3.2):
/// $$(\mathsf{zwj}, \mathsf{prev\_emoji}, \mathsf{drop\_next\_zwj}, \mathsf{grapheme\_open}) \in \mathbb{B}^4$$
#[derive(Debug, Default)]
pub struct EmojiTrace {
    /// The previous print was the immediately-preceding char (no control
    /// since) — a VS16/VS15 continuing that base attaches to it, not the
    /// char at the cursor.
    pub grapheme_open: bool,
    /// The previous printed char was a width-2 emoji.
    pub prev_emoji: bool,
    /// The previous printed char was a ZWJ that followed a width-2 emoji (a
    /// width-2 emoji after it is a ZWJ-sequence continuation, 0 cells).
    pub zwj: bool,
    /// A wide ZWJ-sequence base was dropped (autowrap off at the right
    /// margin): the following ZWJ/continuation chars have no base and are
    /// dropped too.
    pub drop_next_zwj: bool,
    /// A control closed an open ZWJ: the NEXT width-consuming char is fresh
    /// (its byte offset is recorded when it is written).
    pub fresh_pending: bool,
    /// Byte offset just PAST the open trailing ZWJ. TRANSIENT (set and read
    /// within one print run, before any control/scroll can intervene).
    pub zwj_tail: usize,
    /// The recorded write-order boundary data (fresh bases / re-opens /
    /// flushed bases) — `(B, F)` in the spec.
    pub boundaries: SplitBoundaries,
}

impl EmojiTrace {
    // ── Per-byte fresh-base trace (`fb`) record/clear ──
    //
    // `fb[k]` is a bitmask: bit 0 = fresh base (the DFA restarts there), bit
    // 1 = re-open boundary (print-time only, consumed by a continuation).
    // `fb` is aligned to the LOGICAL text; a marker past the current end is
    // a FUTURE marker (the spec's "set at write time" — the old byte-offset
    // list stored the future offset, the per-byte trace extends).

    const FB_FRESH: u8 = 0x01;
    const FB_REOPEN: u8 = 0x02;
    const FB_ICH: u8 = 0x04;

    /// Record a fresh base at byte offset `abs` (bit 0 of the sparse trace).
    #[inline]
    pub fn mark_fresh(&mut self, abs: usize) {
        self.mark_bit(abs, Self::FB_FRESH);
    }

    /// Relocate a fresh-base flag from `pos` to `pos + diff` — the ICH case:
    /// the generic fb splice restores the flag AT the insert point and clears
    /// its shifted copy (the "pure insert" survive rule), but an ICH is a
    /// SHIFT: the fresh base (a standalone mark) moves RIGHT with the
    /// content, past the inserted blanks.
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

    /// Set `bit` on the trace at `abs`, recording a sparse entry (a byte
    /// past the current end is a FUTURE marker — the spec's "set at write
    /// time").
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

    /// Clear the FRESH bits in `[start, end)` — an ERASE (ECH/ED/EL/DCH)
    /// removes the chars, so their fresh-base markers must NOT survive the
    /// generic `splice_fb` "edit at a boundary survives" rule (a stale
    /// marker would re-classify the replacement blank/mark as a fresh base).
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

    /// Splice the sparse trace through a `[pos, pos+old_len) → diff` byte
    /// edit, preserving the `b == pos` survive rule (an edit AT a boundary
    /// does not destroy it) and clearing the shifted copy of a pure insert.
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
            let flags = if old_len == 0 && diff > 0 && na == (pos as isize + diff) as usize {
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

/// The cursor advance (`cell`) of one printed char, given the write-order
/// state. `dfa_w` is `Some` for a buffered char (part of the segmented DFA),
/// `None` for a direct mark. Shared by terminal consumers so they use ONE
/// classification.
///
/// Spec §3.2:
/// $$\mathsf{cell} =
/// \begin{cases}
/// 0   & \mathsf{zwj} \land \mathsf{target}(c) \\
/// w   & \mathsf{raw} > 0 \land w = 0 \\
/// w   & \mathsf{followed\_by\_zwj} \land w > \mathsf{raw} \\
/// \mathsf{raw} & \text{otherwise}
/// \end{cases}
/// \quad (\text{buffered}), \qquad
/// \mathsf{cell} =
/// \begin{cases}
/// 0   & \mathsf{zwj} \land \mathsf{target}(c) \land \lnot\mathsf{wrap\_pending} \\
/// \mathsf{raw} & \text{otherwise}
/// \end{cases}
/// \quad (\text{direct})$$
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
                // A combining mark: the whole-text DFA's width is 1 for a
                // LONE mark (its own cell) and 0 for an attached one — trust
                // it (the native `raw` is always 0 for marks, so returning
                // `raw` would attach every mark, losing the lone-mark cell).
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

/// The row-wrap width of one printed char: the DFA width, but never less than
/// the cursor advance (`cell`). A 0-cell ZWJ continuation still occupies its
/// DFA width for the autowrap decision.
///
/// Spec §Corollary 8:
/// $$\mathsf{wrap\_width}(\mathsf{cell}, \mathsf{dfa\_w}) = \max(\mathsf{cell}, \mathsf{dfa\_w})$$
#[inline]
pub fn wrap_width(cell: usize, dfa_w: Option<usize>) -> usize {
    dfa_w.map_or(cell, |w| cell.max(w))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn override_flush_width_unwidens_a_reped_base() {
        // ❤ is text-default (native width 1), but its trailing ZWJ widens it
        // to 2 in the whole-text DFA. A REP'd base must be un-widened back.
        let text = "❤\u{200D}";
        let mut w = crate::cell_widths(text);
        assert_eq!(w, vec![2, 0]);
        override_flush_width(text, &[3usize], 0, &mut w);
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
