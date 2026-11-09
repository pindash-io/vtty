// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property test: feed curated and pseudo-random VT bytes through the parser
//! into a `Screen`, asserting the screen's structural invariants after every
//! chunk. Exercises the implicit state machines — autowrap, origin mode,
//! insert/replace, scroll margins, alt-screen swap, and the ring buffer — by
//! driving them with a deterministic byte generator.

use vtty::Parser;
use vtty_screen::Screen;

/// xorshift64 PRNG (deterministic per run).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// A structured-random VT-ish byte chunk: biased toward escape introducers,
/// CSI/OSC/DCS punctuation, params, mode/set letters, and printable ASCII,
/// with multi-byte UTF-8 leads and continuation bytes so the UTF-8 partial
/// and malformed-substitution paths are exercised too.
fn random_chunk(rng: &mut Rng) -> Vec<u8> {
    let len = 1 + (rng.next() % 64) as usize;
    let mut buf = Vec::with_capacity(len);
    for _ in 0..len {
        buf.push(match rng.next() % 30 {
            0 => 0x1b,                                // ESC
            1 => b'[',                                // CSI introducer
            2 => b']',                                // OSC introducer
            3 => b'P',                                // DCS introducer / DCH
            4 => b';',                                // param separator
            5 => b':',                                // sub-param separator
            6 => b'0' + (rng.next() % 10) as u8,      // digit
            7 => b'?',                                // private marker
            8 => b'h',                                // SM / DECSET
            9 => b'l',                                // RM / DECRST
            10 => b'm',                               // SGR final
            11 => b'J',                               // ED
            12 => b'K',                               // EL
            13 => b'r',                               // DECSTBM
            14 => b'L',                               // IL
            15 => b'M',                               // DL
            16 => b'@',                               // ICH
            17 => b'X',                               // ECH
            18 => b'S',                               // SU
            19 => b'T',                               // SD
            20 => b'7',                               // DECSC
            21 => b'8',                               // DECRC
            22 => b'x',                               // DECFRA
            23 => b'z',                               // DECERA
            24 => 0x20 + (rng.next() % 0x5f) as u8,   // printable ASCII
            25 => 0xc2 + (rng.next() % 0x1e) as u8,   // 2-byte UTF-8 lead
            26 => 0xe0 + (rng.next() % 0x10) as u8,   // 3-byte lead (CJK)
            27 => 0xf0 + (rng.next() % 0x05) as u8,   // 4-byte lead (emoji)
            28 => 0x80 + (rng.next() % 0x40) as u8,   // continuation byte
            _ => rng.next() as u8,                    // anything (C0/high)
        });
    }
    buf
}

#[test]
fn screen_invariants_hold_under_vt_input() {
    let mut screen = Screen::new(80, 24);
    let mut parser = Parser::new();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);

    // Curated sequences that toggle the trickiest state machines: alt-screen
    // swap, margins, origin mode, insert mode, wide/combining glyphs, OSC,
    // DCS, DECALN, and soft reset.
    let curated: &[&[u8]] = &[
        b"hello world\r\n",
        "\u{1b}[?1049h".as_bytes(),      // enter alt screen (clear)
        "alt screen 中中".as_bytes(),
        "\u{1b}[?1049l".as_bytes(),      // leave alt screen
        "\u{1b}[2;4r".as_bytes(),        // DECSTBM margins
        "\u{1b}[?6h".as_bytes(),         // DECOM origin mode
        "\u{1b}[1;1H".as_bytes(),        // CUP (origin-relative)
        "\u{1b}[?6l".as_bytes(),         // DECOM off
        "\u{1b}[4h".as_bytes(),          // IRM insert mode
        "中中中中中".as_bytes(),          // wide glyphs in insert mode
        "\u{1b}[4l".as_bytes(),          // IRM off
        "\u{1b}[2J".as_bytes(),          // ED whole screen
        "\u{1b}[3;1H\u{1b}[L\u{1b}[M".as_bytes(), // IL then DL
        "\u{1b}[1;2H\u{1b}[2@\u{1b}[2P".as_bytes(), // ICH then DCH
        "\u{1b}]0;title\u{7}".as_bytes(), // OSC title (BEL)
        "\u{1b}]52;;QUJDREVGR0g=\u{1b}\\".as_bytes(), // OSC clipboard (ST)
        "\u{1b}P1;2qpayload\u{1b}\\".as_bytes(), // DCS passthrough
        "\u{1b}#8".as_bytes(),          // DECALN
        "\u{1b}[!p".as_bytes(),         // DECSTR soft reset
        "\u{1b}c".as_bytes(),           // RIS full reset
        "👨\u{200d}👩\u{200d}👧".as_bytes(), // ZWJ emoji family
        "e\u{301} x".as_bytes(),        // combining mark
        "\t\r\n\x08".as_bytes(),        // HT CR LF BS
    ];
    for seq in curated {
        parser.advance(&mut screen, seq);
        screen.check_invariants();
    }

    // Resize is a public API, not a byte sequence; exercise it between feeds.
    for (cols, rows) in [(80, 24), (40, 40), (120, 10), (80, 24)] {
        screen.resize(cols, rows);
        screen.check_invariants();
    }

    // Structured random bytes.
    for _ in 0..100_000 {
        let chunk = random_chunk(&mut rng);
        parser.advance(&mut screen, &chunk);
        screen.check_invariants();
    }
}

