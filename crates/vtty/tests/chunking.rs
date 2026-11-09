// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Chunking-invariance: a byte stream must parse to the same canonical byte
//! stream no matter how it is split across `advance` calls.
//!
//! This is the observable contract of the parser's streaming state — the
//! UTF-8 partial decoder, the OSC deferral, and the CSI/DCS parameter
//! accumulation must all make the result independent of chunk boundaries.
//! Each event's [`Event::write_bytes`] gives the canonical on-wire bytes, so
//! comparing the concatenation sidesteps the fact that a `PrintBytes` run is
//! coalesced differently when a chunk boundary splits it.

use smallvec::SmallVec;
use vtty::{Event, Parser};

/// The corpus: plain text, multi-byte UTF-8, malformed UTF-8, C0/C1 controls,
/// every escape-sequence family, and payloads long enough to cross the OSC
/// inline buffer (`SmallVec<u8, 56>`) and force deferred→accumulated copy.
fn corpus() -> Vec<Vec<u8>> {
    let mut corpus = vec![
        b"".to_vec(),
        b"hello".to_vec(),
        b"hello\nworld\r\n".to_vec(),
        "λ中🚀".as_bytes().to_vec(),
        "e\u{301}👨\u{200D}👩\u{200D}👧".as_bytes().to_vec(),
        b"\x1b[31mred\x1b[0m".to_vec(),
        b"\x1b[38:2::255;0;128mtruecolor\x1b[m".to_vec(),
        b"\x1b]0;title\x07".to_vec(),
        b"\x1b]0;title\x1b\\".to_vec(),
        b"\x1b]4;1;rgb:ff/00/00\x07".to_vec(),
        b"\x1bP1;2qpayload\x1b\\".to_vec(),
        b"\x1b_Gkitty\x1b\\".to_vec(),
        b"\x1b[2;4r\x1b[H\x1b[?25l\x1b[?7h".to_vec(),
        b"\x1b(0\x1b#8\x1b7\x1b8".to_vec(),
        b"\x1b[1 2m".to_vec(),
        b"\x9b31m\x9d0;x\x9c".to_vec(),
        b"\xc2\x80\xc2\x9b".to_vec(),
        b"\xff\xc0\xaf abc".to_vec(),
        b"a\x1b".to_vec(),
        b"\xf0\x9f".to_vec(),
    ];

    // OSC past the 56-byte inline threshold: forces the deferred payload to be
    // flushed into `osc` when a chunk boundary lands inside it.
    let mut long_osc = b"\x1b]0;".to_vec();
    long_osc.extend(core::iter::repeat_n(b'a', 60));
    long_osc.push(0x07);
    corpus.push(long_osc);

    // A DCS passthrough long enough to cross chunk boundaries mid-payload.
    let mut long_dcs = b"\x1bPq".to_vec();
    long_dcs.extend(core::iter::repeat_n(b'x', 60));
    long_dcs.extend_from_slice(b"\x1b\\");
    corpus.push(long_dcs);

    corpus
}

/// Concatenate each event's canonical wire bytes.
fn to_bytes(events: &[Event]) -> Vec<u8> {
    let mut out = SmallVec::<u8, 64>::new();
    for event in events {
        event.write_bytes(&mut out);
    }
    out.to_vec()
}

/// Feed one chunk, appending its events' wire bytes to `out`.
fn feed(p: &mut Parser, out: &mut SmallVec<u8, 64>, chunk: &[u8]) {
    for event in p.parse_events(chunk) {
        event.write_bytes(out);
    }
}

fn parse_once(mut p: Parser, input: &[u8]) -> Vec<u8> {
    to_bytes(&p.parse_events(input))
}

fn parse_split(mut p: Parser, input: &[u8], split: usize) -> Vec<u8> {
    let mut out = SmallVec::<u8, 64>::new();
    feed(&mut p, &mut out, &input[..split]);
    feed(&mut p, &mut out, &input[split..]);
    out.to_vec()
}

fn parse_bytewise(mut p: Parser, input: &[u8]) -> Vec<u8> {
    let mut out = SmallVec::<u8, 64>::new();
    for &b in input {
        feed(&mut p, &mut out, core::slice::from_ref(&b));
    }
    out.to_vec()
}

fn chunking_check(make: impl Fn() -> Parser) {
    for input in corpus() {
        let whole = parse_once(make(), &input);
        for split in 0..=input.len() {
            assert_eq!(
                parse_split(make(), &input, split),
                whole,
                "split at {split} of {input:?}"
            );
        }
        assert_eq!(
            parse_bytewise(make(), &input),
            whole,
            "bytewise of {input:?}"
        );
    }
}

#[test]
fn chunking_invariance_scalar() {
    chunking_check(Parser::new);
}

#[cfg(feature = "simd")]
#[test]
fn chunking_invariance_simd() {
    chunking_check(Parser::with_simd);
}
