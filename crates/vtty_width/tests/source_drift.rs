// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The two width `Source` impls (`Bytes` and `Codepoints`) must agree: the
//! same text decoded on the fly vs fed as pre-decoded code points must yield
//! identical per-char widths. This guards against drift between the two
//! copies of the scan/decode logic.

use vtty_width::{cell_widths, cell_widths_from_codepoints};

fn assert_sources_agree(s: &str) {
    let cps: Vec<u32> = s.chars().map(u32::from).collect();
    assert_eq!(
        cell_widths(s),
        cell_widths_from_codepoints(&cps),
        "Bytes vs Codepoints source drift for {s:?}"
    );
}

/// Every valid BMP char, fed as a lone char through both sources.
#[test]
fn every_bmp_char_matches_between_sources() {
    for cp in 0..=0xFFFF_u32 {
        // Surrogates (0xD800..=0xDFFF) are not chars.
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let mut buf = [0u8; 4];
        assert_sources_agree(c.encode_utf8(&mut buf));
    }
}

/// Multi-char sequences that exercise the emoji DFA, Hangul, combining marks,
/// and the printable-ASCII fast path — the cases a lone-char sweep cannot see.
#[test]
fn sequences_match_between_sources() {
    for s in [
        "👨\u{200D}👩\u{200D}👧",
        "❤\u{FE0F}",
        "❤\u{FE0E}",
        "1\u{FE0F}\u{20E3}",
        "1\u{20E3}",
        "👍\u{1F3FB}",
        "🇺🇸🇨🇦",
        "e\u{301}",
        "가나",
        "각ᆨ",
        "中文字",
        "hello world",
        "a\tb\nc",
        "\u{200B}",
        "\u{1160}\u{11A8}",
        "®\u{1160}",
        "🏳️\u{200D}🌈",
        "👁️\u{200D}🗨️",
        "👩\u{200D}🚀",
    ] {
        assert_sources_agree(s);
    }
}
