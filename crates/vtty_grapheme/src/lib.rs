// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UAX #29 extended grapheme cluster segmentation for the terminal grid,
//! with per-cluster display cell widths from [`vtty_width`].
//!
//! `vtty_width` answers "what is the display width of this text" per *char*
//! and merges emoji sequences; `vtty_grapheme` answers "where do the
//! clusters start and end". A grapheme cluster is the unit the grid,
//! selection, and editing operate on: one cluster ≈ one (or two) cells.
//!
//! The boundaries implement the classic UAX #29 grapheme rules (GB1–GB13)
//! over `vtty_width`'s baked [`Gcb`] property, with `Extended_Pictographic`
//! folded in as [`Gcb::ExtPict`] — one property lookup per char, no
//! `icu_segmenter` dependency.
//!
//! ## Example
//!
//! ```
//! use vtty_grapheme::{cluster_widths, graphemes};
//!
//! let text = "👨‍👩‍👧 e\u{301} 中";
//!
//! let clusters: Vec<&str> = graphemes(text).map(|g| g.as_str(text)).collect();
//! assert_eq!(clusters, ["👨‍👩‍👧", " ", "e\u{301}", " ", "中"]);
//!
//! let widths: Vec<usize> = cluster_widths(text).collect();
//! assert_eq!(widths, [2, 1, 1, 1, 2]);
//! ```

#![forbid(unsafe_code)]

use vtty_width::Gcb;

/// A grapheme cluster: a byte range `[start, end)` into the source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Grapheme {
    start: usize,
    end: usize,
}

impl Grapheme {
    /// Construct a cluster spanning `[start, end)`.
    #[inline]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Byte offset of the first byte of the cluster.
    #[inline]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Byte offset one past the last byte of the cluster.
    #[inline]
    pub const fn end(self) -> usize {
        self.end
    }

    /// The cluster's length in bytes.
    #[inline]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Whether the cluster is empty (never produced by [`graphemes`]).
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The cluster's substring of `text`.
    #[inline]
    pub fn as_str(self, text: &str) -> &str {
        &text[self.start..self.end]
    }
}

/// The GB11 lookahead state: whether the current cluster so far ends with
/// `Extended_Pictographic Extend*` (awaiting `ZWJ`) or that plus `ZWJ`
/// (awaiting the following `Extended_Pictographic`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gb11 {
    None,
    ExtPict,
    Zwj,
}

/// Advance the GB11 state for a char being added to the current cluster.
#[inline]
fn step_gb11(prev: Gb11, gcb: Gcb) -> Gb11 {
    match gcb {
        // GB9: Extend always attaches; it does not change the chain.
        Gcb::Extend => prev,
        Gcb::ZWJ => {
            if prev == Gb11::ExtPict {
                Gb11::Zwj
            } else {
                Gb11::None
            }
        }
        Gcb::ExtPict => Gb11::ExtPict,
        _ => Gb11::None,
    }
}

/// Whether there is a cluster boundary between `left` (already in the
/// cluster) and `right` (the candidate), per UAX #29 GB3–GB13.
#[inline]
fn breaks_between(left: Gcb, right: Gcb, gb11: Gb11, ri: u32) -> bool {
    // GB3: CR × LF.
    if left == Gcb::CR && right == Gcb::LF {
        return false;
    }
    // GB4: (Control | CR | LF) ÷.
    if left.is_control() {
        return true;
    }
    // GB5: ÷ (Control | CR | LF).
    if right.is_control() {
        return true;
    }
    // GB6: L × (L | V | LV | LVT).
    if left == Gcb::L && matches!(right, Gcb::L | Gcb::V | Gcb::LV | Gcb::LVT) {
        return false;
    }
    // GB7: (LV | V) × (V | T).
    if matches!(left, Gcb::LV | Gcb::V) && matches!(right, Gcb::V | Gcb::T) {
        return false;
    }
    // GB8: (LVT | T) × T.
    if matches!(left, Gcb::LVT | Gcb::T) && right == Gcb::T {
        return false;
    }
    // GB9: × (Extend | ZWJ).
    if matches!(right, Gcb::Extend | Gcb::ZWJ) {
        return false;
    }
    // GB9a: × SpacingMark.
    if right == Gcb::SpacingMark {
        return false;
    }
    // GB9b: Prepend ×.
    if left == Gcb::Prepend {
        return false;
    }
    // GB11: Extended_Pictographic Extend* ZWJ × Extended_Pictographic.
    if right == Gcb::ExtPict && gb11 == Gb11::Zwj {
        return false;
    }
    // GB12/GB13: (RI RI)* RI × RI (odd-length runs keep their pair).
    if left == Gcb::RegionalIndicator && right == Gcb::RegionalIndicator && ri % 2 == 1 {
        return false;
    }
    true
}

