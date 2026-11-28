// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Incremental UTF-8 decoding for the byte parser.

use vtty_core::Perform;

use crate::limits::MAX_UTF8_CONTINUATIONS;

/// A partially-decoded UTF-8 code point carried across `advance` calls.
///
/// Multi-byte code points are decoded incrementally: a lead byte (plus any
/// already-arrived continuation bytes) at the end of one chunk is held here
/// until the remaining continuation bytes arrive in a later chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PartialUtf8 {
    /// The code point bits accumulated so far.
    cp: u32,
    /// Continuation bytes still needed to complete it (`0` = none pending).
    need: u8,
}

impl PartialUtf8 {
    /// An empty (idle) partial.
    pub(crate) const fn new() -> Self {
        Self { cp: 0, need: 0 }
    }

    /// Whether a code point is mid-decode.
    pub(crate) const fn pending(self) -> bool {
        self.need > 0
    }

    /// The accumulated code point (valid once [`Self::pending`] is `false`
    /// after a [`Self::feed`] that consumed a continuation byte).
    pub(crate) const fn codepoint(self) -> u32 {
        self.cp
    }

    /// Feed one byte into the pending sequence. Returns `true` when the byte
    /// was consumed as a continuation (the sequence is then complete iff
    /// [`Self::pending`] is now `false`). Returns `false` when the byte was
    /// not a continuation: the partial is cleared and the caller must emit
    /// `U+FFFD` and reprocess the byte.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    pub(crate) const fn feed(&mut self, b: u8) -> bool {
        if matches!(b, 0x80..0xc0) {
            // A code point needs at most 21 bits (3 lead + 3×6 continuation),
            // so `cp << 6` never overflows `u32`.
            self.cp = (self.cp << 6) | (b & 0x3f) as u32;
            self.need -= 1;
            true
        } else {
            self.need = 0;
            false
        }
    }

    /// Clear the pending partial, discarding any accumulated bits.
    pub(crate) const fn clear(&mut self) {
        self.need = 0;
    }
}

/// Expected length and lead bits for a UTF-8 lead byte, from one range
/// dispatch; `expected_len` is `0` for invalid and continuation bytes.
#[inline]
const fn utf8_lead(lead: u8) -> (usize, u32) {
    match lead {
        0x00..=0x7f => (1, lead as u32),
        0xc2..=0xdf => (2, (lead & 0x1f) as u32),
        0xe0..=0xef => (3, (lead & 0x0f) as u32),
        0xf0..=0xf4 => (4, (lead & 0x07) as u32),
        _ => (0, 0),
    }
}

/// Decode `N` validated UTF-8 bytes (lead + `N-1` continuations). `N` is
/// 2/3/4; the `match N` folds to per-length shifts at compile time.
#[cfg(feature = "simd")]
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn decode_utf8_n<const N: usize>(b: &[u8]) -> u32 {
    match N {
        2 => ((b[0] as u32 & 0x1f) << 6) | (b[1] as u32 & 0x3f),
        3 => ((b[0] as u32 & 0x0f) << 12) | ((b[1] as u32 & 0x3f) << 6) | (b[2] as u32 & 0x3f),
        _ => {
            ((b[0] as u32 & 0x07) << 18)
                | ((b[1] as u32 & 0x3f) << 12)
                | ((b[2] as u32 & 0x3f) << 6)
                | (b[3] as u32 & 0x3f)
        }
    }
}

/// Whether `second` fits `lead`; the four constrained leads (`0xE0`, `0xED`,
/// `0xF0`, `0xF4`) reject the non-shortest and surrogate ranges.
#[inline]
const fn utf8_second_byte_valid(lead: u8, second: u8) -> bool {
    match lead {
        0xe0 => second >= 0xa0 && second <= 0xbf,
        0xed => second >= 0x80 && second <= 0x9f,
        0xf0 => second >= 0x90 && second <= 0xbf,
        0xf4 => second >= 0x80 && second <= 0x8f,
        _ => true,
    }
}

