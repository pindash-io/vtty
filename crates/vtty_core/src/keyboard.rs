// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kitty keyboard protocol flags (`CSI > flags u` / `CSI < count u`).
//!
//! Reference: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>.

bitflags::bitflags! {
    /// Kitty keyboard protocol flags, reported by `CSI > flags u` and pushed
    /// by `CSI < count u`. Each bit enables a progressive feature tier.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct KittyFlags: u8 {
        /// Disambiguate escape codes (`CSI u`)
        const DISAMBIGUATE = 1;
        /// Report event types (key press/repeat/release)
        const REPORT_EVENT_TYPES = 2;
        /// Report alternate keys (shift+tab, etc.)
        const REPORT_ALTERNATE_KEYS = 4;
        /// Report all keys as escape sequences
        const REPORT_ALL_KEYS = 8;
        /// Report associated text (IME/composing input)
        const REPORT_ASSOCIATED_TEXT = 16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
