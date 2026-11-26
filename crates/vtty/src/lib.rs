// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A zero-allocation, panic-free DEC VT500 terminal parser with UTF-8 decoding
//! and `no_std` support.
//!
//! `vtty` turns bytes into typed events via the `Perform` callback trait and
//! re-exports the terminal model types (`ColorId`, `SgrFlags`, `Charset`, …)
//! from `vtty_core`. The companion `vtty_codec` crate provides the compact
//! binary `Encode`/`Decode` implementations for the owned types.
//!
//! The parser implements the DEC VT500 state machine from Paul Flo Williams'
//! [A parser for DEC's ANSI-compatible video terminals][dec-parser].
//!
//! ## Usage
//!
//! ```rust
//! use vtty::{Parser, Perform};
//!
//! struct Screen;
//!
//! impl Perform for Screen {
//!     fn print_bytes(&mut self, _s: &[u8]) { /* printable ASCII run */ }
//!     fn print(&mut self, _c: char) { /* non-ASCII code point */ }
//!     fn execute(&mut self, _b: u8) { /* \n \t \x1b … */ }
//!     fn csi_dispatch(&mut self, _params: &[i32], _sep: u16, _intermediates: &[u8], _final_byte: u8) {
//!         /* CSI Ps ; Ps final */
//!     }
//!     fn osc_dispatch(&mut self, _params: &[&[u8]], _bell: bool) { /* OSC */ }
//! }
//!
//! let mut screen = Screen;
//! let mut parser = Parser::new();
//! parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
//! ```
//!
//! With the `simd` feature enabled, `Parser::with_simd` opts into SIMD
//! scanning and UTF-8 validation.
//!
//! ## Serialization
//!
//! Every owned type round-trips through the compact, `no_std` binary codec:
//!
//! ```
//! use vtty_codec::{decode_from_slice, encode_to_vec};
//! use vtty::{Event, Parser};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut parser = Parser::new();
//! let events = parser.parse_events(b"\x1b[1;31mhi");
//! let bytes = encode_to_vec(&events)?;
//! assert_eq!(decode_from_slice::<Vec<Event>>(&bytes)?, events);
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - Safety: `#![forbid(unsafe_code)]`; malformed PTY input never crashes.
//! - Zero-allocation: one `print_bytes` per ASCII run, one `print_codepoints`
//!   per non-ASCII run; CSI/OSC/DCS state is inline.
//! - `no_std`: optional `simd` (scanning and validation) and `simd-transcode`
//!   (UTF-8 to UTF-32) features, enabled per parser.
//! - UTF-8: maximal-subpart substitution; 8-bit C1 controls (DEC "anywhere").
//! - Wire protocols (on by default, opt out with `default-features = false`):
//!   `reply` (DECRQSS/DECRQM/XTGETTCAP), `termcap` (`Cap`/`CapValue`, implies
//!   `reply`), `status` (`OSC 7501` `ProgramStatus`).
//! - `perf-inline` (on by default): `#[inline(always)]` on the parser hot path;
//!   opt out for smaller code at some throughput cost.
//!
//! ## Test
//!
//! ```bash
//! cargo test -p vtty
//! cargo test -p vtty --test vt_differential -- --nocapture     # parity vs vte
//! cargo test -p vtty --test vt_differential_rio -- --nocapture # parity vs rio-vt
//! ```
//!
//! [dec-parser]: https://vt100.net/emu/dec_ansi_parser
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "simd"), no_std)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "Terminal cell/pixel math truncates widths and coordinates to fixed-size types where the range is known-safe"
)]
// The parser must never panic on untrusted input (a malformed PTY stream is
// normal, not exceptional). These lints keep `unwrap`/`expect`/`panic` out.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_debug_implementations)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

mod limits;
mod parser;
mod scan;
#[cfg(feature = "simd")]
mod simd;
mod table;
mod utf8;
#[cfg(feature = "simd-transcode")]
mod utf8_tables;
mod utils;

pub use parser::Parser;
pub use vtty_core::*;
#[cfg(feature = "status")]
#[cfg_attr(docsrs, doc(cfg(feature = "status")))]
pub use vtty_status::*;

/// Compile-time guarantee: the parser is `Send` + `Sync` (no interior
/// mutability), so it can be owned by a dedicated input thread.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Parser>();
};

/// Parser, [`Perform`], and model types. Free functions stay explicit.
pub mod prelude {
    #[cfg(feature = "reply")]
    #[cfg_attr(docsrs, doc(cfg(feature = "reply")))]
    pub use crate::Reply;
    #[cfg(feature = "termcap")]
    #[cfg_attr(docsrs, doc(cfg(feature = "termcap")))]
    pub use crate::{Cap, CapValue};
    pub use crate::{
        Charset, Charsets, ColorId, ColorSpec, ConformanceLevel, CursorStyle, DEFAULT_BG_ID,
        DEFAULT_FG_ID, Event, Key, KeyEncoding, KeyEventKind, KeyLocation, KeyModes, KeySpec,
        KittyFlags, Modifiers, MouseButton, MouseTracking, Parser, Perform, Recording, Scs, Sgr,
        SgrEffect, SgrFlags, State, StyleFlags, TermFlags, UnderlineStyle,
    };
    #[cfg(feature = "status")]
    #[cfg_attr(docsrs, doc(cfg(feature = "status")))]
    pub use crate::{PerformProgramStatus, ProgramStatus, StatusKind, StatusState};
}

/// Longer-form documentation kept in the repository and mirrored here so it
/// also appears on docs.rs.
#[allow(
    clippy::doc_markdown,
    reason = "embedded repository markdown uses plain-table identifiers, not Rust items"
)]
pub mod _documentation {
    #[doc = include_str!("../ARCHITECTURE.md")]
    pub mod architecture {}
    #[doc = include_str!("../CONFORMANCE.md")]
    pub mod conformance {}
    #[doc = include_str!("../CHANGELOG.md")]
    pub mod changelog {}
}
