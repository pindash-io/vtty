// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! VT input throughput across parsers: vtty, vte, vt100, vtparse and rio-vt.
//!
//! Every parser feeds the same byte corpora into a counting sink so the
//! measurement is parser + dispatch cost, not grid/state mutation. vt100 is
//! the exception: it exposes no standalone parser, so its full screen
//! emulator (`vt100::Parser::process`) is measured as-is.
//!
//! The `vtty` (scalar) and `vtty-simd` (`Parser::with_simd`) paths are both
//! measured in one run — vtty is compiled with the `simd` feature and the
//! path is selected per-parser at runtime.

// The `criterion_group!` macro emits public functions; a benchmark binary has
// no public API to document.
#![allow(missing_docs, reason = "criterion_group! emits public fns in a bin")]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// Counting sink for vtty: every `Perform` callback has a no-op default, so
/// only the hot paths (printable runs, code-point batches and CSI dispatch)
/// are overridden.
#[derive(Debug, Default)]
struct VttySink {
    printed: u64,
    csis: u64,
}

impl vtty::Perform for VttySink {
    fn print(&mut self, _c: char) {
        self.printed += 1;
    }

    fn print_bytes(&mut self, s: &[u8]) {
        self.printed += s.len() as u64;
    }

    fn print_codepoints(&mut self, codepoints: &[u32]) {
        self.printed += codepoints.len() as u64;
    }

    fn csi_dispatch(&mut self, _params: &[i32], _sep: u16, _intermediates: &[u8], _final_byte: u8) {
        self.csis += 1;
    }
}

/// Counting sink for vte.
#[derive(Debug, Default)]
struct VteSink {
    printed: u64,
    csis: u64,
}

impl vte::Perform for VteSink {
    fn print(&mut self, _c: char) {
        self.printed += 1;
    }

    fn csi_dispatch(
        &mut self,
        _params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        self.csis += 1;
    }
}

/// Counting actor for vtparse: `VTActor` has no defaulted methods, so the
/// full surface is implemented.
#[derive(Debug, Default)]
struct VtparseSink {
    printed: u64,
    csis: u64,
}

impl vtparse::VTActor for VtparseSink {
    fn print(&mut self, _b: char) {
        self.printed += 1;
    }

    fn execute_c0_or_c1(&mut self, _control: u8) {}

    fn dcs_hook(
        &mut self,
        _mode: u8,
        _params: &[i64],
        _intermediates: &[u8],
        _ignored_excess_intermediates: bool,
    ) {
    }

    fn dcs_put(&mut self, _byte: u8) {}

    fn dcs_unhook(&mut self) {}

    fn esc_dispatch(
        &mut self,
        _params: &[i64],
        _intermediates: &[u8],
        _ignored_excess_intermediates: bool,
        _byte: u8,
    ) {
    }

    fn csi_dispatch(
        &mut self,
        _params: &[vtparse::CsiParam],
        _parameters_truncated: bool,
        _byte: u8,
    ) {
        self.csis += 1;
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]]) {}

    fn apc_dispatch(&mut self, _data: Vec<u8>) {}
}

/// Counting sink for rio-vt's standalone parser.
#[derive(Debug, Default)]
struct RioSink {
    printed: u64,
    csis: u64,
}

impl rio_vt::performer::parser::Perform for RioSink {
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        self.printed += codepoints.len() as u64;
    }

    fn csi_dispatch(
        &mut self,
        _params: &rio_vt::performer::parser::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        self.csis += 1;
    }
}

/// Register one parser benchmark for one case. Expands to inline closures so
/// `iter_batched` owns them (a helper function would move captured closures
/// out of `bench_function`'s `FnMut`).
macro_rules! bench_parser {
    ($group:expr, $parser:expr, $case:expr, $bytes:expr, $setup:expr, |$($arg:pat_param),*| $body:block) => {
        $group.bench_function(BenchmarkId::new($parser, $case), |b| {
            b.iter_batched(
                || $setup,
                |$($arg),*| $body,
                BatchSize::LargeInput,
            );
        });
    };
}

