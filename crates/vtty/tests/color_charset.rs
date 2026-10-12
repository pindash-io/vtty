// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Charset translation tests.

use vtty::{Charset, dec_special_graphics};

#[test]
fn dec_special_graphics_translation() {
    assert_eq!(dec_special_graphics('_'), ' ');
    assert_eq!(Charset::Dec.map('_'), ' ');
    assert_eq!(Charset::Dec.map('q'), '─');
    assert_eq!(Charset::Dec.map('x'), '│');
    assert_eq!(Charset::Dec.map('a'), '▒');
    assert_eq!(Charset::Dec.map('~'), '·');
    // Outside the mapped range is a passthrough.
    assert_eq!(Charset::Dec.map('A'), 'A');
}

#[test]
fn charset_map() {
    assert_eq!(Charset::Ascii.map('q'), 'q');
    assert_eq!(Charset::Dec.map('q'), '─');
    assert_eq!(Charset::Dec.map('λ'), 'λ');
}
