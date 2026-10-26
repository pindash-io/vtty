// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property tests: the streaming width model and its collected per-char form
//! agree for arbitrary input.

use proptest::prelude::*;
use vtty_width::{cell_width_sum, cell_widths};

proptest! {
    /// One width per input char, in char order.
    #[test]
    fn one_width_per_char(chars in prop::collection::vec(any::<char>(), 0..64)) {
        let text: String = chars.into_iter().collect();
        prop_assert_eq!(cell_widths(&text).len(), text.chars().count());
    }

    /// Every per-char cell width is 0, 1, or 2.
    #[test]
    fn widths_are_0_1_or_2(chars in prop::collection::vec(any::<char>(), 0..64)) {
        let text: String = chars.into_iter().collect();
        for &w in &cell_widths(&text) {
            prop_assert!(w <= 2, "width {w} out of range for {text:?}");
        }
    }

    /// The collected per-char sum equals the streaming total.
    #[test]
    fn collected_sum_equals_streaming_sum(chars in prop::collection::vec(any::<char>(), 0..64)) {
        let text: String = chars.into_iter().collect();
        let collected: usize = cell_widths(&text).iter().sum();
        prop_assert_eq!(collected, cell_width_sum(&text));
    }

    /// Printable ASCII takes the one-cell-per-byte fast path.
    #[test]
    fn printable_ascii_is_one_cell_per_byte(
        chars in prop::collection::vec(prop::char::range('\u{20}', '\u{7e}'), 0..64)
    ) {
        let text: String = chars.into_iter().collect();
        prop_assert_eq!(cell_width_sum(&text), text.len());
    }
}
