// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The owned, trait-free event API: `Event` + `Recording`.

use alloc::string::ToString;
use alloc::vec::Vec;
use smallvec::SmallVec;

use crate::perform::Perform;

/// A parsed escape-sequence event: the dispatch as data instead of a
/// [`crate::Perform`] callback. Params/intermediates use inline `SmallVec`s
/// (≤6 params, ≤2 intermediates, no heap); only OSC bodies allocate.
/// Mirrors `vtparse`'s `VTAction` for differential testing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// A single printable code point (UTF-8 decoded).
    Print(char),
    /// A contiguous run of printable ASCII (`0x20..=0x7e`) — the bulk hot
    /// path, preserved as one event (not per-byte `Print`).
    PrintBytes(Vec<u8>),
    /// A C0/C1 control byte.
    Execute(u8),
    /// A CSI sequence: `ESC [ <params> <intermediates> <final>`.
    CsiDispatch {
        /// Collected numeric parameters (inline up to 6).
        params: SmallVec<i32, 6>,
        /// Bit `i` is set when parameter `i` was followed by a colon (the
        /// `4:2` vs `4;2` distinction).
        sep: u16,
        /// Intermediate bytes: private markers + `0x20..0x2f`.
        intermediates: SmallVec<u8, 2>,
        /// The final byte selecting the command.
        final_byte: u8,
    },
    /// An escape sequence: `ESC <intermediates> <byte>`.
    EscDispatch {
        /// Intermediate bytes (`0x20..0x2f`).
        intermediates: SmallVec<u8, 2>,
        /// The final byte selecting the command.
        byte: u8,
    },
    /// A DCS header: `ESC P <params> <intermediates> <final>`.
    DcsHook {
        /// Collected numeric parameters (inline up to 6).
        params: SmallVec<i32, 6>,
        /// Intermediate bytes: private markers + `0x20..0x2f`.
        intermediates: SmallVec<u8, 2>,
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
        params: SmallVec<Vec<u8>, 8>,
        /// Whether the sequence was terminated by `BEL` (vs `ST`).
        bell_terminated: bool,
    },
}

impl Event {
    /// Append the canonical escape-sequence bytes to `out` — the textual
    /// inverse of the parser, so re-parsing the appended bytes reproduces
    /// `self` (modulo the `-1` empty-param sentinel).
    pub fn write_bytes<const N: usize>(&self, out: &mut SmallVec<u8, N>) {
        match self {
            Self::Print(c) => {
                let mut buf = [0_u8; 4];
                out.extend_from_slice_copy(c.encode_utf8(&mut buf).as_bytes());
            }
            Self::PrintBytes(s) => out.extend_from_slice_copy(s),
            Self::Execute(b) => out.push(*b),
            Self::CsiDispatch {
                params,
                sep,
                intermediates,
                final_byte,
            } => {
                out.extend_from_slice_copy(b"\x1b[");
                let (prefix, suffix) = split_intermediates(intermediates);
                out.extend_from_slice_copy(&prefix);
                push_params(out, params, *sep);
                out.extend_from_slice_copy(&suffix);
                out.push(*final_byte);
            }
            Self::EscDispatch {
                intermediates,
                byte,
            } => {
                out.extend_from_slice_copy(b"\x1b");
                out.extend_from_slice_copy(intermediates);
                out.push(*byte);
            }
            Self::DcsHook {
                params,
                intermediates,
                final_byte,
            } => {
                out.extend_from_slice_copy(b"\x1bP");
                let (prefix, suffix) = split_intermediates(intermediates);
                out.extend_from_slice_copy(&prefix);
                push_params(out, params, 0);
                out.extend_from_slice_copy(&suffix);
                out.push(*final_byte);
            }
            Self::DcsPut(b) => out.push(*b),
            Self::DcsUnhook => out.extend_from_slice_copy(b"\x1b\\"),
            Self::OscDispatch {
                params,
                bell_terminated,
            } => {
                out.extend_from_slice_copy(b"\x1b]");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        out.push(b';');
                    }
                    out.extend_from_slice_copy(p);
                }
                if *bell_terminated {
                    out.push(0x07);
                } else {
                    out.extend_from_slice_copy(b"\x1b\\");
                }
            }
        }
    }
}

/// Split CSI/DCS intermediates into the prefix markers (`? > = <`, emitted
/// before the params) and the suffix bytes (`0x20..0x2f`, emitted after).
fn split_intermediates(ints: &[u8]) -> (SmallVec<u8, 2>, SmallVec<u8, 2>) {
    let mut prefix = SmallVec::new();
    let mut suffix = SmallVec::new();
    for &b in ints {
        if matches!(b, b'?' | b'>' | b'=' | b'<') {
            prefix.push(b);
        } else {
            suffix.push(b);
        }
    }
    (prefix, suffix)
}

/// Append numeric params, `;`- or `:`-joined (`sep` bit `i` = param `i`
/// followed by a colon).
fn push_params<const N: usize>(out: &mut SmallVec<u8, N>, params: &[i32], sep: u16) {
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            let colon = i - 1 < 16 && sep & (1 << (i - 1)) != 0;
            out.push(if colon { b':' } else { b';' });
        }
        out.extend_from_slice_copy(p.to_string().as_bytes());
    }
}

