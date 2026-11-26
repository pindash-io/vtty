// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SWAR byte scanners for the hot paths: the ground-state printable-run
//! scanner and the OSC/DCS payload-boundary scanners. Eight bytes per
//! iteration, `no_std`, no allocation. Byte classes live in [`crate::utils`].

#[cfg(feature = "simd")]
use crate::utils::is_esc;
use crate::utils::{
    CAN, DEL, ESC, SEMICOLON, SUB, is_dcs_boundary, is_non_printable, is_osc_boundary,
};

/// Byte-splat constants: byte `xx` repeated in every lane of a 64-bit word.
const SPLAT_01: u64 = 0x0101_0101_0101_0101;
const SPLAT_SPACE: u64 = 0x2020_2020_2020_2020;
const SPLAT_C0_C1: u64 = 0x6060_6060_6060_6060;
const SPLAT_7F: u64 = 0x7f7f_7f7f_7f7f_7f7f;
const SPLAT_80: u64 = 0x8080_8080_8080_8080;

/// SWAR byte-equality mask: bits set where the little-endian word `w` holds
/// `b`. The classic zero-byte detector (`x.wrapping_sub(SPLAT_01) & !x &
/// SPLAT_80`) applied to the byte's splat.
///
/// A set bit always lands on a byte's high bit, i.e. position `8*n + 7`, so
/// `trailing_zeros() / 8` recovers the byte index `n` exactly (the `/8`
/// truncates the `+7`).
#[cfg_attr(feature = "perf-inline", inline(always))]
const fn swar_byte_eq(w: u64, b: u8) -> u64 {
    let x = w ^ (SPLAT_01 * (b as u64));
    x.wrapping_sub(SPLAT_01) & !x & SPLAT_80
}

/// Minimum run length that pays for the SIMD dispatch; below it the SWAR path
/// is faster, and ANSI-dense input produces runs this short.
#[cfg(feature = "simd")]
const SIMD_MIN_RUN: usize = 16;

/// Run the SWAR scan unless `simd` is on, the parser opted in, and the first
/// `SIMD_MIN_RUN` bytes hold no stop.
macro_rules! scan_or_simd {
    ($bytes:expr, $simd:expr, $mask:expr, $scalar:expr, $vector:path) => {{
        #[cfg(feature = "simd")]
        if $simd
            && $bytes.len() >= SIMD_MIN_RUN
            && first_stop_in_word::<SIMD_MIN_RUN>($bytes, $mask).is_none()
        {
            return $vector($bytes);
        }
        #[cfg(not(feature = "simd"))]
        let _ = $simd;
        first_where($bytes, $mask, $scalar)
    }};
}

/// SWAR scan over whole words: index of the first byte whose `stop_mask` bit
/// is set, or `None` when every word is clear.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn first_stop_word(bytes: &[u8], stop_mask: impl Fn(u64) -> u64) -> Option<usize> {
    for (i, chunk) in bytes.as_chunks::<8>().0.iter().enumerate() {
        let stop = stop_mask(u64::from_le_bytes(*chunk));
        if stop == 0 {
            continue;
        }
        return Some(i * 8 + (stop.trailing_zeros() / 8) as usize);
    }
    None
}

/// SWAR scan: [`first_stop_word`] over the whole words of `bytes`, then the
/// <8-byte tail with the scalar `is_stop`, or `bytes.len()` when nothing hits.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn first_where(
    bytes: &[u8],
    stop_mask: impl Fn(u64) -> u64,
    is_stop: impl Fn(&u8) -> bool,
) -> usize {
    let whole = bytes.len() & !7;
    let (words, tail) = bytes.split_at(whole);
    if let Some(i) = first_stop_word(words, stop_mask) {
        return i;
    }
    whole + tail.iter().position(is_stop).unwrap_or(tail.len())
}

/// Index of the first ESC in the first `limit` bytes, or `limit`: the probe that
/// bounds a decode window. SWAR, since SIMD setup loses on a short window.
#[cfg(feature = "simd")]
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_esc_bounded(bytes: &[u8], limit: usize) -> usize {
    let n = bytes.len().min(limit);
    first_where(&bytes[..n], esc_mask, is_esc)
}

/// SWAR mask for `ESC` bytes.
#[cfg(feature = "simd")]
const fn esc_mask(w: u64) -> u64 {
    swar_byte_eq(w, ESC)
}

/// SWAR mask for non-printable bytes (`b < 0x20 || b >= DEL`).
const fn non_printable_mask(w: u64) -> u64 {
    let lt_20 = w.wrapping_sub(SPLAT_SPACE) & !w & SPLAT_80; // b < 0x20
    let gt_del = w & SPLAT_80; // b > DEL
    let x = w ^ SPLAT_7F;
    let eq_7f = x.wrapping_sub(SPLAT_01) & !x & SPLAT_80; // b == DEL
    lt_20 | gt_del | eq_7f
}

