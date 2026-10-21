// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The byte-input VT parser: the `Parser` state machine. The
//! [`crate::Perform`] contract and the owned [`crate::Event`] API are
//! re-exported from `vtty_core`; the UTF-8 incremental decoder lives in
//! `utf8`, the SWAR byte scanners in `scan`.

use alloc::vec::Vec;

use smallvec::SmallVec;

use crate::scan::{first_dcs_boundary, first_non_printable, first_osc_boundary};
use crate::table::{Action, ParseState, transition};
use crate::utf8::{decode_utf8, trailing_partial};
use vtty_core::Perform;
use vtty_core::{Event, Recording};

/// Maximum buffered OSC payload. A hostile program can otherwise stream a
/// gigabyte into `ESC ] … ST` and OOM the parser; xterm.js caps OSC/DCS at
/// `PAYLOAD_LIMIT = 10000000` and aborts the sequence, so we bound here too.
/// 1 MiB comfortably fits the largest real OSC (OSC 52 clipboard ≈ 128 KiB).
const MAX_OSC_PAYLOAD: usize = 1_048_576;

/// Cap on OSC parameters (vte/rio-vt = 16); the remaining payload is dropped.
const MAX_OSC_PARAMS: usize = 16;

/// The byte-input VT parser: UTF-8 decoding plus per-codepoint dispatch.
///
/// Feed bytes with [`Parser::advance`]; the [`Perform`] sink receives the
/// decoded events. Escape-sequence states live in a cold transition table
/// (see `crate::table`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parser {
    state: ParseState,
    /// CSI/DCS parameter currently being accumulated. DEC VT500 params are
    /// 16-bit (0..=65535); `i32` holds the clamped range plus the `-1`
    /// empty-param sentinel.
    param: i32,
    /// True while the current parameter is empty (no digits seen since the
    /// last `;`/`:`). An empty COLON parameter is pushed as `-1` so the SGR
    /// colon forms (`38:2::r:g:b` — empty colorspace) stay distinguishable
    /// from a literal `0`.
    param_empty: bool,
    /// Collected CSI/DCS parameters, capped at [`MAX_PARAM_COUNT`]. A fixed array
    /// plus a separate length makes clearing an O(1) reset (vs
    /// `SmallVec::clear`, which walks every element — ~2% on the CSI path).
    params: [i32; MAX_PARAM_COUNT],
    params_len: usize,
    /// Colon separators: bit `i` is set when parameter `i` was followed by
    /// `:` (rather than `;`). Only the first 16 params are tracked.
    sep: u16,
    /// Collected intermediates (private markers + 0x20..0x2f). The DEC spec
    /// caps these at two; extras are dropped.
    intermediates: [u8; 2],
    intermediates_len: usize,
    /// OSC payload accumulator. Inline up to 56 — nvim's longest OSC is 54
    /// bytes (clipboard base64), so common OSC payloads stay inline.
    osc: SmallVec<u8, 56>,
    /// Set when an OSC payload exceeds [`MAX_OSC_PAYLOAD`]; the whole OSC is
    /// then discarded (xterm.js aborts oversized payloads the same way).
    osc_overflow: bool,
    /// Start offset of each OSC param in `osc` (the params are contiguous in
    /// the buffer — `;` separators are not stored, so the ranges merge at the
    /// boundaries). `starts[0]` is always 0; the extra slot caps the 16th
    /// param so params 17+ are dropped (vte/rio-vt convention).
    osc_starts: [u32; MAX_OSC_PARAMS + 1],
    osc_num_starts: usize,
    /// True while the current OSC payload is still entirely within the current
    /// `advance` input slice (no copy into `osc` yet); `osc_base` is its start
    /// index in that slice. The copy is deferred until the string spans an
    /// `advance` call or is terminated by a cold control byte.
    osc_deferring: bool,
    osc_base: usize,
    /// Partially-decoded UTF-8 code point (u32) split across `advance` calls.
    partial: u32,
    /// Continuation bytes still needed to complete `partial` (0 = none).
    partial_need: u8,
    /// Runtime SIMD toggle: when `true`, the printable-run scan and UTF-8
    /// validation use SIMD. Only present (and only settable, via
    /// [`Parser::with_simd`]) when the `simd` feature is compiled in.
    #[cfg(feature = "simd")]
    simd: bool,
}

