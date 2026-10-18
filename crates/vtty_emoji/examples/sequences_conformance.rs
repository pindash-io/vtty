//! Conformance check against Unicode's `emoji-sequences.txt` (the RGI
//! sequence catalogue: basic emoji, keycaps, flags, modifiers, tags).
//!
//! - `Basic_Emoji` (single char) must collapse to `[2]`.
//! - `Emoji_Keycap_Sequence` (`#*0-9 VS16 U+20E3`) must collapse to `[2, 0, 0]`.
//! - `RGI_Emoji_Flag_Sequence` (RI pair) must collapse to `[2, 0]`.
//! - `RGI_Emoji_Modifier_Sequence` (base + skin tone) must collapse to `[2, 0]`.
//! - `RGI_Emoji_Tag_Sequence` (subdivision flag) must collapse to `[2, 0, …]`.
//!
//! Run:
//!
//! ```bash
//! curl -sL <https://www.unicode.org/Public/17.0.0/emoji/emoji-sequences.txt> -o /tmp/seqs.txt
//! cargo run --release -p vtty_emoji --example sequences_conformance -- /tmp/seqs.txt
//! ```
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/seqs.txt".into());
    let data = std::fs::read_to_string(&path).unwrap();
    let mut basic_ok = 0_usize;
    let mut basic_bad = 0_usize;
    let mut keycap_ok = 0_usize;
    let mut keycap_bad = 0_usize;
    let mut flag_ok = 0_usize;
    let mut flag_bad = 0_usize;
    let mut modifier_ok = 0_usize;
    let mut modifier_bad = 0_usize;
    let mut tag_ok = 0_usize;
    let mut tag_bad = 0_usize;
    let mut bad: Vec<String> = Vec::new();
    for line in data.lines() {
        // `cps ; kind ; name # …` — three `;`-fields, then a comment.
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(';');
        let Some(cps) = fields.next().map(str::trim) else {
            continue;
        };
        let Some(kind) = fields.next().map(str::trim) else {
            continue;
        };
        // Expand `CP..CP` ranges (Basic_Emoji only) into individual chars.
        let mut seq: Vec<char> = Vec::new();
        for tok in cps.split_whitespace() {
            if let Some((a, b)) = tok.split_once("..") {
                let a = u32::from_str_radix(a, 16).unwrap();
                let b = u32::from_str_radix(b, 16).unwrap();
                seq.push(char::from_u32(a).unwrap());
                let _ = b;
            } else {
                seq.push(
                    u32::from_str_radix(tok, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .unwrap(),
                );
            }
        }
        let s: String = seq.iter().collect();
        let w = vtty_emoji::cell_widths(&s);
        let ok = match kind {
            "Basic_Emoji" => {
                // Single char (already presentation) → `[2]`, or a text-default
                // char + VS16 → `[2, 0]` — both collapse to one 2-cell grapheme.
                if w.first() == Some(&2) && w.iter().skip(1).all(|&x| x == 0) {
                    basic_ok += 1;
                    true
                } else {
                    basic_bad += 1;
                    false
                }
            }
            "Emoji_Keycap_Sequence" => {
                if w == [2, 0, 0] {
                    keycap_ok += 1;
                    true
                } else {
                    keycap_bad += 1;
                    false
                }
            }
            "RGI_Emoji_Flag_Sequence" => {
                if w == [2, 0] {
                    flag_ok += 1;
                    true
                } else {
                    flag_bad += 1;
                    false
                }
            }
            "RGI_Emoji_Modifier_Sequence" => {
                if w == [2, 0] {
                    modifier_ok += 1;
                    true
                } else {
                    modifier_bad += 1;
                    false
                }
            }
            "RGI_Emoji_Tag_Sequence" => {
                if w.first() == Some(&2) && w.iter().skip(1).all(|&x| x == 0) {
                    tag_ok += 1;
                    true
                } else {
                    tag_bad += 1;
                    false
                }
            }
            _ => true,
        };
        if !ok && bad.len() < 12 {
            bad.push(format!("{kind}: {s:?} -> {w:?}"));
        }
    }
    println!("basic    emoji: {basic_ok} ok, {basic_bad} bad");
    println!("keycap   sequence: {keycap_ok} ok, {keycap_bad} bad");
    println!("flag     sequence: {flag_ok} ok, {flag_bad} bad");
    println!("modifier sequence: {modifier_ok} ok, {modifier_bad} bad");
    println!("tag      sequence: {tag_ok} ok, {tag_bad} bad");
    for b in bad {
        println!("  {b}");
    }
}
