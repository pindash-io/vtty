// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Parser end-to-end tests (ported from the old `vt` crate's parser suite).

use vtty::{Event, Parser, Perform};

#[derive(Default)]
#[allow(
    clippy::type_complexity,
    reason = "event vectors mirror the parser's callback signatures"
)]
struct Collecting {
    printed: String,
    executed: Vec<u8>,
    csi: Vec<(Vec<u16>, Vec<u8>, char, Vec<bool>)>,
    osc: Vec<Vec<Vec<u8>>>,
    esc: Vec<(Vec<u8>, u8)>,
}

impl Perform for Collecting {
    fn print(&mut self, c: char) {
        self.printed.push(c);
    }
    fn print_bytes(&mut self, s: &[u8]) {
        for &b in s {
            self.printed.push(b as char);
        }
    }
    fn execute(&mut self, b: u8) {
        self.executed.push(b);
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the -1 sentinel surfaces as u16::MAX"
    )]
    fn csi_dispatch(&mut self, params: &[i32], sep: u16, inters: &[u8], final_byte: u8) {
        // The `-1` empty-param sentinel surfaces as `u16::MAX` after the
        // `i32 → u16` cast (asserted by `csi_trailing_empty_param_is_sentinel`).
        let values: Vec<u16> = params.iter().map(|&p| p as u16).collect();
        // Bit `i` of `sep` marks param `i` as followed by a colon.
        let seps: Vec<bool> = (0..values.len())
            .map(|i| i < 16 && sep & (1 << i) != 0)
            .collect();
        self.csi
            .push((values, inters.to_vec(), final_byte as char, seps));
    }
    fn esc_dispatch(&mut self, inters: &[u8], b: u8) {
        self.esc.push((inters.to_vec(), b));
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell: bool) {
        self.osc.push(params.iter().map(|p| p.to_vec()).collect());
    }
}

fn parse(bytes: &[u8]) -> Collecting {
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, bytes);
    c
}

#[test]
fn print_ascii_and_utf8() {
    let c = parse("aλ🚀\n".as_bytes());
    assert_eq!(c.printed, "aλ🚀");
    assert_eq!(c.executed, vec![b'\n']);
}

#[cfg(feature = "simd")]
#[test]
fn simd_and_scalar_paths_agree() {
    // `Parser::new()` (scalar) and `Parser::with_simd()` must produce the
    // same event stream.
    let bytes = b"\x1b[31mred \xce\xbb \xf0\x9f\x9a\x80 \x1b[0mplain\n";

    let mut scalar = Parser::new();
    let mut a = Collecting::default();
    scalar.advance(&mut a, bytes);

    let mut simd = Parser::with_simd();
    let mut b = Collecting::default();
    simd.advance(&mut b, bytes);

    assert_eq!(a.printed, b.printed);
    assert_eq!(a.executed, b.executed);
    assert_eq!(a.csi, b.csi);
    assert_eq!(a.osc, b.osc);
    assert_eq!(a.esc, b.esc);
}

/// Sink that records `print_codepoints` batches and lone `print` calls
/// separately, so the bulk non-ASCII path can be asserted.
#[cfg(feature = "simd")]
#[derive(Default)]
struct CodepointBatches {
    batches: Vec<Vec<u32>>,
    executed: Vec<u8>,
}

#[cfg(feature = "simd")]
impl Perform for CodepointBatches {
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        self.batches.push(codepoints.to_vec());
    }
    fn print(&mut self, c: char) {
        self.batches.push(vec![c as u32]);
    }
    fn execute(&mut self, b: u8) {
        self.executed.push(b);
    }
}

#[cfg(feature = "simd")]
#[test]
fn print_codepoints_batches_utf8_runs() {
    // 11 × 3 bytes = 33 bytes of CJK, no ESC: long enough to reach the SIMD
    // batched decode (probe window is 32 bytes). The flattened code-point
    // stream must reproduce the text under both paths; the SIMD path should
    // emit it as one batch.
    let text = "中中中中中中中中中中中";

    let mut p = Parser::with_simd();
    let mut sink = CodepointBatches::default();
    p.advance(&mut sink, text.as_bytes());

    let flat: Vec<u32> = sink.batches.iter().flatten().copied().collect();
    assert_eq!(flat, text.chars().map(|c| c as u32).collect::<Vec<_>>());
    #[cfg(feature = "simd")]
    assert!(
        sink.batches.iter().any(|b| b.len() > 1),
        "SIMD path should batch code points, got {:?}",
        sink.batches
    );
}

