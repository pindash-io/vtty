// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SIMD-accelerated byte-scan and UTF-8 decode helpers for the parser's hot
//! path. Compiled only with the `simd` feature (which also implies `std`).

#[cfg(not(target_arch = "aarch64"))]
use std::sync::OnceLock;

use fearless_simd::{Level, Simd, dispatch, prelude::*, u8x16, u32x4};

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
///
/// Used by the SIMD UTF-8 decode path to cap its validated window at the next
/// sequence introducer: the dispatch loop returns at ESC, so validating past it
/// re-validates the same window for every isolated multi-byte char (the
/// bracketed-paste O(n²) regression). Same runtime-dispatched SIMD classifier
/// as `first_non_printable` / `first_csi_final`.
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
/// early-return scan — the magic-multiply bitmask + `trailing_zeros`
/// alternative was *slower* here (the `[i8; 16] → [u8; 16]` `map` plus two
/// `u64` multiplies cost more than the early return for the common
/// low-position matches in CSI final bytes).
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
/// The caller guarantees `bytes` starts with four validated 3-byte sequences.
/// This is the simdutf `0x924` fast path: one fixed byte shuffle gathers each
/// 3-byte group into a u32 lane, then mask + shift + or assemble the code
/// points (lead contributes its low 4 bits, each continuation its low 6).
#[inline(always)]
fn decode_3byte<S: Simd>(simd: S, bytes: &[u8]) -> [u32; 4] {
    let v = u8x16::from_slice(simd, bytes);
    // Gather [2,1,0] [5,4,3] [8,7,6] [11,10,9] into four little-endian u32
    // lanes (0xff pads the high byte of each lane).
    let sh = u8x16::from_slice(
        simd,
        &[2, 1, 0, 0xff, 5, 4, 3, 0xff, 8, 7, 6, 0xff, 11, 10, 9, 0xff],
    );
    let perm = simd.swizzle_dyn_u8x16(v, sh);
    let p: u32x4<S> = perm.bitcast();
    let ascii = p & simd.splat_u32x4(0x7f);
    let middle = simd.shr_u32x4(p & simd.splat_u32x4(0x3f00), 2);
    let high = simd.shr_u32x4(p & simd.splat_u32x4(0x0f0000), 4);
    ((ascii | middle) | high).into()
}

/// Decode four consecutive 3-byte code points at `bytes[i..i+12]` via the
/// runtime-dispatched SIMD level, or `None` if the backend cannot run it.
#[inline(always)]
pub(crate) fn decode_3byte_dispatch(bytes: &[u8]) -> [u32; 4] {
    let level = level();
    dispatch!(level, simd => decode_3byte(simd, bytes))
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
        let out = decode_3byte_dispatch(&bytes);
        assert_eq!(out, [0x4e2d, 0x6587, 0x65e5, 0x672c]);
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
