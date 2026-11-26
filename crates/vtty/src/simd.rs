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
use crate::utils::{
    C0_C1_MASK, CAN, DEL, ESC, SEMICOLON, SPACE, SUB, is_dcs_boundary, is_esc, is_non_printable,
    is_osc_boundary,
};

/// One UTF-8 byte's seven payload bits; the markers sit above them.
const PAYLOAD_MASK: u32 = 0x7f;

/// What terminates the scan run.
#[derive(Clone, Copy)]
enum ScanKind {
    /// Any byte outside `0x20..=0x7e`.
    NonPrintable,
    /// The `ESC` byte.
    Esc,
    /// An OSC payload boundary.
    OscBoundary,
    /// A DCS passthrough boundary.
    DcsBoundary,
}

/// Two explicit byte checks, then the vector scan: 0- and 1-byte runs are the
/// common shape (ANSI-dense input, `word\r\n` lines, a `;` right after an OSC
/// separator), where dispatch and chunk setup cost more than the scan.
macro_rules! scan_or_fast {
    ($bytes:expr, $kind:ident, $is_stop:expr) => {{
        if matches!($bytes.first(), Some(b) if $is_stop(b)) {
            return 0;
        }
        if matches!($bytes.get(1), Some(b) if $is_stop(b)) {
            return 1;
        }
        let level = Level::new();
        dispatch!(level, simd => first_matching(simd, $bytes, ScanKind::$kind))
    }};
}

/// Return the index of the first byte outside printable ASCII (`0x20..=0x7e`;
/// `0x7f` DEL is dispatched via `execute`), or `bytes.len()` if every byte is
/// printable.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_non_printable(bytes: &[u8]) -> usize {
    scan_or_fast!(bytes, NonPrintable, is_non_printable)
}

/// Index of the first `ESC` byte (`0x1b`), or `bytes.len()` if none. Caps the
/// SIMD UTF-8 decode window at the next sequence introducer, which must
/// reach the escape state rather than be dispatched as `execute`.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_esc(bytes: &[u8]) -> usize {
    let level = Level::new();
    dispatch!(level, simd => first_matching(simd, bytes, ScanKind::Esc))
}

/// Index of the first OSC payload boundary; see [`is_osc_boundary`].
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_osc_boundary(bytes: &[u8]) -> usize {
    scan_or_fast!(bytes, OscBoundary, is_osc_boundary)
}

/// Index of the first DCS passthrough boundary; see [`is_dcs_boundary`].
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_dcs_boundary(bytes: &[u8]) -> usize {
    scan_or_fast!(bytes, DcsBoundary, is_dcs_boundary)
}

/// Classify a 16-byte chunk: the lanes where `kind`'s scan terminator appears.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn classify_scan<S: Simd>(simd: S, v: u8x16<S>, kind: ScanKind) -> mask8x16<S> {
    match kind {
        ScanKind::NonPrintable => {
            // b < SPACE (C0) or b >= DEL (DEL + high-bit).
            simd.simd_lt_u8x16(v, simd.splat_u8x16(SPACE))
                | simd.simd_ge_u8x16(v, simd.splat_u8x16(DEL))
        }
        ScanKind::Esc => simd.simd_eq_u8x16(v, simd.splat_u8x16(ESC)),
        ScanKind::OscBoundary => {
            // (b & C0_C1_MASK) == 0 → C0 or C1; or b == ';'.
            simd.simd_eq_u8x16(v & simd.splat_u8x16(C0_C1_MASK), simd.splat_u8x16(0))
                | simd.simd_eq_u8x16(v, simd.splat_u8x16(SEMICOLON))
        }
        ScanKind::DcsBoundary => {
            // b > DEL, or CAN/SUB/ESC/DEL.
            simd.simd_gt_u8x16(v, simd.splat_u8x16(DEL))
                | simd.simd_eq_u8x16(v, simd.splat_u8x16(CAN))
                | simd.simd_eq_u8x16(v, simd.splat_u8x16(SUB))
                | simd.simd_eq_u8x16(v, simd.splat_u8x16(ESC))
                | simd.simd_eq_u8x16(v, simd.splat_u8x16(DEL))
        }
    }
}

