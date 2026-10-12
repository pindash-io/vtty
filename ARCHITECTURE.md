# Architecture

A high-level map of the `vtty` workspace: what each crate does, how the parser is
structured, and the invariants the code relies on.

## Workspace layout

```
crates/
  vtty/        # the core byte-input DEC VT500 parser (no_std default)
  vtty_emoji/  # display cell width + emoji-sequence handling (ICU4X/parley)
  vtty_ffi/    # raw C ABI over vtty (cdylib + staticlib)
  vtty_bench/  # Criterion throughput benchmarks vs vte/vt100/vtparse/rio-vt
```

`vtty` is the heart. `vtty_emoji` is a leaf utility. `vtty_ffi` is a thin C
wrapper. `vtty_bench` is tooling, not shipped.

## vtty crate

### Design invariants

- **Zero-allocation hot path** — `Parser::advance` dispatches to a `Perform`
  callback sink without materializing events. The owned `Event` enum exists
  only for tests/tooling (`Parser::parse_events`).
- **Panic-free** — `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
  Malformed PTY input is normal, never a panic.
- **`no_std` by default** — `alloc` only. The optional `simd` feature pulls in
  `std` (via `fearless_simd`/`simdutf8`) and is selected per-parser at runtime
  (`Parser::with_simd()`).
- **Byte-accurate UTF-8** — code points split across `advance` calls are carried
  over as a partial, never misdecoded to `U+FFFD`.

### Resource limits

The parser is O(n) in time and bounds every buffer it owns, so a hostile PTY
stream cannot force unbounded allocation or quadratic work:

| resource | limit | behavior when exceeded |
| --- | --- | --- |
| OSC payload | 1 MiB (`MAX_OSC_PAYLOAD`) | whole payload discarded (`osc_overflow`) |
| OSC params | 16 (`MAX_OSC_PARAMS`) | params 17+ dropped |
| CSI/DCS params | 32 (`MAX_PARAMS`) | params beyond 32 dropped |
| intermediates | 2 | extra bytes set `ignore` |
| UTF-8 partial | 4 bytes | carried in `partial` |
| hot-path allocation | zero | `Perform` callbacks, no event materialization |
| recursion | none | fully iterative |

The SIMD UTF-8 window is capped at 4096 bytes and at the next ESC to avoid
re-validating the same window per isolated multi-byte char (O(n × CHUNK)).

The owned `Parser::parse_events` (`Recording`) is the exception: it allocates one
`Event` per dispatch into an unbounded `Vec`, so it is for tests/tooling, not
untrusted input. Sinks (a sixel decoder, the `State` screen, the FFI text buffer)
own their own accumulation caps; the parser only bounds its own buffers.

### Modules

| file | role |
| --- | --- |
| `parser.rs` | the `Parser` state machine, inline per-state dispatch, boundary scans |
| `table.rs` | compile-time DEC VT500 transition table |
| `utf8.rs` | incremental UTF-8 decoding |
| `simd.rs` | SIMD byte scans + validated decode (`simd` feature) |
| `perform.rs` | the `Perform` callback trait |
| `recording.rs` | owned `Event` enum + `Recording` |
| `state.rs` | byte-cursor `State` sink |
| `mode.rs` | `TermFlags` (DEC/ANSI mode flags) |
| `style.rs` | `SgrFlags`, `StyleFlags`, `UnderlineStyle`, `CursorStyle` |
| `color.rs` | `ColorId` palette index |
| `charset.rs` | ISO 2022 charsets + DEC Special Graphics |
| `keyboard.rs` | kitty keyboard protocol flags |
| `mouse.rs` | mouse event encoding (X10/SGR/UTF-8/pixel) |
| `reply.rs` | query reply builder (DECRQSS/DECRQM/XTGETTCAP) |
| `termcap.rs` | XTGETTCAP capability table |
| `hex.rs` | hex encode/decode for query payloads |

### Parser data flow

```
bytes ──▶ Parser::advance ──▶ per-state dispatch ──▶ Perform sink
                                   │              (print / print_bytes / print_codepoints /
                                   │               execute / csi_dispatch / esc_dispatch /
                                   │               hook / put_slice / unhook / osc_dispatch)
                                   └──▶ Recording (owned API)
