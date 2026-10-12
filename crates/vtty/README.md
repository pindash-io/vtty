# vtty

<!-- cargo-rdme start -->

Zero-allocation, panic-free DEC VT500 terminal parser with UTF-8 decoding
and `no_std` support.

## Usage

```rust
use vtty::{Parser, Perform};

struct Screen;

impl Perform for Screen {
    fn print_bytes(&mut self, s: &[u8]) { /* printable ASCII run */ }
    fn print(&mut self, c: char) { /* non-ASCII code point */ }
    fn execute(&mut self, b: u8) { /* \n \t \x1b … */ }
    fn csi_dispatch(&mut self, params: &[i32], sep: u16, intermediates: &[u8], final_byte: u8) {
        /* CSI Ps ; Ps final */
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) { /* OSC */ }
}

let mut parser = Parser::new();
parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");

// With the `simd` feature enabled, opt into SIMD scanning + validation:
let mut parser = Parser::with_simd();
parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
```

Also available: `Event`/`Recording` (owned enum API), `State` (byte-cursor
sink with line/cursor geometry), `Reply` (query reply builder), and
`ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`, `MouseButton`, `Charset`, `Cap`.

## Design

- **Panic-free** — no `unwrap`/`expect`/`panic`; malformed input is normal.
- **Zero-allocation hot path** — printable ASCII runs dispatch via one
  `print_bytes` and non-ASCII via one `print_codepoints`; CSI/OSC/DCS state
  is inline (`SmallVec`).
- **`no_std` by default** — the `simd` feature compiles in SIMD scanning +
  validation; enable it per parser with `Parser::with_simd()` (scalar
  `Parser::new()` is the default).
- **Byte-accurate UTF-8** — partial code points split across `advance` calls
  are carried over, never misdecoded to `U+FFFD`.

## Test

```bash
cargo test -p vtty
cargo test -p vtty --test vt_differential -- --nocapture  # parity vs vte
```

<!-- cargo-rdme end -->
