use super::*;
use alloc::vec;
use vtty::Parser;

fn run(screen: &mut Screen, bytes: &[u8]) {
    Parser::new().advance(screen, bytes);
}

#[test]
fn print_and_cursor() {
    let mut s = Screen::new(10, 3);
    run(&mut s, b"hi");
    assert_eq!(s.cursor(), (0, 2));
    assert_eq!(s.render_line(0).as_deref(), Some("hi"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
}

#[test]
fn autowrap_at_last_column() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"abc");
    assert_eq!(s.cursor(), (0, 2));
    run(&mut s, b"d");
    // `d` wraps to the next line.
    assert_eq!(s.cursor(), (1, 1));
    assert_eq!(s.render_line(0).as_deref(), Some("abc"));
    assert_eq!(s.render_line(1).as_deref(), Some("d"));
}

#[test]
fn double_width_glyph() {
    let mut s = Screen::new(5, 2);
    run(&mut s, "a中b".as_bytes());
    assert_eq!(s.render_line(0).as_deref(), Some("a中b"));
    assert_eq!(s.cell(0, 2).map(Cell::is_continuation), Some(true));
}

#[test]
fn combining_mark_attaches() {
    let mut s = Screen::new(4, 1);
    run(&mut s, "e\u{301}x".as_bytes());
    assert_eq!(s.render_line(0).as_deref(), Some("e\u{301}x"));
    assert_eq!(s.cell(0, 0).map(|c| c.marks()), Some(&['\u{301}'][..]));
}

#[test]
fn skin_tone_attaches_to_wide() {
    let mut s = Screen::new(4, 1);
    run(&mut s, "👋\u{1F3FF}".as_bytes());
    assert_eq!(s.render_line(0).as_deref(), Some("👋\u{1F3FF}"));
    assert_eq!(s.cell(0, 0).map(|c| c.marks()), Some(&['\u{1F3FF}'][..]));
    assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
}

#[test]
fn zwj_emoji_family_is_one_cell() {
    let mut s = Screen::new(8, 1);
    run(&mut s, "👨\u{200D}👩\u{200D}👧x".as_bytes());
    // The whole family occupies two cells (one glyph); `x` lands after.
    assert_eq!(s.render_line(0).as_deref(), Some("👨\u{200D}👩\u{200D}👧x"));
    assert_eq!(s.cell(0, 0).map(|c| c.ch), Some('👨'));
    assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
    assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('x'));
}

#[test]
fn render_ansi_bold_red() {
    let mut s = Screen::new(8, 1);
    run(&mut s, b"\x1b[1;31mhi");
    assert_eq!(s.render_ansi_line(0), "\x1b[0;1;38;5;1mhi\x1b[0m");
}

#[test]
fn render_ansi_truecolor() {
    let mut s = Screen::new(8, 1);
    run(&mut s, b"\x1b[38;2;255;0;128mX");
    assert_eq!(s.render_ansi_line(0), "\x1b[0;38;2;255;0;128mX\x1b[0m");
}

#[test]
fn render_ansi_style_change() {
    let mut s = Screen::new(8, 1);
    run(&mut s, b"\x1b[31mR\x1b[32mG");
    assert_eq!(s.render_ansi_line(0), "\x1b[0;38;5;1mR\x1b[0;38;5;2mG\x1b[0m");
}

#[test]
fn render_ansi_trims_trailing_blank() {
    let mut s = Screen::new(8, 1);
    run(&mut s, b"hi");
    // Plain text needs no escape sequences and no trailing blanks.
    assert_eq!(s.render_ansi_line(0), "hi");
}

#[test]
fn lf_cr_tab_bs() {
    let mut s = Screen::new(16, 3);
    run(&mut s, b"ab\ncd");
    // LF keeps the column, so `cd` lands on row 1 at columns 2..4.
    assert_eq!(s.cursor(), (1, 4));
    run(&mut s, b"\r");
    assert_eq!(s.cursor(), (1, 0));
    run(&mut s, b"\t");
    assert_eq!(s.cursor(), (1, 8));
    run(&mut s, b"\x08");
    assert_eq!(s.cursor(), (1, 7));
}

#[test]
fn cursor_movement() {
    let mut s = Screen::new(10, 5);
    run(&mut s, b"\x1b[3;4H");
    assert_eq!(s.cursor(), (2, 3));
    run(&mut s, b"\x1b[A");
    assert_eq!(s.cursor(), (1, 3));
    run(&mut s, b"\x1b[2C");
    assert_eq!(s.cursor(), (1, 5));
    run(&mut s, b"\x1b[2D");
    assert_eq!(s.cursor(), (1, 3));
}

#[test]
fn erase_display_and_line() {
    let mut s = Screen::new(5, 3);
    run(&mut s, b"hello\r\nworld");
    run(&mut s, b"\x1b[2J");
    assert_eq!(s.lines(), vec!["", "", ""]);
}

#[test]
fn sgr_flags_and_colors() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[1;31mX");
    assert_eq!(
        s.cell(0, 0).map(|c| c.sgr.contains(SgrFlags::BOLD)),
        Some(true)
    );
    assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(1)));
    run(&mut s, b"\x1b[0m");
    run(&mut s, b"Y");
    assert_eq!(s.cell(0, 1).map(|c| c.sgr), Some(SgrFlags::DEFAULT));
    assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(DEFAULT_FG_ID));
}

#[test]
fn truecolor_sgr() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[38;2;255;0;128mX");
    assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(258)));
    assert_eq!(s.rgb, vec![vtty_core::rgb(255, 0, 128)]);
}

#[test]
fn color_rgb_resolves_palette() {
    let mut s = Screen::new(4, 1);
    // Default fg/bg resolve to the configured defaults.
    assert_eq!(s.color_rgb(DEFAULT_FG_ID), Some(0xffffff));
    assert_eq!(s.color_rgb(DEFAULT_BG_ID), Some(0x000000));
    // Indexed colors: 0..=15 use the palette, 16..=255 the xterm table.
    assert_eq!(s.color_rgb(ColorId(1)), Some(0xcd0000));
    assert_eq!(s.color_rgb(ColorId(196)), Some(0xff0000));
    // Truecolor ids resolve through the dynamic palette.
    run(&mut s, b"\x1b[38;2;0;128;255mX");
    assert_eq!(s.color_rgb(ColorId(258)), Some(0x0080ff));
}

#[test]
fn set_palette_changes_colors() {
    let mut s = Screen::new(4, 1);
    s.set_palette([0x123456; 16]);
    s.set_default_colors(0xf0f0f0, 0x101010);
    assert_eq!(s.color_rgb(ColorId(1)), Some(0x123456));
    assert_eq!(s.color_rgb(DEFAULT_FG_ID), Some(0xf0f0f0));
    assert_eq!(s.color_rgb(DEFAULT_BG_ID), Some(0x101010));
}

