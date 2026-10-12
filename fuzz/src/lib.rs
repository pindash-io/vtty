// Shared helpers for the differential fuzz targets.

pub use vtty::Event;

/// Records vte's `Perform` callbacks into `vtty::Event`.
pub struct VteRec {
    pub acts: Vec<Event>,
}

impl vte::Perform for VteRec {
    fn print(&mut self, c: char) {
        self.acts.push(Event::Print(c));
    }
    fn execute(&mut self, b: u8) {
        self.acts.push(Event::Execute(b));
    }
    fn csi_dispatch(&mut self, params: &vte::Params, int: &[u8], _ignore: bool, f: char) {
        let flat: smallvec::SmallVec<[i32; 6]> =
            params.iter().flatten().map(|&v| v as i32).collect();
        self.acts.push(Event::CsiDispatch {
            params: flat,
            intermediates: int.iter().copied().collect(),
            final_byte: f as u8,
        });
    }
    fn esc_dispatch(&mut self, int: &[u8], _ignore: bool, b: u8) {
        self.acts.push(Event::EscDispatch {
            intermediates: int.iter().copied().collect(),
            byte: b,
        });
    }
    fn hook(&mut self, params: &vte::Params, int: &[u8], _ignore: bool, f: char) {
        let flat: smallvec::SmallVec<[i32; 6]> =
            params.iter().flatten().map(|&v| v as i32).collect();
        self.acts.push(Event::DcsHook {
            params: flat,
            intermediates: int.iter().copied().collect(),
            final_byte: f as u8,
        });
    }
    fn put(&mut self, b: u8) {
        self.acts.push(Event::DcsPut(b));
    }
    fn unhook(&mut self) {
        self.acts.push(Event::DcsUnhook);
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
        self.acts.push(Event::OscDispatch {
            params: params.iter().map(|p| p.to_vec()).collect(),
            bell_terminated: bell,
        });
    }
}

/// Run vte, mapping its `Perform` callbacks to `vtty::Event`.
pub fn run_vte(bytes: &[u8]) -> Vec<Event> {
    let mut rec = VteRec { acts: Vec::new() };
    let mut p = vte::Parser::new();
    p.advance(&mut rec, bytes);
    rec.acts
}

/// Run vtty (scalar path) and normalize to the vte-family convention.
pub fn run_vtty(bytes: &[u8]) -> Vec<Event> {
    run_vtty_parser(vtty::Parser::new(), bytes)
}

/// Run vtty (SIMD path) and normalize to the vte-family convention.
pub fn run_vtty_simd(bytes: &[u8]) -> Vec<Event> {
    run_vtty_parser(vtty::Parser::with_simd(), bytes)
}

/// Normalize vtty's representation to the vte-family convention:
/// - the `-1` empty-colon sentinel becomes 0;
/// - a param-less CSI/DCS dispatches `[]` in vtty but `[0]` in vte.
fn run_vtty_parser(mut p: vtty::Parser, bytes: &[u8]) -> Vec<Event> {
    let mut events = p.parse_events(bytes);
    for e in &mut events {
        let params: &mut smallvec::SmallVec<[i32; 6]> = match e {
            Event::CsiDispatch { params, .. } | Event::DcsHook { params, .. } => params,
            _ => continue,
        };
        for p in params.iter_mut() {
            if *p == -1 {
                *p = 0;
            }
        }
        if params.is_empty() {
            params.push(0);
        }
    }
    events
}

/// Whether `second` is a valid second byte for a constrained 3/4-byte `lead`
/// (mirrors `vtty`'s `utf8_second_byte_valid`).
fn utf8_second_byte_valid(lead: u8, second: u8) -> bool {
    match lead {
        0xe0 => (0xa0..=0xbf).contains(&second),
        0xed => (0x80..=0x9f).contains(&second),
        0xf0 => (0x90..=0xbf).contains(&second),
        0xf4 => (0x80..=0x8f).contains(&second),
        _ => true,
    }
}

