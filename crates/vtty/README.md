# vtty

<!-- cargo-rdme start -->

A **zero-allocation**, **panic-free** DEC VT500 terminal parser with
**UTF-8** decoding and **`no_std`** support.

`vtty` turns bytes into typed events via the `Perform` callback trait and
re-exports the terminal model types (`ColorId`, `SgrFlags`, `Charset`, …)
from `vtty_core`. The companion `vtty_codec` crate provides the compact
binary `Encode`/`Decode` implementations for the owned types.

The parser implements the DEC VT500 state machine from Paul Flo Williams'
[A parser for DEC's ANSI-compatible video terminals][dec-parser].

## Usage

```rust
use vtty::{Parser, Perform};

struct Screen;

impl Perform for Screen {
    fn print_bytes(&mut self, _s: &[u8]) { /* printable ASCII run */ }
    fn print(&mut self, _c: char) { /* non-ASCII code point */ }
    fn execute(&mut self, _b: u8) { /* \n \t \x1b … */ }
    fn csi_dispatch(&mut self, _params: &[i32], _sep: u16, _intermediates: &[u8], _final_byte: u8) {
        /* CSI Ps ; Ps final */
    }
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell: bool) { /* OSC */ }
}

let mut screen = Screen;
let mut parser = Parser::new();
parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
```

With the `simd` feature enabled, `Parser::with_simd` opts into SIMD
scanning and UTF-8 validation.

Also available: `Event`/`Recording` (owned enum API), `State` (byte-cursor
sink with line/cursor geometry), `Reply` (query reply builder),
`ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`, `MouseButton`, `Charset`,
and `Cap`.

## Serialization

Every owned type round-trips through the compact, `no_std` binary codec:

```rust
use vtty_codec::{decode_from_slice, encode_to_vec};
use vtty::{Event, Parser};

let mut parser = Parser::new();
let events = parser.parse_events(b"\x1b[1;31mhi");
let bytes = encode_to_vec(&events)?;
assert_eq!(decode_from_slice::<Vec<Event>>(&bytes)?, events);
```

## Features

- **Safety** — `#![forbid(unsafe_code)]`; malformed PTY input never
  crashes.
- **Zero-allocation** — one `print_bytes` per ASCII run, one
  `print_codepoints` per non-ASCII run; CSI/OSC/DCS state is inline.
- **`no_std`** — optional `simd` (scanning + validation) and
  `simd-transcode` (UTF-8→UTF-32) features, enabled per parser.
- **UTF-8** — maximal-subpart substitution; 8-bit C1 controls (DEC
  “anywhere”).
- **Wire protocols** (on by default, opt out with
  `default-features = false`) — `reply` (DECRQSS/DECRQM/XTGETTCAP),
  `termcap` (`Cap`/`CapValue`, implies `reply`), `status` (`OSC 7501`
  `ProgramStatus`).
- **`perf-inline`** (on by default) — `#[inline(always)]` on the parser
  hot path; opt out for smaller code at some throughput cost.

## Test

```bash
cargo test -p vtty
cargo test -p vtty --test vt_differential -- --nocapture     # parity vs vte
cargo test -p vtty --test vt_differential_rio -- --nocapture # parity vs rio-vt
```

[dec-parser]: https://vt100.net/emu/dec_ansi_parser

<!-- cargo-rdme end -->

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