#[test]
fn csi_no_params_is_empty() {
    // A param-less CSI (`CSI H`) dispatches with an empty param list (so
    // decoders can distinguish it from an explicit `CSI 0 …`).
    let c = parse(b"\x1b[H");
    assert_eq!(c.csi.len(), 1);
    assert_eq!(c.csi[0].0, vec![]);
    assert_eq!(c.csi[0].2, 'H');
}

#[test]
fn csi_semicolon_params() {
    let c = parse(b"\x1b[1;4H");
    assert_eq!(c.csi[0].0, vec![1, 4]);
    assert_eq!(c.csi[0].3, vec![false, false]);
}

#[test]
fn csi_trailing_empty_param_is_sentinel() {
    // ConPTY emits SGR with a trailing `;` (e.g. `ESC[48;2;R;G;B;m`). The
    // trailing empty param must be distinguishable from an explicit `0` (which
    // SGR would treat as "reset"), so it is dispatched as the `-1` sentinel
    // (surfaced here as `u16::MAX` after `Params::from_i32`).
    let c = parse(b"\x1b[48;2;255;128;0;m");
    assert_eq!(c.csi[0].0, vec![48, 2, 255, 128, 0, u16::MAX]);
}

#[test]
fn csi_paramless_split_across_writes_is_empty() {
    // `CSI s` (SCOSC) split across two `advance` calls must still dispatch
    // with an empty param list — not `[0]`, which the decoder reads as
    // DECSLRM. (Regression for the non-ground final-byte fast path.)
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, b"\x1b[");
    p.advance(&mut c, b"s");
    assert_eq!(c.csi.len(), 1);
    assert_eq!(c.csi[0].0, vec![]);
    assert_eq!(c.csi[0].2, 's');
}

#[test]
fn csi_colon_params() {
    let c = parse(b"\x1b[4:2m");
    assert_eq!(c.csi[0].0, vec![4, 2]);
    // param 0 (the 4) was followed by a colon.
    assert_eq!(c.csi[0].3, vec![true, false]);
}

#[test]
fn csi_colon_rgb() {
    let c = parse(b"\x1b[38:2:255:0:0m");
    assert_eq!(c.csi[0].0, vec![38, 2, 255, 0, 0]);
    assert_eq!(c.csi[0].3, vec![true, true, true, true, false]);
}

#[test]
fn csi_private_marker_intermediate() {
    let c = parse(b"\x1b[?25h");
    assert_eq!(c.csi[0].2, 'h');
    assert_eq!(c.csi[0].1, vec![b'?']);
    assert_eq!(c.csi[0].0, vec![25]);
}

#[test]
fn esc_dispatch_with_intermediate() {
    let c = parse(b"\x1b(0");
    assert_eq!(c.esc[0].0, vec![b'(']);
    assert_eq!(c.esc[0].1, b'0');
}

#[test]
fn osc_bel_terminated() {
    let c = parse(b"\x1b]0;title\x07");
    assert_eq!(c.osc.len(), 1);
    assert_eq!(c.osc[0], vec![b"0".to_vec(), b"title".to_vec()]);
}

#[test]
fn osc_st_terminated() {
    let c = parse(b"\x1b]2;x\x1b\\");
    assert_eq!(c.osc.len(), 1);
    assert_eq!(c.osc[0], vec![b"2".to_vec(), b"x".to_vec()]);
}

#[test]
fn osc_payload_over_limit_is_discarded() {
    // A payload at/under the cap dispatches; one byte over is dropped whole.
    let mut under = Parser::new();
    let mut c = Collecting::default();
    under.advance(&mut c, b"\x1b]52;c;");
    under.advance(&mut c, &vec![b'a'; 1024]);
    under.advance(&mut c, b"\x07");
    assert_eq!(c.osc.len(), 1, "small OSC must dispatch");

    let mut over = Parser::new();
    let mut c = Collecting::default();
    over.advance(&mut c, b"\x1b]52;c;");
    over.advance(&mut c, &vec![b'a'; 1_048_577]); // MAX_OSC_PAYLOAD + 1
    over.advance(&mut c, b"\x07");
    assert!(c.osc.is_empty(), "oversized OSC must be discarded");

    // A parser that saw an oversized OSC still parses the next one cleanly.
    over.advance(&mut c, b"\x1b]0;ok\x07");
    assert_eq!(c.osc.len(), 1);
    assert_eq!(c.osc[0], vec![b"0".to_vec(), b"ok".to_vec()]);
}

