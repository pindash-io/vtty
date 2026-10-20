//! Throughput comparison of grapheme cluster segmentation strategies.
//!
//! ```bash
//! cargo run -p vtty_grapheme --release --example bench
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use icu_segmenter::GraphemeClusterSegmenter;

fn corpus(case: &str) -> String {
    const N: usize = 256 * 1024;
    let line = match case {
        "ascii" => "the quick brown fox jumps over the lazy dog 0123456789\r\n",
        "cjk" => "中文字日本語テキスト中文テスト\r\n",
        "emoji" => "👨‍👩‍👧 🚀✨🎯 👍🏽\r\n",
        "combining" => "e\u{301} a\u{308} n\u{303}\r\n",
        "mixed" => "ascii 中文字 👨‍👩‍👧 e\u{301} 123\r\n",
        _ => unreachable!(),
    };
    line.repeat(N / line.len() + 1)
}

fn throughput<F: FnMut(&str) -> usize>(label: &str, text: &str, mut f: F) {
    // Warmup.
    for _ in 0..8 {
        black_box(f(text));
    }
    let start = Instant::now();
    let mut iters = 0_u64;
    while start.elapsed() < Duration::from_millis(1500) {
        for _ in 0..16 {
            black_box(f(text));
        }
        iters += 16;
    }
    let gib = iters as f64 * text.len() as f64 / (1 << 30) as f64;
    let rate = gib / start.elapsed().as_secs_f64();
    println!("  {label:<40} {rate:>8.2} GiB/s");
}

fn main() {
    for case in ["ascii", "cjk", "emoji", "combining", "mixed"] {
        let text = corpus(case);
        println!("{case} ({:.1} KiB):", text.len() as f64 / 1024.0);

        throughput("vtty_grapheme graphemes().count()", &text, |t| {
            vtty_grapheme::graphemes(t).count()
        });
        throughput("vtty_grapheme cluster_width_sum()", &text, |t| {
            vtty_grapheme::cluster_width_sum(t)
        });
        throughput("vtty_emoji cell_width_sum()", &text, |t| {
            vtty_emoji::cell_width_sum(t)
        });
        throughput("icu_segmenter grapheme count", &text, |t| {
            GraphemeClusterSegmenter::new().segment_str(t).count()
        });
    }
}
