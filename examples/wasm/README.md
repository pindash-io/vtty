# vtty — web (WASM) example

A minimal browser demo: feed bytes to the `vtty` parser and read the text it
produces. A `Terminal` wraps `vtty::Parser` + the built-in `vtty::State`
byte-cursor sink; JavaScript feeds an ANSI string and shows the result.

```
examples/wasm/
├── Cargo.toml      # wasm-bindgen + vtty (default-features off)
├── build.sh        # wasm-pack build + local server
├── src/lib.rs      # Terminal: Parser + State, feed()/text()/clear()
└── www/
    ├── index.html
    ├── main.js     # feed a demo, display the parsed text
    └── style.css
```

## Build & run

```bash
./build.sh                       # wasm-pack build + python http.server
# or, manually:
wasm-pack build --target web --out-dir www/pkg --release
cd www && python3 -m http.server 8000
```

Open <http://localhost:8000>. The **Demo** button feeds a string with SGR
colors, bold, underline and multibyte text; the output shows the text with
every escape sequence consumed. Type into the box — `\x1b` and `\n` are
interpreted, so `\x1b[31mred\x1b[0m` parses to `red`.