impl Parser {
    /// An empty parser in the ground state.
    pub const DEFAULT: Self = Self {
        state: ParseState::Ground,
        param: 0,
        param_empty: true,
        params: [0; MAX_PARAM_COUNT],
        params_len: 0,
        sep: 0,
        intermediates: [0; 2],
        intermediates_len: 0,
        osc: SmallVec::new(),
        osc_overflow: false,
        osc_starts: [0; MAX_OSC_PARAMS + 1],
        osc_num_starts: 0,
        osc_deferring: false,
        osc_base: 0,
        partial: 0,
        partial_need: 0,
        #[cfg(feature = "simd")]
        simd: false,
    };

    /// An empty parser in the ground state (no allocation).
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// An empty parser in the ground state with SIMD fast paths enabled.
    /// No allocation.
    #[cfg(feature = "simd")]
    pub const fn with_simd() -> Self {
        let mut p = Self::DEFAULT;
        p.simd = true;
        p
    }

    /// Whether the SIMD fast paths are enabled for this parser. Always `false`
    /// when the `simd` feature is not compiled in.
    #[cfg(feature = "simd")]
    #[inline]
    const fn simd_enabled(&self) -> bool {
        self.simd
    }

    #[cfg(not(feature = "simd"))]
    #[inline]
    const fn simd_enabled(&self) -> bool {
        false
    }

    /// Parse bytes, collecting every dispatch into a `Vec<Event>` — the
    /// convenience enum API; [`Self::advance`] + [`Perform`] is the
    /// zero-allocation hot path. Unbounded `Vec`, so tests/tooling only.
    pub fn parse_events(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut c = Recording::default();
        self.advance(&mut c, bytes);
        c.events
    }

    /// Parse bytes, dispatching each decoded event to the sink `p`.
    ///
    /// This is the zero-allocation hot path (no intermediate `Event`s); see
    /// [`Self::parse_events`] for the owned enum API.
    #[inline]
    pub fn advance<P: Perform>(&mut self, p: &mut P, bytes: &[u8]) {
        // Fast return: an empty chunk cannot complete a pending partial nor
        // dispatch anything, so skip the partial and main loops entirely.
        if bytes.is_empty() {
            return;
        }
        let mut i = 0;

        // Complete a partially-decoded UTF-8 code point carried over from the
        // previous call (accumulate continuation bytes into the u32).
        if self.partial_need > 0 {
            // `completed` separates a fully-consumed partial (emit one char)
            // from an aborted one (emit U+FFFD once, no second char).
            let mut completed = false;
            while self.partial_need > 0 && i < bytes.len() {
                let b = bytes[i];
                if (0x80..0xc0).contains(&b) {
                    self.partial = (self.partial << 6) | (b & 0x3f) as u32;
                    self.partial_need -= 1;
                    i += 1;
                    completed = self.partial_need == 0;
                } else {
                    // Not a continuation byte — abort the partial, leave the
                    // byte for the main loop.
                    self.partial_need = 0;
                    p.print(char::REPLACEMENT_CHARACTER);
                    break;
                }
            }
            if completed {
                // Test the numeric code point first: the control ranges never
                // need a `char`, so convert only on the print path.
                if matches!(self.partial, 0x00..=0x1f | 0x7f | 0x80..=0x9f) {
                    p.execute(self.partial as u8);
                } else {
                    p.print(char::from_u32(self.partial).unwrap_or(char::REPLACEMENT_CHARACTER));
                }
            }
        }

        // Detect a trailing partial code point once here, handing the partial-
        // free prefix length to the ground loop (avoids re-scanning per entry).
        let valid_end = match trailing_partial(bytes) {
            Some((v, cp, need)) => {
                self.partial = cp;
                self.partial_need = need;
                v
            }
            None => bytes.len(),
        };

        while i < bytes.len() {
            match self.state {
                ParseState::Ground => i = self.advance_ground(p, bytes, i, valid_end),
                ParseState::OscString => i = self.advance_osc_run(p, bytes, i),
                ParseState::DcsPassthrough => i = self.advance_dcs_run(p, bytes, i),
                ParseState::Escape => i = self.advance_escape_byte(p, bytes, i),
                ParseState::DcsEntry => i = self.advance_dcs_entry_byte(p, bytes, i),
                ParseState::CsiParam | ParseState::DcsParam => {
                    i = self.advance_param_run(p, bytes, i);
                }
                _ => i = self.advance_non_ground(p, bytes, i),
            }
        }

        // An OSC payload that reached the end of this input chunk without a
        // terminator is flushed into `osc` so the next `advance` continues it.
        if self.state == ParseState::OscString && self.osc_deferring {
            self.osc_flush(bytes, bytes.len());
        }
    }

