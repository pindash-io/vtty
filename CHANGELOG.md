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

### Fixed

- Fixed display-cell width for control/format characters and Hangul V/T jamo,
  which were previously counted as one cell in some sequences.
- Fixed C1 control cancellation inside OSC/DCS strings.
- Fixed the run end-char boundary in `vtty_width::cell_col_char_end_from_widths`.

## [0.1.0]

Initial release: the `vtty` DEC VT500 parser, the `vtty_core` terminal model,
`vtty_width` display-width/emoji segmentation, `vtty_grapheme` UAX #29
segmentation, `vtty_status` OSC 7501, `vtty_codec` binary codec, and `vtty_ffi`
C ABI.