/// Iterator over the grapheme clusters of a UTF-8 string.
///
/// Yields [`Grapheme`] ranges in order; the first cluster starts at 0 and
/// the last ends at `text.len()`.
#[derive(Debug)]
pub struct Graphemes<'a> {
    iter: core::str::CharIndices<'a>,
    text: &'a str,
    /// Remaining all-ASCII bytes to yield as one-byte clusters (fast path).
    ascii: usize,
    /// Byte offset of the current cluster's first byte.
    cluster_start: usize,
    /// The GCB of the char preceding the current position, if any.
    left: Option<Gcb>,
    gb11: Gb11,
    /// Consecutive regional indicators in the current cluster.
    ri: u32,
    finished: bool,
}

/// Segment `text` into extended grapheme clusters (UAX #29, rule-based).
///
/// Uses `vtty_width`'s baked GCB property; no runtime data loading or
/// allocation on this path. Pure ASCII takes a per-byte fast path.
pub fn graphemes(text: &str) -> Graphemes<'_> {
    let bytes = text.as_bytes();
    let printable = vtty_width::first_non_printable_ascii(bytes) == bytes.len();
    Graphemes {
        iter: if printable {
            "".char_indices()
        } else {
            text.char_indices()
        },
        text,
        ascii: if printable { bytes.len() } else { 0 },
        cluster_start: 0,
        left: None,
        gb11: Gb11::None,
        ri: 0,
        finished: text.is_empty(),
    }
}

impl Graphemes<'_> {
    /// Start a new cluster at `idx` with `gcb` as its first char (GB1 and the
    /// break branch share this reset).
    fn start_cluster(&mut self, idx: usize, gcb: Gcb) {
        self.cluster_start = idx;
        self.left = Some(gcb);
        self.gb11 = step_gb11(Gb11::None, gcb);
        self.ri = u32::from(gcb == Gcb::RegionalIndicator);
    }
}

impl Iterator for Graphemes<'_> {
    type Item = Grapheme;

    fn next(&mut self) -> Option<Grapheme> {
        if self.finished {
            return None;
        }
        // All-ASCII fast path: every byte is its own cluster.
        if self.ascii > 0 {
            let start = self.cluster_start;
            self.ascii -= 1;
            self.cluster_start += 1;
            if self.ascii == 0 {
                self.finished = true;
            }
            return Some(Grapheme::new(start, start + 1));
        }
        loop {
            let Some((idx, ch)) = self.iter.next() else {
                // GB2: ÷ eot — emit the final cluster.
                self.finished = true;
                return Some(Grapheme::new(self.cluster_start, self.text.len()));
            };

            let gcb = Gcb::get(ch);

            match self.left {
                None => {
                    // GB1: sot ÷ — the first char always starts a cluster.
                    self.start_cluster(idx, gcb);
                }
                Some(left) => {
                    if breaks_between(left, gcb, self.gb11, self.ri) {
                        // Break before `ch`: emit the cluster ending at `idx`.
                        let cluster = Grapheme::new(self.cluster_start, idx);
                        self.start_cluster(idx, gcb);
                        return Some(cluster);
                    }
                    // No break: extend the current cluster.
                    self.left = Some(gcb);
                    self.gb11 = step_gb11(self.gb11, gcb);
                    if gcb == Gcb::RegionalIndicator {
                        self.ri += 1;
                    } else {
                        self.ri = 0;
                    }
                }
            }
        }
    }
}

/// Iterator over per-cluster display cell widths (1 or 2; 0 for a cluster
/// that is only zero-width marks).
#[derive(Debug)]
pub struct ClusterWidths<'a> {
    graphemes: Graphemes<'a>,
    text: &'a str,
}

/// Per-cluster display cell width: [`vtty_width::cell_width_sum`] of each
/// grapheme cluster in `text`.
pub fn cluster_widths(text: &str) -> ClusterWidths<'_> {
    ClusterWidths {
        graphemes: graphemes(text),
        text,
    }
}

impl Iterator for ClusterWidths<'_> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        let cluster = self.graphemes.next()?;
        Some(vtty_width::cell_width_sum(cluster.as_str(self.text)))
    }
}

