// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SWAR byte scanners for the hot paths: the ground-state printable-run
//! scanner and the OSC/DCS payload-boundary scanners. Eight bytes per
//! iteration, `no_std`, no allocation.

/// Byte-splat constants: byte `xx` repeated in every lane of a 64-bit word.
const SPLAT_01: u64 = 0x0101_0101_0101_0101;
const SPLAT_20: u64 = 0x2020_2020_2020_2020;
const SPLAT_7F: u64 = 0x7f7f_7f7f_7f7f_7f7f;
const SPLAT_80: u64 = 0x8080_8080_8080_8080;

/// SWAR byte-equality mask: bits set where the 8-byte little-endian word `w`
/// contains byte `b` (`x.wrapping_sub(SPLAT_01) & !x & SPLAT_80` is the
/// classic zero-byte detector applied to `w ^ b`).
#[inline(always)]
const fn swar_byte_eq(w: u64, b: u64) -> u64 {
    let x = w ^ (SPLAT_01 * b);
    x.wrapping_sub(SPLAT_01) & !x & SPLAT_80
}

/// SWAR scan: index of the first byte whose `stop_mask(word)` bit is set
/// (one bit per byte, LSB = first byte), or `bytes.len()`. `is_stop` is the
/// scalar predicate for the <8-byte tail.
#[inline(always)]
fn first_where(
    bytes: &[u8],
    stop_mask: impl Fn(u64) -> u64,
    is_stop: impl Fn(u8) -> bool,
) -> usize {
    let (chunks, tail) = bytes.as_chunks::<8>();
    for (i, chunk) in chunks.iter().enumerate() {
        let stop = stop_mask(u64::from_le_bytes(*chunk));
        if stop != 0 {
            return i * 8 + (stop.trailing_zeros() / 8) as usize;
        }
    }
    let base = chunks.len() * 8;
    for (j, &b) in tail.iter().enumerate() {
        if is_stop(b) {
            return base + j;
        }
    }
    bytes.len()
}

/// SWAR mask for non-printable bytes (`b < 0x20 || b >= 0x7f`).
const fn non_printable_mask(w: u64) -> u64 {
    let lt_20 = w.wrapping_sub(SPLAT_20) & !w & SPLAT_80; // b < 0x20
    let ge_80 = w & SPLAT_80; // b >= 0x80
    let x = w ^ SPLAT_7F;
    let eq_7f = x.wrapping_sub(SPLAT_01) & !x & SPLAT_80; // b == 0x7f
    lt_20 | ge_80 | eq_7f
}

/// Scalar predicate for non-printable bytes.
const fn is_non_printable(b: u8) -> bool {
    b < 0x20 || b >= 0x7f
}

/// Below this length the SIMD scan cannot complete even one 16-byte vector
/// chunk (`as_chunks::<16>` yields zero chunks), so it would pay
/// `Level::new()` + `dispatch!` + `first_matching` setup and then fall
/// through to an identical scalar tail loop — pure overhead. ANSI/CSI/OSC/DCS-
/// dense input produces runs this short (a word between two escapes), so the
/// guard keeps the hot path on the SWAR scanner and skips the per-scan
/// dispatch entirely.
#[cfg(feature = "simd")]
const SIMD_MIN_RUN: usize = 16;

