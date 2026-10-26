// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SIMD-accelerated byte-scan and UTF-8 decode helpers for the parser's hot
//! path. Compiled only with the `simd` feature (which also implies `std`).

#[cfg(feature = "simd-transcode")]
use fearless_simd::u16x8;
use fearless_simd::{Level, Simd, dispatch, mask8x16, prelude::*, u8x16, u32x4};

#[cfg(feature = "simd-transcode")]
use crate::utf8::decode_utf8_n;
#[cfg(feature = "simd-transcode")]
use crate::utf8_tables::{SHUFUTF8, UTF8BIGINDEX};

/// Return the index of the first byte that is NOT printable ASCII
/// (`0x20..=0x7e`; `DEL` `0x7f` is a C0 control dispatched via `execute`),
/// or `bytes.len()` if every byte is printable.
#[inline(always)]
pub(crate) fn first_non_printable(bytes: &[u8]) -> usize {
    // Fast path: the first byte is usually the match, skipping SIMD dispatch
    // and chunk setup on printable runs.
    if let Some(&b) = bytes.first()
        && !(0x20..=0x7e).contains(&b)
    {
        return 0;
    }
    // Second fast path: a one-byte printable run (the CSI-dense
    // `ESC [ … m X ESC …` pattern) skips the 16-byte SIMD scan.
    if let Some(&b) = bytes.get(1)
        && !(0x20..=0x7e).contains(&b)
    {
        return 1;
    }
    let level = Level::new();
    dispatch!(level, simd => first_matching(simd, bytes, ScanKind::NonPrintable))
}

/// Index of the first `ESC` byte (`0x1b`), or `bytes.len()` if none. Caps the
/// SIMD UTF-8 decode window at the next sequence introducer, which must
/// reach the escape state rather than be dispatched as `execute`.
#[inline(always)]
pub(crate) fn first_esc(bytes: &[u8]) -> usize {
    let level = Level::new();
    dispatch!(level, simd => first_matching(simd, bytes, ScanKind::Esc))
}

/// What terminates the scan run.
#[derive(Clone, Copy)]
enum ScanKind {
    /// Non-printable (`b < 0x20 || b >= 0x7f`).
    NonPrintable,
    /// The `ESC` byte (`0x1b`) — the state-machine sequence introducer.
    Esc,
}

/// Classify a 16-byte chunk: the lanes where `kind`'s scan terminator appears.
#[inline(always)]
fn classify_scan<S: Simd>(simd: S, v: u8x16<S>, kind: ScanKind) -> mask8x16<S> {
    match kind {
        ScanKind::NonPrintable => {
            // b < 0x20 (control) or b >= 0x7f (DEL + high-bit).
            simd.simd_lt_u8x16(v, simd.splat_u8x16(0x20))
                | simd.simd_ge_u8x16(v, simd.splat_u8x16(0x7f))
        }
        ScanKind::Esc => {
            // b == 0x1b.
            simd.simd_eq_u8x16(v, simd.splat_u8x16(0x1b))
        }
    }
}