#[test]
fn decstbm_scroll_region() {
    let mut s = Screen::new(3, 5);
    // Margins rows 2-4 (1-based) = 1..=3 (0-based).
    run(&mut s, b"\x1b[2;4r");
    // Setting margins homes the cursor to the top margin.
    assert_eq!(s.cursor(), (1, 0));
    // Four lines scroll the region (rows 1-3); `a` scrolls out.
    // CR before each LF so each line starts at column 0.
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    // Rows above and below the region are untouched.
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    assert_eq!(s.render_line(4).as_deref(), Some(""));
    assert_eq!(s.render_line(1).as_deref(), Some("b"));
    assert_eq!(s.render_line(2).as_deref(), Some("c"));
    assert_eq!(s.render_line(3).as_deref(), Some("d"));
}

#[test]
fn save_restore_cursor() {
    let mut s = Screen::new(8, 3);
    run(&mut s, b"\x1b[2;3H");
    run(&mut s, b"\x1b7"); // DECSC
    run(&mut s, b"\x1b[1;1H");
    run(&mut s, b"\x1b8"); // DECRC
    assert_eq!(s.cursor(), (1, 2));
}

#[test]
fn decaln_fills_e() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"abc\x1b#8");
    assert_eq!(s.render_line(0).as_deref(), Some("EEE"));
    assert_eq!(s.render_line(1).as_deref(), Some("EEE"));
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn decstr_preserves_content_and_homes_cursor() {
    let mut s = Screen::new(4, 2);
    run(&mut s, b"ab\x1b[2;3H"); // content, cursor to (1, 2)
    run(&mut s, b"\x1b[!p");
    assert_eq!(s.render_line(0).as_deref(), Some("ab"));
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn decstr_resets_style() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[1;31m"); // bold red active
    run(&mut s, b"\x1b[!p");
    run(&mut s, b"X");
    assert_eq!(s.render_ansi_line(0), "X"); // default, no SGR escapes
}

#[test]
fn decstr_clears_insert_mode() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"12\x1b[1G\x1b[4h"); // IRM on at column 0
    run(&mut s, b"\x1b[!p");
    run(&mut s, b"AB"); // replace, not shift
    assert_eq!(s.render_line(0).as_deref(), Some("AB"));
}

#[test]
fn selective_erase_in_line_skips_protected() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde\x1b[1;3H\x1b[1\"qc"); // protect "c"
    run(&mut s, b"\x1b[?2K"); // DECSEL 2: erase whole line, keep protected
    assert_eq!(s.render_line(0).as_deref(), Some("  c"));
    assert_eq!(s.cell(0, 2).map(|c| c.protected), Some(true));
    assert_eq!(s.cell(0, 0).map(|c| c.ch), Some(' '));
}

#[test]
fn selective_erase_in_display_skips_protected() {
    let mut s = Screen::new(4, 2);
    run(&mut s, b"abcd\r\nEFGH");
    run(&mut s, b"\x1b[1;2H\x1b[1\"qbc"); // protect "bc" on row 0
    run(&mut s, b"\x1b[?2J"); // DECSED 2: erase display, keep protected
    assert_eq!(s.render_line(0).as_deref(), Some(" bc"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
}

#[test]
fn plain_erase_ignores_protection() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde\x1b[1;3H\x1b[1\"qc"); // protect "c"
    run(&mut s, b"\x1b[2J"); // ED 2 (non-selective) erases everything
    assert_eq!(s.render_line(0).as_deref(), Some(""));
}

#[test]
fn decsca_zero_clears_protection() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde\x1b[1;3H\x1b[1\"qc"); // protect "c"
    run(&mut s, b"\x1b[1;4H\x1b[0\"qd"); // DECSCA 0: "d" is erasable
    run(&mut s, b"\x1b[?2K"); // selective erase line
    assert_eq!(s.render_line(0).as_deref(), Some("  c"));
}

#[test]
fn reverse_video_swaps_default_colors() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"X\x1b[?5h"); // DECSCNM on
    // Default fg/bg swap to explicit black-on-white.
    assert_eq!(
        s.render_ansi_line(0),
        "\x1b[0;38;2;0;0;0;48;2;255;255;255mX\x1b[0m"
    );
}

#[test]
fn reverse_video_swaps_fg_bg() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[31mX"); // red foreground
    run(&mut s, b"\x1b[?5h"); // DECSCNM on
    // Red fg becomes red bg; the default bg becomes the foreground.
    assert_eq!(
        s.render_ansi_line(0),
        "\x1b[0;38;2;0;0;0;48;2;205;0;0mX\x1b[0m"
    );
}

#[test]
fn reverse_video_off_restores_render() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"X\x1b[?5h\x1b[?5l"); // DECSCNM on, then off
    assert_eq!(s.render_ansi_line(0), "X");
}

#[test]
fn decfra_fills_rectangle() {
    let mut s = Screen::new(4, 3);
    run(&mut s, b"\x1b[65;2;3;1;2$x"); // fill cols 2-3, rows 1-2 with 'A'
    assert_eq!(s.render_line(0).as_deref(), Some(" AA"));
    assert_eq!(s.render_line(1).as_deref(), Some(" AA"));
    assert_eq!(s.render_line(2).as_deref(), Some(""));
}

#[test]
fn decera_erases_rectangle() {
    let mut s = Screen::new(4, 2);
    run(&mut s, b"abcdefgh");
    run(&mut s, b"\x1b[2;3;1;2$z"); // erase cols 2-3, rows 1-2
    assert_eq!(s.render_line(0).as_deref(), Some("a  d"));
    assert_eq!(s.render_line(1).as_deref(), Some("e  h"));
}

#[test]
fn decsera_skips_protected() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"abcdef");
    run(&mut s, b"\x1b[1;2H\x1b[1\"qb"); // protect "b"
    run(&mut s, b"\x1b[1;3;1;2${"); // DECSERA whole screen
    assert_eq!(s.render_line(0).as_deref(), Some(" b"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
}

#[test]
fn deccra_copies_rectangle() {
    let mut s = Screen::new(4, 2);
    run(&mut s, b"abcd");
    // Copy row 1 cols 1-2 ("ab") to row 2 col 3.
    run(&mut s, b"\x1b[2;1;2;1;1;3$v");
    assert_eq!(s.render_line(0).as_deref(), Some("abcd"));
    assert_eq!(s.render_line(1).as_deref(), Some("  ab"));
}

#[test]
fn deccara_applies_attrs() {
    let mut s = Screen::new(3, 1);
    run(&mut s, b"abc");
    // Bold + red on cols 2-3 of row 1 (cells 'b', 'c').
    run(&mut s, b"\x1b[1;2;1;3;1;31$r");
    assert_eq!(s.render_ansi_line(0), "a\x1b[0;1;38;5;1mbc\x1b[0m");
}

#[test]
fn decrara_toggles_attrs() {
    let mut s = Screen::new(2, 1);
    run(&mut s, b"\x1b[1mab"); // "ab" bold
    // Reverse bold on col 1 (cell 'a').
    run(&mut s, b"\x1b[1;1;1;1;1$t");
    assert_eq!(s.render_ansi_line(0), "a\x1b[0;1mb\x1b[0m");
}

#[test]
fn insert_and_delete_lines() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"aaa\r\nbbb\r\nccc");
    run(&mut s, b"\x1b[2;1H\x1b[L"); // insert line at row 2 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("aaa"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
    assert_eq!(s.render_line(2).as_deref(), Some("bbb"));
}

