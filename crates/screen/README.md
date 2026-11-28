# vtty_screen

<!-- cargo-rdme start -->

A minimal cell-grid terminal screen for `vtty`.

`Screen` is a `Perform` sink: fed bytes through `vtty::Parser`, it keeps a
`rows × cols` grid of styled cells, a cursor, DECSTBM scroll margins, and the
common DEC cursor, erase, insert, delete and scroll commands
(EK-VT520-RM). Render with [`Screen::render_line`](https://docs.rs/vtty_screen/latest/vtty_screen/struct.Screen.html#method.render_line)/[`Screen::lines`](https://docs.rs/vtty_screen/latest/vtty_screen/struct.Screen.html#method.lines) for
plain text or [`Screen::render_ansi`](https://docs.rs/vtty_screen/latest/vtty_screen/struct.Screen.html#method.render_ansi) for SGR-styled text.

Covered: SGR, DECSC/DECRC, DECALN, the alternate screen (DEC
47/1046/1047/1049), scrollback with reflow, and tab stops (HTS/TBC).
Combining marks and ZWJ families (up to eight glyphs) attach to their base
cell. Sixel, `ReGIS` and soft fonts are out of scope: this is a text grid,
not a pixel emulator.

<!-- cargo-rdme end -->

## Usage

```rust,ignore
use vtty::Parser;
use vtty_screen::Screen;

let mut screen = Screen::new(80, 24);
Parser::new().advance(&mut screen, b"\x1b[1;31mhello\x1b[0m\r\n");
assert_eq!(screen.render_line(0), "hello");
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
