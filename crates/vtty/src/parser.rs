// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The byte-input VT parser: the `Parser` state machine. The
//! [`crate::Perform`] contract and the owned [`crate::Event`] API are
//! re-exported from `vtty_core`; the UTF-8 incremental decoder lives in
//! `utf8`, the SWAR byte scanners in `scan`.

use alloc::vec::Vec;

use smallvec::SmallVec;

use vtty_core::{Event, Perform, Recording, set_param_colon_sep};

use crate::scan::{first_dcs_boundary, first_non_printable, first_osc_boundary};
use crate::table::{Action, ParseState, transition};
use crate::utf8::{PartialUtf8, decode_utf8, trailing_partial};

/// Maximum buffered OSC payload. A hostile program can otherwise stream a
/// gigabyte into `ESC ] … ST` and OOM the parser; xterm.js caps OSC/DCS at
/// `PAYLOAD_LIMIT = 10000000` and aborts the sequence, so we bound here too.
/// 1 MiB comfortably fits the largest real OSC (`OSC 52` clipboard ≈ 128 KiB).
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
    /// Where the current OSC payload lives: deferred in the `advance` input
    /// slice (no copy into `osc` yet) or accumulated.
    osc_buffer: OscBuffer,
    /// A UTF-8 code point split across `advance` calls (lead + already-arrived
    /// continuation bytes held until the rest arrive).
    partial: PartialUtf8,
    /// Runtime SIMD toggle: when `true`, the printable-run scan and UTF-8
    /// validation use SIMD. Only present (and only settable, via
    /// [`Parser::with_simd`]) when the `simd` feature is compiled in.
    #[cfg(feature = "simd")]
    simd: bool,
    /// Test-only: route the escape-sequence states through the transition
    /// table (skipping the inline hot-path handlers) so tests can
    /// differential the table against the optimized paths.
    #[cfg(test)]
    cold: bool,
}

/// Where the current OSC payload lives: still entirely within the current
/// `advance` input slice (deferred, no copy) or accumulated into `osc`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OscBuffer {
    Deferred {
        base: usize,
    },
    #[default]
    Accumulated,
}

