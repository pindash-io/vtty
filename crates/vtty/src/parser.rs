// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The byte-input VT parser: the `Parser` state machine. The
//! [`crate::Perform`] contract and the owned [`crate::Event`] API come from
//! `vtty_core`; the UTF-8 incremental decoder lives in `utf8`, the SWAR byte
//! scanners in `scan`.

use alloc::vec::Vec;

use smallvec::SmallVec;

use vtty_core::{Event, Perform, Recording, set_param_colon_sep};

use crate::limits::{
    MAX_INTERMEDIATES, MAX_OSC_PARAMS, MAX_OSC_PAYLOAD, MAX_PARAM_COUNT, MAX_PARAM_VALUE,
};
use crate::scan::{
    first_dcs_boundary, first_non_printable, first_non_printable_word, first_osc_boundary,
};
use crate::table::{Action, ParseState, transition};
use crate::utf8::{PartialUtf8, decode_utf8, trailing_partial};

/// The byte-input VT parser: UTF-8 decoding plus per-codepoint dispatch.
///
/// CSI/DCS parameter accumulator: the current digit run, the collected
/// parameters and the colon-separator bits.
#[derive(Debug, PartialEq, Eq)]
struct Params {
    /// Current parameter being accumulated. DEC VT500 params are 16-bit
    /// (0..=65535); `i32` holds the clamped range plus the `-1` empty-param
    /// sentinel.
    param: i32,
    /// True while the current parameter is empty (no digits seen since the
    /// last `;`/`:`). An empty COLON parameter is pushed as `-1` so SGR colon
    /// forms (`38:2::r:g:b` — empty colorspace) stay distinguishable from a
    /// literal `0`.
    empty: bool,
    /// Collected parameters, capped at [`MAX_PARAM_COUNT`]. A fixed array plus
    /// a separate length makes clearing O(1).
    values: [i32; MAX_PARAM_COUNT],
    len: usize,
    /// Colon separators: bit `i` is set when parameter `i` was followed by
    /// `:` (rather than `;`). Only the first 16 params are tracked.
    sep: u16,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            param: 0,
            empty: true,
            values: [0; MAX_PARAM_COUNT],
            len: 0,
            sep: 0,
        }
    }
}

/// Collected CSI/DCS intermediates (private markers + `0x20..0x2f`). The DEC
/// spec caps these at two; extras are dropped.
#[derive(Debug, Default, PartialEq, Eq)]
struct Intermediates {
    bytes: [u8; MAX_INTERMEDIATES],
    len: usize,
}

/// OSC payload accumulator.
#[derive(Debug, Default, PartialEq, Eq)]
struct Osc {
    /// Accumulated payload bytes. Inline up to 56 — nvim's longest OSC is 54
    /// bytes (clipboard base64), so common OSC payloads stay inline.
    payload: SmallVec<u8, 56>,
    /// Set when the payload exceeds [`MAX_OSC_PAYLOAD`]; the whole OSC is
    /// then discarded.
    overflow: bool,
    /// Start offset of each OSC param in `payload` (the params are contiguous
    /// in the buffer — `;` separators are not stored). `starts[0]` is always
    /// 0; the extra slot holds the last param's end offset, so params 17+ are
    /// dropped.
    starts: [u32; MAX_OSC_PARAMS + 1],
    num_starts: usize,
    /// Where the current payload lives: deferred in the `advance` input slice
    /// (no copy into `payload` yet) or accumulated.
    buffer: OscBuffer,
}

