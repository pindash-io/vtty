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

## API

- **Width** — `cell_width`, `cell_widths`, `cell_width_sum`,
  `cell_col_byte_widths`, `cell_advance`.
- **Predicates** — `vs_width`, `is_zero_width`.
- **Emoji segmentation** — `mark_attaches`, `mark_is_stray`, `is_vs_base`,
  `is_vs_target`, `is_zwj_target`.
- **`Properties`** — packed emoji flags + width in one lookup.

## Features

`simd` + `baked` (default) | `icu` (runtime ICU4X lookups).

## Credits

The emoji DFA is provided by [`parley_emoji`](https://github.com/linebender/parley) (MIT).

<!-- cargo-rdme end -->
