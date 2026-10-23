//! Unified emoji conformance engine (see `CONFORMANCE.md`).
//!
//! Validates all four Unicode 17.0.0 emoji data files in one pass:
//!
//! ```bash
//! cargo run --release -p vtty_width --example conformance -- \
//!   /tmp/emoji-test.txt /tmp/emoji-zwj-sequences.txt \
//!   /tmp/emoji-variation-sequences.txt /tmp/emoji-sequences.txt
//! ```

/// A sequence collapses to ONE 2-cell grapheme: lead 2, every continuation 0.
fn collapse(w: &[usize]) -> bool {
    w.first() == Some(&2) && w.iter().skip(1).all(|&x| x == 0)
}

/// Parse a whitespace-separated codepoint field, expanding `CP..CP` ranges.
fn parse_cps(field: &str) -> Vec<char> {
    let mut out = Vec::new();
    for tok in field.split_whitespace() {
        if let Some((a, b)) = tok.split_once("..") {
            let a = u32::from_str_radix(a, 16).unwrap();
            let b = u32::from_str_radix(b, 16).unwrap();
            out.push(char::from_u32(a).unwrap());
            let _ = b;
        } else {
            out.push(
                u32::from_str_radix(tok, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap(),
            );
        }
    }
    out
}

fn widths(cps: &[char]) -> Vec<usize> {
    let s: String = cps.iter().collect();
    vtty_width::cell_widths(&s)
}

struct Tally {
    ok: usize,
    bad: usize,
}

impl Tally {
    const fn new() -> Self {
        Self { ok: 0, bad: 0 }
    }
    fn bump(&mut self, ok: bool) {
        if ok {
            self.ok += 1;
        } else {
            self.bad += 1;
        }
    }
}

fn emoji_test(data: &str) -> (Tally, Tally, Tally, Tally, Vec<String>) {
    let mut fq = Tally::new();
    let mut mq = Tally::new();
    let mut unq = Tally::new();
    let mut comp = Tally::new();
    let mut bad = Vec::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((cps, rest)) = line.split_once(';') else {
            continue;
        };
        let Some((qual, _)) = rest.split_once('#') else {
            continue;
        };
        let qual = qual.trim();
        let chars = parse_cps(cps);
        if chars.is_empty() {
            continue;
        }
        let w = widths(&chars);
        let s: String = chars.iter().collect();
        let ok = match qual {
            "fully-qualified" | "minimally-qualified" => {
                let t = if qual == "fully-qualified" {
                    &mut fq
                } else {
                    &mut mq
                };
                let ok = collapse(&w);
                t.bump(ok);
                ok
            }
            "unqualified" => {
                let ok = if chars.len() == 1 {
                    let wide_cjk = w == [2] && vtty_width::cell_width(chars[0]) == Some(2);
                    w == [1] || wide_cjk
                } else if chars.contains(&'\u{200D}') {
                    collapse(&w)
                } else {
                    // Keycap without VS16 → text.
                    w == [1, 0]
                };
                unq.bump(ok);
                ok
            }
            "component" => {
                let is_skin_tone = matches!(chars[0], '\u{1F3FB}'..='\u{1F3FF}');
                let ok = if is_skin_tone { w == [1] } else { w == [2] };
                comp.bump(ok);
                ok
            }
            _ => true,
        };
        if !ok && bad.len() < 15 {
            bad.push(format!("{s:?} {cps} {qual} got={w:?}"));
        }
    }
    (fq, mq, unq, comp, bad)
}

fn zwj_sequences(data: &str) -> (Tally, Vec<String>) {
    let mut t = Tally::new();
    let mut bad = Vec::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((cps, _)) = line.split_once(';') else {
            continue;
        };
        let chars = parse_cps(cps);
        if chars.is_empty() {
            continue;
        }
        let w = widths(&chars);
        let ok = collapse(&w);
        t.bump(ok);
        if !ok && bad.len() < 15 {
            bad.push(format!("{} got={w:?}", cps));
        }
    }
    (t, bad)
}

