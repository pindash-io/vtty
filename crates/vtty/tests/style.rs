// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SGR flag code-mapping, decoration subset, and style-enum parsing.

use vtty::{SgrFlags, StyleFlags, UnderlineStyle};

#[test]
fn sgr_set_code_maps_to_flag() {
    assert_eq!(SgrFlags::from(1), SgrFlags::BOLD);
    assert_eq!(SgrFlags::from(2), SgrFlags::FAINT);
    assert_eq!(SgrFlags::from(3), SgrFlags::ITALIC);
    assert_eq!(SgrFlags::from(4), SgrFlags::UNDERLINE);
    assert_eq!(SgrFlags::from(5), SgrFlags::BLINK);
    assert_eq!(SgrFlags::from(6), SgrFlags::BLINK_RAPID);
    assert_eq!(SgrFlags::from(7), SgrFlags::INVERSE);
    assert_eq!(SgrFlags::from(8), SgrFlags::INVISIBLE);
    assert_eq!(SgrFlags::from(9), SgrFlags::STRIKETHROUGH);
    assert_eq!(SgrFlags::from(21), SgrFlags::UNDERLINE);
    assert_eq!(SgrFlags::from(53), SgrFlags::OVERLINE);
    assert_eq!(SgrFlags::from(99), SgrFlags::DEFAULT);
}

#[test]
fn sgr_reset_code_maps_to_flags() {
    // Reset codes carry the RESET mark so the caller can tell clear from set.
    assert_eq!(
        SgrFlags::from(22),
        SgrFlags::BOLD | SgrFlags::FAINT | SgrFlags::RESET
    );
    assert_eq!(SgrFlags::from(23), SgrFlags::ITALIC | SgrFlags::RESET);
    assert_eq!(SgrFlags::from(24), SgrFlags::UNDERLINE | SgrFlags::RESET);
    assert_eq!(
        SgrFlags::from(25),
        SgrFlags::BLINK | SgrFlags::BLINK_RAPID | SgrFlags::RESET
    );
    assert_eq!(SgrFlags::from(27), SgrFlags::INVERSE | SgrFlags::RESET);
    assert_eq!(SgrFlags::from(28), SgrFlags::INVISIBLE | SgrFlags::RESET);
    assert_eq!(
        SgrFlags::from(29),
        SgrFlags::STRIKETHROUGH | SgrFlags::RESET
    );
    assert_eq!(SgrFlags::from(55), SgrFlags::OVERLINE | SgrFlags::RESET);
    assert_eq!(SgrFlags::from(99), SgrFlags::DEFAULT);
}

#[test]
fn decorations_extract_low_five_bits() {
    let all = SgrFlags::BOLD | SgrFlags::ITALIC | SgrFlags::UNDERLINE;
    assert_eq!(
        all.decorations(),
        StyleFlags::BOLD | StyleFlags::ITALIC | StyleFlags::UNDERLINE
    );

    // BLINK lives in bit 8 and must not leak into the u8 decoration subset.
    assert_eq!((SgrFlags::BLINK).decorations(), StyleFlags::empty());
}

#[test]
#[allow(
    clippy::cast_possible_truncation,
    reason = "decoration bits fit u8 by definition"
)]
fn style_flag_bits_mirror_sgr() {
    assert_eq!(
        StyleFlags::UNDERLINE.bits(),
        SgrFlags::UNDERLINE.bits() as u8
    );
    assert_eq!(
        StyleFlags::STRIKETHROUGH.bits(),
        SgrFlags::STRIKETHROUGH.bits() as u8
    );
    assert_eq!(StyleFlags::OVERLINE.bits(), SgrFlags::OVERLINE.bits() as u8);
    assert_eq!(StyleFlags::BOLD.bits(), SgrFlags::BOLD.bits() as u8);
    assert_eq!(StyleFlags::ITALIC.bits(), SgrFlags::ITALIC.bits() as u8);
}

#[test]
fn underline_style_from_u8() {
    assert_eq!(UnderlineStyle::from_u8(0), UnderlineStyle::None);
    assert_eq!(UnderlineStyle::from_u8(1), UnderlineStyle::Single);
    assert_eq!(UnderlineStyle::from_u8(2), UnderlineStyle::Double);
    assert_eq!(UnderlineStyle::from_u8(3), UnderlineStyle::Curly);
    assert_eq!(UnderlineStyle::from_u8(4), UnderlineStyle::Dotted);
    assert_eq!(UnderlineStyle::from_u8(5), UnderlineStyle::Dashed);
    // Unknown sub-params default to single (plain `4`).
    assert_eq!(UnderlineStyle::from_u8(9), UnderlineStyle::Single);
}
