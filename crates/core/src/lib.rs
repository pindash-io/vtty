// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The irreducible core of `vtty`: the [`Perform`] callback contract, the
//! owned [`Event`]/[`Recording`] API, the terminal model types (`ColorId`,
//! `SgrFlags`, `Charset`, `TermFlags`, …), and the reference byte-cursor
//! [`State`] sink.
//!
//! ## Features
//!
//! - `reply`: DECRQSS/DECRQM/XTGETTCAP answer formatting (`Reply`).
//! - `termcap`: XTGETTCAP terminfo capability table (`Cap`/`CapValue`);
//!   implies `reply`.
//!
//! ## Stability
//!
//! `#[non_exhaustive]` marks enums that mirror a still-growing external
//! vocabulary, so a future variant cannot break a downstream `match` in
//! silence. Closed sets such as [`ColorSpec`] and [`SgrEffect`] stay
//! exhaustive. Types with public fields stay constructible by struct
//! literal, so they are never `#[non_exhaustive]`.
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
mod num;
mod perform;
#[cfg(feature = "reply")]
mod reply;
mod state;
mod style;
#[cfg(feature = "termcap")]
mod termcap;

pub use charset::{Charset, Charsets, Scs};
pub use color::ColorId;
pub use color::{DEFAULT_BG_ID, DEFAULT_FG_ID};
pub use color::{
    blend_faint, format_osc_background, format_osc_color_reply, format_osc_palette_reply,
    hex_to_rgb_array, hex_to_rgba, hex_to_rgba_array, hex_to_rgba_u32, indexed_rgb,
    osc_to_rgba_u32, rgb_spec, rgb_to_u32, rgba_to_u32, u32_to_array, u32_to_rgba,
};
pub use conformance::ConformanceLevel;
pub use event::{Event, Recording};
pub use event::{param_colon_sep, set_param_colon_sep};
pub use keyboard::{
    Key, KeyEncoding, KeyEventKind, KeyLocation, KeyModes, KeySpec, KittyFlags, Modifiers,
};
pub use keyboard::{encode_key, legacy_ctrl};
pub use mode::{MOUSE_MASK, TermFlags};
pub use mouse::{MouseButton, MouseTracking};
pub use num::write_decimal;
pub use perform::Perform;
#[cfg(feature = "reply")]
#[cfg_attr(docsrs, doc(cfg(feature = "reply")))]
pub use reply::Reply;
pub use state::State;
pub use style::Effects;
pub use style::Sgr;
pub use style::{ColorSpec, CursorStyle, SgrEffect, SgrFlags, StyleFlags, UnderlineStyle};
pub use style::{parse_effects, parse_extended, single_code, single_effect, two_simple_effects};
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