/// Index of the first byte matching `kind`, or `bytes.len()` if none. Scans
/// 4×16-byte chunks per iteration so the independent loads pipeline; the
/// final (<16) tail falls back to the scalar predicate.
#[inline(always)]
fn first_matching<S: Simd>(simd: S, bytes: &[u8], kind: ScanKind) -> usize {
    let (chunks, remainder) = bytes.as_chunks::<16>();
    let mut offset = 0;

    // Unroll 4× (64 bytes/iter), classifying each chunk lazily so an early
    // match returns before the later chunks are loaded. The long printable
    // runs (the common no-match case) still pipeline: the loads are pure, so
    // the branch predictor speculates through the not-taken checks.
    let (quads, leftover) = chunks.as_chunks::<4>();
    for quad in quads {
        let m0 = classify_scan(simd, u8x16::from_slice(simd, &quad[0]), kind);
        if simd.any_true_mask8x16(m0) {
            return offset + first_set(m0.into());
        }
        let m1 = classify_scan(simd, u8x16::from_slice(simd, &quad[1]), kind);
        if simd.any_true_mask8x16(m1) {
            return offset + 16 + first_set(m1.into());
        }
        let m2 = classify_scan(simd, u8x16::from_slice(simd, &quad[2]), kind);
        if simd.any_true_mask8x16(m2) {
            return offset + 32 + first_set(m2.into());
        }
        let m3 = classify_scan(simd, u8x16::from_slice(simd, &quad[3]), kind);
        if simd.any_true_mask8x16(m3) {
            return offset + 48 + first_set(m3.into());
        }
        offset += 64;
    }

    // Leftover full chunks (0..=3).
    for chunk in leftover {
        let m = classify_scan(simd, u8x16::from_slice(simd, chunk), kind);
        if simd.any_true_mask8x16(m) {
            return offset + first_set(m.into());
        }
        offset += 16;
    }

    // Scalar tail (< 16 bytes).
    if let Some(j) = remainder.iter().position(|&b| matches_kind(b, kind)) {
        return offset + j;
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
    mask.iter().position(|&m| m != 0).unwrap_or(16)
}

/// Mask + shift + or a shuffled `u32x4` (the 3-byte `0x924` layout) into four
/// code points. Shared by [`decode_3byte`] and the 2/3-byte-mix table path.
#[inline(always)]
fn finish_u32x4_3byte<S: Simd>(simd: S, p: u32x4<S>) -> [u32; 4] {
    let ascii = p & simd.splat_u32x4(0x7f);
    let middle = simd.shr_u32x4(p & simd.splat_u32x4(0x3f00), 2);
    let high = simd.shr_u32x4(p & simd.splat_u32x4(0x0f0000), 4);
    ((ascii | middle) | high).into()
}

/// Decode four consecutive 3-byte code points (12 bytes). The simdutf
/// `0x924` fast path: one fixed shuffle gathers each 3-byte group into a u32
/// lane, then mask + shift + or.
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
    finish_u32x4_3byte(simd, perm.bitcast())
}

/// Decode four consecutive 3-byte code points (12 bytes) into `[u32; 4]` via
/// the runtime-dispatched SIMD level (used by the non-transcode scalar loop).
#[cfg(not(feature = "simd-transcode"))]
#[inline(always)]
pub(crate) fn decode_3byte_dispatch(bytes: &[u8]) -> [u32; 4] {
    let level = Level::new();
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
        let arr = finish_u32x4_3byte(simd, perm.bitcast());
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
        let (cp, len) = match b0 {
            0x00..=0x7f => (b0 as u32, 1),
            0xe0..=0xef => (decode_utf8_n::<3>(&bytes[pos..]), 3),
            0xf0..=0xf4 => (decode_utf8_n::<4>(&bytes[pos..]), 4),
            _ => (decode_utf8_n::<2>(&bytes[pos..]), 2),
        };
        out[written] = cp;
        written += 1;
        pos += len;
    }
    (written, pos)
}

/// Runtime-dispatched wrapper for [`decode_block`].
#[cfg(feature = "simd-transcode")]
#[inline(always)]
pub(crate) fn decode_block_dispatch(bytes: &[u8], out: &mut [u32]) -> (usize, usize) {
    let level = Level::new();
    dispatch!(level, simd => decode_block(simd, bytes, out))
}

/// Whether any code point in `cps` is a control (`< 0x20`, `0x7f`, or
/// `0x80..=0x9f`) — the ones the dispatch loop must split out via `execute`
/// instead of a single `print_codepoints` run.
#[cfg(feature = "simd-transcode")]
#[inline(always)]
pub(crate) fn any_control(cps: &[u32]) -> bool {
    let level = Level::new();
    dispatch!(level, simd => any_control_impl(simd, cps))
}

#[cfg(feature = "simd-transcode")]
fn any_control_impl<S: Simd>(simd: S, cps: &[u32]) -> bool {
    let (chunks, tail) = cps.as_chunks::<4>();
    // Branch-free accumulate: an early `return true` serializes the loop on
    // the movemask dependency; `|=` lets the SIMD iterations pipeline.
    let mut any = false;
    for chunk in chunks {
        let v = u32x4::from_slice(simd, chunk);
        let c0 = simd.simd_lt_u32x4(v, simd.splat_u32x4(0x20));
        let c1 = simd.simd_ge_u32x4(v, simd.splat_u32x4(0x7f))
            & simd.simd_le_u32x4(v, simd.splat_u32x4(0x9f));
        any |= simd.any_true_mask32x4(c0 | c1);
    }
    if any {
        return true;
    }
    tail.iter()
        .any(|&cp| cp < 0x20 || (0x7f..=0x9f).contains(&cp))
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
        let out = dispatch!(Level::new(), simd => decode_3byte(simd, &bytes));
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