/// Records parser events into a `Vec<Event>` (the enum-based, trait-free API).
///
/// The [`crate::Perform`] callback path is the zero-allocation hot path; this
/// recording allocates one `Event` per dispatch, for tests and tooling.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Recording {
    /// Decoded events in dispatch order.
    pub events: Vec<Event>,
}

impl Recording {
    /// An empty recording.
    pub const DEFAULT: Self = Self { events: Vec::new() };

    /// An empty recording (no events).
    #[inline]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// Append one event (chainable).
    pub fn push(&mut self, event: Event) -> &mut Self {
        self.events.push(event);
        self
    }

    /// Whether no events were recorded.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// The number of recorded events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Iterate the recorded events (in recording order).
    pub fn iter(&self) -> core::slice::Iter<'_, Event> {
        self.events.iter()
    }

    /// Drain the recorded events (in recording order), leaving the log empty.
    pub fn drain(&mut self) -> alloc::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// Record a single printable code point.
    pub fn print_char(&mut self, c: char) -> &mut Self {
        self.push(Event::Print(c))
    }

    /// Record a printable ASCII run (the `print_bytes` bulk hot path).
    pub fn print_ascii(&mut self, s: &[u8]) -> &mut Self {
        self.push(Event::PrintBytes(s.to_vec()))
    }

    /// Record a C0 control byte.
    pub fn control(&mut self, byte: u8) -> &mut Self {
        self.push(Event::Execute(byte))
    }

    /// Record a raw CSI sequence.
    pub fn csi(
        &mut self,
        params: &[i32],
        sep: u16,
        intermediates: &[u8],
        final_byte: u8,
    ) -> &mut Self {
        self.push(Event::CsiDispatch {
            params: SmallVec::from_slice_copy(params),
            sep,
            intermediates: SmallVec::from_slice_copy(intermediates),
            final_byte,
        })
    }

    /// Record a raw ESC sequence.
    pub fn esc(&mut self, intermediates: &[u8], byte: u8) -> &mut Self {
        self.push(Event::EscDispatch {
            intermediates: SmallVec::from_slice_copy(intermediates),
            byte,
        })
    }

    /// Record a DCS header.
    pub fn dcs_hook(&mut self, params: &[i32], intermediates: &[u8], final_byte: u8) -> &mut Self {
        self.push(Event::DcsHook {
            params: SmallVec::from_slice_copy(params),
            intermediates: SmallVec::from_slice_copy(intermediates),
            final_byte,
        })
    }

    /// Record a DCS payload byte.
    pub fn dcs_put(&mut self, byte: u8) -> &mut Self {
        self.push(Event::DcsPut(byte))
    }

    /// Record the end of a DCS string.
    pub fn dcs_unhook(&mut self) -> &mut Self {
        self.push(Event::DcsUnhook)
    }

    /// Record a raw OSC command.
    pub fn osc(&mut self, params: &[&[u8]], bell_terminated: bool) -> &mut Self {
        self.push(Event::OscDispatch {
            params: SmallVec::from_iter(params.iter().map(|p| p.to_vec())),
            bell_terminated,
        })
    }

    /// Replay every recorded event onto `target` in order, leaving the log
    /// intact so the same recording can be replayed onto multiple sinks.
    pub fn replay<P: Perform>(&self, target: &mut P) {
        for event in &self.events {
            match event {
                Event::Print(c) => target.print(*c),
                Event::PrintBytes(s) => target.print_bytes(s),
                Event::Execute(b) => target.execute(*b),
                Event::CsiDispatch {
                    params,
                    sep,
                    intermediates,
                    final_byte,
                } => target.csi_dispatch(params, *sep, intermediates, *final_byte),
                Event::EscDispatch {
                    intermediates,
                    byte,
                } => target.esc_dispatch(intermediates, *byte),
                Event::DcsHook {
                    params,
                    intermediates,
                    final_byte,
                } => target.hook(params, intermediates, *final_byte),
                Event::DcsPut(b) => target.put(*b),
                Event::DcsUnhook => target.unhook(),
                Event::OscDispatch {
                    params,
                    bell_terminated,
                } => {
                    let refs: Vec<&[u8]> = params.iter().map(|v| v.as_slice()).collect();
                    target.osc_dispatch(&refs, *bell_terminated);
                }
            }
        }
    }
}

impl Perform for Recording {
    fn print(&mut self, c: char) {
        self.events.push(Event::Print(c));
    }
    fn print_bytes(&mut self, s: &[u8]) {
        self.events.push(Event::PrintBytes(s.to_vec()));
    }
    fn execute(&mut self, byte: u8) {
        self.events.push(Event::Execute(byte));
    }
    fn csi_dispatch(&mut self, params: &[i32], sep: u16, intermediates: &[u8], final_byte: u8) {
        self.events.push(Event::CsiDispatch {
            params: SmallVec::from_slice_copy(params),
            sep,
            intermediates: SmallVec::from_slice_copy(intermediates),
            final_byte,
        });
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        self.events.push(Event::EscDispatch {
            intermediates: SmallVec::from_slice_copy(intermediates),
            byte,
        });
    }
    fn hook(&mut self, params: &[i32], intermediates: &[u8], final_byte: u8) {
        self.events.push(Event::DcsHook {
            params: SmallVec::from_slice_copy(params),
            intermediates: SmallVec::from_slice_copy(intermediates),
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
            params: SmallVec::from_iter(params.iter().map(|p| p.to_vec())),
            bell_terminated,
        });
    }
}
