// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `State` byte-cursor geometry tests: line iteration and cursor position.

use vtty::{Parser, State};

fn drive(bytes: &[u8]) -> State {
    let mut parser = Parser::new();
    let mut state = State::new();
    parser.advance(&mut state, bytes);
    state
}

#[test]
fn empty_state_has_no_lines() {
    let s = State::new();
    assert_eq!(s.line_count(), 0);
    assert_eq!(s.cursor_row(), 0);
    assert_eq!(s.cursor_col(), 0);
    assert_eq!(s.line_range(0), None);
}

#[test]
fn line_and_cursor_geometry() {
    let s = drive(b"hello\nworld");
    assert_eq!(s.text, "hello\nworld");
    assert_eq!(s.line_count(), 2);
    assert_eq!(s.cursor_row(), 1);
    assert_eq!(s.cursor_col(), 5);
    assert_eq!(s.line_range(0), Some(0..5));
    assert_eq!(s.line_range(1), Some(6..11));
    assert_eq!(s.line_range(2), None);
}

#[test]
fn trailing_newline_counts_as_empty_line() {
    let s = drive(b"a\n");
    assert_eq!(s.line_count(), 2);
    assert_eq!(s.line_range(0), Some(0..1));
    assert_eq!(s.line_range(1), Some(2..2));
    assert_eq!(s.line_range(2), None);
}

#[test]
fn cr_is_normalized_to_newline() {
    // `State::execute` maps both `\n` and `\r` to `\n`, so CRLF makes two
    // line breaks in the byte-cursor state.
    let s = drive(b"a\r\nb");
    assert_eq!(s.text, "a\n\nb");
    assert_eq!(s.line_count(), 3);
    assert_eq!(s.cursor_row(), 2);
    assert_eq!(s.cursor_col(), 1);
}
