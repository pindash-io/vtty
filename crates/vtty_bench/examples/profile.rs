//! Profiling harness: run one corpus through the parser in a tight loop so
//! `sample` (macOS) can attribute time to hot functions.
//!
//! ```bash
//! cargo build -p vtty_bench --release --example profile
//! target/release/examples/profile <case> [simd|scalar] &
//! sample <pid> 8 -file /tmp/profile.txt
//! ```
//!
//! Cases: `plain`, `utf8_mixed`, `sgr_churn`, `cursor_motion`.

use std::hint::black_box;
use std::time::{Duration, Instant};

fn main() {
    let case = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "utf8_mixed".into());
    let simd = std::env::args()
        .nth(2)
        .map(|s| s == "simd")
        .unwrap_or(false);

    let bytes = corpus(&case);
    let mut parser = if simd {
        vtty::Parser::with_simd()
    } else {
        vtty::Parser::new()
    };
    let mut sink = Sink::default();

    // Warm up once so SIMD level detection and caches settle.
    parser.advance(&mut sink, &bytes);

    let start = Instant::now();
    let mut iterations = 0_u64;
    while start.elapsed() < Duration::from_secs(20) {
        for _ in 0..64 {
            parser.advance(&mut sink, &bytes);
        }
        iterations += 64;
    }
    let elapsed = start.elapsed();
    let total_bytes = iterations as u128 * bytes.len() as u128;
    let gibs = total_bytes as f64 / elapsed.as_secs_f64() / (1 << 30) as f64;
    println!("{case} simd={simd}: {iterations} iters, {:.2} GiB/s", gibs);
    black_box(&sink);
}

#[derive(Default)]
struct Sink {
    printed: u64,
    csis: u64,
}

impl vtty::Perform for Sink {
    fn print(&mut self, _c: char) {
        self.printed += 1;
    }
    fn print_bytes(&mut self, s: &[u8]) {
        self.printed += s.len() as u64;
    }
    fn print_codepoints(&mut self, cps: &[u32]) {
        self.printed += cps.len() as u64;
    }
    fn csi_dispatch(&mut self, _p: &[i32], _s: u16, _i: &[u8], _f: u8) {
        self.csis += 1;
    }
}

fn corpus(case: &str) -> Vec<u8> {
    const BYTES: usize = 2 * 1024 * 1024;
    match case {
        "plain" => {
            let line = "the quick brown fox jumps over the lazy dog 0123456789 lorem ipsum dolor sit amet\r\n";
            line.repeat(BYTES / line.len() + 1).into_bytes()
        }
        "utf8_mixed" => {
            let mut line = String::new();
            for i in 0..40 {
                line += &format!("ascii{i} ");
                line += "日本語テキスト ";
                line += "🚀✨🎯 ";
                line += "e\u{301} ";
            }
            line += "\r\n";
            line.repeat(BYTES / line.len() + 1).into_bytes()
        }
        "sgr_churn" => {
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
            line.repeat(BYTES / line.len() + 1).into_bytes()
        }
        "cursor_motion" => {
            let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            let mut line = String::new();
            for r in 0..24 {
                for c in 0..80 {
                    let ch = text.as_bytes()[(r * 80 + c) % 26] as char;
                    line += &format!("\x1b[{r};{c}H{ch}");
                }
            }
            line += "\r\n";
            line.repeat(BYTES / line.len() + 1).into_bytes()
        }
        _ => {
            eprintln!("unknown case {case}; try plain, utf8_mixed, sgr_churn, cursor_motion");
            std::process::exit(1);
        }
    }
}
