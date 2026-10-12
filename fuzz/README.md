# Fuzzing

Differential fuzzing against reference parsers, plus a robustness target.

## Targets

| target | comparison | status |
| --- | --- | --- |
| `differential` | vtty (scalar) vs `vte` | stable |
| `differential-simd` | vtty (SIMD) vs `vte` | stable |
| `differential-rio` | vtty vs `rio-vt` | stable |
| `robustness` | no-panic for vtty/vte/vt100/vtparse/rio-vt | stable |
| `random` | structured random VT bytes, vtty vs vte | stable |

## Setup

```bash
cargo install cargo-fuzz
rustup toolchain install nightly
```

## Run

```bash
cargo fuzz run differential
cargo fuzz run differential-simd
cargo fuzz run differential-rio
cargo fuzz run robustness
```

## Seed corpus

`fuzz/seeds/` holds hand-picked VT streams (truecolor repaint, OSC clipboard,
DCS/Sixel, bracketed paste, UTF-8 runs, CSI param edge cases) to give the
byte-mutation targets a head start. Seed a target's corpus before running:

```bash
mkdir -p fuzz/corpus/differential
cp fuzz/seeds/* fuzz/corpus/differential/
cargo fuzz run differential
```

## Known divergences (filtered)

Shared by the `vte`/`rio-vt` targets (`fuzz/src/lib.rs`):

- `0x7f` DEL — vtty dispatches it as a C0 control, vte prints it.
- Raw 8-bit C1 (`0x80..=0x9f`) — vtty follows DEC's "anywhere" rule; vte
  drops them inside escape sequences (ground-state introducers diverge too).
- `ESC X` / `ESC ^` / `ESC _` (SOS/PM/APC) — vtty dispatches hook/put/unhook
  (kitty graphics), vte drops them.
- rio-vt allows four intermediates; vtty/vte cap at two (filtered only in the
  `differential-rio` target).

UTF-8 decode is **not** filtered: vtty, vte and rio-vt all implement W3C
"Substitution of Maximal Subparts" (each invalid maximal subpart becomes one
U+FFFD; a standalone C1 byte keeps `execute` semantics), so malformed UTF-8 —
e.g. a truncated lead followed by printable ASCII and a C1 byte — decodes
identically across all three and needs no special-casing.

`vtparse` is not used as a differential baseline: its state machine diverges
from DEC/vte (`CsiParam::P` fuses separators/private markers/intermediates,
invalid UTF-8 is dropped instead of U+FFFD-replaced, a leading `:` enters
CsiIgnore, empty OSC/CSI params differ). It is still run in `robustness` as a
no-panic check only.