fn variation_sequences(data: &str) -> (Tally, Tally, Tally, Vec<String>) {
    let mut text = Tally::new();
    let mut emoji = Tally::new();
    let mut keycap = Tally::new();
    let mut bad = Vec::new();
    for line in data.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((cps, style)) = line.split_once(';') else {
            continue;
        };
        let style = style.trim().trim_end_matches(';').trim();
        let chars = parse_cps(cps);
        if chars.len() < 2 {
            continue;
        }
        let w = widths(&chars);
        let s: String = chars.iter().collect();
        let ok = match style {
            "text style" => {
                let ok = w == [1, 0];
                text.bump(ok);
                ok
            }
            "emoji style" => {
                let is_keycap = matches!(chars[0], '0'..='9' | '#' | '*');
                if is_keycap {
                    let ok = w == [1, 0];
                    keycap.bump(ok);
                    ok
                } else {
                    let ok = w == [2, 0];
                    emoji.bump(ok);
                    ok
                }
            }
            _ => true,
        };
        if !ok && bad.len() < 15 {
            bad.push(format!("{s:?} {cps} {style} got={w:?}"));
        }
    }
    (text, emoji, keycap, bad)
}

fn sequences(data: &str) -> (Tally, Tally, Tally, Tally, Tally, Vec<String>) {
    let mut basic = Tally::new();
    let mut keycap = Tally::new();
    let mut flag = Tally::new();
    let mut modifier = Tally::new();
    let mut tag = Tally::new();
    let mut bad = Vec::new();
    for line in data.lines() {
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
        let chars = parse_cps(cps);
        if chars.is_empty() {
            continue;
        }
        let w = widths(&chars);
        let s: String = chars.iter().collect();
        let ok = match kind {
            "Basic_Emoji" | "RGI_Emoji_Flag_Sequence" | "RGI_Emoji_Modifier_Sequence" => {
                let t = match kind {
                    "Basic_Emoji" => &mut basic,
                    "RGI_Emoji_Flag_Sequence" => &mut flag,
                    _ => &mut modifier,
                };
                let ok = collapse(&w);
                t.bump(ok);
                ok
            }
            "Emoji_Keycap_Sequence" => {
                let ok = w == [2, 0, 0];
                keycap.bump(ok);
                ok
            }
            "RGI_Emoji_Tag_Sequence" => {
                let ok = collapse(&w);
                tag.bump(ok);
                ok
            }
            _ => true,
        };
        if !ok && bad.len() < 15 {
            bad.push(format!("{s:?} {kind} got={w:?}"));
        }
    }
    (basic, keycap, flag, modifier, tag, bad)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = if args.len() == 4 {
        args
    } else {
        vec![
            "/tmp/emoji-test.txt".into(),
            "/tmp/emoji-zwj-sequences.txt".into(),
            "/tmp/emoji-variation-sequences.txt".into(),
            "/tmp/emoji-sequences.txt".into(),
        ]
    };
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}"));

    let (fq, mq, unq, comp, bad1) = emoji_test(&read(&paths[0]));
    let (zwj, bad2) = zwj_sequences(&read(&paths[1]));
    let (vtext, vemoji, vkeycap, bad3) = variation_sequences(&read(&paths[2]));
    let (basic, keycap, flag, modifier, tag, bad4) = sequences(&read(&paths[3]));

    println!("emoji-test.txt");
    println!("  fully-qualified:     {}/{}", fq.ok, fq.ok + fq.bad);
    println!("  minimally-qualified: {}/{}", mq.ok, mq.ok + mq.bad);
    println!("  unqualified:         {}/{}", unq.ok, unq.ok + unq.bad);
    println!("  component:           {}/{}", comp.ok, comp.ok + comp.bad);
    println!("emoji-zwj-sequences.txt: {}/{}", zwj.ok, zwj.ok + zwj.bad);
    println!(
        "emoji-variation-sequences.txt: text {}/{}, emoji {}/{}, keycap {}/{}",
        vtext.ok,
        vtext.ok + vtext.bad,
        vemoji.ok,
        vemoji.ok + vemoji.bad,
        vkeycap.ok,
        vkeycap.ok + vkeycap.bad,
    );
    println!("emoji-sequences.txt");
    println!(
        "  basic: {}/{}, keycap: {}/{}, flag: {}/{}, modifier: {}/{}, tag: {}/{}",
        basic.ok,
        basic.ok + basic.bad,
        keycap.ok,
        keycap.ok + keycap.bad,
        flag.ok,
        flag.ok + flag.bad,
        modifier.ok,
        modifier.ok + modifier.bad,
        tag.ok,
        tag.ok + tag.bad,
    );

    let mut any_bad = false;
    for b in bad1.iter().chain(&bad2).chain(&bad3).chain(&bad4) {
        if !any_bad {
            println!("mismatches:");
            any_bad = true;
        }
        println!("  {b}");
    }
}
