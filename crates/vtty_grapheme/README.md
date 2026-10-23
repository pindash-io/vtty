# vtty_grapheme

<!-- cargo-rdme start -->

UAX #29 extended grapheme cluster segmentation for the terminal grid,
with per-cluster display cell widths from [`vtty_width`](https://docs.rs/vtty_width/latest/vtty_width/).

`vtty_width` answers "what is the display width of this text" per *char*
and merges emoji sequences; `vtty_grapheme` answers "where do the
clusters start and end". A grapheme cluster is the unit the grid,
selection, and editing operate on: one cluster ≈ one (or two) cells.

The boundaries implement the classic UAX #29 grapheme rules (GB1–GB13)
over `vtty_width`'s baked [`Gcb`](https://docs.rs/vtty_width/latest/vtty_width/properties/enum.Gcb.html) property, with `Extended_Pictographic`
folded in as [`Gcb::ExtPict`](https://docs.rs/vtty_width/latest/vtty_width/properties/enum.Gcb.html#variant.ExtPict) — one property lookup per char, no
`icu_segmenter` dependency.

## Example

```rust
use vtty_grapheme::{cluster_widths, graphemes};

let text = "👨‍👩‍👧 e\u{301} 中";

let clusters: Vec<&str> = graphemes(text).map(|g| g.as_str(text)).collect();
assert_eq!(clusters, ["👨‍👩‍👧", " ", "e\u{301}", " ", "中"]);

let widths: Vec<usize> = cluster_widths(text).collect();
assert_eq!(widths, [2, 1, 1, 1, 2]);
```

<!-- cargo-rdme end -->

## API

| function | returns |
| --- | --- |
| `graphemes(&str)` | iterator of `Grapheme` (byte ranges) |
| `cluster_widths(&str)` | iterator of per-cluster cell widths |
| `cluster_count(&str)` | number of clusters |
| `cluster_width_sum(&str)` | total cell width |

## Future work

The rules implement the classic GB1–GB13 set. Not yet covered: GB9c (Indic
conjunct breaks, Unicode 15.1+) and the newer emoji GCB values
(`E_Base`/`E_Modifier`/`Glue_After_Zwj`) — the latter are unnecessary for
width because `vtty_width`'s DFA already merges emoji sequences. If exact
ICU4X parity is needed, add GB9c and bake `Indic_Conjunct_Break`.
