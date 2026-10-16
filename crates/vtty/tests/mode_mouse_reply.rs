// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Mode table, mouse encoding, and query/response formatting.

use vtty::{
    ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, MouseButton, MouseTracking, Reply, SgrFlags, TermFlags,
};

#[test]
fn mode_private_param_roundtrip() {
    assert_eq!(TermFlags::from_private_param(1), TermFlags::CURSOR_KEYS);
    assert_eq!(TermFlags::from_private_param(6), TermFlags::ORIGIN);
    assert_eq!(TermFlags::from_private_param(25), TermFlags::CURSOR_VISIBLE);
    assert_eq!(TermFlags::from_private_param(12), TermFlags::CURSOR_BLINK);
    assert_eq!(TermFlags::from_private_param(1000), TermFlags::MOUSE_NORMAL);
    assert_eq!(TermFlags::from_private_param(1006), TermFlags::MOUSE_SGR);
    assert_eq!(
        TermFlags::from_private_param(1047),
        TermFlags::ALT_SCREEN_1047
    );
    assert_eq!(
        TermFlags::from_private_param(1049),
        TermFlags::ALT_SCREEN_1049
    );
    // 4 (IRM) is an ANSI mode, not a DEC private mode.
    assert_eq!(TermFlags::from_private_param(4), TermFlags::NONE);
    assert_eq!(TermFlags::from_private_param(999), TermFlags::NONE);
}

#[test]
fn mode_private_param_values() {
    assert_eq!(TermFlags::CURSOR_KEYS.private_param(), 1);
    assert_eq!(TermFlags::CURSOR_VISIBLE.private_param(), 25);
    assert_eq!(TermFlags::CURSOR_BLINK.private_param(), 12);
    assert_eq!(TermFlags::SYNC_OUTPUT.private_param(), 2026);
    // Insert (ANSI), LNM (ANSI) and None have no private parameter.
    assert_eq!(TermFlags::INSERT.private_param(), 0);
    assert_eq!(TermFlags::LINEFEED.private_param(), 0);
    assert_eq!(TermFlags::NONE.private_param(), 0);
}

#[test]
fn mode_line_mouse_paste_table() {
    assert_eq!(TermFlags::from_private_param(9), TermFlags::MOUSE_X10);
    assert_eq!(
        TermFlags::from_private_param(2004),
        TermFlags::BRACKETED_PASTE
    );
    assert_eq!(TermFlags::MOUSE_X10.private_param(), 9);
    assert_eq!(TermFlags::BRACKETED_PASTE.private_param(), 2004);
}

#[test]
fn mode_code_ansi_vs_private() {
    // ANSI modes (no `?`): IRM 4 and LNM 20.
    assert_eq!(TermFlags::from_mode_code(4, false), TermFlags::INSERT);
    assert_eq!(TermFlags::from_mode_code(20, false), TermFlags::LINEFEED);
    // They are not DEC private modes, so the `?` form is unknown.
    assert_eq!(TermFlags::from_mode_code(4, true), TermFlags::NONE);
    assert_eq!(TermFlags::from_mode_code(20, true), TermFlags::NONE);
    // DEC private modes only resolve in the `?` form.
    assert_eq!(
        TermFlags::from_mode_code(25, true),
        TermFlags::CURSOR_VISIBLE
    );
    assert_eq!(TermFlags::from_mode_code(25, false), TermFlags::NONE);
    assert_eq!(TermFlags::from_mode_code(1006, true), TermFlags::MOUSE_SGR);
    assert_eq!(TermFlags::from_mode_code(1006, false), TermFlags::NONE);
    // Unknown in both forms.
    assert_eq!(TermFlags::from_mode_code(999, false), TermFlags::NONE);
    assert_eq!(TermFlags::from_mode_code(999, true), TermFlags::NONE);
}

#[test]
fn mode_code_any_lookup() {
    // DECRQM/XTSAVE/XTRESTORE look up any tracked mode by code, ANSI or DEC.
    assert_eq!(TermFlags::from_any_mode_code(4), TermFlags::INSERT);
    assert_eq!(TermFlags::from_any_mode_code(20), TermFlags::LINEFEED);
    assert_eq!(TermFlags::from_any_mode_code(25), TermFlags::CURSOR_VISIBLE);
    assert_eq!(TermFlags::from_any_mode_code(1006), TermFlags::MOUSE_SGR);
    assert_eq!(TermFlags::from_any_mode_code(999), TermFlags::NONE);
}

#[test]
fn mouse_sgr_encoding() {
    assert_eq!(MouseButton::Left.encode_sgr(3, 4, true), "\x1b[<0;4;5M");
    assert_eq!(MouseButton::Left.encode_sgr(3, 4, false), "\x1b[<0;4;5m");
    assert_eq!(MouseButton::Right.encode_sgr(4, 9, false), "\x1b[<2;5;10m");
    assert_eq!(
        MouseButton::WheelUp.encode_sgr(10, 20, true),
        "\x1b[<64;11;21M"
    );
    // Pure motion (no button) is button 3 in SGR: 32 | 3 = 35.
    assert_eq!(
        MouseButton::MotionNoButton.encode_sgr(3, 4, true),
        "\x1b[<35;4;5M"
    );
    // Motion with the left button held (drag) is button 0: 32.
    assert_eq!(MouseButton::Motion.encode_sgr(3, 4, true), "\x1b[<32;4;5M");
    // DEC 1005 UTF-8 mouse: X10 prefix + UTF-8 coordinates.
    assert_eq!(MouseButton::Left.encode_utf8(0, 0), "\x1b[M !!");
    // Coordinates past 0x7F encode as 2-byte UTF-8 (U+0080).
    assert_eq!(MouseButton::Left.encode_utf8(95, 95), "\x1b[M \u{80}\u{80}");
}

