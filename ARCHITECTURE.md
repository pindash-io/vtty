# Architecture

A map of the `vtty` workspace: what each crate does and the invariants the code
relies on.

## Workspace

- `vtty` — the parser facade: re-exports `vtty_core` plus feature-gated wire
  protocols.
- `vtty_core` — the irreducible core: the `Perform`/`Event`/`Recording`
  contract, terminal model types, and the byte-cursor `State` sink. Shared
  with `vtty_codec`.
- `vtty_codec` — `no_std` binary `Encode`/`Decode` codec (generic + `vtty_core`
  impls).
- `vtty_width` — display cell width + emoji sequences (ICU4X/parley).
- `vtty_grapheme` — UAX #29 grapheme cluster segmentation.
- `vtty_status` — `OSC 7501` Program Status Protocol.
- `vtty_screen` — a minimal cell-grid terminal screen (`Screen` `Perform` sink).
- `vtty_ffi` — raw C ABI over `vtty`.
- `vtty_bench` — throughput benchmarks vs `vte`/`vt100`/`vtparse`/`rio-vt`.

## Design invariants

- **Zero-allocation hot path** — `Parser::advance` dispatches to a `Perform`
  sink; the owned `Event` API (`parse_events`) is tests/tooling only.
- **Panic-free** — `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`panic`.
- **`no_std` by default** — optional `simd`/`simd-transcode` pull in `std`,
  selected per parser (`Parser::with_simd()`).
- **Byte-accurate UTF-8** — partial code points are carried across calls.

## Resource limits

The parser is O(n) and bounds every buffer it owns:

| resource | limit | on overflow |
| --- | --- | --- |
| OSC payload | 1 MiB | discarded |
| OSC params | 16 | dropped |
| CSI/DCS params | 32 | dropped |
| intermediates | 2 | set `ignore` |
| UTF-8 partial | 4 bytes | carried in `Parser` |
| SIMD UTF-8 window | 4096 bytes, capped at `ESC` | re-entry re-validates |

`parse_events` (`Recording`) is the exception — unbounded `Vec` — so it is for
tests/tooling, not untrusted input.

## Modules

`vtty` (engine):

| file | role |
| --- | --- |
| `parser.rs` | `Parser` state machine, inline per-state dispatch, boundary scans |
| `table.rs` | compile-time DEC VT500 transition table |
| `scan.rs` | SWAR boundary scanners |
| `utf8.rs` | incremental UTF-8 decoding |
| `simd.rs` | SIMD byte scans + UTF-8 transcode (`simd`/`simd-transcode`) |
| `utf8_tables.rs` | compile-time SIMD transcode tables (`simd-transcode`) |

`vtty_core` (contract + model + wire):

| file | role |
| --- | --- |
| `perform.rs` | `Perform` callback contract |
| `event.rs` | owned `Event` + `Recording` |
| `state.rs` | byte-cursor `State` sink |
| `mode.rs` / `style.rs` / `color.rs` | `TermFlags`, `SgrFlags`, `ColorId`, … |
| `charset.rs` | ISO 2022 charsets + DEC Special Graphics |
| `keyboard.rs` / `mouse.rs` | kitty flags, mouse encoding |
| `reply.rs` / `termcap.rs` | query replies (DECRQSS/DECRQM/XTGETTCAP) — `reply`/`termcap` features |
| `status.rs` | `OSC 7501` Program Status Protocol — `status` feature |

## Parser data flow

`advance` matches `self.state` and dispatches inline `#[inline(always)]`
handlers for the hot states (`advance_ground`, `advance_osc_run`,
`advance_dcs_run`, `advance_escape_byte`, `advance_dcs_entry_byte`,
`advance_param_run`); only rare bytes fall through to the cold
`advance_non_ground` (transition-table + `apply`).

- **Ground** — SIMD-scan the printable ASCII run (`print_bytes`), then a bulk
  CSI fast path (`ESC [` scan-final → parse → `csi_dispatch`), then `ESC`/`C0`/
  raw `C1`/UTF-8.
- **Escape sequences** — OSC/DCS payload runs use a SWAR boundary scan +
  `put_slice`; CSI/DCS digit runs loop inline.
- **OSC** — params are byte ranges into one payload buffer (`;` not buffered);
  a string that starts and ends within one `advance` dispatches slices straight
  from the input, only spanning strings are copied into `osc`.

### Transition table

The DEC VT500 grammar is expanded at compile time into `[state][byte]` (14 ×
256), each entry an `(Action, ParseState)` tuple (4 bytes). `transition` is
one indexed load, no unpacking.

### Parameters

- Clamped to `0..=65535`; empty colon param is the `-1` sentinel; param-less
  CSI dispatches `[]` (not `[0]`); `sep: u16` records `:` vs `;` separators;
  `params: [i32; 32]` + `params_len` gives O(1) reset.

## UTF-8 & SIMD

- `trailing_partial` detects a split code point; the partial is carried in
  `Parser`. `decode_utf8` dispatches multi-byte runs; with `simd` it validates
  a window via `simdutf8` and batch-dispatches (ASCII via `print_bytes`,
  non-ASCII via `print_codepoints`, C1 via `execute`).
- Malformed input follows W3C "Substitution of Maximal Subparts" — one `U+FFFD`
  per invalid maximal subpart; standalone C1 keeps `execute` semantics (same as
  `vte`, `rio-vt`, ghostty).
- `simd` — `fearless_simd` runtime dispatch (SSE2/SSE4.2/AVX2/AVX-512/NEON);
  `first_non_printable`/`first_esc` scan 16 bytes at a time.
- `simd-transcode` — `decode_block` transcodes a validated window in 64-byte
  blocks (one end-of-code-point mask per block, 12-byte chunk loop) via
  `decode_utf8_12` (the `0x924` 3-byte fast path + `utf8bigindex`/`shufutf8`
  tables, computed by `const fn` in `utf8_tables.rs`).

## Public API

- **Callback**: `Perform` + `Parser::advance`.
- **Owned events**: `Event` + `Recording` (`parse_events`).
- **Byte-cursor**: `State`.
- **Replies**: `Reply` + `Cap`/`CapValue` (`reply`/`termcap` features).
- **Status**: `ProgramStatus` (`status` feature).
- **Types**: `ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`, `MouseButton`,
  `Charset`, etc.

## Testing & tooling

- Unit tests per module; `tests/vt_differential.rs` (vs `vte`) and
  `tests/vt_differential_rio.rs` (vs `rio-vt`).
- The exhaustive DEC VT500 state-machine differential (`table.rs` and the C1
  "anywhere" tests in `parser.rs`) — see `CONFORMANCE.md` for the
  DEC-vs-vtty coverage matrix.
- `fuzz/` — libFuzzer: `differential`, `differential-simd`, `differential-rio`,
  `robustness`, `random` (see `fuzz/README.md`).
- `vtty_bench` — throughput (counting sinks), `vtty` vs `vtty-simd`.
- `rustfmt.toml`, `.taplo.toml`, `.typos.toml` — formatting and spell-check.
