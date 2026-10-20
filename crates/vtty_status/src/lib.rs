// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! OSC 7501 Program Status Protocol for `vtty`.
//!
//! A program reports what it is doing (idle, working, done, blocked, error,
//! or clear) to the terminal. This crate is the optional status-protocol
//! extension: the [`ProgramStatus`] codec plus the [`PerformProgramStatus`]
//! hook, which a [`vtty_core::Perform`] sink implements to opt in.
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