```

`advance` matches `self.state` and dispatches inline `#[inline(always)]`
handlers for the hot states — `advance_ground`, `advance_osc_run`,
`advance_dcs_run`, `advance_escape_byte`, `advance_dcs_entry_byte`,
`advance_param_run` — so a sequence's enter/payload/exit bytes never pay a cold
call. Only rare bytes (`ESC` restart, `DEL`, raw C1 inside non-ground states)
fall through to `advance_non_ground` (the table + `apply` path).

#### Ground state (`advance_ground`, the hot path)

1. `SIMD`-scan for the next non-printable byte (`first_non_printable`), dispatch
   the printable ASCII run in bulk via `print_bytes`.
2. Bulk CSI fast path: on `ESC [` scan for the final byte, parse params in one
   pass, dispatch `csi_dispatch` — skipping the per-byte state machine. This
   dominates `SGR`-heavy streams (cacademo-style).
3. Handle `ESC` (enter escape state), `DEL`/other `C0` controls (`execute`), raw
   8-bit `C1` controls (`c1_transition`), and multi-byte UTF-8 (`decode_utf8`).

`trailing_partial` is computed once per `advance` (on the whole chunk) and the
partial-free prefix length is passed down, rather than re-scanning the buffer
tail on every ground re-entry. If an escape sequence consumes past that prefix
(its payload is raw bytes, not UTF-8), the ground loop clears the stale partial.

#### Escape-sequence states

Hot sequence bytes are handled inline, not by the cold table:

- `advance_escape_byte` — a complete ESC dispatcher: the `[`/`]`/`P`/`X`/`^`/`_`
  introducers, intermediates (`push_intermediate`), ESC finals (`esc_dispatch`,
  including ST `\`), and C0 controls.
- `advance_osc_run` / `advance_dcs_run` — bulk payload runs (SWAR boundary scan +
  `put_slice` / deferred append) plus the BEL/ESC/ST terminators inline.
- `advance_dcs_entry_byte` — DCS final-byte `hook` inline.
- `advance_param_run` — CSI/DCS digit/`;`/`:` runs in a tight loop.

`advance_non_ground` (`#[inline(never)]`) is now only the cold fallback for rare
bytes: `ESC` restart, `DEL`, raw C1, and the non-hot states (`CsiEntry`,
intermediates, `SosPmApcString` content, …). It still does the transition-table
lookup + `apply` (bitflag checks) and the CSI/DCS final fast paths.

#### OSC strings

OSC params are tracked as merged byte ranges into a payload buffer — `;`
separators are not buffered; `osc_starts` records each param's start offset (up
to 16; later params are dropped). When an OSC starts **and ends within one
`advance` call**, the dispatch slices are taken straight from the input buffer
(`osc_deferring`); only strings that span calls are flushed into `osc` (copying
the `;`-free segments). `first_osc_boundary` finds the next terminator/`;` with a
single SWAR zero-byte test — C0/C1 both satisfy `(b & 0x60) == 0`.

### Transition table (`table.rs`)

