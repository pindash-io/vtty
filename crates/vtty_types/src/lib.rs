// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Terminal model types and the [`Perform`] callback trait for `vtty`.
//!
//! This is the data plane: the [`Perform`] trait, the owned
//! [`Event`]/[`Recording`] API, and the terminal model types (`ColorId`,
//! `SgrFlags`, `Charset`, `ProgramStatus`, …). The `vtty` parser crate depends
//! on it and re-exports its API; `vtty_codec` depends on it so serialization
//! does not have to pull in the parser.
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

mod charset;
mod color;
mod event;
mod keyboard;
mod mode;
mod mouse;
mod perform;
mod reply;
mod state;
mod status;
mod style;
mod termcap;

pub use charset::{Charset, Charsets, dec_special_graphics};
pub use color::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID};
pub use event::{Event, Recording};
pub use keyboard::KittyFlags;
pub use mode::{MOUSE_MASK, TermFlags};
pub use mouse::{MouseButton, MouseTracking};
pub use perform::Perform;
pub use reply::Reply;
pub use state::State;
pub use status::{PerformProgramStatus, ProgramStatus, StatusKind, StatusState};
pub use style::{CursorStyle, SgrFlags, StyleFlags, UnderlineStyle};
pub use termcap::{Cap, CapValue};
