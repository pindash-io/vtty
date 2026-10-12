<div align="center">

# vtty

**Zero-allocation, panic-free DEC VT500 terminal parser with UTF-8 decoding and
`no_std` support**

[![dependency status](https://deps.rs/repo/github/pindash-io/vtty/status.svg)](https://deps.rs/repo/github/pindash-io/vtty)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/pindash-io/vtty/workflows/CI/badge.svg)](https://github.com/pindash-io/vtty/actions)

</div>

## Crates

| crate | purpose |
| --- | --- |
| [`vtty`](crates/vtty) | the parser — core crate |
| [`vtty_emoji`](crates/vtty_emoji) | display cell width + emoji sequences |
| [`vtty_ffi`](crates/vtty_ffi) | raw C ABI |
| [`vtty_bench`](crates/vtty_bench) | throughput benchmarks |

## Highlights

- **Panic-free** — `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`panic`.
- **Zero-allocation hot path** — bulk `print_bytes` / `print_codepoints`.
- **`no_std` by default** — optional `simd` / `simd-transcode` features.
- **W3C-correct UTF-8** — maximal-subpart substitution; 8-bit clean.

## Usage

```rust
use vtty::{Parser, Perform};

struct Screen;

impl Perform for Screen {
    fn print_bytes(&mut self, s: &[u8]) {}
    fn print(&mut self, c: char) {}
    fn execute(&mut self, b: u8) {}
    fn csi_dispatch(&mut self, params: &[i32], sep: u16, i: &[u8], f: u8) {}
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {}
}

let mut parser = Parser::new();
parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
```

Owned events: `parser.parse_events(bytes)` → `Vec<Event>`. Also re-exports
`State`, `Reply`, `ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`,
`MouseButton`, `Charset`, `Cap`.

## Benchmarks

Throughput vs `vte`/`vt100`/`vtparse`/`rio-vt` — see
[`crates/vtty_bench`](crates/vtty_bench) for the full table.

## Testing

```bash
cargo test --workspace
cargo test -p vtty --test vt_differential -- --nocapture      # parity vs vte
cargo test -p vtty --test vt_differential_rio -- --nocapture  # parity vs rio-vt
```

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT)
at your option.
