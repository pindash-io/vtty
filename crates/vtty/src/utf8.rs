// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Incremental UTF-8 decoding for the byte parser.

use crate::perform::Perform;

/// Expected byte length and initial code-point bits for a UTF-8 lead byte —
/// the two facts the partial-decoder needs, from one range dispatch. Returns
/// `(expected_len, lead_bits)`; `expected_len` is `0` for invalid
/// (`0x80..=0xc1`, `0xf5..=0xff`) and continuation (`0x80..=0xbf`) bytes.
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

/// Decode `N` validated UTF-8 bytes (lead + `N-1` continuations) into the
/// raw code point. `N` must be 2/3/4; the lead contributes its low
/// 0x1f/0x0f/0x07 bits and each continuation its low 6 bits. `N` is a const
/// generic, so the `match N` folds to the exact per-length decode at compile
/// time (no loop, no length dispatch at runtime).
#[inline(always)]
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

/// Whether `second` is a valid second byte for a 3/4-byte `lead`. 2-byte
/// leads and the unrestricted 3/4-byte leads accept any continuation byte;
/// the four constrained leads (`0xE0`, `0xED`, `0xF0`, `0xF4`) have narrower
/// ranges (UTF-8's non-shortest / non-surrogate / non-overlong rules).
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

/// If `s` ends with a partial (incomplete) UTF-8 sequence, return
/// `(valid_prefix_len, partial_codepoint, remaining_continuation_bytes)`.
/// The partial code point (u32) is carried to the next `advance` call, which
/// accumulates the remaining continuation bytes.
pub(crate) fn trailing_partial(s: &[u8]) -> Option<(usize, u32, u8)> {
    let len = s.len();
    if len == 0 {
        return None;
    }
    let last = s[len - 1];
    if last < 0x80 {
        return None; // complete ASCII
    }
    if (0xc2..=0xf4).contains(&last) {
        // A lead byte at the very end.
        let (expected, cp) = utf8_lead(last);
        return Some((len - 1, cp, (expected - 1) as u8));
    }
    // `last` is neither ASCII, a valid lead, nor a continuation byte — it is
    // an invalid byte (0xC0/0xC1/0xF5..=0xFF). Not a trailing partial; the
    // main loop decodes it as U+FFFD.
    if !(0x80..0xc0).contains(&last) {
        return None;
    }
    // Walk back over continuation bytes to the lead byte.
    let mut back = 1;
    while back < 3 && len > back {
        let b = s[len - 1 - back];
        if (0x80..0xc0).contains(&b) {
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
    for &b in &s[lead_idx + 1..] {
        cp = (cp << 6) | (b & 0x3f) as u32;
    }
    Some((lead_idx, cp, (expected - have) as u8))
}

/// Decode a run of multi-byte UTF-8 starting at `bytes[pos]` (a lead byte
/// `>= 0xA0`), dispatching printable runs via `print_bytes`/`print_codepoints`
/// and controls via `execute`.
/// Returns the number of bytes consumed (at least 1).
///
/// With the `simd` feature this SIMD-validates a window via `simdutf8` and
/// dispatches the WHOLE window (ASCII + non-ASCII) — like rio-vt — so the
/// interleaved ASCII is not re-scanned. The window is `CHUNK` bytes, capped at
/// the next ESC (see below). The scalar path decodes one code point at a time
/// via `utf8_iter`.
///
/// # Why cap at ESC?
///
/// The dispatch loop returns at the first ESC so the state machine can handle
/// the next sequence. Without the cap, `simdutf8` validates the full `CHUNK`
/// window even when the ESC is a few bytes away, so a stream of *isolated*
/// multi-byte chars each followed by an ESC (e.g. an emoji inside bracketed
/// paste: `…🚀\x1b[201~…🚀\x1b[200~…`) re-validates the same window per char —
/// O(n × CHUNK), i.e. quadratic. Capping at the ESC makes the window exactly
/// the bytes the loop will consume. ESC is always a safe cut point: it is not
/// a UTF-8 continuation byte, so the bytes before it are a complete sequence.
///
/// # Malformed UTF-8 (maximal subparts)
///
/// Both the scalar and SIMD paths follow W3C "Substitution of Maximal
/// Subparts": a truncated or ill-formed sequence is replaced with a single
/// U+FFFD spanning its maximal subpart, and a standalone C1 byte
/// (`0x80..=0x9f`) is dispatched via [`Perform::execute`] instead of being
/// replaced. This is the same contract as vte and rio-vt, so a run like
/// `E3 83` `[` `0x88` (truncated 3-byte lead, printable ASCII, C1
/// continuation) decodes identically in every parser: `Print('�')`,
/// `Print('[')`, `Execute(136)`. The scalar path inherits this from
/// `utf8_iter`; the SIMD path validates the window first and only falls back
/// to scalar decoding when the window is short or invalid.
#[inline(always)]
pub(crate) fn decode_utf8<P: Perform>(
    p: &mut P,
    bytes: &[u8],
    valid: usize,
    pos: usize,
    simd: bool,
) -> usize {
    #[cfg(feature = "simd")]
    if simd {
        // A lone char before the next ESC (truecolor-style SGR) is cheaper to
        // decode with the scalar `utf8_iter` path below: probe for a nearby
        // ESC before paying the `first_esc` dispatch + `simdutf8` setup.
        const PROBE: usize = 32;
        let probe_end = valid.min(pos + PROBE);
        let short = bytes[pos..probe_end].contains(&0x1b) || valid - pos < PROBE;
        if !short {
            const CHUNK: usize = 4096;
            let mut end = (pos + CHUNK).min(valid);
            // Cap the validated window at the next ESC: the dispatch loop below
            // returns at ESC anyway, so validating past it re-validates the same
            // window for every isolated multi-byte char (an emoji inside
            // bracketed paste) — quadratic on such streams. SIMD-scanned so the
            // cap costs ~nothing on ESC-free UTF-8 runs. ESC is a safe boundary:
            // it is never a UTF-8 continuation byte, so `bytes[pos..end]` is a
            // complete code-point sequence.
            let rel = crate::simd::first_esc(&bytes[pos..end]);
            if rel < end - pos {
                end = pos + rel;
            }
            if end < valid {
                // Back off to a UTF-8 char boundary so a multi-byte sequence is
                // never split across the chunk.
                let start = end;
                while end > pos && (bytes[end] & 0xC0) == 0x80 {
                    end -= 1;
                }
                if end == pos {
                    end = start;
                }
            }
            if let Ok(s) = simdutf8::basic::from_utf8(&bytes[pos..end]) {
                // Unified SIMD transcode (rio-vt/ghostty shape): process the
                // ESC-capped, validated window in 64-byte blocks — one
                // end-of-code-point mask per block — then 12-byte chunks with
                // the `0x924` 3-byte fast path. C0/C1 controls are split out
                // and executed inline; the rest is one `print_codepoints` run.
                let s = s.as_bytes();
                // Buffer holds the max code points one `decode_block` call can
                // emit: the sub-76-byte scalar tail decodes 1 code point/byte
                // (≤ 75), and the 64-byte SIMD block emits ≤ 54.
                const BATCH: usize = 76;
                let mut buf = [0_u32; BATCH];
                let mut i = 0;
                while i < s.len() {
                    let (count, consumed) = crate::simd::decode_block_dispatch(&s[i..], &mut buf);
                    let mut start = 0;
                    for k in 0..count {
                        let cp = buf[k];
                        if cp < 0x20 || cp == 0x7f || (0x80..=0x9f).contains(&cp) {
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
                    i += consumed;
                }
                return end - pos;
            }
        }
        // fall through to the scalar path (short window / invalid input).
    }
    #[cfg(not(feature = "simd"))]
    let _ = simd;
    let mut it = utf8_iter::Utf8Chars::new(&bytes[pos..valid]);
    // `next()` returns `Some` for non-empty input; the `None` arm is
    // unreachable but keeps this panic-free.
    let Some(c) = it.next() else {
        return 1;
    };
    // The consumed byte count is NOT `c.len_utf8()` for invalid UTF-8
    // (U+FFFD is 3 bytes but a lone byte consumes 1).
    let consumed = (valid - pos) - it.as_slice().len();
    if matches!(c, '\u{80}'..='\u{9f}') {
        p.execute(c as u8);
    } else {
        p.print(c);
    }
    consumed.max(1)
}
