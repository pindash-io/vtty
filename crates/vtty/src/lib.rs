// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Zero-allocation, panic-free DEC VT500 terminal parser with UTF-8 decoding
//! and `no_std` support.
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
//! sink with line/cursor geometry), `Reply` (query reply builder), and
//! `ColorId`, `SgrFlags`, `TermFlags`, `KittyFlags`, `MouseButton`, `Charset`, `Cap`.
//!
//! ## Design
//!
//! - **Panic-free** — `#![forbid(unsafe_code)]`; malformed PTY input never
//!   crashes.
//! - **Zero-allocation hot path** — one `print_bytes` per ASCII run, one
//!   `print_codepoints` per non-ASCII run; CSI/OSC/DCS state is inline.
//! - **`no_std` by default** — optional `simd` (scanning + validation) and
//!   `simd-transcode` (UTF-8→UTF-32) features, enabled per parser.
//! - **W3C-correct UTF-8** — maximal-subpart substitution; 8-bit clean
//!   (DEC “anywhere”).
//!
//! ## Test
//!
//! ```bash
//! cargo test -p vtty
//! cargo test -p vtty --test vt_differential -- --nocapture  # parity vs vte
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

mod charset;
mod color;
mod hex;
mod keyboard;
mod mode;
mod mouse;
mod parser;
mod perform;
mod recording;
mod reply;
#[cfg(feature = "simd")]
mod simd;
mod state;
mod style;
mod table;
mod termcap;
mod utf8;
#[cfg(feature = "simd-transcode")]
mod utf8_tables;

pub use charset::{Charset, Charsets, dec_special_graphics};
pub use color::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID};
pub use hex::{hex_decode, hex_encode};
pub use keyboard::KittyFlags;
pub use mode::{MOUSE_MASK, TermFlags};
pub use mouse::{MouseButton, MouseTracking};
pub use parser::Parser;
pub use perform::Perform;
pub use recording::{Event, Recording};
pub use reply::Reply;
pub use state::State;
pub use style::{CursorStyle, SgrFlags, StyleFlags, UnderlineStyle};
pub use termcap::{Cap, CapValue};