/// Index of the first byte matching `kind`, or `bytes.len()` if none. Scans one
/// 16-byte chunk per iteration; the final (<16) tail uses the scalar predicate.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn first_matching<S: Simd>(simd: S, bytes: &[u8], kind: ScanKind) -> usize {
    let (chunks, remainder) = bytes.as_chunks::<16>();
    let mut offset = 0;
    for chunk in chunks {
        let m = classify_scan(simd, u8x16::from_slice(simd, chunk), kind);
        if simd.any_true_mask8x16(m) {
            return offset + first_set(m.into());
        }
        offset += 16;
    }
    match remainder.iter().position(|b| matches_kind(b, kind)) {
        Some(j) => offset + j,
        None => bytes.len(),
    }
}

/// The scalar predicate for one byte.
#[inline]
const fn matches_kind(b: &u8, kind: ScanKind) -> bool {
    match kind {
        ScanKind::NonPrintable => is_non_printable(b),
        ScanKind::Esc => is_esc(b),
        ScanKind::OscBoundary => is_osc_boundary(b),
        ScanKind::DcsBoundary => is_dcs_boundary(b),
    }
}

/// Index of the first non-zero lane in a 16-lane mask.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn first_set(mask: [i8; 16]) -> usize {
    mask.iter().position(|&m| m != 0).unwrap_or(16)
}

/// Turn a shuffled `u32x4` into four code points.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn finish_u32x4_3byte<S: Simd>(simd: S, p: u32x4<S>) -> [u32; 4] {
    let ascii = p & simd.splat_u32x4(PAYLOAD_MASK);
    let middle = simd.shr_u32x4(p & simd.splat_u32x4(0x3f00), 2);
    let high = simd.shr_u32x4(p & simd.splat_u32x4(0x0f0000), 4);
    ((ascii | middle) | high).into()
}

/// Decode four consecutive 3-byte code points (12 bytes).
#[cfg_attr(feature = "perf-inline", inline(always))]
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
/// the runtime-dispatched SIMD level (used by the scalar mixed loop).
#[cfg(not(feature = "simd-transcode"))]
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn decode_3byte_dispatch(bytes: &[u8]) -> [u32; 4] {
    let level = Level::new();
    dispatch!(level, simd => decode_3byte(simd, bytes))
}

/// Decode up to 12 bytes of validated UTF-8 from a per-byte end-of-code-point mask.
#[cfg(feature = "simd-transcode")]
#[cfg_attr(feature = "perf-inline", inline(always))]
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
        let ascii = p & simd.splat_u16x8(PAYLOAD_MASK as u16);
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
        let ascii = p & simd.splat_u32x4(PAYLOAD_MASK);
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

/// Transcode one 64-byte block of validated UTF-8 to UTF-32.
#[cfg(feature = "simd-transcode")]
#[cfg_attr(feature = "perf-inline", inline(always))]
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
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn decode_block_dispatch(bytes: &[u8], out: &mut [u32]) -> (usize, usize) {
    let level = Level::new();
    dispatch!(level, simd => decode_block(simd, bytes, out))
}

/// Whether any code point in `cps` is a control (`< 0x20`, `DEL`, or
/// `0x80..=0x9f`), the ones the dispatch loop must split out via `execute`
/// instead of a single `print_codepoints` run.
#[cfg(feature = "simd-transcode")]
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn any_control(cps: &[u32]) -> bool {
    let level = Level::new();
    dispatch!(level, simd => any_control_impl(simd, cps))
}

#[cfg(feature = "simd-transcode")]
fn any_control_impl<S: Simd>(simd: S, cps: &[u32]) -> bool {
    let (chunks, tail) = cps.as_chunks::<4>();
    // A control is below SPACE, or DEL and the C1 range above it.
    let space = SPACE as u32;
    let del = DEL as u32;
    let c1_end = 0x9f;
    // Return at the first chunk holding a control. Accumulating the masks and
    // branching once is slower: printable text scans every chunk either way,
    // and the accumulator adds a loop-carried dependency (2.2% on `utf8_mixed`).
    for chunk in chunks {
        let v = u32x4::from_slice(simd, chunk);
        let c0 = simd.simd_lt_u32x4(v, simd.splat_u32x4(space));
        let c1 = simd.simd_ge_u32x4(v, simd.splat_u32x4(del))
            & simd.simd_le_u32x4(v, simd.splat_u32x4(c1_end));
        if simd.any_true_mask32x4(c0 | c1) {
            return true;
        }
    }
    for cp in tail {
        if *cp < space || (del..=c1_end).contains(cp) {
            return true;
        }
    }
    false
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
