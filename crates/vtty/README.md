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

// With the `simd-transcode` feature enabled, opt into SIMD scanning,
// validation and UTF-8→UTF-32 transcode:
let mut parser = Parser::with_simd();
parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
```

Also available: `Event`/`Recording` (owned enum API), `State` (byte-cursor
sink with line/cursor geometry), `Reply` (query reply builder), and
`ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`, `MouseButton`, `Charset`, `Cap`.

## Design

- **Panic-free** — `#![forbid(unsafe_code)]`; malformed PTY input never
  crashes.
- **Zero-allocation hot path** — one `print_bytes` per ASCII run, one
  `print_codepoints` per non-ASCII run; CSI/OSC/DCS state is inline.
- **`no_std` by default** — optional `simd` (scanning + validation) and
  `simd-transcode` (UTF-8→UTF-32) features, enabled per parser.
- **W3C-correct UTF-8** — maximal-subpart substitution; 8-bit clean
  (DEC “anywhere”).

## Test

```bash
cargo test -p vtty
cargo test -p vtty --test vt_differential -- --nocapture  # parity vs vte
```

<!-- cargo-rdme end -->