/// If `s` ends with a partial UTF-8 sequence, return its length and the
/// partial to carry into the next call.
pub(crate) const fn trailing_partial(s: &[u8]) -> Option<(usize, PartialUtf8)> {
    let len = s.len();
    if len == 0 {
        return None;
    }
    let last = s[len - 1];
    if last < 0x80 {
        return None; // complete ASCII
    }
    if matches!(last, 0xc2..=0xf4) {
        // A lead byte at the very end.
        let (expected, cp) = utf8_lead(last);
        return Some((
            len - 1,
            PartialUtf8 {
                cp,
                need: (expected - 1) as u8,
            },
        ));
    }
    // Not a valid lead nor a continuation: the main loop decodes it as U+FFFD.
    if !matches!(last, 0x80..0xc0) {
        return None;
    }
    // Walk back over continuation bytes to the lead byte.
    let mut back = 1;
    while back < MAX_UTF8_CONTINUATIONS && len > back {
        let b = s[len - 1 - back];
        if matches!(b, 0x80..0xc0) {
            back += 1;
        } else {
            break;
        }
    }
    // All continuation bytes and no lead byte → not a partial, but invalid
    // UTF-8; let the main loop decode it as U+FFFD.
    if back >= len {
        return None;
    }
    let lead_idx = len - 1 - back;
    let lead = s[lead_idx];
    let (expected, lead_cp) = utf8_lead(lead);
    let have = back + 1;
    if expected == 0 || have >= expected {
        return None;
    }
    // An out-of-range second byte (`0xF4 0xAD`, `0xE0 0x80`, …) is invalid
    // UTF-8, not a truncated partial; the main loop decodes it as U+FFFD.
    if have >= 2 && !utf8_second_byte_valid(lead, s[lead_idx + 1]) {
        return None;
    }
    let mut cp = lead_cp;
    let mut idx = lead_idx + 1;
    while idx < s.len() {
        cp = (cp << 6) | (s[idx] & 0x3f) as u32;
        idx += 1;
    }
    Some((
        lead_idx,
        PartialUtf8 {
            cp,
            need: (expected - have) as u8,
        },
    ))
}

/// Flush `buf[..n]` via `print_codepoints` if non-empty, resetting `n`.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn flush_cps<P: Perform>(p: &mut P, buf: &[u32], n: &mut usize) {
    if *n == 0 {
        return;
    }
    p.print_codepoints(&buf[..*n]);
    *n = 0;
}

/// Push one code point into the batch, flushing via `print_codepoints` when
/// the 32-slot buffer fills.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn push_cp<P: Perform>(p: &mut P, buf: &mut [u32; 32], n: &mut usize, cp: u32) {
    buf[*n] = cp;
    *n += 1;
    if *n == buf.len() {
        p.print_codepoints(&buf[..*n]);
        *n = 0;
    }
}

/// Scalar mixed-window loop: ASCII runs via `print_bytes`, multi-byte via `print_codepoints`.
#[cfg(all(feature = "simd", not(feature = "simd-transcode")))]
#[cfg_attr(feature = "perf-inline", inline(always))]
fn scalar_mixed_loop<P: Perform>(p: &mut P, s: &[u8]) -> usize {
    let mut i = 0;
    while i < s.len() {
        let b = s[i];
        match b {
            // ESC: leave it for the state-machine transition.
            0x1b => return i,
            // C0 control.
            0x00..=0x1f => {
                p.execute(b);
                i += 1;
            }
            // Printable ASCII run (0x20..=0x7e), SIMD-scanned, with the same
            // print+LF fusion as `advance_ground`.
            0x20..=0x7e => {
                let n = crate::simd::first_non_printable(&s[i..]);
                let end = i + n;
                if end + 1 < s.len() && s[end] == b'\n' {
                    p.print_line(&s[i..end]);
                    i = end + 1;
                } else if end + 2 < s.len() && s[end] == b'\r' && s[end + 1] == b'\n' {
                    p.print_crlf_line(&s[i..end]);
                    i = end + 2;
                } else {
                    p.print_bytes(&s[i..end]);
                    i = end;
                }
            }
            // DEL is a C0 control (destructive backspace).
            0x7f => {
                p.execute(0x7f);
                i += 1;
            }
            // Validated multi-byte lead: batch a run of code points into one
            // `print_codepoints` call.
            _ => {
                const BATCH: usize = 32;
                let mut buf = [0_u32; BATCH];
                let mut n = 0;
                loop {
                    let b0 = s[i];
                    if matches!(b0, 0xe0..=0xef) {
                        // 3-byte lead, never C1; four in a row take simdutf's `0x924` path.
                        if i + 16 <= s.len()
                            && matches!(s[i + 3], 0xe0..=0xef)
                            && matches!(s[i + 6], 0xe0..=0xef)
                            && matches!(s[i + 9], 0xe0..=0xef)
                            && n + 4 <= BATCH
                        {
                            let cps = crate::simd::decode_3byte_dispatch(&s[i..i + 16]);
                            buf[n..n + 4].copy_from_slice(&cps);
                            n += 4;
                            // The 4-wide fill bypasses `push_cp`'s flush-at-32 guard, so flush here.
                            if n == BATCH {
                                p.print_codepoints(&buf[..n]);
                                n = 0;
                            }
                            i += 12;
                        } else {
                            push_cp(p, &mut buf, &mut n, decode_utf8_n::<3>(&s[i..]));
                            i += 3;
                        }
                    } else {
                        // 2- or 4-byte lead.
                        let (cp, len) = match b0 {
                            0xf0..=0xf4 => (decode_utf8_n::<4>(&s[i..]), 4),
                            _ => (decode_utf8_n::<2>(&s[i..]), 2),
                        };
                        if cp <= 0x9f {
                            flush_cps(p, &buf, &mut n);
                            p.execute(cp as u8);
                        } else {
                            push_cp(p, &mut buf, &mut n, cp);
                        }
                        i += len;
                    }
                    if i >= s.len() || s[i] < 0x80 {
                        break;
                    }
                }
                flush_cps(p, &buf, &mut n);
            }
        }
    }
    i
}

