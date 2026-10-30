// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CJK width-mode tests, in their own process: the global `CJK_WIDTH` toggle
//! is process-local, so mutating it here cannot race the parallel unit tests.

#[cfg(any(feature = "baked", feature = "icu"))]
#[test]
fn cjk_width_makes_ambiguous_wide() {
    use vtty_width::{cell_width, set_cjk_width};

    // Ambiguous (A) is 1 cell by default.
    assert_eq!(cell_width('\u{2469}'), Some(1)); // ⑩
    assert_eq!(cell_width('\u{2460}'), Some(1)); // ①
    // Neutral stays 1 regardless.
    assert_eq!(cell_width('a'), Some(1));

    set_cjk_width(true);
    assert_eq!(cell_width('\u{2469}'), Some(2)); // ⑩
    assert_eq!(cell_width('\u{2460}'), Some(2)); // ①
    // Wide is still 2; Neutral still 1.
    assert_eq!(cell_width('\u{4E2D}'), Some(2)); // 中
    assert_eq!(cell_width('a'), Some(1));
    set_cjk_width(false);
    assert_eq!(cell_width('\u{2469}'), Some(1));
}
