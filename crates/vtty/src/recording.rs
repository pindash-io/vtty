// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The owned, trait-free event API: `Event` + `Recording`.

use alloc::vec::Vec;
use smallvec::SmallVec;

use crate::perform::Perform;

/// A parsed escape-sequence event — the dispatch as data, instead of a
/// [`crate::Perform`] callback. Mirrors vtparse's `VTAction`; useful for
/// differential testing and simple consumers.
///
/// Params and intermediates use inline `SmallVec`s (matching the parser's
/// internal storage) so the common case (≤6 params, ≤2 intermediates) does not
/// allocate; only OSC bodies are heap `Vec`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A printable character (UTF-8 decoded).
    Print(char),
    /// A C0/C1 control byte.
    Execute(u8),
    /// A CSI sequence: `ESC [ <params> <intermediates> <final>`.
    CsiDispatch {
        /// Collected numeric parameters (inline up to 6).
        params: SmallVec<[i32; 6]>,
        /// Intermediate bytes: private markers + `0x20..0x2f`.
        intermediates: SmallVec<[u8; 2]>,
        /// The final byte selecting the command.
        final_byte: u8,
    },
    /// An escape sequence: `ESC <intermediates> <byte>`.
    EscDispatch {
        /// Intermediate bytes (`0x20..0x2f`).
        intermediates: SmallVec<[u8; 2]>,
        /// The final byte selecting the command.
        byte: u8,
    },
    /// A DCS header: `ESC P <params> <intermediates> <final>`.
    DcsHook {
        /// Collected numeric parameters (inline up to 6).
        params: SmallVec<[i32; 6]>,
        /// Intermediate bytes: private markers + `0x20..0x2f`.
        intermediates: SmallVec<[u8; 2]>,
        /// The final byte selecting the command.
        final_byte: u8,
    },
    /// A DCS payload byte.
    DcsPut(u8),
    /// The DCS string ended.
    DcsUnhook,
    /// An OSC command ended: `ESC ] <params split on ';'> (BEL | ST)`. The
    /// outer list is inline up to 8 params; each body is a heap `Vec<u8>`
    /// (variable-length byte strings have to be owned).
    OscDispatch {
        /// Parameters split on `;`, each an owned byte string.
        params: SmallVec<[Vec<u8>; 8]>,
        /// Whether the sequence was terminated by BEL (vs ST).
        bell_terminated: bool,
    },
}

/// Records parser events into a `Vec<Event>` (the enum-based, trait-free API).
///
/// The [`crate::Perform`] callback path is the zero-allocation hot path; this
/// recording allocates one `Event` per dispatch, for tests and tooling.
#[derive(Debug, Default)]
pub struct Recording {
    /// Decoded events in dispatch order.
    pub events: Vec<Event>,
}

impl Perform for Recording {
    fn print(&mut self, c: char) {
        self.events.push(Event::Print(c));
    }
    fn print_bytes(&mut self, s: &[u8]) {
        for &b in s {
            self.events.push(Event::Print(b as char));
        }
    }
    fn execute(&mut self, byte: u8) {
        self.events.push(Event::Execute(byte));
    }
    fn csi_dispatch(&mut self, params: &[i32], _sep: u16, intermediates: &[u8], final_byte: u8) {
        self.events.push(Event::CsiDispatch {
            params: params.iter().copied().collect(),
            intermediates: intermediates.iter().copied().collect(),
            final_byte,
        });
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        self.events.push(Event::EscDispatch {
            intermediates: intermediates.iter().copied().collect(),
            byte,
        });
    }
    fn hook(&mut self, params: &[i32], intermediates: &[u8], final_byte: u8) {
        self.events.push(Event::DcsHook {
            params: params.iter().copied().collect(),
            intermediates: intermediates.iter().copied().collect(),
            final_byte,
        });
    }
    fn put(&mut self, byte: u8) {
        self.events.push(Event::DcsPut(byte));
    }
    fn unhook(&mut self) {
        self.events.push(Event::DcsUnhook);
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.events.push(Event::OscDispatch {
            params: params.iter().map(|p| p.to_vec()).collect(),
            bell_terminated,
        });
    }
}