/// Small grids stress autowrap, wide-glyph, and margin edge cases that a
/// large screen never reaches (e.g. a double-width glyph on a one-column
/// grid).
#[test]
fn small_screen_invariants_hold() {
    for (cols, rows) in [(1, 1), (2, 1), (1, 3), (3, 1), (2, 2)] {
        let mut screen = Screen::new(cols, rows);
        let mut parser = Parser::new();
        let mut rng = Rng(0xdead_beef_cafe_f00d);
        for _ in 0..100_000 {
            let chunk = random_chunk(&mut rng);
            parser.advance(&mut screen, &chunk);
            screen.check_invariants();
        }
    }
}

/// Focused regressions the property test surfaced:
/// - DECSTBM with a `0` bottom must not panic (0 = default bottom).
/// - Setting margins homes the cursor and clears a pending wrap.
#[test]
fn margin_regressions() {
    let mut screen = Screen::new(3, 5);
    let mut parser = Parser::new();

    parser.advance(&mut screen, b"\x1b[2;0r"); // bottom 0 → default (last row)
    screen.check_invariants();

    screen = Screen::new(3, 5);
    parser = Parser::new();
    parser.advance(&mut screen, b"abc"); // arm WRAP_PENDING at the last column
    parser.advance(&mut screen, b"\x1b[2;3r"); // margins home the cursor
    screen.check_invariants();
    assert_eq!(screen.cursor(), (1, 0));
}

/// Editing must never split or orphan a double-width glyph's lead/
/// continuation pair.
#[test]
fn wide_glyph_editing_keeps_cells_consistent() {
    let mut screen = Screen::new(4, 1);
    let mut parser = Parser::new();
    parser.advance(&mut screen, "中x".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;1H\x1b[P"); // DCH 1 on the lead
    screen.check_invariants();
    assert_eq!(screen.render_line(0), "x");

    screen = Screen::new(4, 1);
    parser = Parser::new();
    parser.advance(&mut screen, "中x".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;2H\x1b[P"); // DCH 1 on the continuation
    screen.check_invariants();
    assert_eq!(screen.render_line(0), "x");

    screen = Screen::new(4, 1);
    parser = Parser::new();
    parser.advance(&mut screen, "中x".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;1H\x1b[@"); // ICH 1 before the glyph
    screen.check_invariants();
    assert_eq!(screen.render_line(0), " 中x");

    screen = Screen::new(4, 1);
    parser = Parser::new();
    parser.advance(&mut screen, "中".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;1Ha"); // overwrite the lead
    screen.check_invariants();
    assert_eq!(screen.render_line(0), "a");
}

/// A cursor positioned on a double-width continuation acts on the glyph's
/// lead, so erase/edit/write never touch half a glyph.
#[test]
fn cursor_on_continuation_acts_on_lead() {
    // Erase-to-end from the continuation erases the whole glyph.
    let mut screen = Screen::new(4, 1);
    let mut parser = Parser::new();
    parser.advance(&mut screen, "中x".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;2H\x1b[K"); // EL on the continuation
    screen.check_invariants();
    assert_eq!(screen.render_line(0), "");

    // IRM insert on the continuation inserts before the glyph.
    screen = Screen::new(4, 1);
    parser = Parser::new();
    parser.advance(&mut screen, "中x".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;2H\x1b[4ha"); // IRM on + write
    screen.check_invariants();
    assert_eq!(screen.render_line(0), "a中x");

    // Replace-mode write on the continuation overwrites the whole glyph.
    screen = Screen::new(4, 1);
    parser = Parser::new();
    parser.advance(&mut screen, "中x".as_bytes());
    parser.advance(&mut screen, b"\x1b[1;2Ha"); // write on the continuation
    screen.check_invariants();
    assert_eq!(screen.render_line(0), "a x");
}
