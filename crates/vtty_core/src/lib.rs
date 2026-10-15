// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The `vtty` parser engine: a zero-allocation, panic-free DEC VT500 terminal
//! parser with UTF-8 decoding and `no_std` support.
//!
//! This crate is the engine; the public API is the [`vtty`](https://docs.rs/vtty)
//! facade, which re-exports everything here. `vtty_core` exists as a separate
//! crate so that [`vtty_codec`](https://docs.rs/vtty_codec) can implement
//! `Encode`/`Decode` for the engine types without depending on the facade.
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
mod status;
mod style;
mod table;
mod termcap;
mod utf8;
#[cfg(feature = "simd-transcode")]
mod utf8_tables;

pub use charset::{Charset, Charsets, dec_special_graphics};
pub use color::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID};
pub use keyboard::KittyFlags;
pub use mode::{MOUSE_MASK, TermFlags};
pub use mouse::{MouseButton, MouseTracking};
pub use parser::Parser;
pub use perform::Perform;
pub use recording::{Event, Recording};
pub use reply::Reply;
pub use state::State;
pub use status::{PerformProgramStatus, ProgramStatus, StatusKind, StatusState};
pub use style::{CursorStyle, SgrFlags, StyleFlags, UnderlineStyle};
pub use termcap::{Cap, CapValue};
