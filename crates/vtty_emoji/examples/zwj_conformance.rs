//! Conformance check: every RGI_Emoji_ZWJ_Sequence in Unicode's
//! `emoji-zwj-sequences.txt` must collapse to `[2, 0, 0, ...]` (lead 2 cells,
//! every ZWJ continuation 0) via `cell_widths`.
//!
//! Run:
//!   curl -sL https://www.unicode.org/Public/17.0.0/emoji/emoji-zwj-sequences.txt -o /tmp/zwj.txt
//!   cargo run --release -p vtty_emoji --example zwj_conformance -- /tmp/zwj.txt
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/zwj.txt".into());
    let data = std::fs::read_to_string(&path).unwrap();
    let mut total = 0usize;
    let mut ok = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((cps, _)) = line.split_once(';') else {
            continue;
        };
        let cps: Vec<char> = cps
            .split_whitespace()
            .filter_map(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
            .collect();
        if cps.is_empty() {
            continue;
        }
        total += 1;
        let s: String = cps.iter().collect();
        let w = vtty_emoji::cell_widths(&s);
        let expect: Vec<usize> = std::iter::once(2)
            .chain(std::iter::repeat(0).take(cps.len() - 1))
            .collect();
        if w == expect {
            ok += 1;
        } else if bad.len() < 15 {
            bad.push(format!("{s:?} cps={cps:?} got={w:?}"));
        }
    }
    println!("total={total} ok={ok} bad={}", total - ok);
    for b in bad {
        println!("  {b}");
    }
}
