//! PGO profiling workload: runs the parser over representative terminal
//! traffic (CSI-dense SGR/truecolor/cursor + OSC + plain ASCII), exercising
//! both the scalar and SIMD fast paths.

#![allow(
    missing_docs,
    reason = "profiling workload, not part of the public API"
)]

use vtty::{Parser, Perform};

#[derive(Debug, Default)]
struct Sink {
    n: u64,
}

impl Perform for Sink {
    fn print_bytes(&mut self, s: &[u8]) {
        self.n += s.len() as u64;
    }
    fn print(&mut self, _c: char) {
        self.n += 1;
    }
    fn csi_dispatch(&mut self, _p: &[i32], _s: u16, _i: &[u8], _f: u8) {
        self.n += 1;
    }
    fn osc_dispatch(&mut self, _p: &[&[u8]], _b: bool) {
        self.n += 1;
    }
}

fn main() {
    let mut input = Vec::with_capacity(8 * 1024 * 1024);
    for _ in 0..100_000 {
        input.extend_from_slice(b"\x1b[38;2;255;128;0m"); // truecolor SGR
        input.extend_from_slice(b"\x1b[1;31;42m"); // SGR churn
        input.extend_from_slice(b"\x1b[100;42H"); // cursor motion
        input.extend_from_slice(b"\x1b]0;title\x1b\\"); // OSC title
        input.extend_from_slice(b"the quick brown fox "); // plain ASCII
    }

    let mut total = 0_u64;
    for _ in 0..20 {
        let mut scalar = Parser::new();
        let mut s1 = Sink::default();
        scalar.advance(&mut s1, &input);
        total += s1.n;

        let mut simd = Parser::with_simd();
        let mut s2 = Sink::default();
        simd.advance(&mut s2, &input);
        total += s2.n;
    }
    std::hint::black_box(total);
}