    /// Reset to the ground state, clearing any pending partial code point and
    /// buffered OSC payload (no allocation).
    #[inline]
    pub fn reset(&mut self) {
        // Preserve the SIMD toggle: it is a per-parser config, not state.
        #[cfg(feature = "simd")]
        let simd = self.simd;
        *self = Self::DEFAULT;
        #[cfg(feature = "simd")]
        {
            self.simd = simd;
        }
    }

    /// Emit a pending partial UTF-8 code point as `U+FFFD` (W3C substitution)
    /// at end-of-stream. Call once after the final [`Self::advance`].
    #[inline]
    pub fn flush<P: Perform>(&mut self, p: &mut P) {
        if self.partial_need == 0 {
            return;
        }
        self.partial_need = 0;
        p.print(char::REPLACEMENT_CHARACTER);
    }

    /// Ground state: SIMD-scan for the next non-printable byte (control OR
    /// high-bit — which includes ESC), bulk-dispatch printable ASCII runs via
    /// `print_bytes`, and decode multi-byte chars one at a time. The hot path —
    /// kept inline.
    #[inline(always)]
    fn advance_ground<P: Perform>(
        &mut self,
        p: &mut P,
        bytes: &[u8],
        i: usize,
        valid_end: usize,
    ) -> usize {
        let run = &bytes[i..];
        // `valid` is the prefix BEFORE the trailing partial; an escape
        // sequence can consume past `valid_end` (raw bytes), so a later ground
        // re-entry clears the stale partial.
        let valid = if i <= valid_end {
            valid_end - i
        } else {
            self.partial_need = 0;
            run.len()
        };
        let mut pos = 0;
        while pos < valid {
            // SIMD: find the leading printable-ASCII run.
            let n = first_non_printable(&run[pos..valid], self.simd_enabled());
            if n > 0 {
                // Printable ASCII is valid UTF-8 by construction (the SIMD
                // classifier already rejected control/high-bit bytes), so no
                // `from_utf8` re-validation is needed here.
                let end = pos + n;
                // Fused print+LF fast path: the printable run is followed by
                // a bare `\n` (raw LF) or `\r\n` (OPOST) — dispatch ONE fused
                // call instead of `print_bytes` + one/two `execute`s. The
                // default `print_line`/`print_crlf_line` delegate back to
                // those, so a screen that does not specialize is unaffected.
                if end < valid && run[end] == b'\n' {
                    p.print_line(&run[pos..end]);
                    pos = end + 1;
                    continue;
                }
                if end + 1 < valid && run[end] == b'\r' && run[end + 1] == b'\n' {
                    p.print_crlf_line(&run[pos..end]);
                    pos = end + 2;
                    continue;
                }
                p.print_bytes(&run[pos..end]);
                pos = end;
            }
            if pos >= valid {
                break;
            }
            let mut b = run[pos];
            // Bulk CSI fast path: `ESC [ params final` — scan for the final
            // byte and parse params (plus a leading `?`/`>`/`=`/`<` private
            // marker) in one pass. Anything else — a `0x20..=0x2f`
            // intermediate, a private marker after params, C0/DEL, or
            // high-bit — bails to the state machine, which enforces the
            // CsiIntermediate/CsiParam → CsiIgnore transitions.
            if b == 0x1b && run.get(pos + 1) == Some(&b'[') {
                let seq = &run[pos + 2..valid];
                self.params_len = 0;
                self.param = 0;
                self.param_empty = true;
                self.sep = 0;
                self.intermediates_len = 0;
                let mut ok = true;
                let mut f = 0_usize;
                while f < seq.len() {
                    match seq[f] {
                        b'0'..=b'9' => {
                            self.param =
                                (self.param * 10 + (seq[f] - b'0') as i32).min(MAX_PARAM_VALUE);
                            self.param_empty = false;
                            f += 1;
                        }
                        b':' => {
                            // Empty param becomes the `-1` sentinel.
                            self.push_param(if self.param_empty { -1 } else { self.param });
                            if self.params_len < 16 {
                                self.sep |= 1 << (self.params_len - 1);
                            }
                            self.param = 0;
                            self.param_empty = true;
                            f += 1;
                        }
                        b';' => {
                            self.push_param(self.param);
                            self.param = 0;
                            self.param_empty = true;
                            f += 1;
                        }
                        0x3c..=0x3f => {
                            // Private marker (`> = < ?`) — only valid as a CSI
                            // prefix (CsiEntry), before any param or marker.
                            // After params (CsiParam) the DEC state machine
                            // transitions to CsiIgnore and drops the whole
                            // sequence, so bail to the cold path.
                            if self.params_len != 0
                                || !self.param_empty
                                || self.intermediates_len != 0
                            {
                                ok = false;
                                break;
                            }
                            self.push_intermediate(seq[f]);
                            f += 1;
                        }
                        0x40..=0x7e => break, // final byte
                        _ => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok && f < seq.len() {
                    // A param-less CSI (`CSI H`, `CSI m`, `CSI s`) dispatches
                    // `[]`; a trailing empty param (e.g. ConPTY's `SGR …;m`)
                    // is pushed as `-1`.
                    if !self.param_empty || self.params_len != 0 {
                        self.push_param(if self.param_empty { -1 } else { self.param });
                    }
                    p.csi_dispatch(
                        &self.params[..self.params_len],
                        self.sep,
                        &self.intermediates[..self.intermediates_len],
                        seq[f],
                    );
                    self.clear_seq_state();
                    // Keep scanning in this pass: CSI-dense input (SGR
                    // churn, cursor motion) pays a re-entry + a fresh
                    // `first_non_printable` scan per sequence otherwise.
                    pos += 2 + f + 1;
                    continue;
                }
            }
            match b {
                0x1b => {
                    // ESC → enter the escape state (the byte is consumed).
                    self.clear_seq_state();
                    self.state = ParseState::Escape;
                    return i + pos + 1;
                }
                0x00..=0x1f | 0x7f => {
                    // C0 controls (and DEL) often cluster — `\r\n`, `\t\t`,
                    // etc. Execute the whole run in a tight loop without
                    // re-scanning for printable bytes between them. ESC
                    // (0x1b) breaks out so the outer loop enters the escape
                    // state, not `execute`.
                    loop {
                        p.execute(b);
                        pos += 1;
                        if pos >= valid {
                            break;
                        }
                        b = run[pos];
                        if b == 0x1b || !(b == 0x7f || b < 0x20) {
                            break;
                        }
                    }
                }
                0x80..=0x9f => {
                    // Raw 8-bit C1: introducers start their sequence; ST is
                    // a no-op; other C1 controls execute (a lone C1 byte is
                    // invalid UTF-8, so this is the correct fallback).
                    // <https://vt100.net/emu/dec_ansi_parser> §"anywhere".
                    let (action, next) = Self::c1_transition(b);
                    self.apply(p, action, b);
                    if next != ParseState::Ground {
                        self.state = next;
                        return i + pos + 1;
                    }
                    pos += 1;
                }
                _ => {
                    // Multi-byte UTF-8 lead (>= 0xa0).
                    pos += decode_utf8(p, run, valid, pos, self.simd_enabled());
                }
            }
        }
        // Consume the whole run (including any carried-over partial bytes).
        i + run.len()
    }

    /// 8-bit C1 (0x80-0x9f) inside a non-ground state: abort the sequence and
    /// either terminate a string (ST `0x9c`) or execute/re-enter the
    /// introducer — DEC's "anywhere" invariant
    /// (<https://vt100.net/emu/dec_ansi_parser>).
    #[inline]
    fn c1_in_sequence(state: ParseState, b: u8) -> (Action, ParseState) {
        // String states end with their exit action on ST.
        match (state, b) {
            (ParseState::OscString, 0x9c) => (Action::OSC_END, ParseState::Ground),
            (ParseState::DcsPassthrough, 0x9c) => (Action::UNHOOK, ParseState::Ground),
            (ParseState::SosPmApcString, 0x9c) => (Action::NONE, ParseState::Ground),
            _ => Self::c1_transition(b),
        }
    }

    /// Raw 8-bit C1 → (action, next state). Introducers start their sequence;
    /// ST `0x9c` is a no-op; other C1 controls execute and return to ground
    /// (<https://vt100.net/emu/dec_ansi_parser>).
    #[inline]
    fn c1_transition(b: u8) -> (Action, ParseState) {
        match b {
            0x90 => (Action::NONE, ParseState::DcsEntry), // DCS
            0x98 => (Action::NONE, ParseState::SosPmApcString), // SOS
            0x9b => (Action::NONE, ParseState::CsiEntry), // CSI
            0x9c => (Action::NONE, ParseState::Ground),   // ST
            0x9d => (Action::OSC_START, ParseState::OscString), // OSC
            0x9e | 0x9f => (Action::NONE, ParseState::SosPmApcString), // PM/APC
            _ => (Action::EXECUTE, ParseState::Ground),   // other C1
        }
    }

    /// Escape-sequence states — rare, so kept cold and out of the hot loop.
    #[inline(never)]
    fn advance_non_ground<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let b = bytes[i];
        let state = self.state;

        // ESC aborts the current sequence and starts a new one fresh.
        if b == 0x1b {
            self.clear_seq_state();
        }

        // 8-bit C1: DEC's "anywhere" rule — abort the sequence, execute the
        // control, introducers re-enter their state (ST `0x9c` is a no-op).
        // <https://vt100.net/emu/dec_ansi_parser> §"anywhere".
        if (0x80..=0x9f).contains(&b) {
            self.clear_seq_state();
            let (action, next) = Self::c1_in_sequence(state, b);
            self.apply(p, action, b);
            self.state = next;
            return i + 1;
        }

        // The hot string terminators and ESC introducers are dispatched inline
        // from `advance` (see `advance_osc_run` / `advance_dcs_run` /
        // `advance_escape_byte`); this cold function only sees the rare bytes.

        // The CSI/DCS param run is dispatched inline from `advance` (see
        // `advance_param_run`); this function only sees the boundary byte.

        // CSI final-byte fast path: `0x40..=0x7e` from CsiEntry/CsiParam/
        // CsiIntermediate always dispatches (CSI_DISPATCH → Ground). Skip the
        // transition() table lookup for the one dispatch byte per sequence.
        if matches!(
            state,
            ParseState::CsiEntry | ParseState::CsiParam | ParseState::CsiIntermediate
        ) && (0x40..=0x7e).contains(&b)
        {
            // Same trailing-empty sentinel as the bulk/apply CSI paths, so a
            // param-less CSI split across writes still dispatches `[]` (not
            // `[0]`), and a trailing empty is `-1` (not an explicit `0`).
            if !self.param_empty || self.params_len != 0 {
                self.push_param(if self.param_empty { -1 } else { self.param });
            }
            let ints = &self.intermediates[..self.intermediates_len];
            p.csi_dispatch(&self.params[..self.params_len], self.sep, ints, b);
            self.clear_seq_state();
            self.state = ParseState::Ground;
            return i + 1;
        }

        // Parameter / intermediate accumulation for CSI, DCS and ESC states.
        match state {
            ParseState::CsiEntry
            | ParseState::CsiParam
            | ParseState::CsiIntermediate
            | ParseState::DcsEntry
            | ParseState::DcsParam
            | ParseState::DcsIntermediate => match b {
                0x20..=0x2f => {
                    self.push_intermediate(b);
                }
                b'0'..=b'9' => {
                    // DEC VT500 params are 16-bit; clamp to 0..=65535 (vte
                    // does the same) so a huge param can't diverge from the
                    // reference parser.
                    self.param = self
                        .param
                        .saturating_mul(10)
                        .saturating_add((b - b'0') as i32)
                        .min(MAX_PARAM_VALUE);
                    self.param_empty = false;
                }
                b':' => {
                    // Colon separator: mark the just-pushed param so `4:2`
                    // stays distinguishable from `4;2` (SGR underline style);
                    // an empty colon param (`::`) becomes `-1` so the SGR
                    // colorspace form `38:2::r:g:b` survives.
                    self.push_param(if self.param_empty { -1 } else { self.param });
                    if self.params_len < 16 {
                        self.sep |= 1 << (self.params_len - 1);
                    }
                    self.param = 0;
                    self.param_empty = true;
                }
                b';' => {
                    self.push_param(self.param);
                    self.param = 0;
                    self.param_empty = true;
                }
                b'<' | b'=' | b'>' | b'?' => {
                    self.push_intermediate(b);
                }
                _ => {}
            },
            ParseState::Escape | ParseState::EscapeIntermediate if (0x20..=0x2f).contains(&b) => {
                self.push_intermediate(b);
            }
            _ => {}
        }

        let (action, next) = transition(state, b);
        self.apply(p, action, b);
        self.state = next;
        i + 1
    }

    /// Escape-state byte dispatch: the sequence introducers (`[` CSI, `]` OSC,
    /// `P` DCS, `X`/`^`/`_` SOS/PM/APC), ESC finals, intermediates and C0
    /// controls are all handled inline; only the rare bytes (ESC restart, DEL,
    /// raw C1/high bytes) fall through to the cold per-byte state machine.
    #[inline(always)]
    fn advance_escape_byte<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        match bytes[i] {
            0x5b => {
                self.state = ParseState::CsiEntry;
                i + 1
            }
            0x5d => {
                self.osc.clear();
                self.osc_overflow = false;
                self.osc_num_starts = 1;
                self.osc_deferring = true;
                self.osc_base = i + 1;
                self.state = ParseState::OscString;
                i + 1
            }
            0x50 => {
                self.state = ParseState::DcsEntry;
                i + 1
            }
            0x58 | 0x5e | 0x5f => {
                self.state = ParseState::SosPmApcString;
                i + 1
            }
            0x20..=0x2f => {
                self.push_intermediate(bytes[i]);
                self.state = ParseState::EscapeIntermediate;
                i + 1
            }
            // ESC final bytes (esc_dispatch), disjoint from the introducers
            // so the arms above are order-independent.
            0x30..=0x4f | 0x51..=0x57 | 0x59 | 0x5a | 0x5c | 0x60..=0x7e => {
                p.esc_dispatch(&[], bytes[i]);
                self.clear_seq_state();
                self.state = ParseState::Ground;
                i + 1
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                p.execute(bytes[i]);
                i + 1
            }
            0x18 | 0x1a => {
                p.execute(bytes[i]);
                self.state = ParseState::Ground;
                i + 1
            }
            _ => self.advance_non_ground(p, bytes, i),
        }
    }

