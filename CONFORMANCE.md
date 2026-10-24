# Conformance

How `vtty` lines up against DEC's VT500-series behaviour, as specified by
Paul Flo Williams' canonical references on <https://vt100.net/>:

- **[A parser for DEC's ANSI-compatible video terminals](https://vt100.net/emu/dec_ansi_parser)** — the
  state machine `vtty` encodes (`crates/vtty/src/table.rs`, `parser.rs`).
- **[DECronomicon: private control sequences](https://vt100.net/emu/ctrlseq_dec.html)** — the
  DEC private CSI catalog (by intermediate byte).
- **[DECronomicon: private modes](https://vt100.net/emu/dec_private_modes.html)** — DEC
  private modes 1–117.
- **[DECronomicon: device control strings](https://vt100.net/emu/dcsseq_dec.html)** — the
  DEC DCS catalog.
- **EK-VT510-RM** / **EK-VT520-RM** — the VT500-series programmer manuals.

> **Scope note.** `vtty` is a *parser*: it decodes bytes into typed events
> (`csi_dispatch`, `hook`/`put`/`unhook`, `osc_dispatch`, …) and hands them to
> a `Perform` sink. It does **not** interpret cursor movement, editing,
> scrolling, or rendering — those are the screen's job (no screen crate ships
> yet). So "conformance" here means: the parser recognizes the byte grammar
> exactly as DEC did, and the *types* (`TermFlags`, `Reply`, `Cap`) cover what
> a screen needs to answer queries faithfully.

## Parser state machine

The DEC VT500 transition diagram is transcribed twice — once in the production
table (`transition_impl`), once as an independent reference (`dec_step` in
`table.rs` tests) — and compared exhaustively.

| Area | Status | Notes |
| --- | --- | --- |
| Non-ground states × `0x00..=0x7f` | ✅ tested | 13 states × 128 bytes = 1,664 assertions (`table_matches_dec_ansi_parser_diagram`) |
| C0 controls (execute/ignore, CAN/SUB cancel) | ✅ tested | CAN/SUB cancel + return to ground in every state |
| NUL executes (DECNULM, VT500) | ✅ | not treated as a time-fill |
| C1 "anywhere" rule | ✅ tested | introducers re-enter, ST is a no-op, others execute → ground (`c1_transition_matches_dec_anywhere`) |
| C1 interrupts a string | ✅ tested | `osc_end`/`unhook` fires for *any* C1 control, then the control is processed (`c1_interrupt_fires_string_exit_action`) |
| UTF-8 maximal-subpart substitution | ✅ | W3C rule, one U+FFFD per invalid maximal subpart |
| GR (`0xa0..=0xff`) ≡ GL | ⚠️ N/A | `vtty` is UTF-8: `0xa0..=0xff` are UTF-8 lead bytes. DEC's GR≡GL rule applies only to 8-bit charset mode (ISO 8859 / DEC MCS), which a UTF-8 terminal does not use. |

### Deliberate deviations

Modern protocol extensions that intentionally differ from the DEC diagram:

| Case | DEC | vtty | Reason |
| --- | --- | --- | --- |
| `:` in CSI/DCS params | `* ignore` (drop sequence) | sub-parameter separator | kitty `4:3m`, `38:2::r:g:b` |
| SOS/PM/APC content | ignore | `hook` + reuse DCS passthrough | kitty APC `_G` graphics |
| params stored | 16 | 32 (matches vte/rio-vt) | `sep: u16` still tracks only the first 16 colon flags |
| param clamp | `>9999 → 9999` (DECSR: 0–16383) | `0..=65535` (16-bit) | matches vte |

Both deviations are pinned by `vtty_deviation()` in the table test, so a new
divergence cannot slip in unnoticed.

## DEC private modes (DECRQM fidelity)

`mode.rs` tracks the modes a screen must report via DECRQM (`CSI ?Ps$p`). The
full DEC table has 117 entries; vtty tracks the subset that matters, plus the
xterm/kitty modes modern apps probe.

**DEC-native modes tracked** (`TermFlags`):

| Mode | Mnemonic | vtty flag |
| --- | --- | --- |
| 1 | DECCKM | `CURSOR_KEYS` |
| 3 | DECCOLM | `DECCOLM` (tracked, not applied — host owns width) |
| 5 | DECSCNM | `REVERSE` |
| 6 | DECOM | `ORIGIN` |
| 7 | DECAWM | `WRAPAROUND` |
| 25 | DECTCEM | `CURSOR_VISIBLE` |
| 66 | DECNKM | `DECNKM` |
| 67 | DECBKM | `DECBKM` |
| 69 | DECLRMM | `DECLRMM` |
| 80 | DECSDM | `DECSDM` (tracked; Sixel placement is the renderer's job) |

**xterm/kitty modes tracked** (modern protocol, not DEC): 9, 12, 45, 47,
1000–1016, 1035, 1045–1049, 2004, 2026, 2027, 2031, 2048 — see
`TermFlags::from_private_param`.

**Notable DEC modes *not* tracked** (would need a screen to act on them):
DECSCLM (4), DECARM (8), DECRLM (34), DECCRNLM (40), DECNRCM (42), DECAAM
(100), DECCANSM (101), DECNULM (102), DECECM (117), and the VT520 session/
window modes. These are parse-dispatched (the screen sees the `SM`/`RM`
call), just not reflected in a flag — the full catalog is at
<https://vt100.net/emu/dec_private_modes.html>.

## Query / reply protocols

`Reply` (`crates/vtty_core/src/reply.rs`) formats the answers a screen emits.

| Query | Request | Answer | Status |
| --- | --- | --- | --- |
| DECRQSS | `DCS $ q … ST` | `DCS {valid} $ r … ST` | ✅ `Reply::decrqss` |
| DECRQM | `CSI ?Ps$p` | `CSI ?Ps;{state}$y` | ✅ `Reply::decrqm` |
| XTGETTCAP | `DCS + q <hex> ST` | `DCS {ok} + r … ST` | ✅ `Reply::xtgettcap_*` + `Cap` table |
| SGR (DECRQSS `m`) | — | `0;…m` | ✅ `Reply::sgr` |
| OSC 10/11/4 color | `OSC … ?` | `OSC … ST` | ✅ `Reply::osc_color` / `osc_palette` / `osc_background` |
| DECTSR (state report) | `CSI 1 $ u` (DECRQTSR) | `DCS 1 $ s D…D ST` | ✅ `Reply::dectsr` |
| DECCTR (color table) | `CSI 2 $ u` (DECRQTSR) | `DCS 2 $ s D…D ST` | ✅ `Reply::decctr` |
| DECCIR (cursor info) | `CSI 1 $ w` (DECRQPSR) | `DCS 1 $ u … ST` | ✅ `Reply::deccir` |
| DECTABSR (tab stops) | `CSI 2 $ w` (DECRQPSR) | `DCS 2 $ u … ST` | ✅ `Reply::dectabsr` |
| DA1 (primary) | `CSI c` / `CSI 0 c` | `CSI ? {class};{ext…} c` | ✅ `Reply::da1` |
| DA2 (secondary) | `CSI > c` / `CSI > 0 c` | `CSI > {id};{fw};{kb} c` | ✅ `Reply::da2` |
| DA3 (tertiary) | `CSI = c` / `CSI = 0 c` | `DCS ! | {unit-id} ST` | ✅ `Reply::da3` |

> DECTSR uses a **lowercase `s`** final (`DCS 1 $ s`), per the EK-VT510-RM
> errata on vt100.net. DECTSR/DECCTR payloads are the device-attribute / color
> table data; DECCIR/DECTABSR payloads are the cursor / tab-stop state — the
> screen supplies them to the `Reply` builders.

## Device control strings (DCS)

The parser recognizes every DCS header and dispatches `hook` → `put`/`put_slice`
→ `unhook`. Reference handlers are the consumer's job; none ship in the
library yet:

| DCS | Meaning | Dispatch | Handler |
| --- | --- | --- | --- |
| `DCS q … ST` | Sixel (`decSIXEL`) | ✅ | ❌ (raster decode is the renderer's) |
| `DCS p … ST` | ReGIS (`decREGIS`) | ✅ | ❌ |
| `DCS { … ST` | DECDLD soft chars | ✅ | ❌ |
| `DCS | … ST` | DECUDK user keys | ✅ | ❌ |
| `DCS ! z … ST` | DECDMAC macros | ✅ | ❌ |
| `DCS $ q … ST` | DECRQSS | ✅ | ✅ `Reply` |
| `DCS $ s … ST` | DECTSR / DECCTR | ✅ | ✅ `Reply::dectsr` / `decctr` |
| `DCS $ u … ST` | DECCIR / DECTABSR | ✅ | ✅ `Reply::deccir` / `dectabsr` |
| `DCS $ t … ST` | DECRSPS (restore) | ✅ | ❌ restore is the screen's job |
| `DCS $ p … ST` | DECRSTS (restore) | ✅ | ❌ restore is the screen's job |
| `DCS + q … ST` | XTGETTCAP | ✅ | ✅ `Reply` |

## OSC

| Command | Meaning | Status |
| --- | --- | --- |
| `OSC 0;…` / `OSC 1;…` / `OSC 2;…` | DECSIN / DECSWT (icon + title) | ✅ dispatched generically |
| `OSC 8`, `OSC 52`, `OSC 133`, … | hyperlink / clipboard / shell integration | ✅ dispatched generically |
| `OSC 7501` | Program Status Protocol | ✅ `vtty_status` codec (`ProgramStatus`) |

## Charsets (ISO 2022)

`charset.rs` provides the designations real apps use; the rest are
intentionally out of scope for a UTF-8 terminal.

| Designation | Charset | Status |
| --- | --- | --- |
| `ESC ( B` | US-ASCII | ✅ |
| `ESC ( 0` | DEC Special Graphics (line drawing) | ✅ |
| `ESC ( A` | British (`#`→`£`) | ✅ |
| National replacement sets (DECNRCM) | DEC MCS / ISO Latin-1 / … | ❌ N/A (UTF-8 terminal) |

## XTGETTCAP terminfo

`termcap.rs` answers the capability subset real apps probe (nvim, tmux, fish):
`TN`, `Co`, `RGB`, `Tc`, `co`, `li`, `it`, `Smulx`, `Setulc`, `Ss`, `Se`,
`Cs`, `Cr`, `E3`, `sync`, `sixel`, `kitty`, `setaf`/`setab`, `smcup`/`rmcup`,
`smkx`/`rmkx`, `smacs`/`rmacs`, cursor + key strings, and the non-standard
`query-os-name`.

## Resource limits

Bounded, O(n) (see `ARCHITECTURE.md`): OSC payload 1 MiB, OSC params 16,
CSI/DCS params 32, intermediates 2, UTF-8 partial 4 bytes, SIMD window 4096.

## How this is enforced

- `table.rs` — exhaustive state×byte differential vs the DEC diagram (1,664 assertions).
- `parser.rs` — C1 "anywhere" + string-cancellation regression tests.
- `tests/vt_differential.rs` / `vt_differential_rio.rs` — corpus parity vs `vte` / `rio-vt`
  (raw C1 excluded: documented divergence).
- `fuzz/` — libFuzzer differential + robustness targets.

## Known gaps (roadmap)

1. `DECRSPS` (`DCS $ t`) / `DECRSTS` (`DCS $ p`) restore handlers — the parser
   dispatches them, but applying the state is the screen's job.
2. The remaining DEC private modes (4, 8, 34, 40, 42, 100–117) — parse-dispatched
   but not tracked as `TermFlags`; `DECSCL` is covered by `ConformanceLevel`.
3. Reference renderers for Sixel (`DCS q`), DECDLD (`DCS {`), DECUDK (`DCS |`).
4. `vtty_screen` — the cell-grid terminal screen (cursor, SGR, scrolling,
   DECSTBM, alt screen, scrollback with resize reflow, tab stops, combining
   marks, ZWJ emoji families).
