# vtty_screen

<!-- cargo-rdme start -->

A minimal cell-grid terminal screen for `vtty`.

`Screen` is a `Perform` sink: feed it bytes through `vtty::Parser` and it
maintains a `rows × cols` grid of styled cells, a cursor, DECSTBM scroll
margins, and the common DEC cursor/erase/insert/delete commands. Render
with [`Screen::render_line`](https://docs.rs/vtty_screen/latest/vtty_screen/struct.Screen.html#method.render_line)/[`Screen::lines`](https://docs.rs/vtty_screen/latest/vtty_screen/struct.Screen.html#method.lines) (plain text) or
[`Screen::render_ansi`](https://docs.rs/vtty_screen/latest/vtty_screen/struct.Screen.html#method.render_ansi) (SGR-styled text).

This is the MVP — cursor addressing, scrolling, SGR, DECSC/DECRC, DECALN,
the alternate screen buffer (DEC 47/1046/1047/1049), scrollback (with
resize reflow), tab stops (HTS/TBC), resize reflow, and the VT editing
commands. Combining marks and ZWJ emoji families (up to eight trailing
glyphs) attach to their base cell.

The cursor, erase, insert/delete and scroll commands follow EK-VT520-RM
(ANSI Control Functions).

<!-- cargo-rdme end -->

## Usage

```rust,ignore
use vtty::Parser;
use vtty_screen::Screen;

let mut screen = Screen::new(80, 24);
Parser::new().advance(&mut screen, b"\x1b[1;31mhello\x1b[0m\r\n");
assert_eq!(screen.render_line(0), "hello");
```

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.