/// SWAR probe of the first `N` bytes (a multiple of 8): `Some(index)` of the
/// first stop byte when the buffer holds all `N` bytes and the mask hits
/// inside them, `None` otherwise.
///
/// `N = 8` is the scalar short-run fast path; a longer window gates the vector
/// dispatch, which only pays once the run is known to reach that far.
#[cfg_attr(feature = "perf-inline", inline(always))]
fn first_stop_in_word<const N: usize>(
    bytes: &[u8],
    stop_mask: impl Fn(u64) -> u64,
) -> Option<usize> {
    debug_assert!(N > 0 && N.is_multiple_of(8), "probe window is whole words");
    first_stop_word(bytes.get(..N)?, stop_mask)
}

/// [`first_stop_in_word`] for the ground state's printable-run scan.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_non_printable_word(bytes: &[u8]) -> Option<usize> {
    first_stop_in_word::<8>(bytes, non_printable_mask)
}

/// Length of the leading printable-ASCII run (`0x20..=0x7e`); `DEL` is a C0
/// control, not printable.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) fn first_non_printable(bytes: &[u8], simd: bool) -> usize {
    // ANSI-dense input and short lines leave 0- and 1-byte runs, where a full
    // 8-byte SWAR word is overkill.
    if matches!(bytes.first(), Some(b) if is_non_printable(b)) {
        return 0;
    }
    if matches!(bytes.get(1), Some(b) if is_non_printable(b)) {
        return 1;
    }
    // One SWAR word over the first 8 bytes catches short runs (a word then ESC
    // or CR/LF) without the SIMD dispatch; long runs fall through to it.
    if let Some(n) = first_non_printable_word(bytes) {
        return n;
    }
    scan_or_simd!(
        bytes,
        simd,
        non_printable_mask,
        is_non_printable,
        crate::simd::first_non_printable
    )
}

/// SWAR mask for OSC boundaries: C0/C1 (`(b & 0x60) == 0`) or `;`.
const fn osc_stop_mask(w: u64) -> u64 {
    swar_byte_eq(w & SPLAT_C0_C1, 0) | swar_byte_eq(w, SEMICOLON)
}

/// Index of the first byte that ends an OSC payload run; see
/// [`is_osc_boundary`] for the class. The rest are `OSC_PUT` bytes.
#[inline]
pub(crate) fn first_osc_boundary(bytes: &[u8], simd: bool) -> usize {
    if let Some(n) = first_stop_in_word::<8>(bytes, osc_stop_mask) {
        return n;
    }
    scan_or_simd!(
        bytes,
        simd,
        osc_stop_mask,
        is_osc_boundary,
        crate::simd::first_osc_boundary
    )
}

/// SWAR mask for DCS boundaries: `> DEL`, `CAN`, `SUB`, `ESC`, `DEL`.
const fn dcs_stop_mask(w: u64) -> u64 {
    (w & SPLAT_80)
        | swar_byte_eq(w, CAN)
        | swar_byte_eq(w, SUB)
        | swar_byte_eq(w, ESC)
        | swar_byte_eq(w, DEL)
}

/// Index of the first byte that ends a DCS passthrough run; see
/// [`is_dcs_boundary`] for the class.
#[inline]
pub(crate) fn first_dcs_boundary(bytes: &[u8], simd: bool) -> usize {
    if let Some(n) = first_stop_in_word::<8>(bytes, dcs_stop_mask) {
        return n;
    }
    scan_or_simd!(
        bytes,
        simd,
        dcs_stop_mask,
        is_dcs_boundary,
        crate::simd::first_dcs_boundary
    )
}

#[cfg(test)]
mod tests {
    use super::{first_dcs_boundary, first_non_printable, first_osc_boundary};
    use alloc::vec::Vec;

    /// Scalar reference for the bounded ESC probe.
    #[cfg(feature = "simd")]
    fn reference_esc(bytes: &[u8], limit: usize) -> usize {
        let n = bytes.len().min(limit);
        bytes[..n].iter().position(|&b| b == 0x1b).unwrap_or(n)
    }

    #[cfg(feature = "simd")]
    #[test]
    fn first_esc_bounded_matches_scalar() {
        use super::first_esc_bounded;
        // Every ESC position in a 40-byte buffer, plus the no-ESC and
        // limit-capped cases.
        for esc_pos in 0..40 {
            let mut bytes = [b'x'; 40];
            bytes[esc_pos] = 0x1b;
            for limit in [1, 7, 8, 9, 16, 32, 40, 64] {
                assert_eq!(
                    first_esc_bounded(&bytes, limit),
                    reference_esc(&bytes, limit),
                    "esc at {esc_pos} limit {limit}"
                );
            }
        }
        // No ESC: returns the scan length (capped at limit).
        assert_eq!(first_esc_bounded(&[b'x'; 100], 32), 32);
        assert_eq!(first_esc_bounded(&[b'x'; 10], 32), 10);
        assert_eq!(first_esc_bounded(&[], 32), 0);
        // ESC past the limit is not seen.
        let mut bytes = [b'x'; 40];
        bytes[20] = 0x1b;
        assert_eq!(first_esc_bounded(&bytes, 16), 16);
    }