#[test]
fn delete_lines_shifts_up_and_blanks_bottom() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"aaa\r\nbbb\r\nccc");
    run(&mut s, b"\x1b[1;1H\x1b[M"); // DL at row 1 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("bbb"));
    assert_eq!(s.render_line(1).as_deref(), Some("ccc"));
    assert_eq!(s.render_line(2).as_deref(), Some(""));
}

#[test]
fn insert_chars_opens_a_gap() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde");
    run(&mut s, b"\x1b[1;2H\x1b[2@"); // ICH 2 at column 2 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("a  bc"));
    assert_eq!(s.cell(0, 1).map(|c| c.ch), Some(' '));
    assert_eq!(s.cell(0, 3).map(|c| c.ch), Some('b'));
    assert_eq!(s.cell(0, 4).map(|c| c.ch), Some('c'));
}

#[test]
fn delete_chars_closes_the_gap() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde");
    run(&mut s, b"\x1b[1;2H\x1b[2P"); // DCH 2 at column 2 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("ade"));
    assert_eq!(s.cell(0, 1).map(|c| c.ch), Some('d'));
    assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('e'));
    assert_eq!(s.cell(0, 3).map(|c| c.ch), Some(' '));
}

#[test]
fn insert_mode_shifts_row_right() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"12");
    run(&mut s, b"\x1b[1G\x1b[4h"); // IRM on at column 1
    run(&mut s, b"AB");
    assert_eq!(s.render_line(0).as_deref(), Some("AB12"));
}

#[test]
fn insert_mode_reset_restores_replace() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"12\x1b[1G\x1b[4hAB");
    assert_eq!(s.render_line(0).as_deref(), Some("AB12"));
    // Reset IRM: writing replaces instead of shifting.
    run(&mut s, b"\x1b[1G\x1b[4lXY");
    assert_eq!(s.render_line(0).as_deref(), Some("XY12"));
}

#[test]
fn insert_mode_at_last_column_drops_tail() {
    let mut s = Screen::new(3, 1);
    run(&mut s, b"abc");
    run(&mut s, b"\x1b[3G\x1b[4hX"); // IRM on at the last column
    assert_eq!(s.render_line(0).as_deref(), Some("abX"));
    assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('X'));
}

#[test]
fn insert_mode_wide_glyph_shifts_two_cells() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"ab");
    run(&mut s, b"\x1b[1G\x1b[4h"); // IRM on at column 1
    run(&mut s, "中".as_bytes()); // width 2
    assert_eq!(s.render_line(0).as_deref(), Some("中ab"));
    assert_eq!(s.cell(0, 1).map(Cell::is_continuation), Some(true));
}

#[test]
fn line_edit_ignores_cursor_outside_margins() {
    let mut s = Screen::new(3, 4);
    run(&mut s, b"aaa\r\nbbb\r\nccc\r\nddd");
    run(&mut s, b"\x1b[2;3r"); // margins rows 2-3 (1-based)
    // Cursor above the top margin: IL must not move anything.
    run(&mut s, b"\x1b[1;1H\x1b[L");
    assert_eq!(s.lines(), vec!["aaa", "bbb", "ccc", "ddd"]);
    // Cursor below the bottom margin: DL must not move anything.
    run(&mut s, b"\x1b[4;1H\x1b[M");
    assert_eq!(s.lines(), vec!["aaa", "bbb", "ccc", "ddd"]);
}

#[test]
fn erase_in_display_from_cursor_to_end() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"abc\r\ndef\r\nghi");
    run(&mut s, b"\x1b[2;2H\x1b[J"); // ED 0 at row 2 col 2 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("abc"));
    assert_eq!(s.render_line(1).as_deref(), Some("d"));
    assert_eq!(s.render_line(2).as_deref(), Some(""));
}

#[test]
fn erase_in_display_start_to_cursor() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"abc\r\ndef\r\nghi");
    run(&mut s, b"\x1b[2;2H\x1b[1J"); // ED 1
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    assert_eq!(s.render_line(1).as_deref(), Some("  f"));
    assert_eq!(s.render_line(2).as_deref(), Some("ghi"));
}

#[test]
fn erase_in_display_whole_screen_keeps_cursor() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"abc\r\ndef\r\nghi");
    run(&mut s, b"\x1b[2;2H\x1b[2J"); // ED 2
    assert_eq!(s.lines(), vec!["", "", ""]);
    assert_eq!(s.cursor(), (1, 1));
}