impl OscBuffer {
    /// Whether the payload is deferred (still in the input slice).
    #[inline]
    const fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred { .. })
    }
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
        osc_buffer: OscBuffer::Accumulated,
        partial: PartialUtf8::new(),
        #[cfg(feature = "simd")]
        simd: false,
        #[cfg(test)]
        cold: false,
    };

    /// An empty parser in the ground state (no allocation).
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// An empty parser in the ground state with SIMD fast paths enabled.
    /// No allocation.
    #[cfg(feature = "simd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "simd")))]
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

    /// Whether the inline hot-path handlers are disabled (test-only cold
    /// mode). Always `false` outside `cfg(test)`.
    #[cfg(test)]
    #[inline]
    const fn force_cold(&self) -> bool {
        self.cold
    }

    #[cfg(not(test))]
    #[inline]
    const fn force_cold(&self) -> bool {
        false
    }

    /// Parse bytes, collecting every dispatch into a `Vec<Event>` — the
    /// convenience enum API; [`Self::advance`] + [`Perform`] is the
    /// zero-allocation hot path. Unbounded `Vec`, so tests/tooling only.
    #[must_use]
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
        if self.partial.pending() {
            // A consumed continuation byte either completes the code point
            // (emit it) or leaves more to arrive. A non-continuation byte
            // aborts the partial (emit U+FFFD once) and is left for the main
            // loop.
            while i < bytes.len() {
                if !self.partial.feed(bytes[i]) {
                    p.print(char::REPLACEMENT_CHARACTER);
                    break;
                }
                i += 1;
                if !self.partial.pending() {
                    // Test the numeric code point first: the control ranges
                    // never need a `char`, so convert only on the print path.
                    let cp = self.partial.codepoint();
                    if matches!(cp, 0x00..=0x1f | 0x7f | 0x80..=0x9f) {
                        p.execute(cp as u8);
                    } else {
                        p.print(char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER));
                    }
                    break;
                }
            }
        }

        // Detect a trailing partial code point once here, handing the partial-
        // free prefix length to the ground loop (avoids re-scanning per entry).
        let valid_end = match trailing_partial(bytes) {
            Some((v, partial)) => {
                self.partial = partial;
                v
            }
            None => bytes.len(),
        };

        while i < bytes.len() {
            match self.state {
                ParseState::Ground => i = self.advance_ground(p, bytes, i, valid_end),
                ParseState::OscString => i = self.advance_osc_run(p, bytes, i),
                ParseState::DcsPassthrough => i = self.advance_dcs_run(p, bytes, i),
                ParseState::Escape if !self.force_cold() => {
                    i = self.advance_escape_byte(p, bytes, i);
                }
                ParseState::DcsEntry if !self.force_cold() => {
                    i = self.advance_dcs_entry_byte(p, bytes, i);
                }
                ParseState::CsiParam | ParseState::DcsParam => {
                    i = self.advance_param_run(p, bytes, i);
                }
                _ => i = self.advance_non_ground(p, bytes, i),
            }
        }

        // An OSC payload that reached the end of this input chunk without a
        // terminator is flushed into `osc` so the next `advance` continues it.
        if self.state == ParseState::OscString && self.osc_buffer.is_deferred() {
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
        if !self.partial.pending() {
            return;
        }
        self.partial.clear();
        p.print(char::REPLACEMENT_CHARACTER);
    }

    /// Ground state: SIMD-scan for the next non-printable byte (control OR
    /// high-bit — which includes `ESC`), bulk-dispatch printable ASCII runs via
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
            self.partial.clear();
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
            if !self.force_cold() && b == 0x1b && run.get(pos + 1) == Some(&b'[') {
                // Keep scanning in this pass on success: CSI-dense input
                // (SGR churn, cursor motion) pays a re-entry + a fresh
                // `first_non_printable` scan per sequence otherwise.
                if let Some(consumed) = self.try_dispatch_csi(p, &run[pos + 2..valid]) {
                    pos += 2 + consumed;
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

    /// Parse a `ESC [` CSI sequence from `seq` (the bytes after `ESC [`),
    /// dispatching inline. Returns the number of bytes consumed, or `None`
    /// to bail to the state machine (an intermediate, a private marker after
    /// params, C0/DEL, or a high bit).
    ///
    /// The sequence state is already empty on entry (ground-state invariant:
    /// any prior sequence cleared it on dispatch, and the caller clears on
    /// bail), so there is no up-front clear — the success path's
    /// `dispatch_csi` and the bail path's caller both reset it.
    #[inline(always)]
    fn try_dispatch_csi<P: Perform>(&mut self, p: &mut P, seq: &[u8]) -> Option<usize> {
        // Fast CUP (`CSI r;c H` / `f`, the cursor-motion repaint shape):
        // parse the two 1-based params directly, skipping the per-byte param
        // dispatch and the trailing-param push. The extended-color SGR prefix
        // (`38;`/`48;`) is the dominant non-CUP shape — bail immediately (a
        // CUP row 38/48 is rare and falls back to the general path).
        if let Some(&first) = seq.first()
            && first.is_ascii_digit()
            && !(matches!(first, b'3' | b'4') && seq.get(1) == Some(&b'8'))
        {
            let mut i = 0;
            let mut row = 0_i32;
            while i < seq.len() && seq[i].is_ascii_digit() {
                row = (row * 10 + (seq[i] - b'0') as i32).min(MAX_PARAM_VALUE);
                i += 1;
            }
            if seq.get(i) == Some(&b';') {
                i += 1;
                let mut col = 0_i32;
                while i < seq.len() && seq[i].is_ascii_digit() {
                    col = (col * 10 + (seq[i] - b'0') as i32).min(MAX_PARAM_VALUE);
                    i += 1;
                }
                if seq.get(i).is_some_and(|&b| b == b'H' || b == b'f') {
                    self.params[0] = row;
                    self.params[1] = col;
                    self.params_len = 2;
                    let ints = &self.intermediates[..self.intermediates_len];
                    p.csi_dispatch(&self.params[..2], 0, ints, seq[i]);
                    self.clear_seq_state();
                    return Some(i + 1);
                }
            }
        }
        let mut f = 0_usize;
        while f < seq.len() {
            if self.param_byte(seq[f]) {
                f += 1;
                continue;
            }
            match seq[f] {
                0x3c..=0x3f => {
                    // Private marker (`> = < ?`) — only valid as a CSI prefix
                    // (CsiEntry), before any param or marker. After params the
                    // DEC state machine transitions to CsiIgnore and drops the
                    // whole sequence, so bail to the cold path.
                    if self.params_len != 0 || !self.param_empty || self.intermediates_len != 0 {
                        return None;
                    }
                    self.push_intermediate(seq[f]);
                    f += 1;
                }
                0x40..=0x7e => {
                    // A param-less CSI (`CSI H`, `CSI m`, `CSI s`) dispatches
                    // `[]`; a trailing empty param (e.g. ConPTY's `SGR …;m`)
                    // is pushed as `-1`.
                    self.dispatch_csi(p, seq[f]);
                    return Some(f + 1);
                }
                _ => return None,
            }
        }
        None
    }

    /// Raw 8-bit C1 → (action, next state). Introducers start their sequence;
    /// `ST` `0x9c` is a no-op; other C1 controls execute and return to ground
    /// (<https://vt100.net/emu/dec_ansi_parser>).
    #[inline]
    const fn c1_transition(b: u8) -> (Action, ParseState) {
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

        // 8-bit C1: DEC's "anywhere" rule — a C1 control cancels any escape
        // sequence, control sequence or control string in progress. A string
        // state's exit action (`osc_end` / `unhook`) fires first — the
        // deferred OSC payload was already flushed by `advance_osc_run` —
        // then the control is executed or an introducer re-enters its state.
        // <https://vt100.net/emu/dec_ansi_parser> §"anywhere".
        if (0x80..=0x9f).contains(&b) {
            match state {
                ParseState::OscString => self.osc_end(p, false, bytes, i),
                ParseState::DcsPassthrough => p.unhook(),
                _ => {}
            }
            self.clear_seq_state();
            let (action, next) = Self::c1_transition(b);
            self.apply(p, action, b);
            self.state = next;
            return i + 1;
        }

        // String runs, ESC introducers, and CSI/DCS param runs are dispatched
        // inline from `advance`; this cold path only sees rare and boundary
        // bytes.

        // CSI final-byte fast path: `0x40..=0x7e` from CsiEntry/CsiParam/
        // CsiIntermediate always dispatches (CSI_DISPATCH → Ground). Skip the
        // transition() table lookup for the one dispatch byte per sequence.
        if !self.force_cold()
            && matches!(
                state,
                ParseState::CsiEntry | ParseState::CsiParam | ParseState::CsiIntermediate
            )
            && (0x40..=0x7e).contains(&b)
        {
            // Same trailing-empty sentinel as the bulk/apply CSI paths.
            self.dispatch_csi(p, b);
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
                b'0'..=b'9' => self.param_digit(b),
                b':' => self.param_colon(),
                b';' => self.param_semicolon(),
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
        // Returning to ground ends the sequence: clear any collected
        // params/intermediates so a later bulk-CSI fast path (which assumes
        // a clean ground state) cannot inherit a stale, ignored sequence's
        // state. Dispatch actions already clear via dispatch_csi/esc/hook,
        // so this only matters for ignored/aborted sequences.
        if next == ParseState::Ground {
            self.clear_seq_state();
        }
        i + 1
    }

    /// Escape-state byte dispatch: the sequence introducers (`[` CSI, `]` OSC,
    /// `P` DCS, `X`/`^`/`_` `SOS`/`PM`/`APC`), `ESC` finals, intermediates and C0
    /// controls are all handled inline; only the rare bytes (`ESC` restart, `DEL`,
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
                self.osc_buffer = OscBuffer::Deferred { base: i + 1 };
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
                self.dispatch_esc(p, bytes[i]);
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
            self.dispatch_hook(p, b);
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
        let n = first_osc_boundary(run, self.simd_enabled());
        if n > 0 {
            if !self.osc_buffer.is_deferred() {
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
                let offset = if let OscBuffer::Deferred { base } = self.osc_buffer {
                    (i - base - (self.osc_num_starts - 1)) as u32
                } else {
                    self.osc.len() as u32
                };
                self.osc_param_boundary(offset);
                i + 1
            }
            _ => {
                if self.osc_buffer.is_deferred() {
                    self.osc_flush(bytes, i);
                }
                self.advance_non_ground(p, bytes, i)
            }
        }
    }

    /// DCS passthrough bulk run: dispatch the payload prefix via one
    /// `put_slice`, stopping at `CAN`/`SUB`/`ESC`/`DEL` or any byte >= 0x80. Same
    /// inline-dispatch rationale as `advance_osc_run`.
    #[inline]
    fn advance_dcs_run<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let run = &bytes[i..];
        let n = first_dcs_boundary(run, self.simd_enabled());
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
        while j < bytes.len() && self.param_byte(bytes[j]) {
            j += 1;
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
    const fn osc_param_boundary(&mut self, offset: u32) {
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

    /// Dispatch a collected OSC command and reset the OSC state. An oversized
    /// payload is discarded wholesale (a truncated `OSC 52` would otherwise
    /// decode into garbage). With `bytes`/`end_pos` the params are sliced
    /// straight from the input when deferred; otherwise from `osc`.
    #[inline]
    fn osc_end<P: Perform>(&mut self, p: &mut P, bell: bool, bytes: &[u8], end_pos: usize) {
        let n = self.osc_num_starts;
        // The cap applies to the `;`-free payload bytes. In deferred mode the
        // input still holds the `n - 1` `;` separators, so subtract them.
        let payload_len = match self.osc_buffer {
            OscBuffer::Deferred { base } => end_pos - base - (n - 1),
            OscBuffer::Accumulated => self.osc.len(),
        };
        if self.osc_overflow() || payload_len > MAX_OSC_PAYLOAD {
            self.osc_reset();
            return;
        }
        // Unify the deferred (input-sliced) and accumulated (`osc`-sliced)
        // cases: the input holds the `;` separators, so each `;`-free
        // boundary offset shifts by `k` and the source is `bytes` rather
        // than `osc`.
        let (src, base, shift, last_end) = match self.osc_buffer {
            OscBuffer::Deferred { base } => (bytes, base, 1, end_pos),
            OscBuffer::Accumulated => (&self.osc[..], 0, 0, self.osc.len()),
        };
        let m = n.min(MAX_OSC_PARAMS);
        let mut params = [&[][..]; MAX_OSC_PARAMS];
        // The common 1-3 param shapes (title/clipboard/iTerm2) unrolled; the
        // longer tail falls back to the loop.
        if m >= 1 {
            let s0 = base + self.osc_starts[0] as usize;
            let e0 = if 1 < n {
                base + self.osc_starts[1] as usize
            } else {
                last_end
            };
            params[0] = &src[s0..e0];
        }
        if m >= 2 {
            let s1 = base + self.osc_starts[1] as usize + shift;
            let e1 = if 2 < n {
                base + self.osc_starts[2] as usize + shift
            } else {
                last_end
            };
            params[1] = &src[s1..e1];
        }
        if m >= 3 {
            let s2 = base + self.osc_starts[2] as usize + 2 * shift;
            let e2 = if 3 < n {
                base + self.osc_starts[3] as usize + 2 * shift
            } else {
                last_end
            };
            params[2] = &src[s2..e2];
        }
        for (k, dst) in params
            .iter_mut()
            .enumerate()
            .skip(3)
            .take(m.saturating_sub(3))
        {
            let start = base + self.osc_starts[k] as usize + k * shift;
            let end = if k + 1 < n {
                base + self.osc_starts[k + 1] as usize + k * shift
            } else {
                last_end
            };
            *dst = &src[start..end];
        }
        p.osc_dispatch(&params[..m], bell);
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
        self.osc_buffer = OscBuffer::Accumulated;
    }

    /// Flush a deferred OSC payload into `osc` (the string spans `advance`
    /// calls or hits a cold terminator). The recorded `osc_starts` offsets stay
    /// valid — they become `osc`-relative instead of input-relative.
    #[inline]
    fn osc_flush(&mut self, bytes: &[u8], end_pos: usize) {
        let base = match self.osc_buffer {
            OscBuffer::Deferred { base } => base,
            OscBuffer::Accumulated => return, // nothing deferred to flush
        };
        // Bulk-copy the `;`-free segments instead of pushing byte-by-byte.
        for part in bytes[base..end_pos].split(|&b| b == 0x3b) {
            self.osc_extend(part);
        }
        self.osc_buffer = OscBuffer::Accumulated;
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
            self.dispatch_csi(p, byte);
        }
        if action.contains(Action::ESC_DISPATCH) {
            self.dispatch_esc(p, byte);
        }
        if action.contains(Action::HOOK) {
            self.dispatch_hook(p, byte);
        }
        if action.contains(Action::PUT) {
            p.put(byte);
        }
        if action.contains(Action::UNHOOK) {
            p.unhook();
        }
        if action.contains(Action::OSC_START) {
            // Same reset as `osc_reset`, but the first param has already begun.
            self.osc_reset();
            self.osc_num_starts = 1;
        }
        if action.contains(Action::OSC_PUT) {
            self.osc_extend(&[byte]);
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

    const fn clear_seq_state(&mut self) {
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

    /// Push a CSI/DCS parameter, capping at [`MAX_PARAM_COUNT`]. Past the
    /// cap, params are dropped; vte and rio-vt both use 32.
    #[inline]
    const fn push_param(&mut self, value: i32) {
        if self.params_len >= MAX_PARAM_COUNT {
            return;
        }
        self.params[self.params_len] = value;
        self.params_len += 1;
    }

    /// Accumulate a decimal digit into the current parameter (16-bit clamp).
    ///
    /// `param` is always ≤ `MAX_PARAM_VALUE` (65535) on entry — every writer
    /// either zeroes it or caps it via `min` — so `param * 10 + digit` can
    /// never overflow i32, and plain arithmetic replaces the saturating ops.
    #[inline(always)]
    fn param_digit(&mut self, b: u8) {
        self.param = (self.param * 10 + (b - b'0') as i32).min(MAX_PARAM_VALUE);
        self.param_empty = false;
    }

    /// End a parameter on a `:` separator: push it (an empty one becomes the
    /// `-1` sentinel so SGR colon forms like `38:2::r:g:b` survive) and mark
    /// the `sep` bit so `4:2` stays distinguishable from `4;2`.
    #[inline(always)]
    const fn param_colon(&mut self) {
        self.push_param(if self.param_empty { -1 } else { self.param });
        set_param_colon_sep(&mut self.sep, self.params_len - 1);
        self.param = 0;
        self.param_empty = true;
    }

    /// End a parameter on a `;` separator. An empty one becomes the `-2`
    /// sentinel — distinct from the `-1` trailing/colon empty — so `CSI ; Pr
    /// s` (DECSLRM set-right margin) stays distinguishable from `CSI 0;Pr s`,
    /// and SGR can tell a leading empty (`;1m` = reset) from a trailing empty
    /// (`48;2;r;g;b;m` = ignore).
    #[inline(always)]
    const fn param_semicolon(&mut self) {
        self.push_param(if self.param_empty { -2 } else { self.param });
        self.param = 0;
        self.param_empty = true;
    }

    /// Accumulate one CSI/DCS param byte (digit/`:`/`;`). Returns `false` for
    /// any other byte so the caller can handle it.
    #[inline(always)]
    fn param_byte(&mut self, b: u8) -> bool {
        match b {
            b'0'..=b'9' => {
                self.param_digit(b);
                true
            }
            // `;` before `:` — semicolon-separated params dominate real
            // streams (SGR churn, cursor motion), so it wins the branch.
            b';' => {
                self.param_semicolon();
                true
            }
            b':' => {
                self.param_colon();
                true
            }
            _ => false,
        }
    }

    /// Push the trailing parameter before a dispatch. A param-less sequence
    /// stays empty; a trailing empty becomes the `-1` sentinel for CSI (so SGR
    /// can ignore it) but the raw value for DCS.
    #[inline]
    const fn push_trailing_param(&mut self, sentinel: bool) {
        if !self.param_empty || self.params_len != 0 {
            self.push_param(if sentinel && self.param_empty {
                -1
            } else {
                self.param
            });
        }
    }

    /// Dispatch a CSI with the collected params/intermediates.
    #[inline]
    fn dispatch_csi<P: Perform>(&mut self, p: &mut P, final_byte: u8) {
        self.push_trailing_param(true);
        let ints = &self.intermediates[..self.intermediates_len];
        p.csi_dispatch(&self.params[..self.params_len], self.sep, ints, final_byte);
        self.clear_seq_state();
    }

    /// Dispatch a DCS hook with the collected params/intermediates.
    #[inline]
    fn dispatch_hook<P: Perform>(&mut self, p: &mut P, final_byte: u8) {
        self.push_trailing_param(false);
        let ints = &self.intermediates[..self.intermediates_len];
        p.hook(&self.params[..self.params_len], ints, final_byte);
        self.clear_seq_state();
    }

    /// Dispatch an `ESC` sequence with the collected intermediates (empty in the
    /// plain Escape state, where no intermediate byte has been seen yet).
    #[inline]
    fn dispatch_esc<P: Perform>(&mut self, p: &mut P, final_byte: u8) {
        let ints = &self.intermediates[..self.intermediates_len];
        p.esc_dispatch(ints, final_byte);
        self.clear_seq_state();
    }
}

/// Cap on CSI/DCS parameters (vte/rio-vt = 32, xterm = 16).
const MAX_PARAM_COUNT: usize = 32;

/// Max CSI/DCS parameter value: DEC VT500 params are 16-bit (`0..=65535`).
const MAX_PARAM_VALUE: i32 = 65535;

#[cfg(test)]
mod tests {
    use super::{MAX_OSC_PAYLOAD, Parser};
    use crate::Event;
    use crate::Recording;
    use crate::table::{Action, ParseState};
    use alloc::vec;
    use alloc::vec::Vec;
    use smallvec::SmallVec;

    /// Canonical wire bytes of an event stream (concatenated `write_bytes`).
    fn events_bytes(events: &[Event]) -> Vec<u8> {
        let mut out = SmallVec::<u8, 64>::new();
        for event in events {
            event.write_bytes(&mut out);
        }
        out.to_vec()
    }

    /// The inline hot-path handlers (`advance_escape_byte`, the bulk CSI scan,
    /// and the CSI final-byte shortcut) re-implement `transition()` table
    /// entries. A parser with `cold = true` routes those states through the
    /// table instead; both must produce identical events.
    #[test]
    fn hot_and_cold_paths_agree() {
        let corpus: &[&[u8]] = &[
            b"\x1b[31m",
            b"\x1b[38;2;255;0;128m",
            b"\x1b[38:2::255;0;128m",
            b"\x1b[?25l",
            b"\x1b[?1049h",
            b"\x1b[2;4r",
            b"\x1b[1 2m",
            b"\x1b[1;31;44m",
            b"\x1b[H\x1b[2J",
            b"\x1bP1;2qpayload\x1b\\",
            b"\x1b_Gkitty\x1b\\",
            b"\x1b#8",
            b"\x1b7\x1b8",
            b"\x1b(0",
            b"\x1bD\x1bM\x1bE",
            b"text\x1b[31mred\x1b[0mmore",
            b"\x9b31m",
            b"\x1b]title\x07",
        ];
        for input in corpus {
            let hot = Parser::new().parse_events(input);
            let mut cold_parser = Parser::new();
            cold_parser.cold = true;
            let cold = cold_parser.parse_events(input);
            assert_eq!(
                events_bytes(&hot),
                events_bytes(&cold),
                "hot/cold divergence for {input:?}\nhot:  {hot:?}\ncold: {cold:?}"
            );
        }
    }

    /// DEC's "anywhere" invariant for 8-bit C1 controls
    /// (<https://vt100.net/emu/dec_ansi_parser>): introducers re-enter their
    /// state, `ST` `0x9c` is a no-op, and every other C1 control executes and
    /// returns to ground (a lone C1 byte is invalid UTF-8, so this is the
    /// correct fallback for a UTF-8 terminal).
    #[test]
    fn c1_transition_matches_dec_anywhere() {
        for b in 0x80..=0x9f {
            let (action, next) = Parser::c1_transition(b);
            let expected = match b {
                0x90 => (Action::NONE, ParseState::DcsEntry), // DCS
                0x98 => (Action::NONE, ParseState::SosPmApcString), // SOS
                0x9b => (Action::NONE, ParseState::CsiEntry), // CSI
                0x9c => (Action::NONE, ParseState::Ground),   // ST
                0x9d => (Action::OSC_START, ParseState::OscString), // OSC
                0x9e | 0x9f => (Action::NONE, ParseState::SosPmApcString), // PM/APC
                _ => (Action::EXECUTE, ParseState::Ground),
            };
            assert_eq!(
                (action, next),
                expected,
                "byte=0x{b:02x}: c1_transition={action:?},{next:?} expected={expected:?}"
            );
        }
    }

    /// End-to-end: a raw 8-bit C1 control interrupting an OSC string or DCS
    /// passthrough fires the string's exit action (`osc_end` / `unhook`)
    /// before the control is executed or an introducer re-enters its state —
    /// DEC's "anywhere" rule. Regression test: the exit action was previously
    /// dropped for every C1 control except `ST` (`0x9c`).
    #[test]
    fn c1_interrupt_fires_string_exit_action() {
        use crate::Event;

        // C1 controls that are not introducers (and not ST) execute and
        // return to ground; the introducers DCS/SOS/CSI/ST/OSC/PM/APC do not.
        fn executes(b: u8) -> usize {
            usize::from(matches!(b, 0x80..=0x8f | 0x91..=0x97 | 0x99 | 0x9a))
        }

        // OSC interrupted by each C1 control: the OSC dispatches (osc_end)
        // exactly once, then the control is executed (if it executes).
        for b in 0x80..=0x9f {
            let events = Parser::new().parse_events(&[0x1b, b']', b'0', b';', b'x', b]);
            let oscs = events
                .iter()
                .filter(|e| matches!(e, Event::OscDispatch { .. }))
                .count();
            assert_eq!(
                oscs, 1,
                "C1 0x{b:02x} interrupting OSC must dispatch exactly once: {events:?}"
            );
            let execs = events
                .iter()
                .filter(|e| matches!(e, Event::Execute(_)))
                .count();
            assert_eq!(execs, executes(b), "C1 0x{b:02x} execute count: {events:?}");
        }

        // DCS passthrough interrupted by each C1 control: unhook fires exactly
        // once, then the control is executed (if it executes).
        for b in 0x80..=0x9f {
            let events = Parser::new().parse_events(&[0x1b, b'P', b'q', b'x', b'y', b]);
            let unhooks = events
                .iter()
                .filter(|e| matches!(e, Event::DcsUnhook))
                .count();
            assert_eq!(
                unhooks, 1,
                "C1 0x{b:02x} interrupting DCS must unhook exactly once: {events:?}"
            );
            let execs = events
                .iter()
                .filter(|e| matches!(e, Event::Execute(_)))
                .count();
            assert_eq!(execs, executes(b), "C1 0x{b:02x} execute count: {events:?}");
        }
    }

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

    #[test]
    fn empty_input_returns_no_events() {
        assert!(Parser::new().parse_events(b"").is_empty());
    }

    #[test]
    fn osc_param_overflow_drops_excess() {
        let events = Parser::new()
            .parse_events(b"\x1b]0;1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19\x07");
        assert!(!events.is_empty());
    }

    #[test]
    fn c1_utf8_flushes_as_execute() {
        // U+0080 (C2 80) is a C1 control, dispatched as `execute`, not print.
        assert_eq!(
            Parser::new().parse_events(b"\xc2\x80"),
            vec![Event::Execute(0x80)]
        );
    }

    #[test]
    fn c0_controls_in_escape_state_execute() {
        // NUL stays in escape; CAN/SUB also return to ground.
        assert_eq!(
            Parser::new().parse_events(b"\x1b\x00"),
            vec![Event::Execute(0x00)]
        );
        assert_eq!(
            Parser::new().parse_events(b"\x1b\x18"),
            vec![Event::Execute(0x18)]
        );
    }

    #[test]
    fn c1_utf8_split_across_chunks_executes() {
        let mut p = Parser::new();
        let mut rec = Recording::new();
        p.advance(&mut rec, &[0xc2]);
        p.advance(&mut rec, &[0x80]);
        assert_eq!(rec.events, vec![Event::Execute(0x80)]);
    }

    #[test]
    fn multibyte_split_across_chunks_prints() {
        let mut p = Parser::new();
        let mut rec = Recording::new();
        p.advance(&mut rec, &[0xe4]);
        p.advance(&mut rec, &[0xb8, 0xad]); // 中
        assert_eq!(rec.events, vec![Event::Print('中')]);
    }

    #[test]
    fn dcs_dispatches_hook_put_unhook() {
        let events = Parser::new().parse_events(b"\x1bP1;2qab\x1b\\");
        // DcsHook, DcsPut(a), DcsPut(b), DcsUnhook, then EscDispatch for the ST final.
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn csi_with_middle_intermediate_is_ignored() {
        // A middle intermediate pushes the CSI into ignore mode (DEC CsiIgnore).
        assert!(Parser::new().parse_events(b"\x1b[1 2m").is_empty());
    }

    #[test]
    fn ignored_csi_clears_state_before_next_sequence() {
        // `? ) 5 h` collects `?`+`)`, then the digit pushes it into CsiIgnore
        // (no dispatch). The ignored sequence must not leak its collected
        // intermediates into the NEXT CSI's dispatch (fuzz regression).
        let events = Parser::new().parse_events(b"\x1b[?)5h\x1b[1;1H");
        assert_eq!(events.len(), 1, "{events:?}");
        let Event::CsiDispatch {
            params,
            sep,
            intermediates,
            final_byte,
        } = &events[0]
        else {
            unreachable!("length asserted as 1 above");
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], 1);
        assert_eq!(params[1], 1);
        assert_eq!(*sep, 0);
        assert!(intermediates.is_empty(), "{intermediates:?}");
        assert_eq!(*final_byte, b'H');
    }

    #[test]
    fn osc_payload_overflow_is_capped() {
        // A payload past MAX_OSC_PAYLOAD is capped and flagged, never panics.
        let mut input = Vec::with_capacity(MAX_OSC_PAYLOAD + 16);
        input.extend_from_slice(b"\x1b]0;");
        input.extend(core::iter::repeat_n(b'a', MAX_OSC_PAYLOAD + 1));
        input.push(0x07);
        let events = Parser::new().parse_events(&input);
        // An over-limit OSC is dropped wholesale, so it yields no events.
        assert!(events.is_empty());
    }
}
