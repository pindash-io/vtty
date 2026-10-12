// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Hex encode/decode for query payloads (XTGETTCAP / DECRQSS).

use alloc::string::String;
use alloc::vec::Vec;

/// 256-entry nibble table: `0-9` → 0..9, `a-f`/`A-F` → 10..15, anything else
/// → 0xff (invalid).
const HEX: [u8; 256] = {
    let mut t = [0xff_u8; 256];
    let mut c = 0;
    while c < 10 {
        t[b'0' as usize + c] = c as u8;
        c += 1;
    }
    let mut c = 0;
    while c < 6 {
        t[b'a' as usize + c] = 10 + c as u8;
        t[b'A' as usize + c] = 10 + c as u8;
        c += 1;
    }
    t
};

/// Lowercase hex digits, indexed by nibble value.
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Decode a hex-encoded byte string (`DCS + q <hex> ST` DECRQSS-style query
/// payloads). Stops at the first non-hex byte pair.
#[inline]
pub fn hex_decode(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.as_chunks::<2>().0 {
        let hi = HEX[pair[0] as usize];
        let lo = HEX[pair[1] as usize];
        if hi == 0xff || lo == 0xff {
            break;
        }
        out.push((hi << 4) | lo);
    }
    out
}

/// Encode `bytes` as lowercase hex pairs.
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    push_hex(&mut s, bytes);
    s
}

/// Append `bytes` to `s` as lowercase hex pairs (xterm XTGETTCAP encoding).
pub(crate) fn push_hex<W: core::fmt::Write>(s: &mut W, bytes: &[u8]) {
    for &b in bytes {
        let _ = s.write_char(HEX_DIGITS[(b >> 4) as usize] as char);
        let _ = s.write_char(HEX_DIGITS[(b & 0xf) as usize] as char);
    }
}
