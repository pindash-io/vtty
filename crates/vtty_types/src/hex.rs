// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Hex encode/decode for query payloads (XTGETTCAP / DECRQSS), backed by
//! `const_hex`.

use alloc::string::String;
use alloc::vec::Vec;

/// Decode a hex-encoded byte string (DECRQSS-style query payloads). Invalid
/// input decodes to empty.
#[inline]
pub fn hex_decode(s: &[u8]) -> Vec<u8> {
    const_hex::decode(s).unwrap_or_default()
}

/// Encode `bytes` as lowercase hex pairs.
#[inline]
pub fn hex_encode(bytes: &[u8]) -> String {
    const_hex::encode(bytes)
}
