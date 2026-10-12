# vtty_emoji

<!-- cargo-rdme start -->

Display cell width and emoji-sequence handling.

## Usage

```rust
use vtty_emoji::{cell_width, cell_widths, cell_advance};

cell_width('中');                    // Some(2)
cell_width('\u{1F3FB}');             // Some(0) — skin-tone modifier
cell_widths("⚠\u{FE0F}");            // [2, 0] — VS16 widens the base
cell_widths("👨\u{200D}👩\u{200D}👧");    // [2, 0, 0, 0, 0] — ZWJ family
cell_advance(1.6, 10.0);             // 20.0 — snap a fractional advance to 2 cells
```

Also: `cell_col_byte_widths`, `Properties` (packed emoji flags + width), the
`emoji` segmentation DFA (`scan`/`classify`), and `width_icu` (`icu` feature).

## Features

`simd` + `baked` (default) | `icu` (runtime ICU4X lookups).

## Credits

The emoji DFA is ported from [`chansen/c-emoji`](https://github.com/chansen/c-emoji) (MIT).

<!-- cargo-rdme end -->
