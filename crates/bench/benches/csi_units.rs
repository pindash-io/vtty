// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Per-sequence CSI cost: a few dozen KiB of one repeated sequence, so a change
//! to the CSI fast path can be checked in seconds instead of running the whole
//! corpus suite.
//!
//! ```text
//! cargo bench -p vtty_bench --bench csi_units                     # ~30 s
//! cargo bench -p vtty_bench --bench csi_units -- "sgr_38"         # one shape
//! cargo bench -p vtty_bench --bench csi_units -- --save-baseline before
//! cargo bench -p vtty_bench --bench csi_units -- --baseline before   # compare
//! ```
//!
//! Throughput is reported per *sequence* (`Melem/s` — `1e9 / elem_s` is the
//! cost of one dispatch), which is the unit the parser's fast paths are about.
//! The baseline pair above is the supported way to A/B a change: criterion
//! compares each shape against the stored run in the same machine state, so it
//! resolves differences that a cross-binary comparison cannot.

#![allow(missing_docs, reason = "criterion_group! emits public fns in a bin")]

use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
};

/// Sequences per iteration. Small enough that a full run is a few seconds.
const REPS: usize = 2048;

/// One entry per CSI shape the parser specialises, cheapest first.
const UNITS: &[(&str, &[u8])] = &[
    ("empty_m", b"\x1b[m"),
    ("sgr_0m", b"\x1b[0m"),
    ("sgr_31m", b"\x1b[31m"),
    ("sgr_1;31m", b"\x1b[1;31m"),
    ("sgr_1;31;44m", b"\x1b[1;31;44m"),
    ("sgr_38;5;196m", b"\x1b[38;5;196m"),
    ("sgr_38;2;11;22;33m", b"\x1b[38;2;11;22;33m"),
    ("cup_0;0H", b"\x1b[0;0H"),
    ("cup_10;20H", b"\x1b[10;20H"),
    ("cup_5A", b"\x1b[5A"),
];

/// Counting sink: every `Perform` callback is a no-op default except the ones
/// that consume parsed input, so the numbers are parser cost.
#[derive(Default)]
struct Sink {
    units: u64,
}

impl vtty::Perform for Sink {
    fn print(&mut self, _c: char) {
        self.units += 1;
    }

    fn print_bytes(&mut self, s: &[u8]) {
        self.units += s.len() as u64;
    }

    fn csi_dispatch(&mut self, params: &[i32], _sep: u16, _ints: &[u8], _final_byte: u8) {
        self.units += params.len() as u64;
    }
}

fn corpus(unit: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(unit.len() * REPS);
    for _ in 0..REPS {
        out.extend_from_slice(unit);
    }
    out
}

/// A deterministic shuffle of every shape. Repeating one sequence lets the
/// branch predictor learn it perfectly, which flatters whichever path that
/// shape takes; a real stream interleaves them.
fn mixed() -> Vec<u8> {
    let mut state = 0x243f_6a88_85a3_08d3_u64;
    let mut out = Vec::with_capacity(REPS * 8);
    for _ in 0..REPS {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let (_, unit) = UNITS[(state >> 16) as usize % UNITS.len()];
        out.extend_from_slice(unit);
    }
    out
}

fn bench_pair(group: &mut BenchmarkGroup<'_, WallTime>, name: &str, bytes: &[u8]) {
    group.bench_with_input(BenchmarkId::new("vtty", name), bytes, |b, bytes| {
        b.iter_batched(
            || (vtty::Parser::new(), Sink::default()),
            |(mut parser, mut sink)| {
                parser.advance(&mut sink, bytes);
                sink
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_with_input(BenchmarkId::new("vtty-simd", name), bytes, |b, bytes| {
        b.iter_batched(
            || (vtty::Parser::with_simd(), Sink::default()),
            |(mut parser, mut sink)| {
                parser.advance(&mut sink, bytes);
                sink
            },
            BatchSize::LargeInput,
        );
    });
}

fn csi_units(c: &mut Criterion) {
    let mut group = c.benchmark_group("csi_unit");
    group.sample_size(20);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    group.throughput(Throughput::Elements(REPS as u64));

    for (name, unit) in UNITS {
        bench_pair(&mut group, name, &corpus(unit));
    }
    bench_pair(&mut group, "mixed", &mixed());
    group.finish();
}

criterion_group!(benches, csi_units);
criterion_main!(benches);
