// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared helpers for the differential tests: recorders + runners that map
//! each reference parser's `Perform` callbacks onto `vtty::Event`, the corpus
//! builder, and the intermediate-overflow filter used by the rio-vt
//! comparison (rio-vt allows four intermediates; vtty and vte cap at two).

// Each test crate uses a different subset of these helpers.
#![allow(
    dead_code,
    reason = "each test crate uses a different subset of these helpers"
)]

use smallvec::SmallVec;
use vtty_core::Event;

/// Run vtty (scalar path) and normalize to the vte-family convention:
/// - the `-1` empty-colon sentinel becomes 0 (vte uses 0 for an empty param);
/// - a param-less CSI/DCS dispatches `[]` in vtty but `[0]` in vte/rio-vt.
pub(crate) fn run_vtty(bytes: &[u8]) -> Vec<Event> {
    let mut p = vtty_core::Parser::new();
    let mut events = p.parse_events(bytes);
    for e in &mut events {
        let params: &mut SmallVec<i32, 6> = match e {
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

/// Records vte's `Perform` callbacks into `vtty::Event`.
struct VteRec {
    acts: Vec<Event>,
}

impl vte::Perform for VteRec {
    fn print(&mut self, c: char) {
        self.acts.push(Event::Print(c));
    }
    fn execute(&mut self, b: u8) {
        self.acts.push(Event::Execute(b));
    }
    fn csi_dispatch(&mut self, params: &vte::Params, int: &[u8], _ignore: bool, f: char) {
        let flat: SmallVec<i32, 6> = params.iter().flatten().map(|&v| v as i32).collect();
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
        let flat: SmallVec<i32, 6> = params.iter().flatten().map(|&v| v as i32).collect();
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
pub(crate) fn run_vte(bytes: &[u8]) -> Vec<Event> {
    let mut rec = VteRec { acts: Vec::new() };
    let mut p = vte::Parser::new();
    p.advance(&mut rec, bytes);
    rec.acts
}

/// Records rio-vt's `Perform` callbacks into `vtty::Event`. rio-vt is a fork
/// of Alacritty's VTE, so the dispatch surface mirrors vte's.
struct RioRec {
    acts: Vec<Event>,
}

impl rio_vt::performer::parser::Perform for RioRec {
    fn print(&mut self, c: char) {
        self.acts.push(Event::Print(c));
    }
    fn print_str(&mut self, s: &str) {
        self.acts.extend(s.chars().map(Event::Print));
    }
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        self.acts.extend(
            codepoints
                .iter()
                .map(|&cp| Event::Print(char::from_u32(cp).unwrap_or('\u{FFFD}'))),
        );
    }
    fn execute(&mut self, b: u8) {
        self.acts.push(Event::Execute(b));
    }
    fn csi_dispatch(
        &mut self,
        params: &rio_vt::performer::parser::Params,
        int: &[u8],
        _ignore: bool,
        f: char,
    ) {
        let flat: SmallVec<i32, 6> = params.iter().flatten().map(|&v| v as i32).collect();
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
    fn hook(
        &mut self,
        params: &rio_vt::performer::parser::Params,
        int: &[u8],
        _ignore: bool,
        f: char,
    ) {
        let flat: SmallVec<i32, 6> = params.iter().flatten().map(|&v| v as i32).collect();
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

/// Run rio-vt, mapping its `Perform` callbacks to `vtty::Event`.
pub(crate) fn run_rio(bytes: &[u8]) -> Vec<Event> {
    let mut rec = RioRec { acts: Vec::new() };
    let mut p = rio_vt::performer::parser::Parser::new();
    p.advance(&mut rec, bytes);
    rec.acts
}

/// Build the corpus: all C0/C1 controls, ESC/CSI/OSC/DCS variants, UTF-8.
/// DEL (`0x7f`), raw 8-bit C1, and SOS/PM/APC are excluded — vtty diverges
/// from the vte family there on purpose (documented in `parser.rs`).
pub(crate) fn corpus() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = Vec::new();

    // C0 controls (0x00..=0x1f) plus space. DEL (0x7f) is INTENTIONALLY
    // divergent: vtty dispatches it as a C0 control (destructive backspace),
    // while vte includes it in printable runs — the terminal's screens rely
    // on the control path. Covered by vtty's own parser tests.
    for b in 0x00_u8..=0x20 {
        v.push(vec![b]);
    }

    // Raw 8-bit C1 is INTENTIONALLY divergent: vtty follows DEC "anywhere"
    // (recognizes DCS/SOS/CSI/OSC/PM/APC/ST), vte executes every 0x80..0x9f
    // byte. See <https://vt100.net/emu/dec_ansi_parser> and `parser.rs`
    // `c1_transition`; covered by vtty's own tests.

    // C1 controls as two-byte UTF-8 (U+0080..=U+009F) — both decode+execute,
    // so these still match vte.
    for b in 0x80_u8..=0x9f {
        v.push(vec![0xc2, b]);
    }

    // Every ESC final byte (0x20..=0x7e).
    for b in 0x20_u8..=0x7e {
        v.push(vec![0x1b, b]);
    }

    // Every CSI final byte, plain + each private marker + a param.
    for f in 0x40_u8..=0x7e {
        v.push(vec![0x1b, b'[', f]);
        for m in *b"?>=<" {
            v.push(vec![0x1b, b'[', m, f]);
        }
        v.push(vec![0x1b, b'[', b'1', f]);
        v.push(vec![0x1b, b'[', b'1', b';', b'2', f]);
    }

    // Colon / sub-parameter variants.
    for seq in [
        &b"\x1b[1:2m"[..],
        &b"\x1b[38:5:196m"[..],
        &b"\x1b[:1m"[..],
        &b"\x1b[::m"[..],
        &b"\x1b[1;2:3;4m"[..],
        &b"\x1b[?25l"[..],
        &b"\x1b[?25h"[..],
    ] {
        v.push(seq.to_vec());
    }

    // DCS (with and without params) terminated by ST and BEL.
    v.push(b"\x1bP1;2|payload\x1b\\".to_vec());
    v.push(b"\x1bPqpayload\x07".to_vec());
    v.push(b"\x1bP+ppayload\x1b\\".to_vec());

    // OSC terminated by BEL and ST.
    v.push(b"\x1b]0;title\x07".to_vec());
    v.push(b"\x1b]2;title\x1b\\".to_vec());
    v.push(b"\x1b]52;c;base64\x07".to_vec());
    // CAN/SUB terminates the OSC and is executed after the dispatch.
    v.push(b"\x1b]0;title\x1a".to_vec());
    v.push(b"\x1b]0;title\x18".to_vec());

    // SOS/PM/APC: vtty dispatches hook/put/unhook (kitty graphics `_G`), vte
    // drops them — excluded from the differential baseline, covered by the
    // cell-screen kitty-graphics tests.

    // UTF-8: 2/3/4-byte chars, plus an overlong/invalid byte.
    v.push("héllo 世界 🚀".as_bytes().to_vec());
    v.push(vec![0xe2, 0x82, 0xac]); // €
    v.push(vec![0xf0, 0x9f, 0x9a, 0x80]); // 🚀
    v.push(vec![0xc0, 0x80]); // overlong (invalid)
    v.push(vec![0x80]); // lone continuation
    v.push(vec![0xc0]); // invalid 2-byte lead alone
    v.push(vec![0xf5]); // invalid lead (U+110000+ / 5-byte lead)
    v.push(vec![0xf1, 0xfb]); // 0xFB is not a continuation byte
    v.push(vec![0xff, 0xf1, 0xfb]); // invalid lead + valid lead + invalid byte
    // Invalid second byte on a constrained lead: NOT a truncated partial —
    // both parsers decode it as U+FFFD.
    v.push(vec![0xf4, 0xad]);
    v.push(vec![0xe0, 0x80]); // 0xE0 requires 0xA0..=0xBF
    v.push(vec![0xed, 0xa0, 0x80]); // 0xED excludes surrogates
    v.push(vec![0xf0, 0x80, 0x80, 0x80]); // 0xF0 requires 0x90..=0xBF

    // CAN/SUB inside a CSI, ESC aborts and restarts.
    v.push(b"\x1b[1\x18;2m".to_vec());
    v.push(b"\x1b[1\x1b[2m".to_vec());

    // CSI param edge cases: negative, huge, empty, many, leading semicolon.
    for seq in [
        &b"\x1b[-1;2m"[..],
        &b"\x1b[-12@"[..],
        &b"\x1b[-2d"[..],
        &b"\x1b[99999999999999999999m"[..],
        &b"\x1b[1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16m"[..],
        &b"\x1b[;m"[..],
        &b"\x1b[;;m"[..],
        &b"\x1b[;1;2m"[..],
        &b"\x1b[1;;2m"[..],
        &b"\x1b[?;m"[..],
        &b"\x1b[>;m"[..],
        &b"\x1b[1;2;3;"[..],
        &b"\x1b[1:2:3:4m"[..],
        &b"\x1b[38;2;:;:m"[..],
    ] {
        v.push(seq.to_vec());
    }

    // ESC with intermediates (charset / DEC line / charset select).
    for seq in [
        &b"\x1b(0"[..],
        &b"\x1b)B"[..],
        &b"\x1b)0"[..],
        &b"\x1b#8"[..],
        &b"\x1b#3"[..],
        // More than two intermediates: the DEC spec caps at two, so the
        // third is ignored (matching vte).
        &b"\x1b/' .>"[..],
        &b"\x1b%G"[..],
        &b"\x1b%5"[..],
        &b"\x1b*0"[..],
        &b"\x1b+0"[..],
        &b"\x1b="[..],
        &b"\x1b>"[..],
        &b"\x1b7"[..],
        &b"\x1b8"[..],
        &b"\x1bD"[..],
        &b"\x1bE"[..],
        &b"\x1bM"[..],
        &b"\x1bH"[..],
        &b"\x1bc"[..],
        &b"\x1bZ"[..],
    ] {
        v.push(seq.to_vec());
    }

    // OSC edge cases: empty, empty params, many params.
    for seq in [
        &b"\x1b]\x07"[..],
        &b"\x1b];\x07"[..],
        &b"\x1b]0;;\x07"[..],
        &b"\x1b]0;a;b;c;d;e\x07"[..],
        &b"\x1b]52;;Zm9v\x07"[..],
        &b"\x1b]8;;http://x\x1b\\"[..],
        &b"\x1b]8;;\x1b\\"[..],
        &b"\x1b]133;A\x07"[..],
    ] {
        v.push(seq.to_vec());
    }

    // Combining marks + emoji (should be a run of Print actions).
    v.push("e\u{301}x".as_bytes().to_vec());
    v.push("👋🏿".as_bytes().to_vec());
    v.push("🇺🇸".as_bytes().to_vec());

    // Edge cases from wezterm's vendored `vtparse` (Kaku) — colon sub-params,
    // kitty curly underline, OSC-with-ESC-ST, OSC param truncation.
    for seq in [
        &b"\x1b[38:2::128:64:192m"[..], // colon rgb with empty sub-params
        &b"\x1b[4:3m"[..],              // kitty curly underline
        &b"\x1b[58:2::128:64:192m"[..], // colored underline colon rgb
        &b"\x1b]woot\x1b\\"[..],        // OSC then ESC-ST (ESC breaks out)
        &b"\x1b]0;hello\x07"[..],       // OSC terminated by BEL
        &b"\x1b]\x07"[..],              // OSC with no params
    ] {
        v.push(seq.to_vec());
    }

    v
}

/// Some parsers cap escape-sequence intermediates at more than vtty's two
/// (rio-vt uses four). Returns true when an escape sequence collects more
/// than `limit` intermediate/private-marker bytes before its final byte.
pub(crate) fn has_intermediate_overflow(data: &[u8], limit: u8) -> bool {
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

/// Build a human-readable difference report when two event streams differ.
pub(crate) fn diff_report(
    bytes: &[u8],
    left: &[Event],
    right: &[Event],
    left_label: &str,
    right_label: &str,
) -> Option<String> {
    if left == right {
        return None;
    }
    let mut s = String::new();
    s.push_str(&format!("  input: {bytes:?}\n"));
    s.push_str(&format!("  {left_label}: {left:?}\n"));
    s.push_str(&format!("  {right_label}: {right:?}\n"));
    Some(s)
}