/// Known, intentional divergences between vtty and the vte-family parsers
/// (documented in `crates/vtty/tests/vt_differential.rs`). Returns true when
/// `data` contains a byte/sequence that triggers one, so the fuzzer skips it:
///
/// - `0x7f` DEL: vtty dispatches it as a C0 control, vte prints it.
/// - Raw 8-bit C1 (`0x80..=0x9f`): vtty follows DEC's "anywhere" rule and
///   executes/recognizes C1 controls even inside escape sequences; vte
///   predates 8-bit-clean C1 and drops them there. In the ground state only
///   the seven introducers (`0x90 0x98 0x9b 0x9c 0x9d 0x9e 0x9f`) diverge.
/// - `ESC X` / `ESC ^` / `ESC _` (SOS/PM/APC): vtty dispatches them as
///   hook/put/unhook (kitty graphics), vte drops them.
///
/// UTF-8 decode is deliberately *not* filtered: vtty, vte and rio-vt all
/// implement W3C "Substitution of Maximal Subparts" — each invalid maximal
/// subpart becomes a single U+FFFD, and a standalone C1 byte (`0x80..=0x9f`)
/// keeps `execute` semantics instead of being replaced. A malformed run such
/// as a truncated 3-byte lead (`E3 83`) followed by printable ASCII (`[`) and
/// a C1 continuation byte (`0x88`) therefore decodes identically everywhere:
/// `Print('�'), Print('['), Execute(136)`. Because the three parsers share
/// this contract, no UTF-8-malformation case needs filtering here (verified
/// against the `utf8_mixed` corpus and single-byte mutations of it).
pub fn has_known_divergence(data: &[u8]) -> bool {
    if data.contains(&0x7f) {
        return true;
    }
    has_c1_divergence(data)
}

