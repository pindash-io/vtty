// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SIMD-accelerated byte-scan and UTF-8 decode helpers for the parser's hot
//! path. Compiled only with the `simd` feature (which also implies `std`).

#[cfg(not(target_arch = "aarch64"))]
use std::sync::OnceLock;

#[cfg(feature = "simd-transcode")]
use fearless_simd::u16x8;
use fearless_simd::{Level, Simd, dispatch, prelude::*, u8x16, u32x4};

#[cfg(feature = "simd-transcode")]
use crate::utf8::decode_utf8_n;
#[cfg(feature = "simd-transcode")]
use crate::utf8_tables::{SHUFUTF8, UTF8BIGINDEX};

/// Best SIMD level for this target. On arm64 NEON is mandatory, so
/// `Level::baseline()` is a compile-time constant (no `OnceLock` atomic load);
/// on x86 the level is detected once and cached.
#[inline(always)]
fn level() -> Level {
    #[cfg(target_arch = "aarch64")]
    {
        Level::baseline()
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        static LEVEL: OnceLock<Level> = OnceLock::new();
        *LEVEL.get_or_init(Level::new)
    }
}

/// Return the index of the first byte that is NOT printable ASCII
/// (`0x20..=0x7e` — DEL `0x7f` is a C0 control, dispatched via `execute`),
/// or `bytes.len()` if every byte is printable.
#[inline(always)]
pub(crate) fn first_non_printable(bytes: &[u8]) -> usize {
    // Fast path: on escape-dense streams the next byte is usually the match
    // (an ESC introducer or a high-bit multi-byte lead). One byte check costs
    // ~nothing on long printable runs and skips the dispatch + chunk setup.
    if let Some(&b) = bytes.first()
        && !(0x20..=0x7e).contains(&b)
    {
        return 0;
    }
    let level = level();
    dispatch!(level, simd => first_matching(simd, bytes, ScanKind::NonPrintable))
}

/// Return the index of the first ESC byte (`0x1b`), or `bytes.len()` if none.
/// Caps the SIMD UTF-8 decode window at the next sequence introducer — the
/// dispatch loop returns at ESC, so validating past it re-validates the same
/// window per isolated multi-byte char (bracketed-paste O(n²)).
#[inline(always)]
pub(crate) fn first_esc(bytes: &[u8]) -> usize {
    let level = level();
    dispatch!(level, simd => first_matching(simd, bytes, ScanKind::Esc))
}

/// What terminates the scan run.
#[derive(Clone, Copy)]
enum ScanKind {
    /// Non-printable (`b < 0x20 || b >= 0x7f`).
    NonPrintable,
    /// The ESC byte (`0x1b`) — the state-machine sequence introducer.
    Esc,
}

/// Reusable 16-bytes-at-a-time byte scan: return the index of the first
/// byte matching `kind`, or `bytes.len()` if none. The SIMD classifier runs
/// on full 16-byte chunks (`as_chunks`); the final (<16) tail falls back to a
/// scalar predicate.
#[inline(always)]
fn first_matching<S: Simd>(simd: S, bytes: &[u8], kind: ScanKind) -> usize {
    let (chunks, remainder) = bytes.as_chunks::<16>();
    let mut offset = 0;
    for chunk in chunks {
        let v = u8x16::from_slice(simd, chunk);
        let special = match kind {
            ScanKind::NonPrintable => {
                // b < 0x20 (control) or b >= 0x7f (DEL + high-bit).
                simd.simd_lt_u8x16(v, simd.splat_u8x16(0x20))
                    | simd.simd_ge_u8x16(v, simd.splat_u8x16(0x7f))
            }
            ScanKind::Esc => {
                // b == 0x1b.
                simd.simd_eq_u8x16(v, simd.splat_u8x16(0x1b))
            }
        };
        // Fast path: most chunks match nothing — skip the mask
        // extraction + first-hit reduction entirely.
        if simd.any_true_mask8x16(special) {
            let mask: [i8; 16] = special.into();
            return offset + first_set(mask);
        }
        offset += 16;
    }
    // Scalar tail (< 16 bytes).
    for (j, &b) in remainder.iter().enumerate() {
        if matches_kind(b, kind) {
            return offset + j;
        }
    }
    bytes.len()
}

/// The scalar predicate for one byte, shared by the SIMD classifier's tail.
#[inline]
fn matches_kind(b: u8, kind: ScanKind) -> bool {
    match kind {
        ScanKind::NonPrintable => !(0x20..=0x7e).contains(&b),
        ScanKind::Esc => b == 0x1b,
    }
}

/// Index of the first non-zero lane in a 16-lane mask (`0`/`-1`). Scalar
/// early-return — the bitmask + `trailing_zeros` alternative was slower here.
#[inline(always)]
fn first_set(mask: [i8; 16]) -> usize {
    for (j, &m) in mask.iter().enumerate() {
        if m != 0 {
            return j;
        }
    }
    16
}

