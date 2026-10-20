# vtty_grapheme

UAX #29 extended grapheme cluster segmentation for the terminal grid, with
per-cluster display cell widths from [`vtty_emoji`].

`vtty_emoji` answers *"what is the width of this text"* per char and merges
emoji sequences; `vtty_grapheme` answers *"where do the clusters start and
end"*. Together they give the grid/editing unit: one grapheme cluster ≈ one
(or two) cells.

```rust
use vtty_grapheme::{cluster_widths, graphemes};

let text = "👨‍👩‍👧 e\u{301} 中";

let clusters: Vec<&str> = graphemes(text).map(|g| g.as_str(text)).collect();
assert_eq!(clusters, ["👨‍👩‍👧", " ", "e\u{301}", " ", "中"]);

let widths: Vec<usize> = cluster_widths(text).collect();
assert_eq!(widths, [2, 1, 1, 1, 2]);
```

## Why this exists

- **Boundaries**: the classic UAX #29 rules (GB1–GB13) implemented over
  `vtty_emoji`'s baked [`Gcb`] property and `Extended_Pictographic` —
  combining marks, ZWJ emoji, regional-indicator pairs, Hangul jamo,
  spacing/prepend marks, CR/LF. No `icu_segmenter` dependency.
- **Widths**: `vtty_emoji::cell_width_sum` provides the display width of each
  cluster (wide chars, emoji presentation, zero-width marks).

The core invariant (tested): UAX #29 cluster boundaries never split a
`vtty_emoji` width sequence, so `cluster_width_sum(text) ==
vtty_emoji::cell_width_sum(text)`.

## API

| function | returns |
| --- | --- |
| `graphemes(&str)` | iterator of `Grapheme` (byte ranges) |
| `cluster_widths(&str)` | iterator of per-cluster cell widths |
| `cluster_count(&str)` | number of clusters |
| `cluster_width_sum(&str)` | total cell width |

## Relationship to `vtty_emoji`

`vtty_emoji` is the per-char width + emoji-sequence layer; this crate is the
cluster-boundary layer. Keep the parser (`vtty`) allocation-free and streaming:
this crate operates on a *complete line* (`&str`), not on the byte stream.

## Future work

The rules implement the classic GB1–GB13 set. Not yet covered: GB9c (Indic
conjunct breaks, Unicode 15.1+) and the newer emoji GCB values
(`E_Base`/`E_Modifier`/`Glue_After_Zwj`) — the latter are unnecessary for
width because `vtty_emoji`'s DFA already merges emoji sequences. If exact
ICU4X parity is needed, add GB9c and bake `Indic_Conjunct_Break`.