#[test]
fn erase_in_line_to_end() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde\x1b[1;3H\x1b[K"); // EL 0 at column 3 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("ab"));
    assert_eq!(s.cursor(), (0, 2));
}

#[test]
fn erase_in_line_start_to_cursor() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde\x1b[1;3H\x1b[1K"); // EL 1
    assert_eq!(s.render_line(0).as_deref(), Some("   de"));
    assert_eq!(s.cell(0, 2).map(|c| c.ch), Some(' '));
    assert_eq!(s.cell(0, 3).map(|c| c.ch), Some('d'));
}

#[test]
fn erase_in_line_whole_line() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde\x1b[1;3H\x1b[2K"); // EL 2
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    assert_eq!(s.cursor(), (0, 2));
}

#[test]
fn erase_chars_clamps_to_line_end() {
    let mut s = Screen::new(5, 1);
    run(&mut s, b"abcde");
    run(&mut s, b"\x1b[1;2H\x1b[3X"); // ECH 3 at column 2 (1-based)
    assert_eq!(s.render_line(0).as_deref(), Some("a   e"));
    assert_eq!(s.cell(0, 1).map(|c| c.ch), Some(' '));
    assert_eq!(s.cell(0, 3).map(|c| c.ch), Some(' '));
    assert_eq!(s.cell(0, 4).map(|c| c.ch), Some('e'));

    // Oversized count clamps to the line end instead of overflowing.
    run(&mut s, b"\x1b[1;2H\x1b[99X");
    assert_eq!(s.render_line(0).as_deref(), Some("a"));
}

#[test]
fn title() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b]0;hello\x07");
    assert_eq!(s.title(), "hello");
}

#[test]
fn alternate_screen_1049() {
    let mut s = Screen::new(5, 2);
    run(&mut s, b"main");
    assert_eq!(s.render_line(0).as_deref(), Some("main"));
    let main_cursor = s.cursor();

    // Enter the alt screen: blank it and save the main cursor.
    run(&mut s, b"\x1b[?1049h");
    assert!(s.is_alt_screen());
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    run(&mut s, b"alt");
    assert_eq!(s.render_line(0).as_deref(), Some("alt"));

    // Leave: main content and cursor are restored.
    run(&mut s, b"\x1b[?1049l");
    assert!(!s.is_alt_screen());
    assert_eq!(s.render_line(0).as_deref(), Some("main"));
    assert_eq!(s.cursor(), main_cursor);
}

#[test]
fn alternate_screen_47_persists() {
    let mut s = Screen::new(5, 2);
    run(&mut s, b"main");
    run(&mut s, b"\x1b[?47h");
    assert!(s.is_alt_screen());
    // 47 does not clear, but the alt buffer starts blank.
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    run(&mut s, b"alt");
    run(&mut s, b"\x1b[?47l");
    assert_eq!(s.render_line(0).as_deref(), Some("main"));
    // Re-enter: the alt content persists (no clear).
    run(&mut s, b"\x1b[?47h");
    assert_eq!(s.render_line(0).as_deref(), Some("alt"));
    run(&mut s, b"\x1b[?47l");
}

#[test]
fn scrollback_captures_lines() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    assert_eq!(s.scrollback_len(), 2);
    assert_eq!(s.render_line(0).as_deref(), Some("c"));
    assert_eq!(s.render_line(1).as_deref(), Some("d"));
    assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));
    assert_eq!(s.scrollback_text(1).as_deref(), Some("b"));
}

#[test]
fn scrollback_cap_drops_oldest() {
    let mut s = Screen::new(3, 1);
    s.set_max_scrollback(2);
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    assert_eq!(s.scrollback_len(), 2);
    assert_eq!(s.scrollback_text(0).as_deref(), Some("b"));
    assert_eq!(s.scrollback_text(1).as_deref(), Some("c"));
}

#[test]
fn scrollback_disabled() {
    let mut s = Screen::new(3, 1);
    s.set_max_scrollback(0);
    run(&mut s, b"a\r\nb");
    assert_eq!(s.scrollback_len(), 0);
    assert_eq!(s.render_line(0).as_deref(), Some("b"));
}

#[test]
fn scrollback_ignores_scroll_region() {
    let mut s = Screen::new(3, 3);
    // Margins rows 2-3 (1-based) = 1..=2 (0-based), not the full screen.
    run(&mut s, b"\x1b[2;3r");
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    assert_eq!(s.scrollback_len(), 0);
}

#[test]
fn default_tabs_every_8() {
    let mut s = Screen::new(20, 1);
    run(&mut s, b"\t");
    assert_eq!(s.cursor(), (0, 8));
    run(&mut s, b"\t");
    assert_eq!(s.cursor(), (0, 16));
}

#[test]
fn hts_and_tbc() {
    let mut s = Screen::new(20, 1);
    run(&mut s, b"abc\x1bH"); // HTS: set a stop at column 3.
    assert!(s.tab_stop_at(3));
    run(&mut s, b"\r\t"); // tab from column 0 lands on 3.
    assert_eq!(s.cursor(), (0, 3));
    run(&mut s, b"\x1b[g"); // TBC 0: clear the stop at the cursor.
    assert!(!s.tab_stop_at(3));
    run(&mut s, b"\r\t"); // now skips 3 and lands on the default 8.
    assert_eq!(s.cursor(), (0, 8));
}

#[test]
fn tbc_clear_all() {
    let mut s = Screen::new(20, 1);
    run(&mut s, b"\x1b[3g"); // TBC 3: clear every stop.
    assert!(!s.tab_stop_at(8));
    assert!(!s.tab_stop_at(16));
    run(&mut s, b"\t"); // no stops left → clamp to the last column.
    assert_eq!(s.cursor(), (0, 19));
}

#[test]
fn resize_reflows_soft_wrap() {
    let mut s = Screen::new(5, 3);
    run(&mut s, b"helloworld");
    assert_eq!(s.lines(), vec!["hello", "world", ""]);
    // Widening re-joins the soft-wrapped logical line.
    s.resize(10, 2);
    assert_eq!(s.lines(), vec!["helloworld", ""]);
}

#[test]
fn resize_preserves_hard_break() {
    let mut s = Screen::new(5, 3);
    run(&mut s, b"ab\r\ncd");
    s.resize(10, 3);
    assert_eq!(s.lines(), vec!["ab", "cd", ""]);
}

#[test]
fn resize_rewraps_wide_char() {
    let mut s = Screen::new(3, 3);
    run(&mut s, "a中b".as_bytes());
    assert_eq!(s.lines(), vec!["a中", "b", ""]);
    s.resize(4, 3);
    assert_eq!(s.lines(), vec!["a中b", "", ""]);
}

#[test]
fn resize_narrow_rewraps() {
    let mut s = Screen::new(6, 2);
    run(&mut s, b"foobar");
    assert_eq!(s.lines(), vec!["foobar", ""]);
    s.resize(3, 4);
    assert_eq!(s.lines(), vec!["foo", "bar", "", ""]);
}

#[test]
fn resize_keeps_content_in_tall_screen() {
    // A tall viewport pads the grid with trailing blanks; shrinking must
    // keep the content, not those blanks.
    let mut s = Screen::new(20, 40);
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    s.resize(20, 24);
    assert_eq!(s.render_line(0).as_deref(), Some("a"));
    assert_eq!(s.render_line(1).as_deref(), Some("b"));
    assert_eq!(s.render_line(2).as_deref(), Some("c"));
    assert_eq!(s.render_line(3).as_deref(), Some("d"));
    assert_eq!(s.render_line(4).as_deref(), Some(""));
}

