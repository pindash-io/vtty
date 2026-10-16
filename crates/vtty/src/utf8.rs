// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Incremental UTF-8 decoding for the byte parser.

use vtty_types::Perform;

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

/// Decode `N` validated UTF-8 bytes (lead + `N-1` continuations). `N` is
/// 2/3/4; the `match N` folds to per-length shifts at compile time.
#[cfg(feature = "simd")]
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
/// leads and unrestricted 3/4-byte leads accept any continuation; the four
/// constrained leads (`0xE0`, `0xED`, `0xF0`, `0xF4`) have narrower ranges
/// (UTF-8's non-shortest/non-surrogate/non-overlong rules).
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

/// If `s` ends with a partial UTF-8 sequence, return
/// `(valid_prefix_len, partial_codepoint, remaining_continuation_bytes)`;
/// the partial (u32) is carried to the next `advance` call.
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
/// `>= 0xA0`). Returns bytes consumed (at least 1).
///
/// With `simd`, SIMD-validates a window via `simdutf8` and dispatches the
/// whole window (ASCII + non-ASCII) at once; the scalar path decodes one code
/// point at a time. The window is capped at the next ESC — ESC is never a
/// UTF-8 continuation byte, so it is a safe cut point, and without the cap an
/// emoji-before-ESC stream re-validates the same window per char (quadratic).
///
/// Both paths follow W3C "Substitution of Maximal Subparts": a truncated or
/// ill-formed sequence becomes one U+FFFD, and a standalone C1 byte
/// (`0x80..=0x9f`) is dispatched via [`Perform::execute`] — the same contract
/// as vte and rio-vt.
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
        // A lone char before the next ESC (truecolor SGR) is cheaper on the
        // scalar path: probe for a nearby ESC before paying for SIMD setup.
        const PROBE: usize = 32;
        let probe_end = valid.min(pos + PROBE);
        let short = bytes[pos..probe_end].contains(&0x1b) || valid - pos < PROBE;
        if !short {
            const CHUNK: usize = 4096;
            let mut end = (pos + CHUNK).min(valid);
            // Cap the window at the next ESC: validating past it re-validates
            // the same window per isolated multi-byte char (quadratic). ESC is
            // never a UTF-8 continuation, so it is a safe boundary.
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
            if let Ok(s) = simdutf8::basic::from_utf8(&bytes[pos..end]) {
                let s = s.as_bytes();
                #[cfg(feature = "simd-transcode")]
                {
                    // Unified SIMD transcode (rio-vt/ghostty): 64-byte blocks,
                    // one end-of-code-point mask each, 12-byte chunks with the
                    // `0x924` fast path. C0/C1 split out via `execute`; the rest
                    // is one `print_codepoints` run.
                    // Buffer holds the max code points one `decode_block` call
                    // can emit: the sub-76-byte scalar tail decodes 1 code
                    // point/byte (≤ 75), and the 64-byte block emits ≤ 54.
                    const BATCH: usize = 76;
                    let mut buf = [0_u32; BATCH];
                    let mut i = 0;
                    while i < s.len() {
                        let (count, consumed) =
                            crate::simd::decode_block_dispatch(&s[i..], &mut buf);
                        // Fast path: when the block has no controls, dispatch
                        // the whole run with one `print_codepoints` call
                        // instead of the per-code-point split scan.
                        if !crate::simd::any_control(&buf[..count]) {
                            p.print_codepoints(&buf[..count]);
                        } else {
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
                        }
                        i += consumed;
                    }
                    return end - pos;
                }

                #[cfg(not(feature = "simd-transcode"))]
                {
                    // Scalar byte loop: ASCII runs via `print_bytes`, multi-byte
                    // via `print_codepoints`, C0/C1 via `execute`; ESC ends the
                    // chunk for the state machine.
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
                            // Printable ASCII run (0x20..=0x7e) — SIMD-scanned.
                            0x20..=0x7e => {
                                let n = crate::simd::first_non_printable(&s[i..]);
                                p.print_bytes(&s[i..i + n]);
                                i += n;
                            }
                            // DEL is a C0 control (destructive backspace).
                            0x7f => {
                                p.execute(0x7f);
                                i += 1;
                            }
                            // Validated multi-byte lead: batch a run of code
                            // points into one `print_codepoints` call.
                            _ => {
                                const BATCH: usize = 32;
                                let mut buf = [0_u32; BATCH];
                                let mut n = 0;
                                loop {
                                    let b0 = s[i];
                                    if (0xe0..=0xef).contains(&b0) {
                                        // 3-byte lead (CJK etc.) — never C1.
                                        // Four consecutive 3-byte code points
                                        // are SIMD-decoded (simdutf's `0x924`
                                        // path).
                                        if i + 16 <= s.len()
                                            && (0xe0..=0xef).contains(&s[i + 3])
                                            && (0xe0..=0xef).contains(&s[i + 6])
                                            && (0xe0..=0xef).contains(&s[i + 9])
                                            && n + 4 <= BATCH
                                        {
                                            let cps =
                                                crate::simd::decode_3byte_dispatch(&s[i..i + 16]);
                                            buf[n..n + 4].copy_from_slice(&cps);
                                            n += 4;
                                            i += 12;
                                        } else {
                                            buf[n] = decode_utf8_n::<3>(&s[i..]);
                                            n += 1;
                                            if n == BATCH {
                                                p.print_codepoints(&buf[..n]);
                                                n = 0;
                                            }
                                            i += 3;
                                        }
                                    } else {
                                        // 2- or 4-byte lead.
                                        let (cp, len) = match b0 {
                                            0xf0..=0xf4 => (decode_utf8_n::<4>(&s[i..]), 4),
                                            _ => (decode_utf8_n::<2>(&s[i..]), 2),
                                        };
                                        if cp <= 0x9f {
                                            if n > 0 {
                                                p.print_codepoints(&buf[..n]);
                                                n = 0;
                                            }
                                            p.execute(cp as u8);
                                        } else {
                                            buf[n] = cp;
                                            n += 1;
                                            if n == BATCH {
                                                p.print_codepoints(&buf[..n]);
                                                n = 0;
                                            }
                                        }
                                        i += len;
                                    }
                                    if i >= s.len() || s[i] < 0x80 {
                                        break;
                                    }
                                }
                                if n > 0 {
                                    p.print_codepoints(&buf[..n]);
                                }
                            }
                        }
                    }
                    return end - pos;
                }
            }
        }
        // fall through to the scalar path (short window / invalid input).
    }
    #[cfg(not(feature = "simd"))]
    let _ = simd;
    // Scalar decode: batch a run of consecutive non-ASCII code points into a
    // single `print_codepoints` call. Stops at the first byte below `0xa0`
    // (ASCII, C0/C1, ESC), which the caller's dispatch arms handle.
    const BATCH: usize = 32;
    let mut buf = [0_u32; BATCH];
    let mut n = 0;
    let mut it = utf8_iter::Utf8Chars::new(&bytes[pos..valid]);
    while let Some(c) = it.next() {
        let cp = u32::from(c);
        if cp < 0xa0 {
            // A C1 control (U+0080..=U+009F) decoded from its 2-byte form.
            if n > 0 {
                p.print_codepoints(&buf[..n]);
                n = 0;
            }
            p.execute(cp as u8);
        } else {
            buf[n] = cp;
            n += 1;
            if n == BATCH {
                p.print_codepoints(&buf[..n]);
                n = 0;
            }
        }
        let rest = it.as_slice();
        if rest.is_empty() || rest[0] < 0xa0 {
            break;
        }
    }
    if n > 0 {
        p.print_codepoints(&buf[..n]);
    }
    // The consumed byte count is NOT `c.len_utf8()` for invalid UTF-8
    // (U+FFFD is 3 bytes but a lone byte consumes 1).
    let consumed = (valid - pos) - it.as_slice().len();
    consumed.max(1)
}