/// The number of grapheme clusters in `text`.
pub fn cluster_count(text: &str) -> usize {
    graphemes(text).count()
}

/// The total display cell width of `text`, summed per grapheme cluster.
///
/// Equivalent to [`vtty_width::cell_width_sum`] when the cluster boundaries
/// and the emoji width model agree (they do for UAX #29 vs UTS #51 sequences).
pub fn cluster_width_sum(text: &str) -> usize {
    // Printable-ASCII fast path: every byte is one cell, so the total is the
    // byte length. Controls are zero-width and take the general path.
    if vtty_width::first_non_printable_ascii(text.as_bytes()) == text.len() {
        return text.len();
    }
    cluster_widths(text).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clusters_vec(text: &str) -> Vec<&str> {
        graphemes(text).map(|g| g.as_str(text)).collect()
    }

    #[test]
    fn uax29_sample_clusters() {
        // UAX #29 Table 1a sample grapheme clusters.
        assert_eq!(clusters_vec("मांजर"), ["मां", "ज", "र"]);
        // Tamil NA + vowel sign I is a single extended cluster.
        assert_eq!(clusters_vec("நி"), ["நி"]);
        // World map U+1F5FA is one 4-byte cluster.
        assert_eq!(clusters_vec("Hello 🗺"), ["H", "e", "l", "l", "o", " ", "🗺"]);
    }

    #[test]
    fn zwj_family_is_one_cluster() {
        assert_eq!(clusters_vec("👨‍👩‍👧"), ["👨‍👩‍👧"]);
    }

    #[test]
    fn combining_sequence_is_one_cluster() {
        assert_eq!(clusters_vec("e\u{301}"), ["e\u{301}"]);
    }

    #[test]
    fn regional_indicator_pairing() {
        // Two regional indicators form one cluster; four form two.
        assert_eq!(clusters_vec("🇺🇸"), ["🇺🇸"]);
        assert_eq!(clusters_vec("🇺🇸🇨🇦"), ["🇺🇸", "🇨🇦"]);
    }

    #[test]
    fn hangul_jamo_sequence() {
        // L + V jamo form one cluster.
        assert_eq!(clusters_vec("\u{1100}\u{1161}"), ["\u{1100}\u{1161}"]);
    }

    #[test]
    fn empty_string_has_no_clusters() {
        assert_eq!(clusters_vec(""), Vec::<&str>::new());
        assert_eq!(cluster_count(""), 0);
    }

    #[test]
    fn ascii_is_one_cluster_per_byte() {
        assert_eq!(clusters_vec("abc"), ["a", "b", "c"]);
        assert_eq!(cluster_count("abc"), 3);
        assert_eq!(cluster_width_sum("abc"), 3);
    }

    #[test]
    fn control_chars_break() {
        // GB4/GB5: controls break on both sides (CR×LF stays together).
        assert_eq!(clusters_vec("a\nb"), ["a", "\n", "b"]);
        assert_eq!(clusters_vec("a\r\nb"), ["a", "\r\n", "b"]);
    }

    #[test]
    fn cluster_widths_match_cells() {
        // ZWJ family = one cluster, width 2.
        assert_eq!(cluster_widths("👨‍👩‍👧").collect::<Vec<_>>(), [2]);
        // base + combining = one cluster, width 1.
        assert_eq!(cluster_widths("e\u{301}").collect::<Vec<_>>(), [1]);
        // Wide CJK = width 2.
        assert_eq!(cluster_widths("中").collect::<Vec<_>>(), [2]);
        // VS16 widens a text-default emoji base.
        assert_eq!(cluster_widths("❤\u{FE0F}").collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn cluster_width_sum_equals_per_char_sum() {
        // The invariant this crate is built on: cluster boundaries never
        // split a vtty_width width sequence, so the per-cluster sum equals
        // the per-char sum.
        for text in [
            "hello",
            "中文字",
            "👨‍👩‍👧 family",
            "e\u{301} combine",
            "🇺🇸🇨🇦 flags",
            "❤\u{FE0F} vs16",
            "가나 hangul",
            // Controls/format chars break clusters but are zero-width, and
            // Hangul V/T only attach to a Hangul base — the width model and
            // the grapheme model must agree on both.
            "¡\0",
            "a\0b",
            "\u{200B}",
            "®\u{1160}",
            "\u{1160}\u{11A8}",
        ] {
            assert_eq!(
                cluster_width_sum(text),
                vtty_width::cell_width_sum(text),
                "mismatch for {text:?}"
            );
        }
    }
}