#[test]
fn resize_does_not_multiply_blank_lines() {
    // One intentional blank line must stay one blank line when the width
    // shrinks (a blank row stores `cols` blank cells, which would
    // otherwise re-wrap into `ceil(old/new)` rows).
    let mut s = Screen::new(20, 4);
    run(&mut s, b"a\r\n\r\nb");
    assert_eq!(s.render_line(0).as_deref(), Some("a"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
    assert_eq!(s.render_line(2).as_deref(), Some("b"));
    s.resize(10, 4);
    assert_eq!(s.render_line(0).as_deref(), Some("a"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
    assert_eq!(s.render_line(2).as_deref(), Some("b"));
    assert_eq!(s.render_line(3).as_deref(), Some(""));
}

#[test]
fn resize_preserves_wrapped_space() {
    // "hello world" wraps as "hello " + "world"; the trailing space on
    // the soft-wrapped row is content and must survive reflow.
    let mut s = Screen::new(6, 2);
    run(&mut s, b"hello world");
    assert_eq!(s.cell(0, 5).map(|c| c.ch), Some(' '));
    assert_eq!(s.render_line(1).as_deref(), Some("world"));
    s.resize(11, 2);
    assert_eq!(s.render_line(0).as_deref(), Some("hello world"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
}

#[test]
fn resize_reflows_scrollback() {
    let mut s = Screen::new(5, 2);
    run(&mut s, b"aaaaa\r\nbbbbb\r\nccccc\r\nddddd");
    assert_eq!(s.scrollback_len(), 2);
    assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaaa"));
    assert_eq!(s.scrollback_text(1).as_deref(), Some("bbbbb"));
    s.resize(10, 2);
    // History survives the resize (now at the wider width).
    assert_eq!(s.scrollback_len(), 2);
    assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaaa"));
    assert_eq!(s.scrollback_text(1).as_deref(), Some("bbbbb"));
    assert_eq!(s.render_line(0).as_deref(), Some("ccccc"));
    assert_eq!(s.render_line(1).as_deref(), Some("ddddd"));
}

#[test]
fn resize_rejoins_scrollback_soft_wrap() {
    let mut s = Screen::new(5, 2);
    // "helloworld" soft-wraps to two rows; a following line pushes the
    // first half into scrollback.
    run(&mut s, b"helloworld\r\nxy");
    assert_eq!(s.scrollback_text(0).as_deref(), Some("hello"));
    assert_eq!(s.render_line(0).as_deref(), Some("world"));
    assert_eq!(s.render_line(1).as_deref(), Some("xy"));
    // Widening re-joins the soft-wrapped history into the active screen.
    s.resize(10, 2);
    assert_eq!(s.scrollback_len(), 0);
    assert_eq!(s.lines(), vec!["helloworld", "xy"]);
}

#[test]
fn zero_dimensions_clamp() {
    let s = Screen::new(0, 0);
    assert_eq!((s.cols(), s.rows()), (1, 1));
}

#[test]
fn reverse_index_scrolls_down_at_top_margin() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"aaa\r\nbbb\r\nccc");
    run(&mut s, b"\x1b[1;1H\x1bM"); // RI at the top row
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    assert_eq!(s.render_line(1).as_deref(), Some("aaa"));
    assert_eq!(s.render_line(2).as_deref(), Some("bbb"));
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn reverse_index_mid_screen_moves_up() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"\x1b[2;1H\x1bM"); // RI at row 2, not the top
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn ind_and_nel_linefeed() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"\x1b[1;2H\x1bD"); // IND: down one line, keep column
    assert_eq!(s.cursor(), (1, 1));
    run(&mut s, b"\x1bE"); // NEL: home column, then down one line
    assert_eq!(s.cursor(), (2, 0));
}

#[test]
fn origin_mode_clamps_to_margins() {
    let mut s = Screen::new(3, 5);
    run(&mut s, b"\x1b[2;4r\x1b[?6h"); // margins rows 2-4, DECOM on
    run(&mut s, b"\x1b[1;1H"); // CUP 1;1 is margin-relative
    assert_eq!(s.cursor(), (1, 0));
    run(&mut s, b"\x1b[5;1H"); // row 5 clamps to the bottom margin
    assert_eq!(s.cursor(), (3, 0));
    run(&mut s, b"\x1b[1;1H\x1b[A"); // CUU clamps to the top margin
    assert_eq!(s.cursor(), (1, 0));
    run(&mut s, b"\x1b[3;1H\x1b[B"); // CUD clamps to the bottom margin
    assert_eq!(s.cursor(), (3, 0));
    run(&mut s, b"\x1b[?6l\x1b[1;1H"); // DECOM off: CUP 1;1 is row 0
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn invalid_margins_are_ignored() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"\x1b[3;1H"); // move the cursor first
    run(&mut s, b"\x1b[3;1r"); // top >= bottom: ignored
    assert_eq!(s.cursor(), (2, 0));
    run(&mut s, b"\x1b[1;5r"); // bottom >= rows: ignored
    assert_eq!(s.cursor(), (2, 0));
}

#[test]
fn set_margins_with_only_top() {
    let mut s = Screen::new(3, 5);
    run(&mut s, b"\x1b[2r"); // top row 2, bottom defaults to the last row
    assert_eq!(s.cursor(), (1, 0)); // homed to the top margin
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    assert_eq!(s.lines(), vec!["", "a", "b", "c", "d"]);
}

#[test]
fn cursor_movement_commands() {
    let mut s = Screen::new(5, 3);
    run(&mut s, b"\x1b[2;2H"); // CUP
    assert_eq!(s.cursor(), (1, 1));
    run(&mut s, b"\x1b[2A"); // CUU clamps to row 0
    assert_eq!(s.cursor(), (0, 1));
    run(&mut s, b"\x1b[3B"); // CUD clamps to row 2
    assert_eq!(s.cursor(), (2, 1));
    run(&mut s, b"\x1b[2C"); // CUF
    assert_eq!(s.cursor(), (2, 3));
    run(&mut s, b"\x1b[1D"); // CUB
    assert_eq!(s.cursor(), (2, 2));
    run(&mut s, b"\x1b[2G"); // CHA
    assert_eq!(s.cursor(), (2, 1));
    run(&mut s, b"\x1b[1d"); // VPA
    assert_eq!(s.cursor(), (0, 1));
}

#[test]
fn tbc_unknown_mode_is_noop() {
    let mut s = Screen::new(20, 1);
    run(&mut s, b"\x1b[2g"); // unknown TBC mode: no-op
    assert!(s.tab_stop_at(8));
}

#[test]
fn sgr_color_codes_set_fg_and_bg() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[31;44mX"); // red fg, blue bg
    assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(1)));
    assert_eq!(s.cell(0, 0).map(|c| c.bg), Some(ColorId(4)));
    run(&mut s, b"\x1b[92;103mY"); // bright green fg, bright yellow bg
    assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(ColorId(10)));
    assert_eq!(s.cell(0, 1).map(|c| c.bg), Some(ColorId(11)));
    run(&mut s, b"\x1b[39;49mZ"); // default fg and bg
    assert_eq!(s.cell(0, 2).map(|c| c.fg), Some(DEFAULT_FG_ID));
    assert_eq!(s.cell(0, 2).map(|c| c.bg), Some(DEFAULT_BG_ID));
}

