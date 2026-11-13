// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `OSC 7501` Program Status Protocol for `vtty`.
//!
//! The optional status extension: the [`ProgramStatus`] codec (parse/encode)
//! and the [`PerformProgramStatus`] hook, which a [`vtty_core::Perform`] sink
//! implements to receive typed reports.
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
