<div align="center">

# vtty

**Zero-allocation, panic-free DEC VT500 terminal parser with UTF-8 decoding and
`no_std` support**

[![crates.io](https://img.shields.io/crates/v/vtty.svg)](https://crates.io/crates/vtty)
[![docs.rs](https://docs.rs/vtty/badge.svg)](https://docs.rs/vtty)
[![dependency status](https://deps.rs/repo/github/pindash-io/vtty/status.svg)](https://deps.rs/repo/github/pindash-io/vtty)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/pindash-io/vtty/workflows/CI/badge.svg)](https://github.com/pindash-io/vtty/actions)

</div>

## Crates

| crate | purpose |
| --- | --- |
| [`vtty`](crates/vtty) | public facade (re-exports the engine) |
| [`vtty_core`](crates/vtty_core) | the parser engine |
| [`vtty_codec`](crates/vtty_codec) | `no_std` binary `Encode`/`Decode` codec + vtty type impls |
| [`vtty_emoji`](crates/vtty_emoji) | display cell width + emoji sequences |
| [`vtty_ffi`](crates/vtty_ffi) | raw C ABI |
| [`vtty_bench`](crates/vtty_bench) | throughput benchmarks |

## Features

- **Safety** — `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`panic`.
- **Zero-allocation** — bulk `print_bytes` / `print_codepoints`.
- **`no_std`** — optional `simd` / `simd-transcode` features.
- **UTF-8** — maximal-subpart substitution; 8-bit C1 controls.
- **Serialization** — compact, `no_std`, zero-allocation `Encode`/`Decode` codec
  (`vtty_codec`) for events, replies, and terminal state.

## Install

```bash
cargo add vtty
```

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
```

## Serialization

Owned events, replies and terminal state round-trip through a compact,
`no_std` binary codec:

```rust
use vtty_codec::{decode_from_slice, encode_to_vec};
use vtty::{Event, Parser};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Owned events encode/decode as data:
    let mut parser = Parser::new();
    let events = parser.parse_events(b"\x1b[1;31mhi");

    let bytes = encode_to_vec(&events)?;
    let restored = decode_from_slice::<Vec<Event>>(&bytes)?;

    assert_eq!(events, restored);
    Ok(())
}
```

## Benchmarks

Throughput vs `vte`/`vt100`/`vtparse`/`rio-vt` — see
[`crates/vtty_bench`](crates/vtty_bench) for the full table.

## Testing

```bash
cargo test --workspace
cargo test -p vtty_core --test vt_differential -- --nocapture      # parity vs vte
cargo test -p vtty_core --test vt_differential_rio -- --nocapture  # parity vs rio-vt
```

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT)
at your option.
