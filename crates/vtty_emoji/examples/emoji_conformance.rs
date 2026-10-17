//! Conformance check against Unicode's `emoji-test.txt` (the authoritative
//! qualified-emoji list, with fully-/minimally-qualified and unqualified
//! forms plus the emoji components).
//!
//! - FULLY-QUALIFIED entries collapse to `[2, 0, 0, ...]`.
//! - MINIMALLY-QUALIFIED entries (a ZWJ sequence missing a VS16) collapse to
//!   `[2, 0, 0, ...]` — the ZWJ family is still one 2-cell grapheme.
//! - UNQUALIFIED multi-codepoint entries (a ZWJ family whose base is
//!   text-default) collapse to `[2, 0, 0, ...]` — the joiner still merges.
//! - UNQUALIFIED single-codepoint entries are `[1]` (or `[2]` for wide CJK).
//! - COMPONENT entries (hair components) are `[2]`.
//!
//! Run:
//!   curl -sL https://www.unicode.org/Public/17.0.0/emoji/emoji-test.txt -o /tmp/emtest.txt
//!   cargo run --release -p vtty_emoji --example emoji_conformance -- /tmp/emtest.txt
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/emtest.txt".into());
    let data = std::fs::read_to_string(&path).unwrap();
    let mut fq = 0usize;
    let mut fq_ok = 0usize;
    let mut mq = 0usize;
    let mut mq_ok = 0usize;
    let mut unq = 0usize;
    let mut unq_ok = 0usize;
    let mut comp = 0usize;
    let mut comp_ok = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Format: <codepoints> ; <qualification> # <emoji> <ver> <name>
        let Some((cps, rest)) = line.split_once(';') else {
            continue;
        };
        let Some((qual, _rest)) = rest.split_once('#') else {
            continue;
        };
        let qual = qual.trim();
        let chars: Vec<char> = cps
            .split_whitespace()
            .filter_map(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
            .collect();
        if chars.is_empty() {
            continue;
        }
        let s: String = chars.iter().collect();
        let w = vtty_emoji::cell_widths(&s);
        // A 2-cell lead with every continuation 0 — one collapsed grapheme.
        let collapsed = w.first() == Some(&2) && w.iter().skip(1).all(|&x| x == 0);
        let mut ok = true;
        let mut want = String::new();
        match qual {
            "fully-qualified" => {
                fq += 1;
                want = "[2, 0, …]".into();
                if collapsed {
                    fq_ok += 1;
                } else {
                    ok = false;
                }
            }
            "minimally-qualified" => {
                mq += 1;
                want = "[2, 0, …]".into();
                if collapsed {
                    mq_ok += 1;
                } else {
                    ok = false;
                }
            }
            "unqualified" => {
                unq += 1;
                if chars.len() == 1 {
                    // Unqualified means TEXT-default: width is the East Asian
                    // width, so wide CJK symbols (〰 〽 🈂 🈷 ㊗ ㊙ …) are 2,
                    // everything else 1.
                    let wide_cjk =
                        w == [2] && vtty_emoji::cell_width(chars[0]) == Some(2);
                    want = "[1]".into();
                    if w == [1] || wide_cjk {
                        unq_ok += 1;
                    } else {
                        ok = false;
                    }
                } else if chars.contains(&'\u{200D}') {
                    // A ZWJ family whose base is text-default still merges.
                    want = "[2, 0, …]".into();
                    if collapsed {
                        unq_ok += 1;
                    } else {
                        ok = false;
                    }
                } else {
                    // A keycap (`#*0-9 U+20E3`) WITHOUT the VS16 is text.
                    want = "[1, 0]".into();
                    if w == [1, 0] {
                        unq_ok += 1;
                    } else {
                        ok = false;
                    }
                }
            }
            "component" => {
                comp += 1;
                // Hair components (1F9B0..=1F9B3) are emoji-presentation
                // (2 cells); skin-tone modifiers are 0-width combining, so a
                // LONE one is 1 cell.
                let is_skin_tone = matches!(chars[0], '\u{1F3FB}'..='\u{1F3FF}');
                want = if is_skin_tone { "[1]" } else { "[2]" }.into();
                if (is_skin_tone && w == [1]) || (!is_skin_tone && w == [2]) {
                    comp_ok += 1;
                } else {
                    ok = false;
                }
            }
            _ => {}
        }
        if !ok && bad.len() < 15 {
            bad.push(format!("{s:?} {cps} {qual} got={w:?} want={want}"));
        }
    }
    println!("fully-qualified:    {fq_ok}/{fq}");
    println!("minimally-qualified: {mq_ok}/{mq}");
    println!("unqualified:        {unq_ok}/{unq}");
    println!("component:          {comp_ok}/{comp}");
    println!("mismatches (first 15):");
    for b in bad {
        println!("  {b}");
    }
}
