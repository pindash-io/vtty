// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A **zero-allocation**, **panic-free** DEC VT500 terminal parser with
//! **UTF-8** decoding and **`no_std`** support.
//!
//! `vtty` turns bytes into typed events via the `Perform` callback trait and
//! re-exports the terminal model types (`ColorId`, `SgrFlags`, `Charset`, …)
//! from `vtty_core`; the companion `vtty_codec` crate provides the compact
//! binary `Encode`/`Decode` implementations for the owned types.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use vtty::{Parser, Perform};
//!
//! struct Screen;
//!
//! impl Perform for Screen {
//!     fn print_bytes(&mut self, s: &[u8]) { /* printable ASCII run */ }
//!     fn print(&mut self, c: char) { /* non-ASCII code point */ }
//!     fn execute(&mut self, b: u8) { /* \n \t \x1b … */ }
//!     fn csi_dispatch(&mut self, params: &[i32], sep: u16, intermediates: &[u8], final_byte: u8) {
//!         /* CSI Ps ; Ps final */
//!     }
//!     fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) { /* OSC */ }
//! }
//!
//! let mut parser = Parser::new();
//! parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
//!
//! // With the `simd-transcode` feature enabled, opt into SIMD scanning,
//! // validation and UTF-8→UTF-32 transcode:
//! let mut parser = Parser::with_simd();
//! parser.advance(&mut screen, b"\x1b[1;31mhello\x1b[0m");
//! ```
//!
//! Also available: `Event`/`Recording` (owned enum API), `State` (byte-cursor
//! sink with line/cursor geometry), `Reply` (query reply builder),
//! `ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`, `MouseButton`, `Charset`,
//! `Cap`, and a compact binary `vtty_codec` crate (`Encode`/`Decode`) covering
//! all of the above.
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
//! // Owned events encode/decode as data:
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
//! - **Safety** — `#![forbid(unsafe_code)]`; malformed PTY input never
//!   crashes.
//! - **Zero-allocation** — one `print_bytes` per ASCII run, one
//!   `print_codepoints` per non-ASCII run; CSI/OSC/DCS state is inline.
//! - **`no_std`** — optional `simd` (scanning + validation) and
//!   `simd-transcode` (UTF-8→UTF-32) features, enabled per parser.
//! - **UTF-8** — maximal-subpart substitution; 8-bit C1 controls (DEC
//!   “anywhere”).
//! - **Wire protocols** (on by default, opt out with
//!   `default-features = false`) — `reply` (DECRQSS/DECRQM/XTGETTCAP),
//!   `termcap` (`Cap`/`CapValue`, implies `reply`), `status` (OSC 7501
//!   `ProgramStatus`).
//!
//! ## Test
//!
//! ```bash
//! cargo test -p vtty
//! cargo test -p vtty --test vt_differential -- --nocapture     # parity vs vte
//! cargo test -p vtty --test vt_differential_rio -- --nocapture # parity vs rio-vt
//! ```
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

extern crate alloc;

mod parser;
mod scan;
#[cfg(feature = "simd")]
mod simd;
mod table;
mod utf8;
#[cfg(feature = "simd-transcode")]
mod utf8_tables;

pub use parser::Parser;
pub use vtty_core::*;
#[cfg(feature = "status")]
pub use vtty_status::*;
