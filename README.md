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

`cargo bench -p vtty_bench` — 2 MiB corpora, counting sinks, median throughput.

| case | vtty | vtty-simd | vte | vtparse | rio-vt | vt100* |
| --- | --- | --- | --- | --- | --- | --- |
| plain | 2.54 GiB/s | **12.72 GiB/s** | 1.37 GiB/s | 403 MiB/s | 1.46 GiB/s | 133 MiB/s |
| scroll_storm | 1.55 GiB/s | **1.65 GiB/s** | 1.34 GiB/s | 375 MiB/s | 1.31 GiB/s | 74 MiB/s |
| utf8_mixed | 818 MiB/s | 1.21 GiB/s | 823 MiB/s | 225 MiB/s | 1.21 GiB/s | 171 MiB/s |
| ansi_mixed | **682 MiB/s** | 680 MiB/s | 314 MiB/s | 188 MiB/s | 386 MiB/s | 176 MiB/s |
| ansi_two_styles | 847 MiB/s | **856 MiB/s** | 332 MiB/s | 194 MiB/s | 354 MiB/s | 160 MiB/s |
| ansi_bare_csi | 1.14 GiB/s | **1.24 GiB/s** | 492 MiB/s | 210 MiB/s | 459 MiB/s | 155 MiB/s |
| sgr_churn | **675 MiB/s** | 648 MiB/s | 335 MiB/s | 181 MiB/s | 439 MiB/s | 229 MiB/s |
| truecolor_unique | **680 MiB/s** | 634 MiB/s | 340 MiB/s | 207 MiB/s | 478 MiB/s | 262 MiB/s |
| bracketed_paste | **1.12 GiB/s** | 1.04 GiB/s | 733 MiB/s | 283 MiB/s | 739 MiB/s | 150 MiB/s |
| osc_heavy | 1.67 GiB/s | **1.71 GiB/s** | 366 MiB/s | 133 MiB/s | 1.19 GiB/s | 153 MiB/s |
| dcs_sixel | 4.38 GiB/s | **4.52 GiB/s** | 581 MiB/s | 437 MiB/s | 4.15 GiB/s | 461 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

## Testing

```bash
cargo test --workspace
cargo test -p vtty --test vt_differential -- --nocapture      # parity vs vte
cargo test -p vtty --test vt_differential_rio -- --nocapture  # parity vs rio-vt
```

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT)
at your option.