/// A byte-input VT500 state machine that decodes terminal escape sequences
/// and dispatches them to a [`Perform`] sink.
///
/// `Parser` implements the DEC VT500 state machine from Paul Flo Williams'
/// [A parser for DEC's ANSI-compatible video terminals][dec-parser]. It turns
/// raw PTY bytes into a small set of typed callbacks: printable text, C0/C1
/// controls, and the parsed bodies of CSI, OSC, DCS and ESC sequences.
///
/// Parsing state is inline, so [`Parser::new`] never allocates. Reuse one
/// parser across calls; the only heap growth is an OSC payload past 56 bytes.
///
/// # Two dispatch styles
///
/// * [`Parser::advance`] feeds a [`Perform`] implementation — the
///   zero-allocation hot path, with no intermediate [`Event`] values.
/// * [`Parser::parse_events`] collects every dispatch into a `Vec<Event>` — the
///   trait-free convenience API for tests and tooling.
///
/// # Example
///
/// ```
/// use vtty::{Parser, Perform};
///
/// struct Screen;
///
/// impl Perform for Screen {
///     fn print_bytes(&mut self, _s: &[u8]) {}
///     fn csi_dispatch(
///         &mut self,
///         _params: &[i32],
///         _sep: u16,
///         _intermediates: &[u8],
///         _final_byte: u8,
///     ) {}
/// }
///
/// let mut parser = Parser::new();
/// parser.advance(&mut Screen, b"\x1b[1;31mhello\x1b[0m");
/// ```
///
/// # Streaming and chunking
///
/// `Parser` is incremental: bytes may arrive in arbitrarily small chunks, and
/// sequences split across calls to [`Parser::advance`] are reassembled. A
/// UTF-8 code point split across calls is held until the remaining bytes
/// arrive; an unterminated OSC payload is buffered until its terminator.
/// After the final [`Parser::advance`], call [`Parser::flush`] once so a
/// trailing incomplete code point is emitted as `U+FFFD`.
///
/// # Panics
///
/// The parser never panics on untrusted input: truncated sequences, invalid
/// UTF-8 and overlong payloads are handled by maximal subpart substitution
/// and the parser's hard caps. `unwrap`, `expect` and `panic` are denied
/// crate-wide.
///
/// [dec-parser]: https://vt100.net/emu/dec_ansi_parser
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parser {
    state: ParseState,
    params: Params,
    intermediates: Intermediates,
    osc: Osc,
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
/// `advance` input slice (deferred, no copy) or accumulated into `payload`.
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

/// Parse one decimal CSI parameter at `*i`, clamped to [`MAX_PARAM_VALUE`].
/// Absent digits read as `0`, matching what the accumulator yields for an
/// empty parameter.
#[cfg_attr(feature = "perf-inline", inline(always))]
const fn param_at(seq: &[u8], i: &mut usize) -> i32 {
    let mut v = 0_i32;
    while *i < seq.len() {
        let d = seq[*i];
        if !d.is_ascii_digit() {
            break;
        }
        v = v * 10 + (d - b'0') as i32;
        if v > MAX_PARAM_VALUE {
            v = MAX_PARAM_VALUE;
        }
        *i += 1;
    }
    v
}

impl Parser {
    /// An empty parser in the ground state.
    ///
    /// This is equivalent to [`Parser::new`], but can be used in a `const`
    /// context.
    pub const DEFAULT: Self = Self {
        state: ParseState::Ground,
        params: Params {
            param: 0,
            empty: true,
            values: [0; MAX_PARAM_COUNT],
            len: 0,
            sep: 0,
        },
        intermediates: Intermediates {
            bytes: [0; MAX_INTERMEDIATES],
            len: 0,
        },
        osc: Osc {
            payload: SmallVec::new(),
            overflow: false,
            starts: [0; MAX_OSC_PARAMS + 1],
            num_starts: 0,
            buffer: OscBuffer::Accumulated,
        },
        partial: PartialUtf8::new(),
        #[cfg(feature = "simd")]
        simd: false,
        #[cfg(test)]
        cold: false,
    };

    /// An empty parser in the ground state (no allocation).
    ///
    /// This is equivalent to [`Parser::DEFAULT`]. All intermediate parsing
    /// state lives inline, so constructing a parser never allocates. Use
    /// `Parser::with_simd` to also enable SIMD fast paths (when the `simd`
    /// feature is compiled in).
    ///
    /// # Example
    ///
    /// ```
    /// use vtty::Parser;
    ///
    /// let mut parser = Parser::new();
    /// parser.advance(&mut vtty::Recording::default(), b"hello");
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// An empty parser in the ground state with SIMD fast paths enabled.
    ///
    /// This is the same as [`Parser::new`], except the printable-run scan and
    /// UTF-8 validation use SIMD. There is no allocation.
    ///
    /// # Example
    ///
    /// ```
    /// use vtty::Parser;
    ///
    /// let mut parser = Parser::with_simd();
    /// parser.advance(&mut vtty::Recording::default(), b"\x1b[0m");
    /// ```
    #[cfg(feature = "simd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "simd")))]
    #[inline]
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

    /// Parse bytes, collecting every dispatch into a `Vec<Event>`.
    ///
    /// This is the trait-free convenience API: it wraps a [`Recording`] sink
    /// and returns the collected events. Because it allocates one [`Event`]
    /// per dispatch, prefer [`Parser::advance`] with a [`Perform`]
    /// implementation on the hot path.
    ///
    /// # Example
    ///
    /// ```
    /// use vtty::{Event, Parser};
    ///
    /// let mut parser = Parser::new();
    /// let events = parser.parse_events(b"\x1b[31mhi\x1b[0m");
    ///
    /// assert_eq!(events.len(), 3);
    /// assert_eq!(events[1], Event::PrintBytes(b"hi".to_vec()));
    /// ```
    #[must_use]
    pub fn parse_events(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut c = Recording::default();
        self.advance(&mut c, bytes);
        c.events
    }

    /// Feed a chunk of bytes to the parser, dispatching each decoded event to
    /// the sink `p`.
    ///
    /// This is the zero-allocation hot path: events are delivered directly to
    /// `p` without being materialized as [`Event`] values. See
    /// [`Parser::parse_events`] for the owned enum API.
    ///
    /// `advance` is incremental. Bytes may be split across calls at arbitrary
    /// boundaries: multi-byte UTF-8 code points and escape sequences that
    /// straddle a chunk boundary are reassembled transparently. After the
    /// final chunk, call [`Parser::flush`] once so that a trailing incomplete
    /// UTF-8 code point is emitted as `U+FFFD`.
    ///
    /// # Example
    ///
    /// A CSI sequence split across two calls is still one dispatch:
    ///
    /// ```
    /// use vtty::{Event, Parser, Recording};
    ///
    /// let mut parser = Parser::new();
    /// let mut events = Recording::default();
    /// parser.advance(&mut events, b"\x1b[3"); // CSI split here…
    /// parser.advance(&mut events, b"1mhi");   // …and completed here
    ///
    /// assert_eq!(events.events.len(), 2); // one CSI dispatch, one print run
    /// assert_eq!(events.events[1], Event::PrintBytes(b"hi".to_vec()));
    /// ```
    #[inline]
    pub fn advance<P: Perform>(&mut self, p: &mut P, bytes: &[u8]) {
        // An empty chunk cannot complete a pending partial nor dispatch.
        if bytes.is_empty() {
            return;
        }
        let mut i = 0;

        // Complete a partially-decoded UTF-8 code point carried over from the
        // previous call.
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
                if self.partial.pending() {
                    continue;
                }
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

        // Detect a trailing partial code point once, handing the partial-free
        // prefix length to the ground loop (avoids re-scanning per entry).
        let valid_end = match trailing_partial(bytes) {
            Some((v, partial)) => {
                self.partial = partial;
                v
            }
            None => bytes.len(),
        };

        while i < bytes.len() {
            match self.state {
                ParseState::Ground => {
                    i = self.advance_ground(p, bytes, i, valid_end);
                }
                ParseState::OscString if !self.force_cold() => {
                    i = self.advance_osc_run(p, bytes, i);
                }
                ParseState::DcsPassthrough if !self.force_cold() => {
                    i = self.advance_dcs_run(p, bytes, i);
                }
                ParseState::Escape if !self.force_cold() => {
                    i = self.advance_escape_byte(p, bytes, i);
                }
                ParseState::DcsEntry if !self.force_cold() => {
                    i = self.advance_dcs_entry_byte(p, bytes, i);
                }
                ParseState::CsiParam | ParseState::DcsParam if !self.force_cold() => {
                    i = self.advance_param_run(p, bytes, i);
                }
                _ => i = self.advance_non_ground(p, bytes, i),
            }
        }

        // An OSC payload that reached the end of this input chunk without a
        // terminator is flushed into `payload` so the next `advance` continues it.
        if self.state == ParseState::OscString && self.osc.buffer.is_deferred() {
            self.osc_flush(bytes, bytes.len());
        }
    }

    /// Reset to the ground state, clearing any pending partial code point and
    /// buffered OSC payload (no allocation).
    ///
    /// Use this to reuse a parser for a new input stream without leaking a
    /// partially assembled sequence into the next one. The SIMD toggle set by
    /// `Parser::with_simd` is preserved.
    ///
    /// # Example
    ///
    /// ```
    /// use vtty::{Event, Parser, Recording};
    ///
    /// let mut parser = Parser::new();
    /// let mut events = Recording::default();
    /// parser.advance(&mut events, b"\x1b[31"); // incomplete CSI
    /// parser.reset();
    /// parser.advance(&mut events, b"x");
    ///
    /// assert_eq!(events.events, vec![Event::PrintBytes(b"x".to_vec())]);
    /// ```
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
    /// at end-of-stream. Call once after the final [`Parser::advance`].
    ///
    /// # Example
    ///
    /// A complete multi-byte code point followed by an incomplete one:
    ///
    /// ```
    /// use vtty::{Event, Parser, Recording};
    ///
    /// let mut parser = Parser::new();
    /// let mut events = Recording::default();
    /// parser.advance(&mut events, &[0xE2, 0x82, 0xAC]); // U+20AC, complete
    /// parser.advance(&mut events, &[0xF0, 0x9F]);       // U+1F600, incomplete
    /// parser.flush(&mut events);
    ///
    /// assert_eq!(
    ///     events.events,
    ///     vec![Event::Print('\u{20AC}'), Event::Print('\u{FFFD}')],
    /// );
    /// ```
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
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn advance_ground<P: Perform>(
        &mut self,
        p: &mut P,
        bytes: &[u8],
        i: usize,
        valid_end: usize,
    ) -> usize {
        let run = &bytes[i..];
        // `valid` is the prefix before the trailing partial; an escape
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
                let mut end = pos + n;
                // Fused print+LF fast path: `printable\n` (raw LF) or
                // `printable\r\n` (OPOST) dispatch one `print_line`/
                // `print_crlf_line` call. The defaults delegate to
                // `print_bytes` + `execute`, so non-specializing screens
                // are unaffected.
                if end < valid && run[end] == b'\n' {
                    p.print_line(&run[pos..end]);
                    pos = end + 1;
                    continue;
                }
                if end + 1 < valid && run[end] == b'\r' && run[end + 1] == b'\n' {
                    // CRLF storm (`printable\r\n` repeated): SWAR-scan one
                    // 8-byte word per line and dispatch `print_crlf_line`
                    // without re-entering the general ground loop.
                    loop {
                        p.print_crlf_line(&run[pos..end]);
                        pos = end + 2;
                        if pos >= valid {
                            break;
                        }
                        let Some(n) = first_non_printable_word(&run[pos..valid]) else {
                            break; // run longer than one word (or no full word)
                        };
                        if n == 0 {
                            break; // non-printable at `pos`: general path
                        }
                        let next_end = pos + n;
                        if next_end + 1 >= valid
                            || run[next_end] != b'\r'
                            || run[next_end + 1] != b'\n'
                        {
                            // Not a CRLF line: dispatch the bytes and fall
                            // back to the general loop.
                            p.print_bytes(&run[pos..next_end]);
                            pos = next_end;
                            break;
                        }
                        end = next_end;
                    }
                    continue;
                }
                p.print_bytes(&run[pos..end]);
                pos = end;
            }
            if pos >= valid {
                break;
            }
            let mut b = run[pos];
            // Bulk CSI fast path: `ESC [ params final` — scan the final byte
            // and parse params (plus a leading `?`/`>`/`=`/`<` marker) in one
            // pass. Anything else (an intermediate, a private marker after
            // params, C0/DEL, high-bit) bails to the state machine, which
            // enforces CsiIntermediate/CsiParam → CsiIgnore.
            if !self.force_cold() && b == 0x1b && matches!(run.get(pos + 1), Some(&b'[')) {
                // Keep scanning on success: a re-entry costs a fresh
                // `first_non_printable` scan per sequence on CSI-dense input
                // (SGR churn, cursor motion).
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
                    // C0 controls (and DEL) often cluster (`\r\n`, `\t\t`):
                    // execute the run in a tight loop without re-scanning.
                    // ESC breaks out so the outer loop enters the escape state.
                    loop {
                        p.execute(b);
                        pos += 1;
                        if pos >= valid {
                            break;
                        }
                        b = run[pos];
                        if b == 0x1b || !matches!(b, 0x7f | ..0x20) {
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

    /// Dispatch `CSI a;b;c… m` — three or more parameters, the truecolor and
    /// multi-attribute SGR shapes. `k` points at the `;` after `second`.
    /// Returns the bytes consumed, or `None` when the shape is not a plain
    /// parameter list (an empty parameter, a `:` separator or an oversized
    /// list belongs to the byte loop).
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn dispatch_param_list<P: Perform>(
        &mut self,
        p: &mut P,
        seq: &[u8],
        mut k: usize,
        first: i32,
        second: i32,
    ) -> Option<usize> {
        self.params.values[0] = first;
        self.params.values[1] = second;
        let mut n = 2;
        while n < MAX_PARAM_COUNT && seq.get(k) == Some(&b';') {
            let mut next = k + 1;
            if !seq.get(next).is_some_and(u8::is_ascii_digit) {
                break;
            }
            self.params.values[n] = param_at(seq, &mut next);
            n += 1;
            k = next;
        }
        if seq.get(k) != Some(&b'm') {
            return None;
        }
        p.csi_dispatch(&self.params.values[..n], 0, &[], b'm');
        // Dispatch is the last thing this path does, so resetting the length
        // restores the ground-state invariant without a full `clear_seq_state`.
        self.params.len = 0;
        Some(k + 1)
    }

    /// Parse a `ESC [` CSI sequence from `seq`, dispatching inline. `seq` is
    /// the printable run that follows `ESC [`, so it can extend past the final
    /// byte; every read here is bounds-checked. Returns the number of bytes
    /// consumed, or `None` to bail to the state machine (an intermediate, a
    /// private marker after params, C0/DEL, or a high bit).
    ///
    /// The sequence state is already empty on entry (ground-state invariant:
    /// any prior sequence cleared it on dispatch, and the caller clears on
    /// bail), so there is no up-front clear — the success path's
    /// `dispatch_csi` and the bail path's caller both reset it.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn try_dispatch_csi<P: Perform>(&mut self, p: &mut P, seq: &[u8]) -> Option<usize> {
        if seq.is_empty() {
            return None;
        }

        // `CSI ? <digits> h|l` — DEC private mode set/reset, the shape shells
        // and TUIs emit around every prompt. The marker byte otherwise forces
        // the general parameter loop.
        if seq[0] == b'?' {
            let mut i = 1;
            if seq.get(i).is_some_and(u8::is_ascii_digit) {
                let mode = param_at(seq, &mut i);
                if let Some(&next @ (b'h' | b'l')) = seq.get(i) {
                    p.csi_dispatch(&[mode], 0, b"?", next);
                    return Some(i + 1);
                }
            }
        }

        // One or two parameters and a final byte carry almost every other CSI:
        // SGR (`CSI 31m`), cursor addressing (`CSI r;c H`), scroll margins
        // (`CSI 1;24 r`). A longer `;` list goes to `dispatch_param_list`; an
        // empty parameter or a `:` separator primes the accumulator and resumes
        // in the byte loop below.
        let mut i = 0;
        if seq[i].is_ascii_digit() {
            let first = param_at(seq, &mut i);

            if let Some(&next) = seq.get(i) {
                if matches!(next, 0x40..=0x7e) {
                    p.csi_dispatch(&[first], 0, &[], next);
                    return Some(i + 1);
                }
                if next == b';' {
                    let mut j = i + 1;
                    let col = param_at(seq, &mut j);
                    // A column needs at least one digit; `CSI r;H` reports the
                    // `-1` sentinel the loop would push for it.
                    if j > i + 1 && matches!(seq.get(j), Some(0x40..=0x7e)) {
                        p.csi_dispatch(&[first, col], 0, &[], seq[j]);
                        return Some(j + 1);
                    }
                    if seq.get(j) == Some(&b';')
                        && let Some(consumed) = self.dispatch_param_list(p, seq, j, first, col)
                    {
                        return Some(consumed);
                    }
                }
            }

            self.params.param = first;
            self.params.empty = false;
        }

        while i < seq.len() {
            if self.param_byte(seq[i]) {
                i += 1;
                continue;
            }
            match seq[i] {
                0x3c..=0x3f => {
                    // Private marker (`> = < ?`) — only valid as a CSI prefix
                    // (CsiEntry), before any param or marker. After params the
                    // DEC state machine transitions to CsiIgnore and drops the
                    // whole sequence, so bail to the cold path.
                    if self.params.len != 0 || !self.params.empty || self.intermediates.len != 0 {
                        return None;
                    }
                    self.push_intermediate(seq[i]);
                    i += 1;
                }
                0x40..=0x7e => {
                    // A param-less CSI (`CSI H`, `CSI m`, `CSI s`) dispatches
                    // `[]`; a trailing empty param (e.g. ConPTY's `SGR …;m`)
                    // is pushed as `-1`.
                    self.dispatch_csi(p, seq[i]);
                    return Some(i + 1);
                }
                _ => return None,
            }
        }
        None
    }

    /// Raw 8-bit C1 → (action, next state). Introducers start their sequence;
    /// `ST` `0x9c` is a no-op; other C1 controls execute and return to ground
    /// ([DEC parser state diagram]).
    ///
    /// [DEC parser state diagram]: https://vt100.net/emu/dec_ansi_parser
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

        // 8-bit C1 (DEC's "anywhere" rule): a C1 control cancels any sequence
        // in progress. A string state's exit action fires first (the deferred
        // OSC payload was already flushed), then the control executes or an
        // introducer re-enters its state.
        // <https://vt100.net/emu/dec_ansi_parser> §"anywhere".
        if matches!(b, 0x80..=0x9f) {
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
            && matches!(b, 0x40..=0x7e)
            && matches!(
                state,
                ParseState::CsiEntry | ParseState::CsiParam | ParseState::CsiIntermediate
            )
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
            ParseState::Escape | ParseState::EscapeIntermediate if matches!(b, 0x20..=0x2f) => {
                self.push_intermediate(b);
            }
            _ => {}
        }

        let (action, next) = transition(state, b);
        self.apply(p, action, b);
        self.state = next;
        // Returning to ground clears collected params/intermediates, so a
        // later bulk-CSI fast path (which assumes a clean ground state) cannot
        // inherit a stale, ignored sequence. Dispatch actions already clear;
        // this only matters for ignored/aborted sequences.
        if next == ParseState::Ground {
            self.clear_seq_state();
        }
        i + 1
    }

    /// Escape-state byte dispatch: the sequence introducers (`[` CSI, `]` OSC,
    /// `P` DCS, `X`/`^`/`_` `SOS`/`PM`/`APC`), `ESC` finals, intermediates and C0
    /// controls are all handled inline; only the rare bytes (`ESC` restart, `DEL`,
    /// raw C1/high bytes) fall through to the cold per-byte state machine.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn advance_escape_byte<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        match bytes[i] {
            0x5b => {
                self.state = ParseState::CsiEntry;
                i + 1
            }
            0x5d => {
                self.osc.payload.clear();
                self.osc.overflow = false;
                self.osc.num_starts = 1;
                self.osc.buffer = OscBuffer::Deferred { base: i + 1 };
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
            // `ESC <intermediate> <final>` — the 3-byte shape of charset
            // designation (`ESC ( B`), DECALN (`ESC # 8`) and UTF-8 select
            // (`ESC % G`). The cold `EscapeIntermediate` table path costs ~4 ns
            // more.
            0x20..=0x2f if matches!(bytes.get(i + 1), Some(0x30..=0x7e)) => {
                p.esc_dispatch(&bytes[i..i + 1], bytes[i + 1]);
                self.state = ParseState::Ground;
                i + 2
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
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn advance_dcs_entry_byte<P: Perform>(&mut self, p: &mut P, bytes: &[u8], i: usize) -> usize {
        let b = bytes[i];
        if matches!(b, 0x40..=0x7e) {
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
            if !self.osc.buffer.is_deferred() {
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
                // so deferred (input) and accumulated (`payload`) modes agree.
                let offset = if let OscBuffer::Deferred { base } = self.osc.buffer {
                    (i - base - (self.osc.num_starts - 1)) as u32
                } else {
                    self.osc.payload.len() as u32
                };
                self.osc_param_boundary(offset);
                i + 1
            }
            _ => {
                if self.osc.buffer.is_deferred() {
                    self.osc_flush(bytes, i);
                }
                self.advance_non_ground(p, bytes, i)
            }
        }
    }

    /// DCS passthrough bulk run: dispatch the payload prefix via one
    /// `put_slice`, stopping at `CAN`/`SUB`/`ESC`/`DEL` or any byte `> DEL`. Same
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
    #[cfg_attr(feature = "perf-inline", inline(always))]
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
    /// into `payload`, so consecutive ranges merge at their boundary. Params past
    /// the 16th are dropped.
    #[inline]
    const fn osc_param_boundary(&mut self, offset: u32) {
        if self.osc.num_starts > MAX_OSC_PARAMS {
            return;
        }
        self.osc.starts[self.osc.num_starts] = offset;
        self.osc.num_starts += 1;
    }

    /// Append an OSC payload run, capping at [`MAX_OSC_PAYLOAD`] and flagging
    /// overflow when the run is truncated (or was already overflowed).
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn osc_extend(&mut self, run: &[u8]) {
        if self.osc.overflow {
            return;
        }
        let room = MAX_OSC_PAYLOAD - self.osc.payload.len();
        let take = run.len().min(room);
        self.osc.payload.extend_from_slice_copy(&run[..take]);
        self.osc.overflow = take < run.len();
    }

    /// Dispatch a collected OSC command and reset the OSC state. An oversized
    /// payload is discarded wholesale (a truncated `OSC 52` would otherwise
    /// decode into garbage). With `bytes`/`end_pos` the params are sliced
    /// straight from the input when deferred; otherwise from `payload`.
    #[inline]
    fn osc_end<P: Perform>(&mut self, p: &mut P, bell: bool, bytes: &[u8], end_pos: usize) {
        let n = self.osc.num_starts;
        // The cap applies to the `;`-free payload bytes. In deferred mode the
        // input still holds the `n - 1` `;` separators, so subtract them.
        let payload_len = match self.osc.buffer {
            OscBuffer::Deferred { base } => end_pos - base - (n - 1),
            OscBuffer::Accumulated => self.osc.payload.len(),
        };
        if self.osc.overflow || payload_len > MAX_OSC_PAYLOAD {
            self.osc_reset();
            return;
        }
        // Unify the deferred (input-sliced) and accumulated (`payload`-sliced)
        // cases: the input holds the `;` separators, so each `;`-free
        // boundary offset shifts by `k` and the source is `bytes` rather
        // than `payload`.
        let (src, base, shift, last_end) = match self.osc.buffer {
            OscBuffer::Deferred { base } => (bytes, base, 1, end_pos),
            OscBuffer::Accumulated => (&self.osc.payload[..], 0, 0, self.osc.payload.len()),
        };
        let m = n.min(MAX_OSC_PARAMS);
        let mut params = [&[][..]; MAX_OSC_PARAMS];
        for (i, dst) in params.iter_mut().enumerate().take(m) {
            let start = base + self.osc.starts[i] as usize + i * shift;
            let end = if i + 1 < n {
                base + self.osc.starts[i + 1] as usize + i * shift
            } else {
                last_end
            };
            *dst = &src[start..end];
        }
        p.osc_dispatch(&params[..m], bell);
        self.osc_reset();
    }

    /// Reset the OSC accumulation/dispatch state. `osc.clear()` keeps the heap
    /// allocation; `mem::take` would free it and force a malloc on the next
    /// >56-byte payload.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    fn osc_reset(&mut self) {
        self.osc.payload.clear();
        self.osc.overflow = false;
        self.osc.num_starts = 0;
        self.osc.buffer = OscBuffer::Accumulated;
    }

    /// Flush a deferred OSC payload into `payload` (the string spans `advance`
    /// calls or hits a cold terminator). The recorded `starts` offsets stay
    /// valid — they become `payload`-relative instead of input-relative.
    #[inline]
    fn osc_flush(&mut self, bytes: &[u8], end_pos: usize) {
        let base = match self.osc.buffer {
            OscBuffer::Deferred { base } => base,
            OscBuffer::Accumulated => return, // nothing deferred to flush
        };
        // Bulk-copy the `;`-free segments instead of pushing byte-by-byte.
        for part in bytes[base..end_pos].split(|&b| b == 0x3b) {
            self.osc_extend(part);
        }
        self.osc.buffer = OscBuffer::Accumulated;
    }

    #[cfg_attr(feature = "perf-inline", inline(always))]
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
            self.osc.num_starts = 1;
        }
        if action.contains(Action::OSC_PUT) {
            self.osc_extend(&[byte]);
        }
        if action.contains(Action::OSC_END) {
            // Cold terminator (C1 ST / CAN / SUB): the deferred payload was
            // already flushed by the `advance_osc_run` fallback, so dispatch
            // from `payload`.
            self.osc_end(p, byte == 0x07, &[], 0);
        }
        // EXECUTE last: a C0 control that also terminates a string (CAN/SUB in
        // OSC/DCS passthrough) dispatches the string first, then executes.
        if action.contains(Action::EXECUTE) {
            p.execute(byte);
        }
    }

    const fn clear_seq_state(&mut self) {
        self.params.len = 0;
        self.params.sep = 0;
        self.params.param = 0;
        self.params.empty = true;
        self.intermediates.len = 0;
    }

    const fn push_intermediate(&mut self, b: u8) {
        if self.intermediates.len >= self.intermediates.bytes.len() {
            return;
        }
        self.intermediates.bytes[self.intermediates.len] = b;
        self.intermediates.len += 1;
    }

    /// Push a CSI/DCS parameter, capping at [`MAX_PARAM_COUNT`]. Past the
    /// cap, params are dropped; vte and rio-vt both use 32.
    #[inline]
    const fn push_param(&mut self, value: i32) {
        if self.params.len >= MAX_PARAM_COUNT {
            return;
        }
        self.params.values[self.params.len] = value;
        self.params.len += 1;
    }

    /// Accumulate a decimal digit into the current parameter (16-bit clamp).
    ///
    /// `param` is always ≤ `MAX_PARAM_VALUE` (65535) on entry — every writer
    /// either zeroes it or caps it via `min` — so `param * 10 + digit` can
    /// never overflow i32, and plain arithmetic replaces the saturating ops.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    const fn param_digit(&mut self, b: u8) {
        let param = self.params.param * 10 + (b - b'0') as i32;
        self.params.param = if param < MAX_PARAM_VALUE {
            param
        } else {
            MAX_PARAM_VALUE
        };
        self.params.empty = false;
    }

    /// End a parameter on a `:` separator: push it (an empty one becomes the
    /// `-1` sentinel so SGR colon forms like `38:2::r:g:b` survive) and mark
    /// the `sep` bit so `4:2` stays distinguishable from `4;2`.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    const fn param_colon(&mut self) {
        self.push_param(if self.params.empty {
            -1
        } else {
            self.params.param
        });
        set_param_colon_sep(&mut self.params.sep, self.params.len - 1);
        self.params.param = 0;
        self.params.empty = true;
    }

    /// End a parameter on a `;` separator. An empty one becomes the `-2`
    /// sentinel — distinct from the `-1` trailing/colon empty — so `CSI ; Pr
    /// s` (DECSLRM set-right margin) stays distinguishable from `CSI 0;Pr s`,
    /// and SGR can tell a leading empty (`;1m` = reset) from a trailing empty
    /// (`48;2;r;g;b;m` = ignore).
    #[cfg_attr(feature = "perf-inline", inline(always))]
    const fn param_semicolon(&mut self) {
        self.push_param(if self.params.empty {
            -2
        } else {
            self.params.param
        });
        self.params.param = 0;
        self.params.empty = true;
    }

    /// Accumulate one CSI/DCS param byte (digit/`:`/`;`). Returns `false` for
    /// any other byte so the caller can handle it.
    #[cfg_attr(feature = "perf-inline", inline(always))]
    const fn param_byte(&mut self, b: u8) -> bool {
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
        if !self.params.empty || self.params.len != 0 {
            self.push_param(if sentinel && self.params.empty {
                -1
            } else {
                self.params.param
            });
        }
    }

    /// Dispatch a CSI with the collected params/intermediates.
    #[inline]
    fn dispatch_csi<P: Perform>(&mut self, p: &mut P, final_byte: u8) {
        self.push_trailing_param(true);
        let ints = &self.intermediates.bytes[..self.intermediates.len];
        p.csi_dispatch(
            &self.params.values[..self.params.len],
            self.params.sep,
            ints,
            final_byte,
        );
        self.clear_seq_state();
    }

    /// Dispatch a DCS hook with the collected params/intermediates.
    #[inline]
    fn dispatch_hook<P: Perform>(&mut self, p: &mut P, final_byte: u8) {
        self.push_trailing_param(false);
        let ints = &self.intermediates.bytes[..self.intermediates.len];
        p.hook(&self.params.values[..self.params.len], ints, final_byte);
        self.clear_seq_state();
    }

    /// Dispatch an `ESC` sequence with the collected intermediates (empty in the
    /// plain Escape state, where no intermediate byte has been seen yet).
    #[inline]
    fn dispatch_esc<P: Perform>(&mut self, p: &mut P, final_byte: u8) {
        let ints = &self.intermediates.bytes[..self.intermediates.len];
        p.esc_dispatch(ints, final_byte);
        self.clear_seq_state();
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::Event;
    use crate::Recording;
    use crate::limits::MAX_OSC_PAYLOAD;
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
            b"\x1b[1;H",
            b"\x1b[12;H",
            b"\x1b[1;2H",
            b"\x1b[1:2H",
            b"\x1b[3;4;H",
            b"\x1b[;5H",
            b"\x1b[99999999999999m",
            b"\x1b[65536;1H",
            b"\x1b[0;0H",
            b"\x1b[;m",
            b"\x1b[;;m",
            b"\x1b[:m",
            b"\x1b[1:2:3m",
            b"\x1b[?1;2H",
            b"\x1b[1 H",
            b"\x1b[1\x7f",
            b"\x9b5A",
            b"\x1b[2147483648;1H",
            b"\x1b[1;2147483648H",
            b"\x1b[58;5;220m",
            b"\x1b[58:2::1;2;3m",
            b"\x1b[58m",
            b"\x1b[58H",
        ];
        for input in corpus {
            assert_agrees(input);
        }
    }

    /// Assert that the bulk CSI path and the state machine agree on `input`.
    fn assert_agrees(input: &[u8]) {
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

    /// Systematic form of [`hot_and_cold_paths_agree`]: every combination of
    /// digits, both separators, the private marker, an intermediate and a final
    /// byte, over 1..=4-byte runs with and without a tail.
    #[test]
    fn hot_and_cold_paths_agree_systematic() {
        let alphabet = b"018;:Hm ?fJ";
        for len in 1..=4 {
            let mut idx = vec![0_usize; len];
            loop {
                let params: Vec<u8> = idx.iter().map(|&k| alphabet[k]).collect();
                for tail in [&b""[..], &b"Z"[..]] {
                    let mut input = b"\x1b[".to_vec();
                    input.extend_from_slice(&params);
                    input.extend_from_slice(tail);
                    assert_agrees(&input);
                }
                let mut p = 0;
                while p < len && idx[p] + 1 == alphabet.len() {
                    idx[p] = 0;
                    p += 1;
                }
                if p == len {
                    break;
                }
                idx[p] += 1;
            }
        }
    }

    /// Random form of [`hot_and_cold_paths_agree`] across the escape alphabet,
    /// plus chunk invariance: three-byte feeds must match one `parse_events`.
    #[test]
    fn hot_and_cold_paths_agree_random() {
        let alphabet = b"0123456789;:?>=< HfmrAJKhl\x00\x7f\x9b\x1b[";
        let mut state = 0x243f_6a88_85a3_08d3_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..200_000 {
            let len = (next() % 14) as usize;
            let input: Vec<u8> = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            assert_agrees(&input);

            let mut sink = Recording::default();
            let mut parser = Parser::new();
            for chunk in input.chunks(3) {
                parser.advance(&mut sink, chunk);
            }
            parser.flush(&mut sink);
            assert_eq!(
                events_bytes(&sink.events),
                events_bytes(&Parser::new().parse_events(&input)),
                "chunked/whole divergence for {input:?}"
            );
        }
    }

    /// Exhaustive form of [`hot_and_cold_paths_agree`]: every state with both
    /// a hot inline handler and a cold table row, over every byte. `Ground`
    /// is excluded — it is hot-only (UTF-8 decoding + SIMD), with a table
    /// stub of `(NONE, Ground)` for every byte.
    #[test]
    fn hot_and_cold_paths_agree_exhaustive() {
        let states = [
            ParseState::Escape,
            ParseState::DcsEntry,
            ParseState::OscString,
            ParseState::DcsPassthrough,
            ParseState::CsiParam,
            ParseState::DcsParam,
        ];
        for state in states {
            for byte in 0..=255_u8 {
                let mut hot = Parser::new();
                hot.state = state;
                let hot = events_bytes(&hot.parse_events(&[byte]));

                let mut cold = Parser::new();
                cold.cold = true;
                cold.state = state;
                let cold = events_bytes(&cold.parse_events(&[byte]));

                assert_eq!(
                    hot, cold,
                    "hot/cold divergence for {state:?} + 0x{byte:02x}"
                );
            }
        }
    }

    /// DEC's "anywhere" invariant for 8-bit C1 controls
    /// ([DEC parser state diagram]): introducers re-enter their state, `ST`
    /// `0x9c` is a no-op, and every other C1 control executes and returns to
    /// ground (a lone C1 byte is invalid UTF-8, so this is the correct
    /// fallback for a UTF-8 terminal).
    ///
    /// [DEC parser state diagram]: https://vt100.net/emu/dec_ansi_parser
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
        // intermediates into the next CSI's dispatch (fuzz regression).
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