/// Plain ASCII line repeated to `total` bytes.
fn plain(total: usize) -> Vec<u8> {
    let line =
        "the quick brown fox jumps over the lazy dog 0123456789 lorem ipsum dolor sit amet\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// ASCII plus multibyte UTF-8 (CJK + emoji + combining marks) mixed into the
/// printable stream; exercises each parser's UTF-8 decode path.
fn utf8_mixed(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..40 {
        line += &format!("ascii{i} ");
        line += "日本語テキスト ";
        line += "🚀✨🎯 ";
        line += "e\u{301} ";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// 256-color pairs, truecolor bold, italic/underline toggles, resets between
/// every word.
fn ansi_mixed(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..10 {
        line += &format!(
            "\x1b[38;5;{}m\x1b[48;5;{}mword{i}\x1b[0m ",
            (i * 37) % 256,
            (i * 53 + 8) % 256
        );
        line += &format!(
            "\x1b[1;38;2;{};{};{}mbold\x1b[0m ",
            (i * 31) % 256,
            (i * 67) % 256,
            (i * 13) % 256
        );
        line += if i % 2 == 1 {
            "\x1b[3mital\x1b[0m "
        } else {
            "\x1b[4munder\x1b[0m "
        };
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Same word rhythm, only two styles alternating: isolates SGR parse and
/// dispatch cost.
fn ansi_two_styles(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..30 {
        line += if i % 2 == 0 {
            "\x1b[31mword\x1b[0m "
        } else {
            "\x1b[44mword\x1b[0m "
        };
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Escape density without SGR payloads: bare reset sequences between words,
/// measuring the state-machine and dispatch floor.
fn ansi_bare_csi(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for _ in 0..30 {
        line += "\x1b[mword ";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Full-screen repaint with a distinct truecolor fg+bg pair on every cell:
/// SGR-dense, param-heavy, style-churn worst case.
fn truecolor_unique(total: usize) -> Vec<u8> {
    let mut out = String::new();
    let mut frame = 0_usize;
    while out.len() < total {
        for y in 0..ROWS as usize {
            out += &format!("\x1b[{};1H", y + 1);
            for x in 0..COLS as usize {
                let v = (x * 7 + y * 13 + frame * 31) % 253;
                let w = (x * 11 + y * 3 + frame * 17) % 251;
                out += &format!("\x1b[38;2;{v};{v};{v}m\x1b[48;2;{w};{w};{w}m\u{2580}");
            }
        }
        frame += 1;
    }
    out.into_bytes()
}

/// Bracketed paste (`ESC[200~` … `ESC[201~`) wrapping content that includes
/// embedded SGR and control bytes, exercising paste-marker CSI dispatch plus
/// ground-state re-entry over mixed printable/control content.
fn bracketed_paste(total: usize) -> Vec<u8> {
    let mut line = String::new();
    line += "\x1b[200~";
    for i in 0..20 {
        line += &format!("pasted block {i}: ");
        line += "quoted 'text' and \"double\" quotes\t";
        line += "日本語 🚀 ";
        line += "\x1b[32mstyled\x1b[0m ";
        line += "\r\n";
    }
    line += "\x1b[201~";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// OSC-dense stream: window-title (`BEL`-terminated), clipboard base64
/// (`ST`-terminated), and iTerm2 file payloads, exercising OSC param/payload
/// accumulation across both terminator styles.
fn osc_heavy(total: usize) -> Vec<u8> {
    const B64: &str =
        "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5ejAxMjM0NTY3ODk";
    let mut line = String::new();
    for i in 0..20 {
        line += &format!("\x1b]0;window {i}\x07");
        line += &format!("\x1b]52;c;{B64}\x1b\\");
        line += &format!("\x1b]1337;File=name={i};size=8:QUJDREVGR0g=\x07");
        line += "text ";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// DCS/Sixel (`ESC P q` … `ESC \`) payloads exercising hook/put/unhook over
/// palette and raster data.
fn dcs_sixel(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..10 {
        line += "\x1bPq";
        line += &format!("#0;2;{i};{i};{i}");
        for row in 0..8 {
            line += &format!("${row}:@ABCDEFGHIJKLMNOP");
        }
        line += "\x1b\\";
        line += "sixel text ";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Short lines and many newlines: mostly C0 line-feed traffic with tiny
/// printable runs, the shape a scrolling log or `tail -f` produces.
fn scroll_storm(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..100 {
        line += &format!("line {i}\r\n");
    }
    line.repeat(total / line.len() + 1).into_bytes()
}

/// A foreground color change before nearly every character: heavy SGR and
/// style traffic rather than raw glyph volume.
fn sgr_churn(total: usize) -> Vec<u8> {
    let text = "the quick brown fox jumps";
    let mut line = String::new();
    let mut color = 0_usize;
    for _ in 0..10 {
        for ch in text.chars() {
            color = (color + 1) % 256;
            line += &format!("\x1b[38;5;{color}m{ch}");
        }
        line += "\x1b[0m";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// CSI cursor positioning before every write (`\x1b[{line};{col}H` + char),
/// sweeping the grid. Exercises the CSI fast path with positional params.
fn cursor_motion(total: usize) -> Vec<u8> {
    let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut line = String::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            let ch = text.as_bytes()[(r * COLS + c) as usize % 26] as char;
            line += &format!("\x1b[{r};{c}H{ch}");
        }
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// DEC 2026 Synchronized Output: escape-heavy repaint frames wrapped in
/// `\x1b[?2026h` / `\x1b[?2026l` (begin/end sync).
fn sync_churn(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for _ in 0..5 {
        line += "\x1b[?2026h";
        for i in 0..8 {
            line += &format!("\x1b[38;5;{}mstyled text\x1b[0m ", (i * 31) % 256);
        }
        line += "\x1b[2J\x1b[H";
        line += "\x1b[?2026l";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

fn corpus() -> Vec<(&'static str, Vec<u8>)> {
    const BYTES: usize = 2 * 1024 * 1024;
    vec![
        ("plain", plain(BYTES)),
        ("scroll_storm", scroll_storm(BYTES)),
        ("utf8_mixed", utf8_mixed(BYTES)),
        ("ansi_mixed", ansi_mixed(BYTES)),
        ("ansi_two_styles", ansi_two_styles(BYTES)),
        ("ansi_bare_csi", ansi_bare_csi(BYTES)),
        ("sgr_churn", sgr_churn(BYTES)),
        ("cursor_motion", cursor_motion(BYTES)),
        ("sync_churn", sync_churn(BYTES)),
        ("truecolor_unique", truecolor_unique(BYTES)),
        ("bracketed_paste", bracketed_paste(BYTES)),
        ("osc_heavy", osc_heavy(BYTES)),
        ("dcs_sixel", dcs_sixel(BYTES)),
    ]
}

fn bench_vt_input(c: &mut Criterion) {
    let cases = corpus();
    let mut group = c.benchmark_group("vt_input");
    group.sample_size(20);

    for (case, bytes) in &cases {
        let case: &str = case;
        let bytes: &[u8] = bytes.as_slice();
        group.throughput(Throughput::Bytes(bytes.len() as u64));

        bench_parser!(
            group,
            "vtty",
            case,
            bytes,
            (vtty::Parser::new(), VttySink::default()),
            |(mut parser, mut sink)| {
                parser.advance(&mut sink, bytes);
                sink
            }
        );

        bench_parser!(
            group,
            "vtty-simd",
            case,
            bytes,
            (vtty::Parser::with_simd(), VttySink::default()),
            |(mut parser, mut sink)| {
                parser.advance(&mut sink, bytes);
                sink
            }
        );

        bench_parser!(
            group,
            "vte",
            case,
            bytes,
            (vte::Parser::new(), VteSink::default()),
            |(mut parser, mut sink)| {
                parser.advance(&mut sink, bytes);
                sink
            }
        );

        bench_parser!(
            group,
            "vtparse",
            case,
            bytes,
            (vtparse::VTParser::new(), VtparseSink::default()),
            |(mut parser, mut sink)| {
                parser.parse(bytes, &mut sink);
                sink
            }
        );

        bench_parser!(
            group,
            "rio-vt",
            case,
            bytes,
            (rio_vt::performer::parser::Parser::new(), RioSink::default(),),
            |(mut parser, mut sink)| {
                parser.advance(&mut sink, bytes);
                sink
            }
        );

        bench_parser!(
            group,
            "vt100",
            case,
            bytes,
            vt100::Parser::new(
                std::num::NonZero::new(ROWS).expect("ROWS is non-zero"),
                std::num::NonZero::new(COLS).expect("COLS is non-zero"),
                0,
            ),
            |mut parser| {
                parser.process(bytes);
                parser
            }
        );
    }

    group.finish();
}

criterion_group!(benches, bench_vt_input);
criterion_main!(benches);