/// SIMD-decode four consecutive 3-byte code points (12 bytes) into `[u32; 4]`.
/// Caller guarantees four validated 3-byte sequences. The simdutf `0x924`
/// fast path: one fixed shuffle gathers each 3-byte group into a u32 lane,
/// then mask + shift + or.
#[inline(always)]
fn decode_3byte<S: Simd>(simd: S, bytes: &[u8]) -> [u32; 4] {
    let v = u8x16::from_slice(simd, bytes);
    // Gather [2,1,0] [5,4,3] [8,7,6] [11,10,9] into four little-endian u32
    // lanes (0xff pads the high byte of each lane).
    let sh = u8x16::from_slice(
        simd,
        &[2, 1, 0, 0xff, 5, 4, 3, 0xff, 8, 7, 6, 0xff, 11, 10, 9, 0xff],
    );
    let perm = simd.swizzle_dyn_precise_u8x16(v, sh);
    let p: u32x4<S> = perm.bitcast();
    let ascii = p & simd.splat_u32x4(0x7f);
    let middle = simd.shr_u32x4(p & simd.splat_u32x4(0x3f00), 2);
    let high = simd.shr_u32x4(p & simd.splat_u32x4(0x0f0000), 4);
    ((ascii | middle) | high).into()
}

/// Decode four consecutive 3-byte code points (12 bytes) into `[u32; 4]` via
/// the runtime-dispatched SIMD level (used by the non-transcode scalar loop).
#[cfg(not(feature = "simd-transcode"))]
#[inline(always)]
pub(crate) fn decode_3byte_dispatch(bytes: &[u8]) -> [u32; 4] {
    let level = level();
    dispatch!(level, simd => decode_3byte(simd, bytes))
}

/// Decode up to 12 bytes of validated UTF-8 into `out`, using the pre-shifted
/// `end_mask` (bit `i` = byte `i` ends a code point). Returns
/// `(code_points_written, bytes_consumed)`. Mirrors simdutf's
/// `convert_masked_utf8_to_utf32` (`0x924` 3-byte fast path, then the table).
#[cfg(feature = "simd-transcode")]
#[inline(always)]
fn decode_utf8_12<S: Simd>(
    simd: S,
    bytes: &[u8],
    end_mask: u64,
    out: &mut [u32],
) -> (usize, usize) {
    let v = u8x16::from_slice(simd, bytes);
    let m12 = (end_mask & 0xfff) as usize;

    // 4 × 3-byte fast path.
    if m12 == 0x924 {
        let arr = decode_3byte(simd, bytes);
        out[..4].copy_from_slice(&arr);
        return (4, 12);
    }

    let idx = UTF8BIGINDEX[m12][0] as usize;
    let consumed = UTF8BIGINDEX[m12][1] as usize;
    if idx < 64 {
        // Six code points (1/2-byte mix): u16x8 → widen.
        let sh = u8x16::from_slice(simd, &SHUFUTF8[idx]);
        let perm = simd.swizzle_dyn_precise_u8x16(v, sh);
        let p: u16x8<S> = perm.bitcast();
        let ascii = p & simd.splat_u16x8(0x7f);
        let high = simd.shr_u16x8(p & simd.splat_u16x8(0x1f00), 2);
        let composed = ascii | high;
        let (lo, hi) = composed.widen();
        let lo: [u32; 4] = lo.into();
        let hi: [u32; 4] = hi.into();
        out[..4].copy_from_slice(&lo);
        out[4..6].copy_from_slice(&hi[..2]);
        (6, consumed)
    } else if idx < 145 {
        // Four code points (2/3-byte mix).
        let sh = u8x16::from_slice(simd, &SHUFUTF8[idx]);
        let perm = simd.swizzle_dyn_precise_u8x16(v, sh);
        let p: u32x4<S> = perm.bitcast();
        let ascii = p & simd.splat_u32x4(0x7f);
        let middle = simd.shr_u32x4(p & simd.splat_u32x4(0x3f00), 2);
        let high = simd.shr_u32x4(p & simd.splat_u32x4(0x0f0000), 4);
        let composed = (ascii | middle) | high;
        let arr: [u32; 4] = composed.into();
        out[..4].copy_from_slice(&arr);
        (4, consumed)
    } else if idx < 209 {
        // Three code points (4-byte involved).
        let sh = u8x16::from_slice(simd, &SHUFUTF8[idx]);
        let perm = simd.swizzle_dyn_precise_u8x16(v, sh);
        let p: u32x4<S> = perm.bitcast();
        let ascii = p & simd.splat_u32x4(0x7f);
        let middle = simd.shr_u32x4(p & simd.splat_u32x4(0x3f00), 2);
        let correct = simd.shr_u32x4(p & simd.splat_u32x4(0x400000), 1);
        let middlehigh = simd.shr_u32x4(correct ^ (p & simd.splat_u32x4(0x3f0000)), 4);
        let high = simd.shr_u32x4(p & simd.splat_u32x4(0x07000000), 6);
        let composed = (ascii | middle) | (high | middlehigh);
        let arr: [u32; 4] = composed.into();
        out[..3].copy_from_slice(&arr[..3]);
        (3, consumed)
    } else {
        (0, consumed)
    }
}