    /// `DcsEntry` byte dispatch: the final byte (0x40..=0x7e) hooks inline;
    /// digits/separators/intermediates fall through to the cold state machine.
    #[inline(always)]
    fn advance_dcs_entry_byte<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let b = bytes[i];
        if (0x40..=0x7e).contains(&b) {
            // Same trailing-param convention as `apply(HOOK)`: a param-less
            // DCS hooks with `[]`, not `[0]`.
            if !self.param_empty || self.params_len != 0 {
                self.push_param(self.param);
            }
            let ints = &self.intermediates[..self.intermediates_len];
            p.hook(&self.params[..self.params_len], ints, b);
            self.clear_seq_state();
            self.state = ParseState::DcsPassthrough;
            return i + 1;
        }
        self.advance_non_ground(p, bytes, i)
    }

    /// OSC string bulk run: append the payload prefix (everything up to the
    /// next C0/C1 boundary) in one `extend_from_slice`. Dispatched inline
    /// from `advance` so the hot OSC copy path does not pay the cold
    /// `advance_non_ground` call + prologue; boundary bytes fall through.
    #[inline]
    fn advance_osc_run<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let run = &bytes[i..];
        let n = first_osc_boundary(run);
        if n > 0 {
            if !self.osc_deferring {
                self.osc_extend(&run[..n]);
            }
            return i + n;
        }
        // Boundary byte: dispatch the common OSC terminator/separator bytes
        // inline instead of through the cold table path.
        match bytes[i] {
            0x07 => {
                self.osc_end(p, true, bytes, i);
                self.state = ParseState::Ground;
                i + 1
            }
            0x1b => {
                self.clear_seq_state();
                self.osc_end(p, false, bytes, i);
                self.state = ParseState::Escape;
                i + 1
            }
            0x3b => {
                // The offset is `;`-free: subtract the separators already seen
                // so deferred (input) and accumulated (`osc`) modes agree.
                let offset = if self.osc_deferring {
                    (i - self.osc_base - (self.osc_num_starts - 1)) as u32
                } else {
                    self.osc.len() as u32
                };
                self.osc_param_boundary(offset);
                i + 1
            }
            _ => {
                if self.osc_deferring {
                    self.osc_flush(bytes, i);
                }
                self.advance_non_ground(p, bytes, i)
            }
        }
    }

    /// DCS passthrough bulk run: dispatch the payload prefix via one
    /// `put_slice`, stopping at CAN/SUB/ESC/DEL or any byte >= 0x80. Same
    /// inline-dispatch rationale as `advance_osc_run`.
    #[inline]
    fn advance_dcs_run<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let run = &bytes[i..];
        let n = first_dcs_boundary(run);
        if n > 0 {
            p.put_slice(&run[..n]);
            return i + n;
        }
        // Boundary byte: dispatch the common DCS terminator (ESC) inline.
        if bytes[i] == 0x1b {
            self.clear_seq_state();
            p.unhook();
            self.state = ParseState::Escape;
            return i + 1;
        }
        self.advance_non_ground(p, bytes, i)
    }

    /// CSI/DCS param run: digits, `;` and `:` never dispatch, so consume the
    /// whole run in a tight loop. Dispatched inline from `advance`; the
    /// boundary byte falls through to `advance_non_ground`.
    #[inline(always)]
    fn advance_param_run<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let mut j = i;
        while j < bytes.len() {
            match bytes[j] {
                b'0'..=b'9' => {
                    self.param = self
                        .param
                        .saturating_mul(10)
                        .saturating_add((bytes[j] - b'0') as i32)
                        .min(MAX_PARAM_VALUE);
                    self.param_empty = false;
                    j += 1;
                }
                b':' => {
                    self.push_param(if self.param_empty { -1 } else { self.param });
                    if self.params_len < 16 {
                        self.sep |= 1 << (self.params_len - 1);
                    }
                    self.param = 0;
                    self.param_empty = true;
                    j += 1;
                }
                b';' => {
                    self.push_param(self.param);
                    self.param = 0;
                    self.param_empty = true;
                    j += 1;
                }
                _ => break,
            }
        }
        if j > i {
            return j;
        }
        self.advance_non_ground(p, bytes, i)
    }

    /// Record the start of the next OSC param. `;` separators are not buffered
    /// into `osc`, so consecutive ranges merge at their boundary. Params past
    /// the 16th are dropped.
    #[inline]
    fn osc_param_boundary(&mut self, offset: u32) {
        if self.osc_num_starts > MAX_OSC_PARAMS {
            return;
        }
        self.osc_starts[self.osc_num_starts] = offset;
        self.osc_num_starts += 1;
    }

    /// Whether the OSC payload has hit [`MAX_OSC_PAYLOAD`] and must be
    /// discarded wholesale.
    #[inline(always)]
    const fn osc_overflow(&self) -> bool {
        self.osc_overflow
    }

    /// Append an OSC payload run, capping at [`MAX_OSC_PAYLOAD`] and flagging
    /// overflow when the run is truncated (or was already overflowed).
    #[inline(always)]
    fn osc_extend(&mut self, run: &[u8]) {
        if self.osc_overflow {
            return;
        }
        let room = MAX_OSC_PAYLOAD - self.osc.len();
        let take = run.len().min(room);
        self.osc.extend_from_slice_copy(&run[..take]);
        self.osc_overflow = take < run.len();
    }

    /// Append one OSC payload byte, discarding (marking overflow) at the cap.
    #[inline(always)]
    fn osc_push(&mut self, byte: u8) {
        self.osc_extend(&[byte]);
    }

    /// Dispatch a collected OSC command and reset the OSC state. An oversized
    /// payload is discarded wholesale (a truncated OSC 52 would otherwise
    /// decode into garbage). With `bytes`/`end_pos` the params are sliced
    /// straight from the input when deferred; otherwise from `osc`.
    #[inline]
    fn osc_end<P: Perform>(&mut self, p: &mut P, bell: bool, bytes: &[u8], end_pos: usize) {
        const EMPTY: [&[u8]; MAX_OSC_PARAMS] = [&[]; MAX_OSC_PARAMS];
        let deferring = self.osc_deferring;
        let n = self.osc_num_starts;
        // The cap applies to the `;`-free payload bytes. In deferred mode the
        // input still holds the `n - 1` `;` separators, so subtract them.
        let payload_len = if deferring {
            end_pos - self.osc_base - (n - 1)
        } else {
            self.osc.len()
        };
        if !self.osc_overflow() && payload_len <= MAX_OSC_PAYLOAD {
            let m = n.min(MAX_OSC_PARAMS);
            // Unify the deferred (input-sliced) and accumulated (`osc`-sliced)
            // cases: the input holds the `;` separators, so each `;`-free
            // boundary offset shifts by `k` and the source is `bytes` rather
            // than `osc`.
            let base = if deferring { self.osc_base } else { 0 };
            let shift = deferring as usize;
            let last_end = if deferring { end_pos } else { self.osc.len() };
            let src: &[u8] = if deferring { bytes } else { &self.osc };
            let mut params = EMPTY;
            for (k, dst) in params.iter_mut().enumerate().take(m) {
                let start = base + self.osc_starts[k] as usize + k * shift;
                let end = if k + 1 < n {
                    base + self.osc_starts[k + 1] as usize + k * shift
                } else {
                    last_end
                };
                *dst = &src[start..end];
            }
            p.osc_dispatch(&params[..m], bell);
        }
        self.osc_reset();
    }

    /// Reset the OSC accumulation/dispatch state. `osc.clear()` keeps the heap
    /// allocation (rio-vt does the same); `mem::take` would free it and force
    /// a malloc on the next >56-byte payload.
    #[inline(always)]
    fn osc_reset(&mut self) {
        self.osc.clear();
        self.osc_overflow = false;
        self.osc_num_starts = 0;
        self.osc_deferring = false;
    }

    /// Flush a deferred OSC payload into `osc` (the string spans `advance`
    /// calls or hits a cold terminator). The recorded `osc_starts` offsets stay
    /// valid — they become `osc`-relative instead of input-relative.
    #[inline]
    fn osc_flush(&mut self, bytes: &[u8], end_pos: usize) {
        // Bulk-copy the `;`-free segments instead of pushing byte-by-byte.
        for part in bytes[self.osc_base..end_pos].split(|&b| b == 0x3b) {
            self.osc_extend(part);
        }
        self.osc_deferring = false;
    }

    #[inline(always)]
    fn apply<P: Perform>(&mut self, p: &mut P, action: Action, byte: u8) {
        if action.is_empty() {
            return;
        }
        if action.contains(Action::PRINT) {
            p.print(byte as char);
        }
        if action.contains(Action::CSI_DISPATCH) {
            // A param-less CSI dispatches `[]` (not `[0]`); a trailing empty is
            // the `-1` sentinel, so SGR can ignore it instead of resetting.
            if !self.param_empty || self.params_len != 0 {
                self.push_param(if self.param_empty { -1 } else { self.param });
            }
            let ints = &self.intermediates[..self.intermediates_len];
            p.csi_dispatch(&self.params[..self.params_len], self.sep, ints, byte);
            self.clear_seq_state();
        }
        if action.contains(Action::ESC_DISPATCH) {
            let ints = &self.intermediates[..self.intermediates_len];
            p.esc_dispatch(ints, byte);
            self.clear_seq_state();
        }
        if action.contains(Action::HOOK) {
            if !self.param_empty || self.params_len != 0 {
                self.push_param(self.param);
            }
            let ints = &self.intermediates[..self.intermediates_len];
            p.hook(&self.params[..self.params_len], ints, byte);
            self.clear_seq_state();
        }
        if action.contains(Action::PUT) {
            p.put(byte);
        }
        if action.contains(Action::UNHOOK) {
            p.unhook();
        }
        if action.contains(Action::OSC_START) {
            self.osc.clear();
            self.osc_overflow = false;
            self.osc_num_starts = 1;
            self.osc_deferring = false;
        }
        if action.contains(Action::OSC_PUT) {
            self.osc_push(byte);
        }
        if action.contains(Action::OSC_END) {
            // Cold terminator (C1 ST / CAN / SUB): the deferred payload was
            // already flushed by the `advance_osc_run` fallback, so dispatch
            // from `osc`.
            self.osc_end(p, byte == 0x07, &[], 0);
        }
        // EXECUTE last: a C0 control that also terminates a string (CAN/SUB in
        // OSC/DCS passthrough) dispatches the string first, then executes —
        // matching vte's order.
        if action.contains(Action::EXECUTE) {
            p.execute(byte);
        }
    }

    fn clear_seq_state(&mut self) {
        self.params_len = 0;
        self.sep = 0;
        self.param = 0;
        self.param_empty = true;
        self.intermediates_len = 0;
    }

    const fn push_intermediate(&mut self, b: u8) {
        if self.intermediates_len >= self.intermediates.len() {
            return;
        }
        self.intermediates[self.intermediates_len] = b;
        self.intermediates_len += 1;
    }

    /// Push a CSI/DCS parameter, capping at [`MAX_PARAM_COUNT`]. A malicious
    /// stream of thousands of `;`/`:` must not grow the param vector without
    /// bound — past the cap, params are dropped (the first 32 are kept,
    /// xterm-style). vte and rio-vt both use 32.
    #[inline]
    fn push_param(&mut self, value: i32) {
        if self.params_len >= MAX_PARAM_COUNT {
            return;
        }
        self.params[self.params_len] = value;
        self.params_len += 1;
    }
}

/// Cap on CSI/DCS parameters (vte/rio-vt = 32, xterm = 16).
const MAX_PARAM_COUNT: usize = 32;

/// Max CSI/DCS parameter value: DEC VT500 params are 16-bit (`0..=65535`).
const MAX_PARAM_VALUE: i32 = 65535;

#[cfg(test)]
mod tests {
    use super::Parser;

    #[test]
    fn reset_preserves_simd_config() {
        let mut p = Parser::new();
        assert!(!p.simd_enabled());
        p.reset();
        assert!(!p.simd_enabled());

        #[cfg(feature = "simd")]
        {
            let mut q = Parser::with_simd();
            assert!(q.simd_enabled());
            q.reset();
            assert!(q.simd_enabled(), "reset must preserve the SIMD config");
        }
    }
}