#[test]
fn sgr_indexed_and_rgb_bg_colors() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[38;5;196;48;5;21mX"); // indexed fg and bg
    assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(196)));
    assert_eq!(s.cell(0, 0).map(|c| c.bg), Some(ColorId(21)));
    run(&mut s, b"\x1b[48;2;1;2;3mY"); // RGB background
    assert_eq!(s.cell(0, 1).map(|c| c.bg), Some(ColorId(258)));
    assert_eq!(s.color_rgb(ColorId(258)), Some(0x010203));
}

#[test]
fn sgr_empty_resets_and_underline_color_skipped() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[31mX\x1b[mY"); // bare CSI m resets
    assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(DEFAULT_FG_ID));
    run(&mut s, b"\x1b[58;5;196mZ"); // underline color is skipped
    assert_eq!(s.cell(0, 2).map(|c| c.ch), Some('Z'));
    assert_eq!(s.cell(0, 2).map(|c| c.fg), Some(DEFAULT_FG_ID));
}

#[test]
fn sgr_reset_flags_clear_attributes() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[1;3mX"); // bold + italic
    assert_eq!(
        s.cell(0, 0).map(|c| c.sgr.contains(SgrFlags::BOLD)),
        Some(true)
    );
    assert_eq!(
        s.cell(0, 0).map(|c| c.sgr.contains(SgrFlags::ITALIC)),
        Some(true)
    );
    run(&mut s, b"\x1b[22mY"); // 22 clears bold (and faint), italic stays
    assert_eq!(
        s.cell(0, 1).map(|c| c.sgr.contains(SgrFlags::BOLD)),
        Some(false)
    );
    assert_eq!(
        s.cell(0, 1).map(|c| c.sgr.contains(SgrFlags::ITALIC)),
        Some(true)
    );
}

#[test]
fn ris_resets_screen() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"abc\r\n\x1b[31m");
    s.set_max_scrollback(5);
    run(&mut s, b"\x1bc"); // RIS
    assert_eq!(s.lines(), vec!["", "", ""]);
    assert_eq!(s.cursor(), (0, 0));
    assert_eq!(s.scrollback_len(), 0);
}

#[test]
fn alt_screen_reenter_and_leave_are_noops() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"main");
    run(&mut s, b"\x1b[?1049h"); // enter alt
    assert!(s.is_alt_screen());
    run(&mut s, b"\x1b[?1049h"); // re-enter: no-op
    assert!(s.is_alt_screen());
    run(&mut s, b"\x1b[?1049l"); // leave
    assert!(!s.is_alt_screen());
    run(&mut s, b"\x1b[?1049l"); // leave again: no-op
    assert!(!s.is_alt_screen());
}

#[test]
fn cell_and_render_line_out_of_range() {
    let s = Screen::new(3, 2);
    assert_eq!(s.cell(2, 0), None);
    assert_eq!(s.cell(0, 3), None);
    assert_eq!(s.render_line(2), None, "out-of-range row is None");
}

#[test]
fn render_ansi_multiple_rows() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"a\r\n\x1b[1mb");
    let ansi = s.render_ansi();
    assert!(ansi.contains("a\r\n"));
    assert!(ansi.contains("b"));
    // Wide glyphs skip their continuation cell.
    run(&mut s, b"\x1b[2J\x1b[1;1H");
    run(&mut s, "中".as_bytes());
    assert!(s.render_ansi_line(0).contains("中"));
}

#[test]
fn max_scrollback_getter_and_shrink() {
    let mut s = Screen::new(3, 2);
    s.set_max_scrollback(5);
    assert_eq!(s.max_scrollback(), 5);
    run(&mut s, b"a\r\nb\r\nc\r\nd\r\ne");
    assert_eq!(s.scrollback_len(), 3);
    s.set_max_scrollback(2); // shrinking truncates the oldest
    assert_eq!(s.scrollback_len(), 2);
    assert_eq!(s.scrollback_text(0).as_deref(), Some("b"));
}

#[test]
fn resize_same_size_is_noop() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"abc");
    s.resize(3, 2);
    assert_eq!(s.render_line(0).as_deref(), Some("abc"));
}

#[test]
fn scrollback_line_returns_raw_cells() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"a\r\nb\r\nc");
    assert_eq!(s.scrollback_line(0).map(|r| r.len()), Some(3));
    assert_eq!(s.scrollback_line(1), None);
}

#[test]
fn cnl_and_cpl_move_and_home() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"\x1b[1;2H\x1b[E"); // CNL: down one, home column
    assert_eq!(s.cursor(), (1, 0));
    run(&mut s, b"\x1b[F"); // CPL: up one, home column
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn su_and_sd_scroll() {
    let mut s = Screen::new(3, 3);
    run(&mut s, b"aaa\r\nbbb\r\nccc");
    run(&mut s, b"\x1b[1;1H\x1b[T"); // SD: scroll down
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    assert_eq!(s.render_line(1).as_deref(), Some("aaa"));
    assert_eq!(s.render_line(2).as_deref(), Some("bbb"));
    run(&mut s, b"\x1b[1;1H\x1b[S"); // SU: scroll up
    assert_eq!(s.render_line(0).as_deref(), Some("aaa"));
    assert_eq!(s.render_line(1).as_deref(), Some("bbb"));
    assert_eq!(s.render_line(2).as_deref(), Some(""));
}

#[test]
fn region_scroll_n_batches() {
    let mut s = Screen::new(3, 5);
    run(&mut s, b"\x1b[2;4r"); // margins rows 1..=3 (0-based)
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    assert_eq!(s.render_line(1).as_deref(), Some("b"));
    assert_eq!(s.render_line(2).as_deref(), Some("c"));
    assert_eq!(s.render_line(3).as_deref(), Some("d"));

    // SU 2 scrolls the region up two lines in one pass.
    run(&mut s, b"\x1b[2S");
    assert_eq!(s.render_line(1).as_deref(), Some("d"));
    assert_eq!(s.render_line(2).as_deref(), Some(""));
    assert_eq!(s.render_line(3).as_deref(), Some(""));

    // SD 2 scrolls it back down two lines in one pass.
    run(&mut s, b"\x1b[2T");
    assert_eq!(s.render_line(1).as_deref(), Some(""));
    assert_eq!(s.render_line(2).as_deref(), Some(""));
    assert_eq!(s.render_line(3).as_deref(), Some("d"));
}

#[test]
fn csi_question_r_is_decpcterm_and_ignored() {
    let mut s = Screen::new(3, 5);
    run(&mut s, b"\x1b[2;4r"); // DECSTBM: margins rows 2-4, cursor homes to row 1
    run(&mut s, b"\x1b[3;1H"); // move the cursor to row 2
    run(&mut s, b"\x1b[?r"); // DECPCTERM (PCTerm/Scancode mode): out of scope, ignored
    assert_eq!(s.cursor(), (2, 0)); // cursor unchanged — margins were not reset
}