/// Transcode one 64-byte block of validated UTF-8 to UTF-32 (rio-vt/ghostty
/// shape: one end-of-code-point mask per block, then a 12-byte chunk loop).
/// Returns `(code_points_written, bytes_consumed)`; a full block consumes 52
/// bytes (the last 12 overlap into the next call), a sub-64-byte tail falls
/// back to scalar decode.
#[cfg(feature = "simd-transcode")]
#[inline(always)]
fn decode_block<S: Simd>(simd: S, bytes: &[u8], out: &mut [u32]) -> (usize, usize) {
    let n = bytes.len();
    let mut written = 0;
    let mut pos = 0;
    if n >= 76 {
        // 64-bit continuation mask from four 16-byte loads.
        let mut cont = 0_u64;
        for k in 0..4 {
            let v = u8x16::from_slice(simd, &bytes[k * 16..k * 16 + 16]);
            let m = simd.simd_eq_u8x16(v & simd.splat_u8x16(0xc0), simd.splat_u8x16(0x80));
            cont |= simd.to_bitmask_mask8x16(m) << (k * 16);
        }
        let mut end_mask = !(cont >> 1);
        // Process up to 52 bytes; the last 12 overlap into the next block.
        while pos < 52 {
            let (count, consumed) =
                decode_utf8_12(simd, &bytes[pos..pos + 16], end_mask, &mut out[written..]);
            written += count;
            pos += consumed;
            end_mask >>= consumed;
        }
        return (written, pos);
    }
    // Sub-64-byte tail: scalar decode.
    while pos < n {
        let b0 = bytes[pos];
        if b0 < 0x80 {
            out[written] = b0 as u32;
            written += 1;
            pos += 1;
        } else {
            let (cp, len) = if (0xe0..=0xef).contains(&b0) {
                (decode_utf8_n::<3>(&bytes[pos..]), 3)
            } else if (0xf0..=0xf4).contains(&b0) {
                (decode_utf8_n::<4>(&bytes[pos..]), 4)
            } else {
                (decode_utf8_n::<2>(&bytes[pos..]), 2)
            };
            out[written] = cp;
            written += 1;
            pos += len;
        }
    }
    (written, pos)
}

/// Runtime-dispatched wrapper for [`decode_block`].
#[cfg(feature = "simd-transcode")]
#[inline(always)]
pub(crate) fn decode_block_dispatch(bytes: &[u8], out: &mut [u32]) -> (usize, usize) {
    let level = level();
    dispatch!(level, simd => decode_block(simd, bytes, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_3byte_decodes_four_cjk() {
        // 中 (U+4E2D) 文 (U+6587) 日 (U+65E5) 本 (U+672C)
        let bytes = [
            0xe4, 0xb8, 0xad, 0xe6, 0x96, 0x87, 0xe6, 0x97, 0xa5, 0xe6, 0x9c, 0xac, 0, 0, 0, 0,
        ];
        let out = dispatch!(level(), simd => decode_3byte(simd, &bytes));
        assert_eq!(out, [0x4e2d, 0x6587, 0x65e5, 0x672c]);
    }

    #[test]
    #[cfg(feature = "simd-transcode")]
    fn decode_block_decodes_mixed_utf8() {
        // a日🚀e日 (ASCII + 3-byte + 4-byte + ASCII + 3-byte), plus padding to
        // reach the 64-byte block threshold.
        let s = "a日🚀e日";
        let mut bytes = s.as_bytes().to_vec();
        bytes.resize(128, b' ');
        let mut out = [0_u32; 76];
        let mut cps = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let (c, k) = decode_block_dispatch(&bytes[i..], &mut out);
            cps.extend_from_slice(&out[..c]);
            i += k;
        }
        let decoded: Vec<u32> = cps
            .into_iter()
            .take_while(|&cp| cp != b' ' as u32)
            .collect();
        let expected: [u32; 5] = ['a', '日', '🚀', 'e', '日'].map(|c| c as u32);
        assert_eq!(decoded.as_slice(), expected.as_slice());
    }

    #[test]
    fn first_set_finds_first_lane() {
        for j in 0..16 {
            let mut m = [0_i8; 16];
            m[j] = -1;
            assert_eq!(first_set(m), j, "lane {j}");
        }
        // Several set lanes: the first wins.
        let mut m = [0_i8; 16];
        m[3] = -1;
        m[11] = -1;
        assert_eq!(first_set(m), 3);
    }
}