/// Length of the leading printable-ASCII run (`0x20..=0x7e`) — the ground
/// state's bulk `print_bytes` path. SIMD-scanned with the `simd` feature.
/// `DEL` (`0x7f`) is a C0 control, not printable.
#[inline(always)]
pub(crate) fn first_non_printable(bytes: &[u8], simd: bool) -> usize {
    // Fast path: ANSI-dense input (`ESC [ … m X ESC [ …`) and short text
    // lines (`word\r\n`) have 0- and 1-byte printable runs, where a full
    // 8-byte SWAR word is overkill.
    match bytes {
        [b, ..] if is_non_printable(*b) => return 0,
        [_, b, ..] if is_non_printable(*b) => return 1,
        _ => {}
    }
    // One SWAR word over the first 8 bytes catches the short-run pattern
    // (a word followed by ESC or CR/LF) without paying the SIMD dispatch
    // below. Long printable runs fall through — the word scan is ~9 ops
    // against a 32 KiB run, so the overlap with the SIMD pass is negligible.
    if let Some(&word) = bytes.as_chunks::<8>().0.first() {
        let stop = non_printable_mask(u64::from_le_bytes(word));
        if stop != 0 {
            return (stop.trailing_zeros() / 8) as usize;
        }
    }

    #[cfg(feature = "simd")]
    if simd && bytes.len() >= SIMD_MIN_RUN {
        return crate::simd::first_non_printable(bytes);
    }
    #[cfg(not(feature = "simd"))]
    let _ = simd;

    first_where(bytes, non_printable_mask, is_non_printable)
}

/// SWAR mask for OSC boundaries: C0/C1 (`(b & 0x60) == 0`) or the `;`
/// separator.
const fn osc_stop_mask(w: u64) -> u64 {
    swar_byte_eq(w & (SPLAT_01 * 0x60), 0) | swar_byte_eq(w, 0x3b)
}

/// Scalar predicate for OSC boundaries.
const fn osc_is_stop(b: u8) -> bool {
    (b & 0x60) == 0 || b == 0x3b
}

/// Index of the first byte that ends an OSC payload run: a C0 byte
/// (`< 0x20`, including `BEL`/`ESC`), a raw C1 byte (`0x80..=0x9f`), or the `;`
/// param separator (`0x3b`). The rest are `OSC_PUT` bytes.
#[inline]
pub(crate) fn first_osc_boundary(bytes: &[u8], simd: bool) -> usize {
    #[cfg(feature = "simd")]
    if simd && bytes.len() >= SIMD_MIN_RUN {
        return crate::simd::first_osc_boundary(bytes);
    }
    #[cfg(not(feature = "simd"))]
    let _ = simd;

    first_where(bytes, osc_stop_mask, osc_is_stop)
}

/// SWAR mask for DCS boundaries: `>= 0x80`, `CAN`, `SUB`, `ESC`, `DEL`.
const fn dcs_stop_mask(w: u64) -> u64 {
    (w & SPLAT_80)
        | swar_byte_eq(w, 0x18)
        | swar_byte_eq(w, 0x1a)
        | swar_byte_eq(w, 0x1b)
        | swar_byte_eq(w, 0x7f)
}

/// Scalar predicate for DCS boundaries.
const fn dcs_is_stop(b: u8) -> bool {
    b >= 0x80 || b == 0x18 || b == 0x1a || b == 0x1b || b == 0x7f
}

/// Index of the first byte that ends a DCS passthrough run. The PUT class is
/// `0x00..=0x17 | 0x19 | 0x1c..=0x7e`; everything else (`CAN`, `SUB`, `ESC`, `DEL`,
/// and any byte `>= 0x80`) ends the run or is ignored.
#[inline]
pub(crate) fn first_dcs_boundary(bytes: &[u8], simd: bool) -> usize {
    #[cfg(feature = "simd")]
    if simd && bytes.len() >= SIMD_MIN_RUN {
        return crate::simd::first_dcs_boundary(bytes);
    }
    #[cfg(not(feature = "simd"))]
    let _ = simd;

    first_where(bytes, dcs_stop_mask, dcs_is_stop)
}

#[cfg(test)]
mod tests {
    use super::{first_dcs_boundary, first_non_printable, first_osc_boundary};
    use alloc::vec::Vec;

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
            .take_while(|&&x| (0x20..=0x7e).contains(&x))
            .count()
    }

    fn scalar_osc(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .take_while(|&&b| b >= 0x20 && b != 0x3b && !(0x80..=0x9f).contains(&b))
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