/// rio-vt's SIMD UTF-8 decoder (the C++ `simdutf` backend) has an
/// x86_64-only bug: when a ground-state decode chunk contains malformed
/// UTF-8, `simdutf` reports the wrong error offset, so rio-vt slices the
/// valid prefix and corrupts the whole decode (dropping/reordering glyphs
/// *before* the error as well). vtty and vte decode the same bytes correctly
/// (W3C maximal-subpart), and the scalar rio-vt path agrees too — only the
/// `simdutf` AVX2/AVX-512 error path is wrong. Detecting the trigger directly
/// (any malformed UTF-8 in the ground state) keeps the `differential-rio`
/// target from reporting rio-vt's known upstream bug as a vtty divergence.
///
/// Detects: a lone continuation byte (`0x80..=0xbf`), an invalid lead
/// (`0xc0`/`0xc1`/`0xf5..=0xff`), or a truncated/ill-formed multi-byte
/// sequence.
pub fn has_rio_utf8_simd_divergence(data: &[u8]) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Ground,
        Escape,
        EscapeIntermediate,
        Csi,
        Dcs,
        Osc,
        DcsPassthrough,
    }

    let mut mode = Mode::Ground;
    let mut pending = 0_u8; // expected UTF-8 continuation bytes (ground only)
    let mut check_second = false; // next continuation is a constrained 2nd byte
    let mut lead = 0_u8;

    for &b in data {
        match mode {
            Mode::Ground => {
                if b == 0x1b {
                    // ESC aborts an in-flight UTF-8 sequence, so a pending
                    // lead is a truncated sequence (simdutf TooShort path).
                    if pending > 0 {
                        return true;
                    }
                    pending = 0;
                    check_second = false;
                    mode = Mode::Escape;
                    continue;
                }
                if pending > 0 {
                    let valid = (0x80..=0xbf).contains(&b)
                        && !(check_second && !utf8_second_byte_valid(lead, b));
                    if valid {
                        pending -= 1;
                        check_second = false;
                        continue;
                    }
                    // Truncated / ill-formed sequence → simdutf error path.
                    return true;
                }
                match b {
                    // Lone continuation byte (no active sequence).
                    0x80..=0xbf => return true,
                    0xc2..=0xdf => {
                        pending = 1;
                        lead = b;
                    }
                    0xe0..=0xef | 0xf0..=0xf4 => {
                        pending = if b < 0xf0 { 2 } else { 3 };
                        check_second = true;
                        lead = b;
                    }
                    // Overlong / out-of-range lead byte.
                    0xc0 | 0xc1 | 0xf5..=0xff => return true,
                    _ => {}
                }
            }
            Mode::Escape => match b {
                0x1b => {}
                0x18 | 0x1a => mode = Mode::Ground,
                0x20..=0x2f => mode = Mode::EscapeIntermediate,
                b'[' => mode = Mode::Csi,
                b'P' => mode = Mode::Dcs,
                b']' => mode = Mode::Osc,
                // SOS/PM/APC payload runs to ST; not ground-state UTF-8.
                b'X' | b'^' | b'_' => mode = Mode::DcsPassthrough,
                0x30..=0x7e => mode = Mode::Ground,
                _ => {}
            },
            // After an ESC intermediate (`0x20..=0x2f`) the introducers are
            // no longer introducers: every `0x30..=0x7e` byte is an ESC final
            // that dispatches and returns to ground (e.g. `ESC SP ]` is an
            // EscDispatch, not OSC).
            Mode::EscapeIntermediate => match b {
                0x1b => mode = Mode::Escape,
                0x18 | 0x1a => mode = Mode::Ground,
                0x20..=0x2f => mode = Mode::EscapeIntermediate,
                0x30..=0x7e => mode = Mode::Ground,
                _ => mode = Mode::EscapeIntermediate,
            },
            Mode::Csi => match b {
                0x1b => mode = Mode::Escape,
                0x18 | 0x1a => mode = Mode::Ground,
                0x40..=0x7e => mode = Mode::Ground, // CSI final
                _ => {}
            },
            Mode::Dcs => match b {
                0x1b => mode = Mode::Escape,
                0x18 | 0x1a => mode = Mode::Ground,
                0x40..=0x7e => mode = Mode::DcsPassthrough, // DCS hook
                _ => {}
            },
            Mode::Osc => match b {
                0x1b => mode = Mode::Escape,
                0x07 | 0x18 | 0x1a | 0x9c => mode = Mode::Ground,
                _ => {}
            },
            Mode::DcsPassthrough => match b {
                0x1b => mode = Mode::Escape,
                0x18 | 0x1a | 0x9c => mode = Mode::Ground,
                _ => {}
            },
        }
    }
    // A truncated sequence at the very end of the buffer also reaches
    // simdutf's TooShort error path.
    if pending > 0 {
        return true;
    }
    false
}

/// Some parsers cap escape-sequence intermediates at more than vtty's two
/// (rio-vt uses four). Returns true when an escape sequence collects more
/// than `limit` intermediate/private-marker bytes before its final byte.
pub fn has_intermediate_overflow(data: &[u8], limit: u8) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum S {
        Ground,
        Escape,
        CsiDcs,
        Other,
    }

    let mut s = S::Ground;
    let mut count = 0_u8;
    for &b in data {
        match s {
            S::Ground => {
                if b == 0x1b {
                    s = S::Escape;
                    count = 0;
                }
            }
            S::Escape => match b {
                0x1b => count = 0,
                0x20..=0x2f => {
                    count += 1;
                    if count > limit {
                        return true;
                    }
                }
                b'[' | b'P' => {
                    s = S::CsiDcs;
                    count = 0;
                }
                b']' | b'X' | b'^' | b'_' => s = S::Other,
                0x30..=0x7e => s = S::Ground, // ESC final
                0x18 | 0x1a => s = S::Ground,
                _ => {}
            },
            S::CsiDcs => match b {
                0x1b => {
                    s = S::Escape;
                    count = 0;
                }
                0x20..=0x2f | 0x3c..=0x3f => {
                    count += 1;
                    if count > limit {
                        return true;
                    }
                }
                0x40..=0x7e => s = S::Other, // CSI final / DCS hook
                0x18 | 0x1a => s = S::Ground,
                _ => {}
            },
            S::Other => match b {
                0x1b => {
                    s = S::Escape;
                    count = 0;
                }
                0x07 | 0x9c | 0x18 | 0x1a => s = S::Ground,
                _ => {}
            },
        }
    }
    false
}

