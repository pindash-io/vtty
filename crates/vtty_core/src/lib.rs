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
//! ## Features
//!
//! - `reply` — DECRQSS/DECRQM/XTGETTCAP answer formatting (`Reply`).
//! - `termcap` — XTGETTCAP terminfo capability table (`Cap`/`CapValue`);
//!   implies `reply`.
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![no_std]
#![expect(
    clippy::cast_possible_truncation,
    reason = "terminal model types use fixed-size casts where the range is known-safe"
)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_debug_implementations)]
#![deny(missing_docs)]

extern crate alloc;

pub use const_hex;

mod charset;
mod color;
mod conformance;
mod event;
mod keyboard;
mod mode;
mod mouse;
mod perform;
#[cfg(feature = "reply")]
mod reply;
mod state;
mod style;
#[cfg(feature = "termcap")]
mod termcap;

pub use charset::{Charset, Charsets, Scs, dec_special_graphics};
pub use color::{
    ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, blend_faint, format_osc_background,
    format_osc_color_reply, format_osc_palette_reply, indexed_rgb, parse, parse_hex_color, rgb,
    rgba_to_u32, u32_to_rgb, u32_to_rgba,
};
pub use conformance::ConformanceLevel;
pub use event::{Event, Recording};
pub use keyboard::KittyFlags;
pub use mode::{MOUSE_MASK, TermFlags};
pub use mouse::{MouseButton, MouseTracking};
pub use perform::Perform;
#[cfg(feature = "reply")]
#[cfg_attr(docsrs, doc(cfg(feature = "reply")))]
pub use reply::Reply;
pub use state::State;
pub use style::{ColorSpec, CursorStyle, SgrEffect, SgrFlags, StyleFlags, UnderlineStyle};
pub use style::{parse_effects, parse_extended, single_code, single_effect};
#[cfg(feature = "termcap")]
#[cfg_attr(docsrs, doc(cfg(feature = "termcap")))]
pub use termcap::{Cap, CapValue};

/// Compile-time guarantee: the owned event types and the byte-cursor sink are
/// `Send` + `Sync`, so they can be shared or moved across threads.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Event>();
    assert_send_sync::<Recording>();
    assert_send_sync::<State>();
};