/// Decode validated UTF-8 in blocks, dispatching a control-free run with one call.
#[cfg(feature = "simd-transcode")]
#[cfg_attr(feature = "perf-inline", inline(always))]
fn transcode_dispatch<P: Perform>(p: &mut P, s: &[u8]) -> usize {
    // Worst case: a 64-byte block emits <= 54 and the scalar tail 1 per byte.
    const BATCH: usize = 76;
    let mut buf = [0_u32; BATCH];
    let mut i = 0;
    while i < s.len() {
        let (count, consumed) = crate::simd::decode_block_dispatch(&s[i..], &mut buf);
        // A control-free block dispatches the whole run in one call.
        if !crate::simd::any_control(&buf[..count]) {
            p.print_codepoints(&buf[..count]);
        } else {
            let mut start = 0;
            for k in 0..count {
                let cp = buf[k];
                if matches!(cp, ..0x20 | 0x7f | 0x80..=0x9f) {
                    if start < k {
                        p.print_codepoints(&buf[start..k]);
                    }
                    p.execute(cp as u8);
                    start = k + 1;
                }
            }
            if start < count {
                p.print_codepoints(&buf[start..count]);
            }
        }
        i += consumed;
    }
    i
}

/// Decode multi-byte UTF-8 starting at `bytes[pos]` (a lead byte `>= 0xA0`),
/// returning bytes consumed (at least 1).
///
/// With `simd`, a window is validated by `simdutf8` and dispatched in bulk;
/// scalar decodes one code point at a time. The window ends at the next `ESC`,
/// which is never a continuation byte, so the escape state always sees it.
///
/// Both paths follow W3C "Substitution of Maximal Subparts": a truncated or
/// ill-formed sequence yields one U+FFFD, and a standalone C1 byte
/// (`0x80..=0x9f`) dispatches through [`Perform::execute`], as in vte and
/// rio-vt.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn decode_utf8<P: Perform>(
    p: &mut P,
    bytes: &[u8],
    valid: usize,
    pos: usize,
    simd: bool,
) -> usize {
    #[cfg(feature = "simd")]
    if simd {
        // Probe for a nearby ESC before paying for SIMD setup; the probe also
        // bounds `from_utf8` to `PROBE` bytes.
        const PROBE: usize = 32;
        let remaining = valid - pos;
        let esc = crate::scan::first_esc_bounded(&bytes[pos..valid], PROBE);
        let short = esc < remaining.min(PROBE) || remaining < PROBE;
        // The window ends at the next ESC (it must reach the escape state, not
        // `execute`) or at the input end; long windows cap at 4096.
        let end = if short {
            if esc < remaining { pos + esc } else { valid }
        } else {
            let mut end = (pos + crate::limits::SIMD_UTF8_CHUNK).min(valid);
            let rel = crate::simd::first_esc(&bytes[pos..end]);
            if rel < end - pos {
                end = pos + rel;
            }
            if end < valid {
                // Back off to a UTF-8 char boundary so a sequence is never split.
                let start = end;
                while end > pos && (bytes[end] & 0xC0) == 0x80 {
                    end -= 1;
                }
                if end == pos {
                    end = start;
                }
            }
            end
        };
        if let Ok(s) = simdutf8::basic::from_utf8(&bytes[pos..end]) {
            let s = s.as_bytes();
            // SIMD batches a mixed window into one `print_codepoints`; the scalar mixed
            // loop keeps the `print_bytes`/`print_codepoints` split.
            #[cfg(feature = "simd-transcode")]
            {
                return transcode_dispatch(p, s).max(1);
            }
            #[cfg(not(feature = "simd-transcode"))]
            {
                return scalar_mixed_loop(p, s).max(1);
            }
        }
        // Fall through to the scalar path (invalid input).
    }
    #[cfg(not(feature = "simd"))]
    let _ = simd;
    // Scalar: batch consecutive non-ASCII code points into one
    // `print_codepoints`; stop below `0xa0` for the caller's dispatch.
    const BATCH: usize = 32;
    let mut buf = [0_u32; BATCH];
    let mut n = 0;
    let mut it = utf8_iter::Utf8Chars::new(&bytes[pos..valid]);
    while let Some(c) = it.next() {
        let cp = u32::from(c);
        if cp < 0xa0 {
            // A C1 control (U+0080..=U+009F) decoded from its 2-byte form.
            flush_cps(p, &buf, &mut n);
            p.execute(cp as u8);
        } else {
            push_cp(p, &mut buf, &mut n, cp);
        }
        let rest = it.as_slice();
        if rest.is_empty() || rest[0] < 0xa0 {
            break;
        }
    }
    flush_cps(p, &buf, &mut n);
    // The consumed byte count is not `c.len_utf8()` for invalid UTF-8
    // (U+FFFD is 3 bytes but a lone byte consumes 1).
    let consumed = (valid - pos) - it.as_slice().len();
    consumed.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "simd-transcode")]
    #[test]
    fn decode_utf8_batches_short_mixed_window() {
        struct Sink {
            cps: Vec<u32>,
            calls: usize,
            bytes: Vec<u8>,
            execs: Vec<u8>,
        }
        impl Perform for Sink {
            fn print_codepoints(&mut self, cps: &[u32]) {
                self.cps.extend_from_slice(cps);
                self.calls += 1;
            }
            fn print_bytes(&mut self, s: &[u8]) {
                self.bytes.extend_from_slice(s);
            }
            fn execute(&mut self, b: u8) {
                self.execs.push(b);
            }
        }

        // A short mixed window decodes into one print_codepoints run
        // (ASCII + CJK + emoji together), not per-boundary callbacks.
        let input = "日本語 🚀 ".as_bytes();
        let mut sink = Sink {
            cps: vec![],
            calls: 0,
            bytes: vec![],
            execs: vec![],
        };
        let consumed = decode_utf8(&mut sink, input, input.len(), 0, true);
        assert_eq!(consumed, input.len());
        assert_eq!(sink.calls, 1, "one batched print_codepoints");
        let expected: Vec<u32> = "日本語 🚀 ".chars().map(|c| c as u32).collect();
        assert_eq!(sink.cps, expected);
        assert!(sink.execs.is_empty());
        assert!(sink.bytes.is_empty());

        // An ESC inside the probe window bounds the decode, leaving the ESC for the
        // state machine.
        let mut input = "日本語 🚀 ".as_bytes().to_vec();
        input.extend_from_slice(b"\x1b[31m");
        let mut sink = Sink {
            cps: vec![],
            calls: 0,
            bytes: vec![],
            execs: vec![],
        };
        let consumed = decode_utf8(&mut sink, &input, input.len(), 0, true);
        assert_eq!(consumed, "日本語 🚀 ".len());
        assert_eq!(sink.cps, expected);
    }

    #[test]
    fn utf8_lead_lengths_and_bits() {
        assert_eq!(utf8_lead(0x00), (1, 0x00));
        assert_eq!(utf8_lead(0x7f), (1, 0x7f));
        assert_eq!(utf8_lead(0xc2), (2, 0x02));
        assert_eq!(utf8_lead(0xdf), (2, 0x1f));
        assert_eq!(utf8_lead(0xe0), (3, 0x00));
        assert_eq!(utf8_lead(0xef), (3, 0x0f));
        assert_eq!(utf8_lead(0xf0), (4, 0x00));
        assert_eq!(utf8_lead(0xf4), (4, 0x04));
        // Continuation, overlong, and out-of-range bytes are invalid leads.
        assert_eq!(utf8_lead(0x80), (0, 0));
        assert_eq!(utf8_lead(0xbf), (0, 0));
        assert_eq!(utf8_lead(0xc0), (0, 0));
        assert_eq!(utf8_lead(0xc1), (0, 0));
        assert_eq!(utf8_lead(0xf5), (0, 0));
        assert_eq!(utf8_lead(0xff), (0, 0));
    }

    #[test]
    fn utf8_second_byte_constraints() {
        // 0xE0: only 0xA0..=0xBF (no overlong).
        assert!(!utf8_second_byte_valid(0xe0, 0x9f));
        assert!(utf8_second_byte_valid(0xe0, 0xa0));
        assert!(utf8_second_byte_valid(0xe0, 0xbf));
        assert!(!utf8_second_byte_valid(0xe0, 0xc0));
        // 0xED: only 0x80..=0x9F (no surrogates).
        assert!(!utf8_second_byte_valid(0xed, 0x7f));
        assert!(utf8_second_byte_valid(0xed, 0x80));
        assert!(utf8_second_byte_valid(0xed, 0x9f));
        assert!(!utf8_second_byte_valid(0xed, 0xa0));
        // 0xF0: only 0x90..=0xBF (no overlong).
        assert!(!utf8_second_byte_valid(0xf0, 0x8f));
        assert!(utf8_second_byte_valid(0xf0, 0x90));
        assert!(utf8_second_byte_valid(0xf0, 0xbf));
        // 0xF4: only 0x80..=0x8F (no > U+10FFFF).
        assert!(utf8_second_byte_valid(0xf4, 0x80));
        assert!(utf8_second_byte_valid(0xf4, 0x8f));
        assert!(!utf8_second_byte_valid(0xf4, 0x90));
        // Unconstrained leads accept any continuation.
        assert!(utf8_second_byte_valid(0xe1, 0x80));
        assert!(utf8_second_byte_valid(0xc2, 0x80));
    }

    #[test]
    fn trailing_partial_detects_truncation() {
        // Empty and ASCII tails have no partial.
        assert_eq!(trailing_partial(b""), None);
        assert_eq!(trailing_partial(b"abc"), None);
        // A lead byte at the very end is a partial.
        assert_eq!(
            trailing_partial(b"ab\xe4"),
            Some((2, PartialUtf8 { cp: 0x04, need: 2 }))
        );
        // Invalid and overlong bytes are not partials.
        assert_eq!(trailing_partial(b"a\xff"), None);
        assert_eq!(trailing_partial(b"a\xc0"), None);
        assert_eq!(trailing_partial(b"\xc0\x80"), None);
        // All continuations and no lead is not a partial.
        assert_eq!(trailing_partial(b"\x80\x80"), None);
        assert_eq!(trailing_partial(b"\x80\x80\x80\x80"), None);
        // A truncated 3-byte sequence carries its code point forward.
        assert_eq!(
            trailing_partial(b"\xe4\xb8"),
            Some((0, PartialUtf8 { cp: 0x138, need: 1 }))
        );
        // A truncated 4-byte sequence (the first three bytes of U+1F600).
        assert_eq!(
            trailing_partial(b"\xf0\x9f\x98"),
            Some((0, PartialUtf8 { cp: 0x7d8, need: 1 }))
        );
        // A complete sequence is not a partial.
        assert_eq!(trailing_partial(b"\xe4\xb8\x80"), None);
        // Out-of-range second bytes are invalid, not partial.
        assert_eq!(trailing_partial(b"\xf4\x90\x80"), None);
        assert_eq!(trailing_partial(b"\xe0\x80"), None);
    }
}