    #[test]
    fn first_non_printable_matches_scalar() {
        for pos in 0..16 {
            for byte in 0..=255_u8 {
                let mut bytes = [b'x'; 16];
                bytes[pos] = byte;
                assert_eq!(
                    first_non_printable(&bytes, false),
                    reference(&bytes),
                    "byte {byte:#04x} at pos {pos}"
                );
            }
        }
        assert_eq!(first_non_printable(&[b'x'; 16], false), 16);
        assert_eq!(first_non_printable(&[], false), 0);
    }

    #[test]
    fn first_non_printable_matches_scalar_random() {
        // Deterministic xorshift64; random buffers exercise the SWAR borrow
        // interactions and multi-non-printable chunks.
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..10_000 {
            let len = (next() % 64) as usize;
            let mut bytes = [0_u8; 64];
            for b in bytes.iter_mut().take(len) {
                *b = next() as u8;
            }
            assert_eq!(
                first_non_printable(&bytes[..len], false),
                reference(&bytes[..len]),
                "len {len} bytes {:?}",
                &bytes[..len]
            );
        }
    }

    /// Scalar reference for `first_non_printable` (length of the leading
    /// printable-ASCII run).
    fn reference(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .take_while(|&&x| matches!(x, 0x20..=0x7e))
            .count()
    }

    fn scalar_osc(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .take_while(|&&b| b >= 0x20 && b != 0x3b && !matches!(b, 0x80..=0x9f))
            .count()
    }

    fn scalar_dcs(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .take_while(|&&b| b < 0x80 && !matches!(b, 0x18 | 0x1a | 0x1b | 0x7f))
            .count()
    }

    #[test]
    fn osc_boundary_matches_scalar_for_all_pairs() {
        for a in 0..=255_u8 {
            for b in 0..=255_u8 {
                let bytes = [a, b];
                assert_eq!(
                    first_osc_boundary(&bytes, false),
                    scalar_osc(&bytes),
                    "OSC boundary mismatch for [{a:#04x}, {b:#04x}]"
                );
            }
        }
    }

    #[test]
    fn dcs_boundary_matches_scalar_for_all_pairs() {
        for a in 0..=255_u8 {
            for b in 0..=255_u8 {
                let bytes = [a, b];
                assert_eq!(
                    first_dcs_boundary(&bytes, false),
                    scalar_dcs(&bytes),
                    "DCS boundary mismatch for [{a:#04x}, {b:#04x}]"
                );
            }
        }
    }

    #[test]
    fn boundaries_match_scalar_for_long_runs() {
        // A pseudo-sixel/OSC payload plus every boundary byte somewhere in it.
        let mut bytes = Vec::new();
        for _ in 0..20 {
            bytes.extend_from_slice(b"$3:@ABCDEFGHIJKLMNOP");
        }
        for &b in &[
            0x00_u8, 0x07, 0x18, 0x19, 0x1a, 0x1b, 0x1f, 0x7f, 0x80, 0x9c, 0x9f, 0xa0, 0xff,
        ] {
            let mut v = bytes.clone();
            v.push(b);
            v.extend_from_slice(b"tail");
            assert_eq!(
                first_osc_boundary(&v, false),
                scalar_osc(&v),
                "OSC for {b:#04x}"
            );
            assert_eq!(
                first_dcs_boundary(&v, false),
                scalar_dcs(&v),
                "DCS for {b:#04x}"
            );
        }
    }

    #[test]
    fn boundaries_swar_matches_scalar_inside_chunks() {
        // Place every byte at every offset within two 8-byte chunks so the SWAR
        // loop (not just the scalar tail) must find the boundary in-chunk.
        for a in 0..=255_u8 {
            for pos in 0..16 {
                let mut bytes = [b'B'; 16];
                bytes[pos] = a;
                assert_eq!(
                    first_osc_boundary(&bytes, false),
                    scalar_osc(&bytes),
                    "OSC SWAR boundary mismatch for {a:#04x} at pos {pos}"
                );
                assert_eq!(
                    first_dcs_boundary(&bytes, false),
                    scalar_dcs(&bytes),
                    "DCS SWAR boundary mismatch for {a:#04x} at pos {pos}"
                );
            }
        }
    }

    #[cfg(feature = "simd")]
    #[test]
    fn simd_boundaries_match_swar() {
        // Cross-check the SIMD 16-byte classifiers against the already-validated
        // SWAR scanners over pseudo-random buffers covering chunk + tail paths.
        let mut seed = 0x0123_4567_89ab_cdef_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..10_000 {
            let len = (next() % 96) as usize;
            let mut bytes = [0_u8; 96];
            for b in bytes.iter_mut().take(len) {
                *b = next() as u8;
            }
            assert_eq!(
                first_osc_boundary(&bytes[..len], true),
                first_osc_boundary(&bytes[..len], false),
                "SIMD/SWAR OSC mismatch at len {len}"
            );
            assert_eq!(
                first_dcs_boundary(&bytes[..len], true),
                first_dcs_boundary(&bytes[..len], false),
                "SIMD/SWAR DCS mismatch at len {len}"
            );
        }
    }
}