#[test]
fn mouse_x10_encoding() {
    // Left button at (0,0): cb=32(' '), cx=33('!'), cy=33('!').
    assert_eq!(
        MouseButton::Left.encode_x10(0, 0).as_slice(),
        &b"\x1b[M !!"[..]
    );
    // Coordinate 95 → cx = 128 = 0x80: a RAW non-UTF-8 byte, not the UTF-8
    // encoding of U+0080.
    assert_eq!(
        MouseButton::Left.encode_x10(95, 95).as_slice(),
        &[0x1b, b'[', b'M', b' ', 0x80, 0x80]
    );
}

#[test]
fn mouse_encode_shared_encoder() {
    // Tracking off → no report.
    let off = MouseTracking::empty();
    assert_eq!(MouseButton::Left.encode(off, 0, 0, true), None);

    // SGR (1006) uses the SGR sequence; release keeps the button code.
    let sgr = MouseTracking::ENABLED | MouseTracking::SGR;
    assert_eq!(
        MouseButton::Left.encode(sgr, 3, 4, true).as_deref(),
        Some(&b"\x1b[<0;4;5M"[..])
    );
    assert_eq!(
        MouseButton::Left.encode(sgr, 3, 4, false).as_deref(),
        Some(&b"\x1b[<0;4;5m"[..])
    );

    // X10 release uses the generic button-3 code.
    let x10 = MouseTracking::ENABLED;
    assert_eq!(
        MouseButton::Left.encode(x10, 0, 0, true).as_deref(),
        Some(&b"\x1b[M !!"[..])
    );
    assert_eq!(
        MouseButton::Left.encode(x10, 0, 0, false).as_deref(),
        Some(&b"\x1b[M#!!"[..])
    );

    // DEC 9 X10 press-only mode suppresses releases (1000 reports both).
    let x10_mode = MouseTracking::ENABLED | MouseTracking::X10;
    assert_eq!(
        MouseButton::Left.encode(x10_mode, 0, 0, true).as_deref(),
        Some(&b"\x1b[M !!"[..])
    );
    assert_eq!(
        MouseButton::Left.encode(x10_mode, 0, 0, false),
        None,
        "DEC 9 reports no releases"
    );

    // SGR-pixel (1016): 1-based pixel coordinates (herdr maps pixel 1,1 to
    // the top-left cell and rejects 0).
    let px = MouseTracking::ENABLED | MouseTracking::PIXEL;
    assert_eq!(
        MouseButton::Left.encode(px, 27, 72, true).as_deref(),
        Some(&b"\x1b[<0;28;73M"[..])
    );

    // X10 motion is only emitted in any-motion (1003) mode.
    assert_eq!(MouseButton::Motion.encode(x10, 0, 0, true), None);
    let x10_any = MouseTracking::ENABLED | MouseTracking::ANY_MOTION;
    assert_eq!(
        MouseButton::Motion.encode(x10_any, 0, 0, true).as_deref(),
        Some(&b"\x1b[M@!!"[..])
    );

    // X10's single-byte coordinates top out at 222 (222 + 1 + 32 = 255);
    // a larger coordinate would wrap and must be dropped (Ghostty).
    assert_eq!(
        MouseButton::Left.encode(x10, 222, 0, true).as_deref(),
        Some(&b"\x1b[M \xff!"[..])
    );
    assert_eq!(MouseButton::Left.encode(x10, 223, 0, true), None);
    assert_eq!(MouseButton::Left.encode(x10, 0, 223, true), None);
}

#[test]
fn decrqm_reply_format() {
    assert_eq!(Reply::new().decrqm(25, true, 1).finish(), "\x1b[?25;1$y");
    assert_eq!(Reply::new().decrqm(25, true, 2).finish(), "\x1b[?25;2$y");
    assert_eq!(Reply::new().decrqm(25, true, 0).finish(), "\x1b[?25;0$y");
    assert_eq!(Reply::new().decrqm(4, false, 2).finish(), "\x1b[4;2$y");
}

#[test]
fn decrqss_reply_format() {
    assert_eq!(Reply::new().decrqss(true, "m").finish(), "\x1bP1$rm\x1b\\");
    assert_eq!(Reply::new().decrqss(false, "x").finish(), "\x1bP0$rx\x1b\\");
}

#[test]
fn sgr_reply_format() {
    let pal = [0_u32; 256];
    assert_eq!(
        Reply::new()
            .sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::empty(), &pal,)
            .finish(),
        "0;39;49"
    );
    assert_eq!(
        Reply::new()
            .sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::BOLD, &pal)
            .finish(),
        "0;1;39;49"
    );
    // 5 = indexed, 258 = an RGB id (0x112233) in the palette.
    let mut pal = [0_u32; 259];
    pal[258] = 0x112233;
    assert_eq!(
        Reply::new()
            .sgr(ColorId(5), ColorId(258), SgrFlags::empty(), &pal)
            .finish(),
        "0;38;5;5;48;2;17;34;51"
    );
}

#[test]
fn reply_is_empty() {
    assert!(Reply::new().is_empty());
    assert!(!Reply::new().decrqss(true, "m").is_empty());
    assert!(!Reply::new().decrqm(25, true, 1).is_empty());
    assert!(
        !Reply::new()
            .sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::empty(), &[])
            .is_empty()
    );
    assert!(!Reply::new().xtgettcap_reply(b"TN", b"xterm").is_empty());
    assert!(!Reply::new().xtgettcap_num(b"co", 80).is_empty());
}
