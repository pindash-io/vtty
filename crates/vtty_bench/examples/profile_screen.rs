//! Profiling harness for the full parser + screen pipeline: run one corpus
//! through `Parser::advance` into a `vtty_screen::Screen` sink in a tight
//! loop so `sample` (macOS) can attribute time to hot functions.
//!
//! ```bash
//! cargo build -p vtty_bench --release --example profile_screen
//! target/release/examples/profile_screen <case> [simd|scalar] &
//! sample <pid> 8 -file /tmp/profile_screen.txt
//! ```
//!
//! Cases: `sgr_nvim`, `sgr_churn`, `truecolor_unique`, `ansi_mixed`, `plain`.

use std::hint::black_box;
use std::time::{Duration, Instant};

const COLS: u32 = 80;
const ROWS: u32 = 24;

fn main() {
    let case = std::env::args().nth(1).unwrap_or_else(|| "sgr_nvim".into());
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
    let mut screen = vtty_screen::Screen::new(COLS, ROWS);

    // Warm up once so SIMD level detection and caches settle.
    parser.advance(&mut screen, &bytes);

    let start = Instant::now();
    let mut iterations = 0_u64;
    while start.elapsed() < Duration::from_secs(20) {
        for _ in 0..64 {
            parser.advance(&mut screen, &bytes);
        }
        iterations += 64;
    }
    let elapsed = start.elapsed();
    let total_bytes = iterations as u128 * bytes.len() as u128;
    let gibs = total_bytes as f64 / elapsed.as_secs_f64() / (1 << 30) as f64;
    println!("{case} simd={simd}: {iterations} iters, {:.2} GiB/s", gibs);
    black_box(&screen);
}

fn corpus(case: &str) -> Vec<u8> {
    const BYTES: usize = 2 * 1024 * 1024;
    match case {
        "plain" => {
            let line = "the quick brown fox jumps over the lazy dog 0123456789 lorem ipsum dolor sit amet\r\n";
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
        "truecolor_unique" => {
            let mut out = String::new();
            let mut frame = 0_usize;
            while out.len() < BYTES {
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
        "sgr_nvim" => {
            let mut line = String::new();
            for i in 0..30 {
                let r = (i * 37) % 256;
                let g = (i * 67) % 256;
                let b = (i * 13) % 256;
                line += &format!("\x1b[1m\x1b[38;2;{r};{g};{b}mfn\x1b[0m ");
                let w = (i * 53 + 8) % 256;
                line += &format!("\x1b[38;2;{w};{w};{w}m\x1b[48;2;{b};{b};{b}m\"text{i}\"\x1b[0m ");
                line += &format!("\x1b[4:3m\x1b[58:2::{r};{g};{b}mlink\x1b[0m ");
                line += "const v = ";
                line += &format!("{i}; ");
            }
            line += "\r\n";
            line.repeat(BYTES / line.len() + 1).into_bytes()
        }
        "ansi_mixed" => {
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
            line.repeat(BYTES / line.len() + 1).into_bytes()
        }
        _ => {
            eprintln!("unknown case {case}");
            std::process::exit(1);
        }
    }
}
