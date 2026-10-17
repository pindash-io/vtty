//! Conformance check against Unicode's `emoji-variation-sequences.txt` (the
//! authoritative list of valid `base + VS15/VS16` presentation sequences).
//!
//! - Every TEXT-STYLE sequence (`base + VS15`) must collapse to `[1, 0]`.
//! - Every EMOJI-STYLE sequence (`base + VS16`) must collapse to `[2, 0]`,
//!   EXCEPT the keycap bases (`#`, `*`, `0-9`) — a lone VS16 without the
//!   trailing U+20E3 is an INCOMPLETE keycap and is dropped (Ghostty drops a
//!   VS16 after a keycap base because it is not `Extended_Pictographic`), so
//!   those 12 stay `[1, 0]`. The complete `base VS16 U+20E3` keycap widens
//!   normally.
//!
//! Run:
//!   curl -sL https://www.unicode.org/Public/17.0.0/ucd/emoji/emoji-variation-sequences.txt -o /tmp/variation.txt
//!   cargo run --release -p vtty_emoji --example variation_conformance -- /tmp/variation.txt
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/variation.txt".into());
    let data = std::fs::read_to_string(&path).unwrap();
    let mut text_ok = 0usize;
    let mut text_bad = 0usize;
    let mut emoji_ok = 0usize;
    let mut keycap = 0usize;
    let mut emoji_bad = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for line in data.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((cps, style)) = line.split_once(';') else {
            continue;
        };
        let style = style.trim().trim_end_matches(';').trim();
        let mut parts = cps.split_whitespace();
        let Some(base) = parts
            .next()
            .and_then(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
        else {
            continue;
        };
        let Some(vs) = parts
            .next()
            .and_then(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
        else {
            continue;
        };
        let s: String = [base, vs].iter().collect();
        let w = vtty_emoji::cell_widths(&s);
        match style {
            "text style" => {
                if w == [1, 0] {
                    text_ok += 1;
                } else {
                    text_bad += 1;
                    if bad.len() < 10 {
                        bad.push(format!("{base:?} VS15 -> {w:?}"));
                    }
                }
            }
            "emoji style" => {
                let is_keycap = matches!(base, '0'..='9' | '#' | '*');
                if is_keycap {
                    // The incomplete keycap: lone VS16 is dropped (Ghostty).
                    if w == [1, 0] {
                        keycap += 1;
                    } else {
                        emoji_bad += 1;
                    }
                } else if w == [2, 0] {
                    emoji_ok += 1;
                } else {
                    emoji_bad += 1;
                    if bad.len() < 10 {
                        bad.push(format!("{base:?} VS16 -> {w:?}"));
                    }
                }
            }
            _ => {}
        }
    }
    println!("text style  (VS15): {text_ok} ok, {text_bad} bad");
    println!(
        "emoji style (VS16): {emoji_ok} ok, {keycap} keycap-incomplete, {emoji_bad} bad"
    );
    for b in bad {
        println!("  {b}");
    }
}