#[test]
fn osc_single_call_payload_over_limit_is_discarded() {
    // A single `advance` call (deferred path) must still cap the payload.
    let mut p = Parser::new();
    let mut c = Collecting::default();
    let mut buf = b"\x1b]0;".to_vec();
    buf.extend_from_slice(&vec![b'a'; 1_048_577]); // MAX_OSC_PAYLOAD + 1
    buf.push(0x07);
    p.advance(&mut c, &buf);
    assert!(
        c.osc.is_empty(),
        "single-call oversized OSC must be discarded"
    );

    // The parser still works afterward.
    p.advance(&mut c, b"\x1b]0;ok\x07");
    assert_eq!(c.osc.len(), 1);
    assert_eq!(c.osc[0], vec![b"0".to_vec(), b"ok".to_vec()]);
}

#[test]
fn osc_via_c1_consuming_trailing_partial_does_not_panic() {
    // Regression: a raw-C1 OSC introducer + payload can consume past a
    // trailing UTF-8 partial, so a later ground re-entry must not underflow
    // `valid_end - i`. The trailing bytes are raw C1 controls (executed), not
    // a partial code point.
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, &[0x04, 0x9d, 0xff, 0xf1, 0x8a, 0x87]);
    assert_eq!(c.executed, vec![0x04, 0x8a, 0x87]);
    assert!(c.printed.is_empty());
}

#[test]
fn utf8_split_across_chunks() {
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, &[0xce]); // first byte of λ
    assert!(c.printed.is_empty());
    p.advance(&mut c, &[0xbb]); // second byte
    assert_eq!(c.printed, "λ");
}

#[test]
fn utf8_partial_abort_emits_single_replacement() {
    // A 3-byte lead (U+4xxx) followed by a NON-continuation byte must emit
    // exactly one U+FFFD for the aborted partial and then decode the byte —
    // NOT re-emit the stale truncated code point as a spurious control char.
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, &[0xe4]); // partial lead, 2 continuations expected
    assert!(c.printed.is_empty());
    p.advance(&mut c, b"X");
    assert_eq!(c.printed, "\u{FFFD}X");
    assert!(
        c.executed.is_empty(),
        "aborted partial must not emit a control: {:?}",
        c.executed
    );
}

#[test]
fn utf8_partial_abort_after_continuation_emits_single_replacement() {
    // A partial that consumes a continuation byte and THEN hits a
    // non-continuation byte must still emit exactly one U+FFFD — not the
    // stale truncated code point plus a replacement char.
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, &[0xe4]); // partial lead, 2 continuations expected
    assert!(c.printed.is_empty());
    p.advance(&mut c, &[0x80, b'X']); // one continuation, then non-continuation
    assert_eq!(c.printed, "\u{FFFD}X");
    assert!(c.executed.is_empty());
}

#[test]
fn paramless_csi_after_aborted_csi_dispatches_empty() {
    // `CSI 1` (digit, no final) followed by `ESC [ H` must reset the partial
    // parameter, so the param-less `CSI H` dispatches `[]` (not `[0]` from a
    // stale `param_empty == false`).
    let mut p = Parser::new();
    let mut c = Collecting::default();
    p.advance(&mut c, b"\x1b[1");
    p.advance(&mut c, b"\x1b[H");
    assert_eq!(c.csi.len(), 1);
    assert_eq!(c.csi[0].0, Vec::<u16>::new());
    assert_eq!(c.csi[0].2, 'H');
}

#[test]
fn standalone_c1_control_executes() {
    // 0x9b (CSI) standalone byte → enters CSI entry (8-bit form); no final
    // byte arrives, so nothing is dispatched or executed.
    let c = parse(&[0x9b]);
    assert!(c.executed.is_empty());
    assert!(c.csi.is_empty());
}

