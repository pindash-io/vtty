// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kitty keyboard protocol flags (`CSI > flags u` / `CSI < count u`).

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
