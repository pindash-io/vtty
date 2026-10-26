// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property tests: grapheme clusters partition the input and the per-cluster
//! width sum agrees with the per-char width model.

use proptest::prelude::*;
use vtty_grapheme::{cluster_count, cluster_width_sum, graphemes};

proptest! {
    /// Graphemes partition the input: contiguous, byte-exact, start-to-end.
    #[test]
    fn graphemes_partition_text(chars in prop::collection::vec(any::<char>(), 0..64)) {
        let text: String = chars.into_iter().collect();
        let clusters: Vec<_> = graphemes(&text).collect();
        if text.is_empty() {
            prop_assert!(clusters.is_empty());
        } else {
            prop_assert_eq!(clusters[0].start(), 0);
            prop_assert_eq!(clusters.last().unwrap().end(), text.len());
            for pair in clusters.windows(2) {
                prop_assert_eq!(pair[0].end(), pair[1].start());
            }
        }
    }

    /// The per-cluster width sum equals the per-char width sum.
    #[test]
    fn cluster_width_sum_matches_cell_width_sum(chars in prop::collection::vec(any::<char>(), 0..64)) {
        let text: String = chars.into_iter().collect();
        prop_assert_eq!(cluster_width_sum(&text), vtty_width::cell_width_sum(&text));
    }

    /// `cluster_count` equals the number of yielded graphemes.
    #[test]
    fn cluster_count_matches_yield(chars in prop::collection::vec(any::<char>(), 0..64)) {
        let text: String = chars.into_iter().collect();
        prop_assert_eq!(cluster_count(&text), graphemes(&text).count());
    }
}
