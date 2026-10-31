// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The [`State`] byte-cursor sink.

use alloc::string::String;
use core::ops::Range;

use crate::perform::Perform;

/// Minimal byte-cursor state (the O(bytes) floor).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct State {
    /// Accumulated text (lines joined by `\n`).
    pub text: String,
    /// Cursor position in `text` as a BYTE offset.
    pub cursor: usize,
    /// Byte offset of the current line's start (the `\n`-free column base).
    pub line_start: usize,
}

impl State {
    /// An empty cursor state.
    pub const DEFAULT: Self = Self {
        text: String::new(),
        cursor: 0,
        line_start: 0,
    };

    /// An empty cursor state at byte offset 0.
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// Number of lines in the text buffer. An empty buffer has 0 lines; each
    /// `\n` starts a new line (a trailing `\n` counts as one empty line).
    pub fn line_count(&self) -> usize {
        let bytes = self.text.as_bytes();
        let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
        newlines + usize::from(!bytes.is_empty())
    }

    /// 0-based row (line index) of the cursor.
    pub fn cursor_row(&self) -> usize {
        let cursor = self.cursor.min(self.text.len());
        self.text[..cursor].bytes().filter(|&b| b == b'\n').count()
    }

    /// 0-based BYTE column of the cursor within its line (offset from the
    /// line start, not a terminal cell column — this byte-cursor state has no
    /// cell grid).
    pub fn cursor_col(&self) -> usize {
        self.cursor.min(self.text.len()) - self.line_start
    }

    /// Byte range `[start, end)` of line `line` (0-based). `end` is exclusive:
    /// it points at the line's `\n` byte, or at the buffer end for the last
    /// line. Returns `None` when `line` is out of range (or the buffer is
    /// empty).
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let bytes = self.text.as_bytes();
        if bytes.is_empty() {
            return None;
        }

        // `match_indices` yields each `\n` byte offset lazily: skip to the
        // newline BEFORE line `line` (start is one past it), then the next
        // newline is the line's end — one pass, no manual index bookkeeping.
        let mut newlines = self.text.match_indices('\n');
        let start = if line == 0 {
            0
        } else {
            newlines.nth(line - 1)?.0 + 1
        };
        let end = newlines.next().map_or_else(|| bytes.len(), |(i, _)| i);
        Some(start..end)
    }
}

impl Perform for State {
    fn print(&mut self, c: char) {
        let pos = self.cursor;
        if pos < self.text.len() {
            self.text.insert(pos, c);
        } else {
            self.text.push(c);
        }
        self.cursor = pos + c.len_utf8();
    }

    fn print_bytes(&mut self, s: &[u8]) {
        // Printable ASCII is valid UTF-8 by construction; a non-UTF-8 run is
        // impossible and is dropped rather than mishandled.
        let Ok(s) = core::str::from_utf8(s) else {
            return;
        };
        let pos = self.cursor;
        if pos < self.text.len() {
            self.text.insert_str(pos, s);
        } else {
            self.text.push_str(s);
        }
        self.cursor = pos + s.len();
    }

    fn execute(&mut self, b: u8) {
        match b {
            b'\n' => {
                self.text.push('\n');
                self.cursor = self.text.len();
                self.line_start = self.cursor;
            }
            // CR returns to column 0 without advancing a line.
            b'\r' => self.cursor = self.line_start,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(text: &str) -> State {
        State {
            text: String::from(text),
            cursor: 0,
            line_start: 0,
        }
    }

    #[test]
    fn line_range_empty() {
        assert_eq!(state("").line_range(0), None);
    }

    #[test]
    fn line_range_single_line() {
        let s = state("abc");
        assert_eq!(s.line_range(0), Some(0..3));
        assert_eq!(s.line_range(1), None);
    }

    #[test]
    fn line_range_multibyte_uses_byte_offsets() {
        // 中 is 3 bytes; the range must be a BYTE range, not a char index.
        let s = state("中a\nb");
        assert_eq!(s.line_range(0), Some(0..4)); // 中(3) + a(1), the \n at 4
        assert_eq!(s.line_range(1), Some(5..6)); // b
    }

    #[test]
    fn line_range_multi_line_and_empty_last() {
        let s = state("a\nb\n");
        assert_eq!(s.line_range(0), Some(0..1));
        assert_eq!(s.line_range(1), Some(2..3));
        assert_eq!(s.line_range(2), Some(4..4)); // trailing empty line
        assert_eq!(s.line_range(3), None);
    }

    #[test]
    fn line_range_last_line_without_newline() {
        let s = state("a\nb");
        assert_eq!(s.line_range(0), Some(0..1));
        assert_eq!(s.line_range(1), Some(2..3));
    }

    #[test]
    fn perform_inserts_at_cursor() {
        let mut s = State::new();
        s.print('a');
        s.print('b');
        assert_eq!(s.text, "ab");
        // Move the cursor back and insert mid-text.
        s.cursor = 1;
        s.print('X');
        assert_eq!(s.text, "aXb");
        s.print_bytes(b"YZ");
        assert_eq!(s.text, "aXYZb");
        // CR returns to the line start without advancing a line.
        s.execute(b'\r');
        assert_eq!(s.cursor, s.line_start);
    }

    #[test]
    fn perform_drops_invalid_utf8_and_ignores_unknown_controls() {
        let mut s = State::new();
        s.print('a');
        // A non-UTF-8 byte run is dropped rather than mishandled.
        s.print_bytes(&[0xff]);
        assert_eq!(s.text, "a");
        // Unknown C0 controls are no-ops.
        s.execute(0x00);
        assert_eq!(s.text, "a");
        assert_eq!(s.cursor, 1);
    }
}