#[test]
fn hvp_matches_cup() {
    let mut s = Screen::new(5, 3);
    run(&mut s, b"\x1b[3;2f"); // HVP: same as CUP
    assert_eq!(s.cursor(), (2, 1));
}

#[test]
fn vt_and_ff_linefeed() {
    let mut s = Screen::new(5, 3);
    run(&mut s, b"a\x0b"); // VT linefeeds, keeping the column
    assert_eq!(s.cursor(), (1, 1));
    run(&mut s, b"\x0c"); // FF linefeeds too
    assert_eq!(s.cursor(), (2, 1));
}

#[test]
fn ignored_csi_finals_are_noop() {
    // The exact CSI finals the screen dispatches. Everything else in
    // `0x40..=0x7e` (the DEC/ANSI final range) must be a no-op, so a new
    // handler cannot slip in without updating this pin.
    let handled = b"@ABCDEFGHJKLMPSTXdfghlmr";
    for f in 0x40..=0x7e {
        if handled.contains(&f) {
            continue;
        }
        let mut s = Screen::new(4, 3);
        run(&mut s, &[b'\x1b', b'[', f]);
        assert_eq!(
            s,
            Screen::new(4, 3),
            "CSI {} (0x{f:02x}) must be a no-op",
            f as char
        );
    }
}

#[test]
fn ignored_esc_finals_are_noop() {
    // ESC finals the screen dispatches, and the six sequence introducers
    // (`P` `X` `[` `]` `^` `_`) that are not finals at all.
    let handled = b"78DEHMc";
    let introducers = b"PX[]^_";
    for f in 0x30..=0x7e {
        if handled.contains(&f) || introducers.contains(&f) {
            continue;
        }
        let mut s = Screen::new(4, 3);
        run(&mut s, &[b'\x1b', f]);
        assert_eq!(
            s,
            Screen::new(4, 3),
            "ESC {} (0x{f:02x}) must be a no-op",
            f as char
        );
    }
}

#[test]
fn tracked_but_unapplied_modes_are_noop() {
    // Modes `vtty_core::TermFlags` tracks but the screen does not apply.
    // Pinned so wiring one in (e.g. DECSCNM) is an explicit change.
    let seqs: &[&[u8]] = &[
        b"\x1b[?1h",
        b"\x1b[?1l",
        b"\x1b[?3h",
        b"\x1b[?3l",
        b"\x1b[?66h",
        b"\x1b[?66l",
        b"\x1b[?67h",
        b"\x1b[?67l",
        b"\x1b[?69h",
        b"\x1b[?69l",
        b"\x1b[?80h",
        b"\x1b[?80l",
    ];
    for seq in seqs {
        let mut s = Screen::new(4, 3);
        run(&mut s, seq);
        assert_eq!(s, Screen::new(4, 3), "{seq:?} must be a no-op");
    }
}

#[test]
fn esc_with_intermediate_is_noop() {
    // Charset designations and other intermediate ESC sequences the
    // screen does not act on (it keeps no active charset).
    let seqs: &[&[u8]] = &[
        b"\x1b(0", // DEC Special Graphics designation
        b"\x1b(B", // US-ASCII
        b"\x1b)0", // G1 designation
        b"\x1b*0", // G2 designation
        b"\x1b+0", // G3 designation
        b"\x1b#3", // DECDHL, unsupported
        b"\x1b#7", // not DECALN
        b"\x1b(8", // intermediate + final 8, not DECRC
    ];
    for seq in seqs {
        let mut s = Screen::new(4, 3);
        run(&mut s, seq);
        assert_eq!(s, Screen::new(4, 3), "{seq:?} must be a no-op");
    }
}

#[test]
fn reverse_index_scrolls_region_down() {
    let mut s = Screen::new(3, 5);
    run(&mut s, b"a\r\nb\r\nc\r\nd"); // rows 0..3
    run(&mut s, b"\x1b[2;4r"); // margins rows 1..=3 (0-based)
    run(&mut s, b"\x1b[2;1H\x1bM"); // RI at the region top scrolls it down
    assert_eq!(s.render_line(0).as_deref(), Some("a"));
    assert_eq!(s.render_line(1).as_deref(), Some(""));
    assert_eq!(s.render_line(2).as_deref(), Some("b"));
    assert_eq!(s.render_line(3).as_deref(), Some("c"));
}

#[test]
fn osc_progress_unknown_mode_is_noop() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"\x1b]9;4;1;40\x07");
    run(&mut s, b"\x1b]9;4;99\x07"); // unknown mode: progress unchanged
    assert_eq!(s.progress(), Progress::Percent(40));
}

#[test]
fn leading_mark_with_no_base_is_dropped() {
    let mut s = Screen::new(4, 1);
    run(&mut s, "\u{301}x".as_bytes()); // lone combining mark at column 0
    assert_eq!(s.render_line(0).as_deref(), Some("x"));
}

#[test]
fn unhandled_sequences_fall_through() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[Z"); // unhandled CSI final (Cbt)
    run(&mut s, b"\x00"); // unhandled C0 execute (NUL)
    run(&mut s, b"\x1b]52;x\x07"); // unhandled OSC (52)
    run(&mut s, b"\x1b6"); // unhandled ESC final
    assert_eq!(s.render_line(0).as_deref(), Some(""));
}

#[test]
fn chunked_multibyte_hits_print() {
    let mut s = Screen::new(4, 1);
    let mut p = Parser::new();
    p.advance(&mut s, &[0xe4]);
    p.advance(&mut s, &[0xb8, 0xad]); // 中
    assert_eq!(s.render_line(0).as_deref(), Some("中"));
}

#[test]
fn invalid_utf8_prints_replacement_char() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\xffx"); // invalid lead byte -> U+FFFD
    assert_eq!(s.render_line(0).as_deref(), Some("\u{FFFD}x"));
}

#[test]
fn dec_private_modes_set_and_reset() {
    let mut s = Screen::new(4, 4);
    run(&mut s, b"\x1b[?7l"); // DECAWM off
    run(&mut s, b"\x1b[?25l"); // DECTCEM off
    run(&mut s, b"\x1b[?9999h"); // unhandled mode: no-op
    // DEC 1048 saves and restores only the cursor.
    run(&mut s, b"\x1b[3;3H\x1b[?1048h");
    run(&mut s, b"\x1b[1;1H\x1b[?1048l");
    assert_eq!(s.cursor(), (2, 2));
}

#[test]
fn sgr_empty_colorspace_and_unknown_color_form() {
    let mut s = Screen::new(4, 1);
    run(&mut s, b"\x1b[38:2::255:0:0mX"); // empty colorspace is skipped
    assert_eq!(s.cell(0, 0).map(|c| c.fg), Some(ColorId(258)));
    assert_eq!(s.color_rgb(ColorId(258)), Some(0xff0000));
    run(&mut s, b"\x1b[0m\x1b[38;3;255mY"); // unknown sub-param: ignored
    assert_eq!(s.cell(0, 1).map(|c| c.fg), Some(DEFAULT_FG_ID));
}

