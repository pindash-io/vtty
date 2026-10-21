// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! OSC 7501 Program Status Protocol for `vtty`.
//!
//! A program reports its state — idle, working, done, blocked, or error — to
//! the terminal, or clears a previously reported status. This crate provides
//! the [`ProgramStatus`] codec and the [`PerformProgramStatus`] hook, which a
//! [`vtty_core::Perform`] sink implements to opt in.
//!
//! ```text
//! ESC ] 7501 ; state=working:app=brew:msg=<base64> ESC \
//! ```
//!
//! Reference: <https://www.superlogical.com/rex/docs/build/program-status>.
#![forbid(unsafe_code)]
#![no_std]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_debug_implementations)]
#![deny(missing_docs)]

extern crate alloc;

mod status;

pub use status::{PerformProgramStatus, ProgramStatus, StatusKind, StatusState};
