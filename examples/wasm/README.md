# vtty — web (WASM) example

A complete browser terminal demo built on the `vtty` parser. A Rust
`Terminal` implements `vtty::Perform` so `Parser::advance` drives a
character-cell screen directly (zero intermediate `Event`s), then draws the
screen onto a `<canvas>` via `web-sys` (an HTML renderer is also available).
The frontend is a small static page.

```
examples/wasm/
├── Cargo.toml      # wasm-bindgen + vtty (default-features off)
├── build.sh        # wasm-pack build + local server
├── src/lib.rs      # Terminal: Perform impl → cell grid → canvas (and HTML)
└── www/
    ├── index.html
    ├── main.js     # feed input, run a color demo, draw to <canvas>
    └── style.css
```

## What it covers

- **Parser → screen** — `feed()` hands bytes to `Parser::advance` with the
  `Terminal` itself as the `Perform` sink.
- **Canvas** — `draw()` paints the cell grid to a `CanvasRenderingContext2d`
  (background rects, glyphs, bold/italic fonts, underline/overline/strike
  lines); `render_html()` / `plain_text()` stay available as alternatives.
- **SGR** — 16-color, 256-color (`38;5;n`), truecolor (`38;2;r;g;b`, both `;`
  and `:` forms), bold/italic/underline/strikethrough/overline/inverse.
- **Cursor** — `CUP`/`CUU`/`CUD`/`CUF`/`CUB`/`CHA`/`VPA`, tab/backspace/CR/LF,
  autowrap, save/restore (`ESC 7/8`, `CSI s/u`).
- **Erase** — `ED` (`CSI J`) and `EL` (`CSI K`) in all modes, plus `ECH`.
- **OSC** — `0`/`2` window title is surfaced to the page header.
- **Modes** — `DECAWM` (`?7h/l`) and `DECTCEM` (`?25h/l`, cursor blink).
- **Selection** — drag to select, double-click a word, triple-click a line,
  `Ctrl/Cmd+C` copies to the clipboard.
- **Code block** — the **Code block** button feeds a syntax-highlighted
  snippet (keywords/strings/comments/numbers via SGR) to show real-world
  rendering.

## Code-block example

`www/main.js` renders a highlighted snippet by wrapping tokens in SGR codes:

```js
// js colors: R=reset K=keyword S=string C=comment N=number T=type
const code = `${K}use${R} vtty::{Parser, Perform};
${K}struct${R} Screen;
${K}impl${R} Perform ${K}for${R} Screen {
    ${K}fn${R} ${T}print_bytes${R}(&${K}mut self${R}, s: &[${N}u8${R}]) {}
}`;
term.feed(code);
```

## Selection API

The `Terminal` exposes the selection as a normalized inclusive cell range
(start ≤ end in row-major order); `draw()` paints it with a highlight
background and `selected_text()` extracts copyable text.

```rust
// Rust (wasm-bindgen) — see src/lib.rs for the full Terminal impl.
term.feed("\x1b[31mhello world\x1b[0m\r\n");

term.select(0, 0, 0, 4);          // drag: inclusive (r0,c0)..=(r1,c1)
assert_eq!(term.selected_text(), "hello");

term.select_word(0, 2);           // double-click: word at (0,2)
assert_eq!(term.selected_text(), "hello");

term.select_line(0);              // triple-click: whole row
term.clear_selection();           // clear
```

```js
// JavaScript — the wiring in www/main.js (mousedown/mousemove/mouseup).
const cellAt = (e) => ({
  col: Math.floor((e.clientX - canvas.getBoundingClientRect().left) / cellW),
  row: Math.floor((e.clientY - canvas.getBoundingClientRect().top) / cellH),
});

// copy: Ctrl/Cmd+C
const text = term.selected_text();
if (text) navigator.clipboard.writeText(text);
```

## Build & run

```bash
./build.sh                       # wasm-pack build + python http.server
# or, manually:
wasm-pack build --target web --out-dir www/pkg --release
cd www && python3 -m http.server 8000
```

Open <http://localhost:8000>. Click **Run demo**, or type into the box —
`\x1b`, `\n`, `\r`, `\t` are interpreted, so `\x1b[31mred\x1b[0m` renders in
red.

## Notes

- One character per cell: wide CJK/emoji are rendered but not padded to their
  two-cell display width (the `vtty_emoji` crate provides that model).
- `faint` (SGR 2) and the scroll-region (`DECSTBM`) are ignored to keep the
  example small; the parser still decodes them.