#[test]
fn put_char_combining_mark_attaches_to_base() {
    let mut s = Screen::new(4, 1);
    s.put_char('a');
    s.put_char('\u{301}'); // width 0 -> attach to 'a'
    assert_eq!(s.render_line(0).as_deref(), Some("a\u{301}"));
}

#[test]
fn render_ansi_line_out_of_range_is_empty() {
    let s = Screen::new(4, 1);
    assert_eq!(s.render_ansi_line(99), "");
}

#[test]
fn render_ansi_resets_after_styled_run() {
    let mut s = Screen::new(6, 1);
    run(&mut s, b"\x1b[1ma\x1b[0mb");
    let ansi = s.render_ansi_line(0);
    assert!(ansi.contains("\x1b[0;1m"));
    assert!(ansi.contains('a'));
    assert!(ansi.contains('b'));
}

#[test]
fn mark_after_space_is_dropped() {
    let mut s = Screen::new(4, 1);
    s.put_char(' ');
    s.put_char('\u{301}'); // no visible base: dropped
    assert_eq!(s.cell(0, 0).map(|c| c.combining_len), Some(0));
}

#[test]
fn put_cluster_empty_is_noop() {
    let mut s = Screen::new(4, 1);
    s.put_cluster("");
    assert_eq!(s.render_line(0).as_deref(), Some(""));
}

#[test]
fn wide_char_without_wrap_clamps_to_one_cell() {
    let mut s = Screen::new(3, 1);
    run(&mut s, b"\x1b[?7l"); // DECAWM off
    run(&mut s, b"ab");
    run(&mut s, "中".as_bytes()); // width 2 at the last column -> width 1
    assert_eq!(s.render_line(0).as_deref(), Some("ab中"));
}

#[test]
fn wide_char_at_last_column_wraps() {
    let mut s = Screen::new(3, 2);
    run(&mut s, b"ab"); // cursor at the last column
    run(&mut s, "中".as_bytes()); // width 2 -> wraps to the next row
    assert_eq!(s.render_line(0).as_deref(), Some("ab"));
    assert_eq!(s.render_line(1).as_deref(), Some("中"));
}

#[test]
fn resize_reflow_clamps_and_wraps_wide() {
    // Width 1: a wide glyph clamps to one cell.
    let mut s = Screen::new(3, 1);
    run(&mut s, "中".as_bytes());
    s.resize(1, 1);
    assert_eq!(s.render_line(0).as_deref(), Some("中"));
    // Width 2: a wide glyph after a single cell wraps to the next row.
    let mut s = Screen::new(4, 1);
    run(&mut s, "a中".as_bytes());
    s.resize(2, 2);
    assert_eq!(s.render_line(0).as_deref(), Some("a"));
    assert_eq!(s.render_line(1).as_deref(), Some("中"));
}

#[test]
fn region_scroll_after_full_screen_scroll() {
    // Full-screen scroll rotates the ring (scroll_top != 0); a later
    // DECSTBM region scroll must materialize the ring first.
    let mut s = Screen::new(3, 3);
    run(&mut s, b"a\r\nb\r\nc\r\nd");
    assert_eq!(s.render_line(0).as_deref(), Some("b"));
    assert_eq!(s.render_line(1).as_deref(), Some("c"));
    assert_eq!(s.render_line(2).as_deref(), Some("d"));
    assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));

    // Region rows 1-2 (1-based) = 0..=1 (0-based).
    run(&mut s, b"\x1b[1;2r");
    run(&mut s, b"e\r\nf\r\ng");
    assert_eq!(s.render_line(0).as_deref(), Some("f"));
    assert_eq!(s.render_line(1).as_deref(), Some("g"));
    assert_eq!(s.render_line(2).as_deref(), Some("d"));
    // Region scrolls still discard, so only `a` remains in scrollback.
    assert_eq!(s.scrollback_len(), 1);
    assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));
}

#[test]
fn resize_after_full_screen_scroll_reflows() {
    // Reflow must materialize the ring before reading rows in order.
    let mut s = Screen::new(4, 2);
    run(&mut s, b"aaaa\r\nbbbb\r\ncccc");
    assert_eq!(s.render_line(0).as_deref(), Some("bbbb"));
    assert_eq!(s.render_line(1).as_deref(), Some("cccc"));
    assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaa"));

    s.resize(8, 2);
    assert_eq!(s.render_line(0).as_deref(), Some("bbbb"));
    assert_eq!(s.render_line(1).as_deref(), Some("cccc"));
    assert_eq!(s.scrollback_text(0).as_deref(), Some("aaaa"));
}

#[test]
fn reverse_index_after_full_screen_scroll() {
    // RI at the top margin rotates the ring back down and blanks row 0.
    let mut s = Screen::new(3, 2);
    run(&mut s, b"a\r\nb\r\nc");
    assert_eq!(s.render_line(0).as_deref(), Some("b"));
    assert_eq!(s.render_line(1).as_deref(), Some("c"));

    run(&mut s, b"\x1b[H\x1bM");
    assert_eq!(s.render_line(0).as_deref(), Some(""));
    assert_eq!(s.render_line(1).as_deref(), Some("b"));
    assert_eq!(s.scrollback_len(), 1);
    assert_eq!(s.scrollback_text(0).as_deref(), Some("a"));
}

#[test]
fn osc_progress_bar() {
    let mut s = Screen::new(3, 2);
    assert_eq!(s.progress(), Progress::Hidden);

    run(&mut s, b"\x1b]9;4;0\x07");
    assert_eq!(s.progress(), Progress::Indeterminate);

    run(&mut s, b"\x1b]9;4;1;40\x07");
    assert_eq!(s.progress(), Progress::Percent(40));

    run(&mut s, b"\x1b]9;4;1;150\x07");
    assert_eq!(s.progress(), Progress::Percent(100));

    run(&mut s, b"\x1b]9;4;2\x07");
    assert_eq!(s.progress(), Progress::Done);

    run(&mut s, b"\x1b]9;4;3\x07");
    assert_eq!(s.progress(), Progress::Error);

    // A non-progress OSC 9 sub-command is ignored.
    run(&mut s, b"\x1b]9;1;hello\x07");
    assert_eq!(s.progress(), Progress::Error);
}

#[test]
fn cursor_horizontal_forward_does_not_overflow() {
    // The forward branch must saturate like the backward branch, so a
    // near-maximum column plus a large param cannot overflow.
    let mut s = Screen::new(3, 2);
    s.active.cursor_col = u32::MAX - 5;
    s.cursor_horizontal(65535, false);
    assert_eq!(s.active.cursor_col, s.cols - 1);
}
