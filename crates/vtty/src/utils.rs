// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Byte classes shared by the scalar scanners in [`crate::scan`] and the vector
//! ones in `crate::simd` (feature `simd`). Each class is defined once here so
//! the two paths cannot drift apart.

/// `CAN` — cancels the sequence in progress.
pub(crate) const CAN: u8 = 0x18;

/// `SUB` — cancels the sequence in progress.
pub(crate) const SUB: u8 = 0x1a;

/// `ESC` — introduces a sequence, and terminates a string in 8-bit mode.
pub(crate) const ESC: u8 = 0x1b;

/// `DEL` — a C0 control, never printable and never part of a string payload.
pub(crate) const DEL: u8 = 0x7f;

/// The OSC parameter separator.
pub(crate) const SEMICOLON: u8 = b';';

/// `ESC`.
#[cfg(feature = "simd")]
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn is_esc(&b: &u8) -> bool {
    b == ESC
}

/// Printable ASCII, `0x20..=0x7e` — from space up to, but excluding, `DEL`.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn is_printable_ascii(&b: &u8) -> bool {
    b >= 0x20 && b < DEL
}

/// Anything outside printable ASCII: a C0/C1 control, `DEL`, or a high-bit
/// (UTF-8 lead) byte. The ground state's scan terminator.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn is_non_printable(b: &u8) -> bool {
    !is_printable_ascii(b)
}

/// C0 (`0x00..=0x1f`) or C1 (`0x80..=0x9f`) — exactly the bytes with bits 5
/// and 6 both clear, so one mask covers both ranges.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn is_c0_or_c1(b: u8) -> bool {
    b & 0x60 == 0
}

/// OSC payload boundary: a C0/C1 control (which includes `BEL` and `ESC`) or
/// the `;` separator. Every other byte is an `OSC_PUT` byte.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn is_osc_boundary(&b: &u8) -> bool {
    is_c0_or_c1(b) || b == SEMICOLON
}

/// DCS passthrough boundary: a high-bit byte, or `CAN`/`SUB`/`ESC`/`DEL`. The
/// put class is `0x00..=0x17 | 0x19 | 0x1c..=0x7e`.
#[cfg_attr(feature = "perf-inline", inline(always))]
pub(crate) const fn is_dcs_boundary(&b: &u8) -> bool {
    b > DEL || matches!(b, CAN | SUB | ESC | DEL)
}