#[test]
fn event_collector_api() {
    let mut p = Parser::new();
    let events = p.parse_events(b"\x1b[31mX");
    assert_eq!(
        events,
        vec![
            Event::CsiDispatch {
                params: vec![31].into_iter().collect(),
                intermediates: Vec::new().into_iter().collect(),
                final_byte: b'm',
            },
            Event::Print('X'),
        ]
    );
}

#[test]
fn dcs_hook_put_unhook() {
    let mut p = Parser::new();
    let events = p.parse_events(b"\x1bP1;2$rpayload\x1b\\");
    assert_eq!(
        events[0],
        Event::DcsHook {
            params: vec![1, 2].into_iter().collect(),
            intermediates: vec![b'$'].into_iter().collect(),
            final_byte: b'r',
        }
    );
    let puts: Vec<u8> = events[1..8]
        .iter()
        .map(|e| match e {
            Event::DcsPut(b) => *b,
            other => panic!("expected DcsPut, got {other:?}"),
        })
        .collect();
    assert_eq!(puts, b"payload");
    assert_eq!(events[8], Event::DcsUnhook);
    // The `\` of the `ESC \` ST terminator dispatches as an escape final,
    // matching vte (the differential baseline).
    assert_eq!(
        events[9],
        Event::EscDispatch {
            intermediates: Vec::new().into_iter().collect(),
            byte: b'\\',
        }
    );
}

#[test]
fn ris_esc_dispatch_empty_intermediates() {
    let c = parse(b"\x1bc");
    assert_eq!(c.esc, vec![(Vec::new(), b'c')]);
}

#[test]
fn dcs_no_params_is_empty() {
    let mut p = Parser::new();
    let events = p.parse_events(b"\x1bPqpayload\x1b\\");
    assert_eq!(
        events[0],
        Event::DcsHook {
            params: Vec::new().into_iter().collect(),
            intermediates: Vec::new().into_iter().collect(),
            final_byte: b'q',
        }
    );
}

#[test]
fn multiple_csi_params() {
    let c = parse(b"\x1b[1;2;3m");
    assert_eq!(c.csi[0].0, vec![1, 2, 3]);
    assert_eq!(c.csi[0].2, 'm');
}

#[test]
fn csi_params_capped_at_32() {
    // A malicious stream with thousands of `;` must not grow the param
    // vector unboundedly — only the first 32 are kept (vte/rio-vt cap).
    let mut seq = vec![0x1b, b'['];
    for _ in 0..1000 {
        seq.push(b'7');
        seq.push(b';');
    }
    seq.push(b'm');
    let c = parse(&seq);
    assert_eq!(c.csi.len(), 1);
    assert_eq!(c.csi[0].0.len(), 32);
    assert_eq!(c.csi[0].2, 'm');
}

// ── 8-bit C1 handling (DEC "anywhere" transitions) ──────────────────────

#[test]
fn c1_csi_introducer_starts_sequence() {
    // 0x9b is the 8-bit form of CSI.
    let c = parse(b"\x9b1;2H");
    assert_eq!(c.csi.len(), 1);
    assert_eq!(c.csi[0].0, vec![1, 2]);
    assert_eq!(c.csi[0].2, 'H');
}

#[test]
fn c1_osc_introducer_and_st_terminator() {
    // 0x9d = OSC, 0x9c = ST (8-bit).
    let c = parse(b"\x9d0;title\x9c");
    assert_eq!(c.osc, vec![vec![b"0".to_vec(), b"title".to_vec()]]);
}

#[test]
fn c1_aborts_in_progress_csi() {
    // A raw CSI (0x9b) inside a 7-bit CSI aborts it and starts fresh.
    let c = parse(b"\x1b[1;\x9b2;3m");
    assert_eq!(c.csi.len(), 1);
    assert_eq!(c.csi[0].0, vec![2, 3]);
    assert_eq!(c.csi[0].2, 'm');
}

#[test]
fn c1_st_is_noop_terminator() {
    // 0x9c (ST) inside a CSI is a no-op and returns to ground; a following
    // printable char is printed.
    let c = parse(b"\x1b[1\x9cX");
    assert!(c.csi.is_empty());
    assert_eq!(c.printed, "X");
}

#[test]
fn c1_other_controls_execute_and_cancel() {
    // 0x84 (IND, a non-introducer C1) inside CSI executes and cancels it.
    let c = parse(b"\x1b[1\x84");
    assert!(c.csi.is_empty());
    assert_eq!(c.executed, vec![0x84]);
}