fn has_c1_divergence(data: &[u8]) -> bool {
    const INTRODUCERS: [u8; 7] = [0x90, 0x98, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f];

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Ground,
        Escape,
        EscapeIntermediate,
        Csi,
        Dcs,
        Osc,
        DcsPassthrough,
    }

    let mut mode = Mode::Ground;
    let mut pending = 0_u8; // expected UTF-8 continuation bytes (ground only)
    let mut check_second = false; // next continuation is a constrained 2nd byte
    let mut lead = 0_u8;

    for &b in data {
        match mode {
            Mode::Ground => {
                if b == 0x1b {
                    // Leaving ground aborts any in-flight UTF-8 sequence.
                    pending = 0;
                    check_second = false;
                    mode = Mode::Escape;
                    continue;
                }
                // A raw C1 introducer (not a UTF-8 continuation) diverges.
                if pending > 0 {
                    let valid = (0x80..=0xbf).contains(&b)
                        && !(check_second && !utf8_second_byte_valid(lead, b));
                    if valid {
                        pending -= 1;
                        check_second = false;
                        continue;
                    }
                    // Broken sequence → reprocess `b` below as a fresh byte.
                    pending = 0;
                    check_second = false;
                }
                if INTRODUCERS.contains(&b) {
                    return true;
                }
                match b {
                    0xc2..=0xdf => {
                        pending = 1;
                        lead = b;
                    }
                    0xe0..=0xef | 0xf0..=0xf4 => {
                        pending = if b < 0xf0 { 2 } else { 3 };
                        check_second = true;
                        lead = b;
                    }
                    _ => {}
                }
            }
            Mode::Escape => {
                if (0x80..=0x9f).contains(&b) {
                    return true;
                }
                match b {
                    0x1b => {}
                    0x18 | 0x1a => mode = Mode::Ground,
                    0x20..=0x2f => mode = Mode::EscapeIntermediate,
                    b'[' => mode = Mode::Csi,
                    b'P' => mode = Mode::Dcs,
                    b']' => mode = Mode::Osc,
                    // SOS/PM/APC introducers: vtty dispatches hook/put/unhook
                    // (kitty graphics), vte drops them.
                    b'X' | b'^' | b'_' => return true,
                    0x30..=0x7e => mode = Mode::Ground,
                    _ => {}
                }
            }
            Mode::EscapeIntermediate => {
                if (0x80..=0x9f).contains(&b) {
                    return true;
                }
                match b {
                    0x1b => mode = Mode::Escape,
                    0x18 | 0x1a => mode = Mode::Ground,
                    0x20..=0x2f => mode = Mode::EscapeIntermediate,
                    0x30..=0x7e => mode = Mode::Ground,
                    _ => mode = Mode::EscapeIntermediate,
                }
            }
            Mode::Csi => {
                if (0x80..=0x9f).contains(&b) {
                    return true;
                }
                match b {
                    0x1b => mode = Mode::Escape,
                    0x18 | 0x1a => mode = Mode::Ground,
                    0x40..=0x7e => mode = Mode::Ground, // CSI final
                    _ => {}
                }
            }
            Mode::Dcs => {
                if (0x80..=0x9f).contains(&b) {
                    return true;
                }
                match b {
                    0x1b => mode = Mode::Escape,
                    0x18 | 0x1a => mode = Mode::Ground,
                    0x40..=0x7e => mode = Mode::DcsPassthrough, // DCS hook
                    _ => {}
                }
            }
            Mode::Osc => {
                if (0x80..=0x9f).contains(&b) {
                    return true;
                }
                match b {
                    0x1b => mode = Mode::Escape,
                    0x07 | 0x18 | 0x1a | 0x9c => mode = Mode::Ground,
                    _ => {}
                }
            }
            Mode::DcsPassthrough => {
                if (0x80..=0x9f).contains(&b) {
                    return true;
                }
                match b {
                    0x1b => mode = Mode::Escape,
                    // BEL is a payload byte here (not a terminator).
                    0x18 | 0x1a | 0x9c => mode = Mode::Ground,
                    _ => {}
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::has_rio_utf8_simd_divergence;

    #[test]
    fn valid_utf8_is_not_flagged() {
        // "a日b" — all well-formed.
        assert!(!has_rio_utf8_simd_divergence(b"a\xe6\x97\xa5b"));
        // Emoji + combining mark.
        assert!(!has_rio_utf8_simd_divergence("🚀✨e\u{301}".as_bytes()));
    }

    #[test]
    fn lone_continuation_is_flagged() {
        // "a日" then a lone 0xB8 continuation byte.
        assert!(has_rio_utf8_simd_divergence(b"a\xe6\x97\xa5\xb8b"));
        // A run of lone continuation bytes.
        assert!(has_rio_utf8_simd_divergence(b"\xb8\xb8\xb8"));
    }

    #[test]
    fn truncated_sequence_is_flagged() {
        // "a" then F0 AC (truncated 4-byte lead).
        assert!(has_rio_utf8_simd_divergence(b"a\xf0\xac\xe8\xaa\x9e"));
    }

    #[test]
    fn invalid_lead_is_flagged() {
        assert!(has_rio_utf8_simd_divergence(b"a\xc0\xaf"));
        assert!(has_rio_utf8_simd_divergence(b"a\xf5\x80\x80\x80"));
    }

    #[test]
    fn esc_aborts_truncated_sequence() {
        // A 3-byte lead immediately followed by ESC is truncated.
        assert!(has_rio_utf8_simd_divergence(b"a\xe6\x1b[200l"));
        // A complete sequence before ESC is NOT truncated.
        assert!(!has_rio_utf8_simd_divergence(b"a\xe6\x97\xa5\x1b"));
    }

    #[test]
    fn trailing_truncated_sequence() {
        // Input ending mid-sequence is truncated (simdutf TooShort).
        assert!(has_rio_utf8_simd_divergence(b"a\xe6\x9c"));
        assert!(has_rio_utf8_simd_divergence(b"\xf0\x9f\x92"));
    }

    #[test]
    fn escape_sequences_are_not_utf8() {
        // A CSI sequence containing 0xB8 in its params is NOT ground UTF-8.
        assert!(!has_rio_utf8_simd_divergence(b"\x1b[\xb8m"));
        // OSC payload bytes are not ground UTF-8 either.
        assert!(!has_rio_utf8_simd_divergence(b"\x1b]0;\xb8title\x07"));
    }

    #[test]
    fn esc_intermediate_final_returns_to_ground() {
        // `ESC SP ]` is an EscDispatch (final `]`), NOT OSC — so the bytes
        // after it are ground-state UTF-8 again. The truncated `D5` lead
        // followed by `1` must be flagged. (This is the exact shape of a
        // CI fuzz-smoke divergence that escaped the pre-EscapeIntermediate
        // filter.)
        assert!(has_rio_utf8_simd_divergence(b"\x1b\xc2\xc2 \x5d\xd5\x31"));
        // Without the `]` final, the escape sequence is still open: `D5 31`
        // are escape-payload/ignored bytes, not ground UTF-8.
        assert!(!has_rio_utf8_simd_divergence(b"\x1b\xc2\xc2 \xd5\x31"));
    }
}
