// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kitty keyboard protocol flags.

use vtty_core::KittyFlags;

#[test]
fn kitty_flag_bits() {
    assert_eq!(KittyFlags::DISAMBIGUATE.bits(), 1);
    assert_eq!(KittyFlags::REPORT_EVENT_TYPES.bits(), 2);
    assert_eq!(KittyFlags::REPORT_ALTERNATE_KEYS.bits(), 4);
    assert_eq!(KittyFlags::REPORT_ALL_KEYS.bits(), 8);
    assert_eq!(KittyFlags::REPORT_ASSOCIATED_TEXT.bits(), 16);
}

#[test]
fn kitty_from_bits_truncate() {
    let f = KittyFlags::from_bits_truncate(0b11);
    assert!(f.contains(KittyFlags::DISAMBIGUATE));
    assert!(f.contains(KittyFlags::REPORT_EVENT_TYPES));
    assert!(!f.contains(KittyFlags::REPORT_ALL_KEYS));
    // Unknown bits (e.g. 32) are dropped.
    assert_eq!(KittyFlags::from_bits_truncate(32), KittyFlags::empty());
}
