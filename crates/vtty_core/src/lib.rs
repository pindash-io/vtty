// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The irreducible core of `vtty`: the [`Perform`] callback contract, the
//! owned [`Event`]/[`Recording`] API, the terminal model types (`ColorId`,
//! `SgrFlags`, `Charset`, `TermFlags`, …), and the reference byte-cursor
//! [`State`] sink.
//!
//! This crate is split out so the `vtty` parser facade and `vtty_codec` can
//! share it without the codec depending on the parser. `vtty` re-exports its
//! API.
//!
//! # Features
//!
//! - `reply` — DECRQSS/DECRQM/XTGETTCAP answer formatting ([`Reply`]).
//! - `termcap` — XTGETTCAP terminfo capability table ([`Cap`]/[`CapValue`]);
//!   implies `reply`.
//! - `status` — OSC 7501 Program Status Protocol ([`ProgramStatus`]).
#![forbid(unsafe_code)]
#![no_std]
#![expect(
    clippy::cast_possible_truncation,
    reason = "terminal model types use fixed-size casts where the range is known-safe"
)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_debug_implementations)]
#![deny(missing_docs)]

extern crate alloc;

#[cfg(feature = "reply")]
pub use const_hex;

mod charset;
mod color;
mod event;
mod keyboard;
mod mode;
mod mouse;
mod perform;
#[cfg(feature = "reply")]
mod reply;
mod state;
#[cfg(feature = "status")]
mod status;
mod style;
#[cfg(feature = "termcap")]
mod termcap;

pub use charset::{Charset, Charsets, dec_special_graphics};
pub use color::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID};
pub use event::{Event, Recording};
pub use keyboard::KittyFlags;
pub use mode::{MOUSE_MASK, TermFlags};
pub use mouse::{MouseButton, MouseTracking};
pub use perform::Perform;
#[cfg(feature = "reply")]
pub use reply::Reply;
pub use state::State;
#[cfg(feature = "status")]
pub use status::{PerformProgramStatus, ProgramStatus, StatusKind, StatusState};
pub use style::{CursorStyle, SgrFlags, StyleFlags, UnderlineStyle};
#[cfg(feature = "termcap")]
pub use termcap::{Cap, CapValue};