The DEC VT500 grammar (Paul Williams' state diagram) is expanded at compile
time into a flat `[state][byte]` lookup:

- 14 states × 256 bytes = 3584 entries.
- Each entry is a packed `u32`: low 16 bits = `Action` bitflags, bits 16..=23 =
  the next `ParseState` (bits 24..=31 are unused).
- `transition(state, b)` is a single indexed load — no per-byte `match`.

### Parameter representation

- Params are clamped to `0..=65535` (DEC VT500 16-bit).
- An **empty colon param** is the `-1` sentinel, so `SGR` colon forms
  (`38:2::r:g:b`) stay distinguishable from an explicit `0`.
- A param-less CSI dispatches `[]` (not `[0]`).
- `sep: u16` tracks `:` vs `;` separators (bit `i` set when param `i` is
  followed by `:`), for the first 16 params.
- `params: [i32; 32]` with a separate `params_len` gives O(1) reset.

### UTF-8 (`utf8.rs`)

- `trailing_partial` detects a code point split at the end of a chunk and
  returns `(valid_prefix_len, partial_codepoint, remaining_continuation_bytes)`;
  computed once per `advance` and the prefix length passed to the ground loop.
- The partial is carried in `Parser` and completed on the next `advance`.
- `decode_utf8` dispatches multi-byte code points; with `simd` it validates a
  window via `simdutf8` and batch-dispatches the whole window (ASCII runs via
  `print_bytes`, non-ASCII via `print_codepoints`, C1 via `execute`).
- Malformed input follows W3C “Substitution of Maximal Subparts”: each invalid
  maximal subpart becomes one `U+FFFD`, and a standalone C1 byte (`0x80..=0x9f`)
  keeps `execute` semantics — the same contract as `vte`, `rio-vt`, and ghostty
  (whose `UTF8Decoder.zig` DFA and `MaximalSubpart` in `src/simd/vt.cpp` are the
  equivalent reference implementations).

### SIMD (`simd.rs`, `simd` feature)

- `fearless_simd` runtime-dispatches to `SSE2`/`SSE4.2`/`AVX2`/`AVX-512`/`NEON`.
- `first_non_printable` / `first_esc` scan 16 bytes at a time, with a scalar
  probe of the first byte to skip dispatch on escape-dense streams.
- `decode_utf8_n::<N>` (const-generic, in `utf8.rs`) decodes a validated
  `N`-byte code point; the `match N` folds to the exact per-length shifts at
  compile time.

### Public API surface

- **Callback**: `Perform` (sink trait) + `Parser::advance`.
- **Owned events**: `Event` + `Recording` (`Parser::parse_events`).
- **Byte-cursor**: `State`.
- **Replies**: `Reply` + `Cap`/`CapValue` (XTGETTCAP).
- **Types**: `ColorId`, `SgrFlags`, `StyleFlags`, `UnderlineStyle`,
  `CursorStyle`, `TermFlags`, `KittyFlags`, `MouseButton`, `MouseTracking`,
  `Charset`, `Charsets`.

## vtty_emoji

- `cell_width` / `cell_widths` / `cell_advance` — display cell widths and
  emoji-sequence handling.
- `baked` (default) + `simd`: a compile-time table generated by `build.rs` from
  ICU4X properties, packed per codepoint.
- `icu`: runtime ICU4X lookups (alternative to `baked`).
- `parley_emoji` provides the emoji segmentation DFA.

## vtty_ffi

- Opaque `vt_state` (parser + byte-cursor `State`) with a C ABI in
  `vtty_ffi.h`.
- `vt_new` / `vt_free` / `vt_reset` / `vt_advance` and accessors
  (`vt_text`, `vt_cursor`, `vt_line_count`, `vt_line_range`, …).
- All offsets are byte offsets into the UTF-8 text buffer.

## Testing

- Unit tests per module.
- `tests/vt_differential.rs` — runs a corpus through both vtty and `vte`,
  normalizes, and asserts identical event streams.
- `tests/vt_differential_rio.rs` — the same corpus against `rio-vt` (which
  allows four intermediates where vtty/vte cap at two, so those are skipped).
- `fuzz/` — libFuzzer targets: `differential` (vtty vs `vte`),
  `differential-simd` (vtty-simd vs `vte`), `differential-rio` (vtty vs
  `rio-vt`), `robustness` (no-panic across vtty/vte/vt100/vtparse/rio-vt),
  and `random` (structured random VT bytes). The rio-vt target skips malformed
  ground-state UTF-8: rio-vt bundles C++ `simdutf` 7.7.1, whose SIMD error-offset
  recovery reports the error position wrongly on x86_64 (fixed in the 9.x
  series; ghostty vendors 9.0.0) — see `fuzz/README.md`.
- `crates/vtty_bench` — throughput comparisons (counting sinks), including the
  `vtty` scalar vs `vtty-simd` split.

## Tooling

- `rustfmt.toml` — Rust formatting.
- `.taplo.toml` — TOML formatting/linting.
- `.typos.toml` — spell-check (terminfo/protocol vocabulary allow-listed).
