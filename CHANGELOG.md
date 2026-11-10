# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added `vtty_screen`, a cell-grid terminal screen crate (`Screen` `Perform`
  sink with scrolling, SGR, reflow, and rendering).
- Added ISO 2022 `Scs` (Select Character Set) state to `vtty_core`, unifying
  charset designation and shifting.
- Added a CJK width mode to `vtty_width` (`set_cjk_width`) that renders
  East Asian Width "Ambiguous" chars as two cells.
- Added `Reply::xtversion` for the `CSI > q` version query.
- Added `Event::write_bytes` for serializing events back to their byte form.
- Added the SGR effect parser to `vtty_core::style` (`SgrEffect`,
  `parse_effects`, `single_code`, `single_effect`).
- Added `Scs::apply_shift` for the SO/SI/SS2/SS3 control-byte shifting.
- Added `Recording` replay and helper methods (`print_char`, `control`, `csi`,
  `esc`, `osc`, …).
- Added `OSC 9;4` progress-bar parsing to `vtty_screen` (`Progress` and
  `Screen::progress`).
- Added IRM insert mode (`CSI 4 h/l`) to `vtty_screen`: glyphs written in
  insert mode shift the rest of the row right instead of overwriting.
- Added `DECSTR` soft reset (`CSI ! p`) to `vtty_screen`: resets cursor,
  styles, margins, modes, and tab stops without clearing the screen.
- Added selective erase to `vtty_screen`: `DECSCA` protect attribute
  (`CSI Ps " q`) plus `DECSED` (`CSI ? Ps J`) and `DECSEL` (`CSI ? Ps K`),
  which erase only non-protected cells.
- Added DECSCNM reverse video (`CSI ?5 h/l`) to `vtty_screen`: `render_ansi`
  swaps foreground and background (default colors resolved to explicit RGB).
- Added VT400 rectangle operations to `vtty_screen`: `DECFRA`, `DECCRA`,
  `DECERA`, `DECSERA`, `DECCARA`, and `DECRARA` (all `CSI … $ …`).
- Added the Layer-3 write-order width model to `vtty_width`
  (`write_order::SplitBoundaries`, `EmojiTrace`, `split_widths`,
  `classify_cell`, `wrap_width`, `splice_offset`) — the pure `(text, B, F)`
  width derivation the terminal screens share.
- Added byte↔cell walkers to `vtty_width`: `cell_col_byte_from_widths`,
  `cell_col_byte_ceil_from_widths`, `grapheme_end_from_widths`,
  `grapheme_tail_end_from_widths`.
- Added the winit-free keyboard model and encoders to `vtty_core::keyboard`
  (`Key`, `Modifiers`, `KeySpec`, `KeyEncoding`, `KeyEventKind`,
  `KeyLocation`, `KeyModes`, `encode_key`, `legacy_ctrl`).
- Added `CursorStyle::to_decscusr` + `CursorStyle::decscusr_blink` (the
  DECSCUSR encode side) and `UnderlineStyle::from_stored`.

### Changed

- **Breaking:** Renamed `vtty_core::TermFlags` to Ghostty-style names
  (`KEYPAD_KEYS`, `BACKARROW_KEY_MODE`, `ENABLE_LEFT_AND_RIGHT_MARGIN`,
  `REVERSE_COLORS`, …).
- **Breaking:** `vtty_width::width_icu` now returns `usize` instead of
  `Option<usize>`.
- **Breaking:** `vtty_width::Selector::from_char` now returns `Selector` (with
  a new `None` variant) instead of `Option<Selector>`.
- **Breaking:** `vtty_width::baked` is no longer public.
- `vtty_codec::{EncodeError, DecodeError}` are now `#[non_exhaustive]`.
- **Breaking:** `Reply` builders are now associated functions returning
  `CompactString`, and the `Reply` codec was dropped.
- **Breaking:** `Reply::termcap` now takes the terminal name as an argument
  instead of a hardcoded `TN` value.
- Pure builders and queries are now `#[must_use]`.
- OSC and DCS payload boundary scanning now uses SIMD when the `simd`
  feature is enabled.
- `vtty_screen` full-screen scrolling now rotates a ring buffer (O(cols) per
  line) instead of memmoving the grid.
- `vtty_codec::EncodeError` now has an `UnknownVariant` variant; encoding a
  `#[non_exhaustive]` enum with an unknown variant errors instead of
  panicking.
- Removed the `VTTY_EMOJI_FAST` build knob from `vtty_width`, which emitted
  `unsafe` code incompatible with the crate's `forbid(unsafe_code)`.
- `vtty_screen` insert/delete lines and multi-line region scrolls now shift
  `n` rows in one pass instead of `n` passes.
- `vtty_screen` printable-ASCII runs now write cells in one bulk pass instead
  of a width lookup and cell write per byte.
- `vtty_screen` scrollback now reuses the oldest line's buffer instead of
  allocating a fresh one per scrolled line.

### Fixed

- Fixed display-cell width for control/format characters and Hangul V/T jamo,
  which were previously counted as one cell in some sequences.
- Fixed C1 control cancellation inside OSC/DCS strings.
- Fixed the run end-char boundary in `vtty_width::cell_col_char_end_from_widths`.
- Fixed `query-os-name` reporting `unknown` on NetBSD and OpenBSD.
- Fixed `vtty_codec` decoding a length prefix larger than the remaining
  input, which could force a huge allocation on a malicious stream.
- `vtty_screen` grid size and offset math now computes in `usize`, so a grid
  near the `u32` cell limit no longer overflows.
- Cursor motion and `Grapheme::len` now saturate, so extreme or inconsistent
  values clamp instead of overflowing or underflowing.
- Fixed `CSI ? r` resetting the scroll margins; it is DECPCTERM and is now
  ignored.

## [0.1.0]

Initial release: the `vtty` DEC VT500 parser, the `vtty_core` terminal model,
`vtty_width` display-width/emoji segmentation, `vtty_grapheme` UAX #29
segmentation, `vtty_status` OSC 7501, `vtty_codec` binary codec, and `vtty_ffi`
C ABI.
