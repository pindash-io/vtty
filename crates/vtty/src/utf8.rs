// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Incremental UTF-8 decoding for the byte parser.

use vtty_core::Perform;

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

/// Flush `buf[..n]` via `print_codepoints` if non-empty, resetting `n`.
#[inline(always)]
fn flush_cps<P: Perform>(p: &mut P, buf: &[u32], n: &mut usize) {
    if *n > 0 {
        p.print_codepoints(&buf[..*n]);
        *n = 0;
    }
}

/// Push one code point into the batch, flushing via `print_codepoints` when
/// the 32-slot buffer fills.
#[inline(always)]
fn push_cp<P: Perform>(p: &mut P, buf: &mut [u32; 32], n: &mut usize, cp: u32) {
    buf[*n] = cp;
    *n += 1;
    if *n == buf.len() {
        p.print_codepoints(&buf[..*n]);
        *n = 0;
    }
}

/// Decode multi-byte UTF-8 starting at `bytes[pos]` (a lead byte `>= 0xA0`).
/// Returns bytes consumed (at least 1).
///
/// With `simd`, validates a window via `simdutf8` and dispatches it in bulk
/// (ASCII + multi-byte together); scalar decodes one code point at a time.
/// The window is capped at the next ESC — ESC is never a UTF-8 continuation,
/// so it is a safe boundary that must reach the escape state rather than be
/// dispatched as `execute`.
///
/// Both paths follow W3C "Substitution of Maximal Subparts": a truncated or
/// ill-formed sequence yields one U+FFFD, and a standalone C1 byte
/// (`0x80..=0x9f`) dispatches via [`Perform::execute`] — the same contract
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
            // Cap at the next ESC: it is never a UTF-8 continuation, and it
            // must reach the escape state (not `execute`). Decode the whole
            // mixed window (ASCII + multi-byte) in one pass rather than one
            // SIMD setup per short non-ASCII run.
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
                            // Use the SAME print+LF fusion as `advance_ground`:
                            // a run followed by `\n`/`\r\n` dispatches the fused
                            // `print_line`/`print_crlf_line` (the cell/line
                            // screens specialize it), instead of `print_bytes` +
                            // two per-byte `execute`s. Without this the SIMD
                            // UTF-8 window loses the fusion for EVERY line after
                            // the first emoji in a chunk (2.6x more `execute`
                            // calls — the mixed-corpus SIMD slowdown).
                            0x20..=0x7e => {
                                let n = crate::simd::first_non_printable(&s[i..]);
                                let end = i + n;
                                if end + 1 < s.len() && s[end] == b'\n' {
                                    p.print_line(&s[i..end]);
                                    i = end + 1;
                                } else if end + 2 < s.len()
                                    && s[end] == b'\r'
                                    && s[end + 1] == b'\n'
                                {
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
                                            push_cp(
                                                p,
                                                &mut buf,
                                                &mut n,
                                                decode_utf8_n::<3>(&s[i..]),
                                            );
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
    // The consumed byte count is NOT `c.len_utf8()` for invalid UTF-8
    // (U+FFFD is 3 bytes but a lone byte consumes 1).
    let consumed = (valid - pos) - it.as_slice().len();
    consumed.max(1)
}
