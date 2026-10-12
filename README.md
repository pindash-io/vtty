# vtty

Zero-allocation, panic-free DEC VT500 terminal parser with UTF-8 decoding and
`no_std` support.

## Workspace

| crate | purpose |
| --- | --- |
| [`vtty`](crates/vtty) | byte-input DEC VT500 parser — the core crate |
| [`vtty_emoji`](crates/vtty_emoji) | display cell width and emoji-sequence handling |
| [`vtty_ffi`](crates/vtty_ffi) | raw C ABI over `vtty` (cdylib + staticlib) |
| [`vtty_bench`](crates/vtty_bench) | throughput benchmarks vs `vte`, `vt100`, `vtparse`, `rio-vt` |

## Highlights

- **Panic-free** — `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`panic`;
  malformed PTY input is normal, never a crash.
- **Zero-allocation hot path** — printable ASCII runs dispatch via one
  `print_bytes`, non-ASCII via one `print_codepoints`; CSI/OSC/DCS state stays
  inline (`SmallVec`). `Perform` callbacks, no event materialization.
- **`no_std` by default** — `alloc` only; the optional `simd` feature adds
  SIMD printable-run scanning + SIMD UTF-8 validation (per-parser opt-in with
  `Parser::with_simd()`).
- **Byte-accurate UTF-8** — code points split across `advance` calls are
  carried over as a partial, never misdecoded to `U+FFFD`.
- **W3C-correct malformed UTF-8** — each invalid maximal subpart becomes one
  `U+FFFD`, and a standalone C1 byte keeps `execute` semantics — the same
  contract as `vte`, `rio-vt`, and ghostty.
- **8-bit clean (DEC “anywhere”)** — raw C1 controls abort any sequence in
  progress and execute, instead of being swallowed.
- **Differential + fuzz tested** — event-stream parity against `vte` and
  `rio-vt`, plus libFuzzer targets (`differential`, `differential-simd`,
  `differential-rio`, `robustness`, `random`).

## Usage

### Rust parser

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
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) { /* OSC */ }
}

let mut parser = Parser::new();
parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
```

An owned enum API is also available: `parser.parse_events(bytes)` returns
`Vec<Event>`. The crate also re-exports `State` (byte-cursor sink with
line/cursor geometry), `Reply` (query reply builder), `ColorId`, `SgrFlags`,
`TermFlags`, `KittyFlags`, `MouseButton`, `Charset`, and `Cap`.

### Emoji cell widths

```rust
use vtty_emoji::{cell_advance, cell_width, cell_widths};

cell_width('中');              // Some(2)
cell_widths("⚠\u{FE0F}");      // [2, 0] — VS16 widens the base
cell_advance(1.6, 10.0);       // 20.0 — snap a fractional advance to 2 cells
```

### C ABI

```c
#include "vtty_ffi.h"

vt_state *s = vt_new();
vt_advance(s, (const uint8_t *)"hello\nworld", 11);

size_t rows = vt_line_count(s); // 2
size_t len  = vt_text_len(s);   // 11

vt_free(s);
```

Build the library with `cargo build -p vtty_ffi` (cdylib + staticlib), then
link a C program against it:

```sh
# macOS / Linux (add -lutil on Linux for forkpty)
cc -O2 crates/vtty_ffi/examples/term_demo.c \
   -I crates/vtty_ffi -L target/release -lvtty_ffi -o term_demo
```

`crates/vtty_ffi/examples/term_demo.c` is a minimal interactive terminal
(Ctrl-D exits); the header is at `crates/vtty_ffi/vtty_ffi.h`.

## Feature flags

- `vtty` — `simd`: SIMD printable-run scanner + SIMD UTF-8 validation,
  selected per-parser with `Parser::with_simd()` (scalar `Parser::new()` is
  the default).
- `vtty` — `simd-transcode`: implies `simd`; adds the UTF-8→UTF-32 transcode
  (64-byte-block, single-mask, rio-vt/ghostty) and its ~70 KB of lookup
  tables.
- `vtty_emoji` — `simd` + `baked` (default), or `icu` for runtime ICU4X
  property lookups.

## Benchmarks

```bash
cargo bench -p vtty_bench
```

Runs the same byte corpora (plain ASCII, short-line scroll, multibyte UTF-8,
ANSI/SGR-dense, per-character SGR churn, truecolor, bracketed paste, OSC,
DCS/Sixel) through scalar `vtty`, SIMD `vtty-simd`, `vte`, `vt100`, `vtparse`,
and `rio-vt` with counting sinks, so the numbers reflect parser + dispatch
cost.

Results (median throughput, 2 MiB input; higher is better):

| case | vtty | vtty-simd | vte | vtparse | rio-vt | vt100* |
| --- | --- | --- | --- | --- | --- | --- |
| plain | 2.54 GiB/s | **12.72 GiB/s** | 1.37 GiB/s | 403 MiB/s | 1.46 GiB/s | 133 MiB/s |
| scroll_storm | 1.22 GiB/s | **1.48 GiB/s** | 1.34 GiB/s | 382 MiB/s | 1.32 GiB/s | 75 MiB/s |
| utf8_mixed | 818 MiB/s | 1.21 GiB/s | 823 MiB/s | 225 MiB/s | 1.21 GiB/s | 171 MiB/s |
| ansi_mixed | **682 MiB/s** | 680 MiB/s | 314 MiB/s | 188 MiB/s | 386 MiB/s | 176 MiB/s |
| ansi_two_styles | 847 MiB/s | **856 MiB/s** | 332 MiB/s | 194 MiB/s | 354 MiB/s | 160 MiB/s |
| ansi_bare_csi | 1.14 GiB/s | **1.24 GiB/s** | 492 MiB/s | 210 MiB/s | 459 MiB/s | 155 MiB/s |
| sgr_churn | **706 MiB/s** | 678 MiB/s | 337 MiB/s | 191 MiB/s | 442 MiB/s | 229 MiB/s |
| truecolor_unique | **680 MiB/s** | 634 MiB/s | 340 MiB/s | 207 MiB/s | 478 MiB/s | 262 MiB/s |
| bracketed_paste | **1.12 GiB/s** | 1.04 GiB/s | 733 MiB/s | 283 MiB/s | 739 MiB/s | 150 MiB/s |
| osc_heavy | 1.67 GiB/s | **1.71 GiB/s** | 366 MiB/s | 133 MiB/s | 1.19 GiB/s | 153 MiB/s |
| dcs_sixel | 4.38 GiB/s | **4.52 GiB/s** | 581 MiB/s | 437 MiB/s | 4.15 GiB/s | 461 MiB/s |

*vt100 is a full-screen emulator (no standalone parser), so it is not an
apples-to-apples parser comparison.

`utf8_mixed` (ASCII + CJK + emoji + combining marks) is a statistical tie
between `vtty-simd` and `rio-vt` (both ≈ 1.21 GiB/s); the aligned 64-byte-block
UTF-8→UTF-32 transcode closed what was previously an ~8% gap.

## Testing

```bash
cargo test --workspace
cargo test -p vtty --test vt_differential -- --nocapture      # parity vs vte
cargo test -p vtty --test vt_differential_rio -- --nocapture  # parity vs rio-vt
```

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT)
at your option.
