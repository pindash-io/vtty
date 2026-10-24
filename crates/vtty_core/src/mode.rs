// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! DEC/ANSI terminal mode flags: ANSI `CSI Ps h/l` (SM/RM) and DEC private
//! `CSI ? Pm h/l` (DECSET/DECRST), plus `CSI ? Ps $ p` (DECRQM) reporting.
//!
//! DEC private-mode numbers follow the [DECronomicon private-modes
//! table](https://vt100.net/emu/dec_private_modes.html); the xterm/kitty
//! extensions (mouse, alternate screen, synchronized output) follow [xterm's
//! ctlseqs](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html).

bitflags::bitflags! {
    /// Settable terminal modes tracked by a screen, as a single flag set.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TermFlags: u64 {
        /// No mode — returned by [`TermFlags::from_private_param`] for
        /// unknown codes.
        const NONE = 0;
        /// IRM (`CSI 4 h/l`) — insert/replace mode.
        const INSERT = 1 << 0;
        /// LNM (`CSI 20 h/l`) — linefeed/newline mode.
        const LINEFEED = 1 << 3;
        /// DECCKM (`CSI ?1 h/l`) — application cursor keys (arrows send SS3).
        const CURSOR_KEYS = 1 << 10;
        /// DECCOLM (`CSI ?3 h/l`) — 132-column mode (tracked; the host owns
        /// the real width).
        const DECCOLM = 1 << 13;
        /// DECSCNM (`CSI ?5 h/l`) — reverse video (swap fg/bg at render time).
        const REVERSE = 1 << 11;
        /// DECOM (`CSI ?6 h/l`) — origin mode (positioning relative to the
        /// scroll region).
        const ORIGIN = 1 << 1;
        /// DECAWM (`CSI ?7 h/l`) — autowrap.
        const WRAPAROUND = 1 << 2;
        /// DEC 9 — X10 mouse reporting.
        const MOUSE_X10 = 1 << 6;
        /// DEC 12 (`CSI ?12 h/l`) — cursor blinking (AT&T 610).
        const CURSOR_BLINK = 1 << 32;
        /// DECTCEM (`CSI ?25 h/l`) — cursor visible.
        const CURSOR_VISIBLE = 1 << 4;
        /// DEC 45 (`CSI ?45 h/l`) — reverse wrap-around (BS at the left
        /// margin wraps up to the previous row's right margin).
        const REVERSE_WRAP = 1 << 22;
        /// DEC 47 — alternate screen buffer (persistent: re-entering restores
        /// its content and cursor).
        const ALT_SCREEN = 1 << 14;
        /// DEC 66 — application keypad mode (DECNKM).
        const DECNKM = 1 << 24;
        /// DEC 67 — backarrow key mode (DECBKM): Backspace sends `\x08`
        /// instead of `\x7f`.
        const DECBKM = 1 << 33;
        /// DEC 1035 (`CSI ?1035 h/l`) — ignore the numlock state for the
        /// keypad (the keypad is always numeric, never application).
        const NUMLOCK = 1 << 34;
        /// DEC 69 — left/right margin mode (DECLRMM).
        const DECLRMM = 1 << 23;
        /// DEC 80 — Sixel display mode (scrolling vs. fixed). Tracked only;
        /// the image renderer owns the actual Sixel placement.
        const DECSDM = 1 << 27;
        /// DEC 1000 — mouse press tracking.
        const MOUSE_NORMAL = 1 << 7;
        /// DEC 1002 — mouse drag tracking.
        const MOUSE_BUTTON = 1 << 8;
        /// DEC 1003 — mouse any-motion tracking.
        const MOUSE_ANY = 1 << 9;
        /// DEC 1004 — focus in/out reporting.
        const FOCUS = 1 << 18;
        /// DEC 1005 — UTF-8 mouse reporting.
        const UTF8_MOUSE = 1 << 19;
        /// DEC 1006 — SGR mouse protocol (button codes are `CSI < b ; x ; y`).
        const MOUSE_SGR = 1 << 12;
        /// DEC 1007 — alternate scroll (wheel sends arrows).
        const ALTERNATE_SCROLL = 1 << 20;
        /// DEC 1015 — urxvt mouse protocol. Tracked for DECRQM fidelity;
        /// never emitted (SGR 1006 takes precedence when both are set).
        const MOUSE_URXVT = 1 << 28;
        /// DEC 1016 — SGR-pixel mouse (pixel coordinates).
        const MOUSE_PIXEL = 1 << 26;
        /// DEC 1045 (`CSI ?1045 h/l`) — extended reverse wrap-around: like
        /// DEC 45 but crosses NON-wrapped rows and wraps around the scroll
        /// region's top edge (xterm XTREVWRAP2).
        const REVERSE_WRAP_EXTENDED = 1 << 30;
        /// DEC 1046 — alternate screen, save/restore cursor.
        const VI = 1 << 21;
        /// DEC 1047 — alternate screen that clears on entry but does NOT
        /// save/restore the cursor (unlike 1049).
        const ALT_SCREEN_1047 = 1 << 31;
        /// DEC 1049 — alternate screen that clears on entry and saves/
        /// restores the main-screen cursor (1047 + 1048).
        const ALT_SCREEN_1049 = 1 << 15;
        /// XTPUSHKEYS (`CSI ?2004 h/l`) — bracketed paste.
        const BRACKETED_PASTE = 1 << 5;
        /// DEC 2026 — synchronized output.
        const SYNC_OUTPUT = 1 << 16;
        /// DEC 2027 — Unicode core mode (grapheme-cluster width).
        const UNICODE_CORE = 1 << 25;
        /// XTCOLORMOD (`CSI ?2031 h/l`) — report color-scheme changes.
        const COLOR_SCHEME_UPDATES = 1 << 17;
        /// DEC 2048 — in-band window resize: the terminal reports size
        /// changes as `CSI 48;lines;cols;pixel_lines;pixel_cols t` instead of
        /// the app relying on SIGWINCH.
        const IN_BAND_RESIZE = 1 << 29;
    }
}

impl TermFlags {
    /// `(flag bits, DEC private-mode number)` pairs, kept in one place so
    /// [`TermFlags::private_param`] and [`TermFlags::from_private_param`]
    /// cannot drift apart. ANSI-only modes (INSERT `4`, LINEFEED `20`) have
    /// no DEC private number and are absent.
    const PRIVATE_MODES: &'static [(u64, i32)] = &[
        (Self::CURSOR_KEYS.bits(), 1),              // DECCKM
        (Self::DECCOLM.bits(), 3),                  // DECCOLM (tracked; host owns width)
        (Self::REVERSE.bits(), 5),                  // DECSCNM
        (Self::ORIGIN.bits(), 6),                   // DECOM
        (Self::WRAPAROUND.bits(), 7),               // DECAWM
        (Self::MOUSE_X10.bits(), 9),                // DEC 9
        (Self::CURSOR_BLINK.bits(), 12),            // DEC 12
        (Self::CURSOR_VISIBLE.bits(), 25),          // DECTCEM
        (Self::REVERSE_WRAP.bits(), 45),            // DEC 45
        (Self::ALT_SCREEN.bits(), 47),              // DEC 47
        (Self::DECNKM.bits(), 66),                  // DECNKM
        (Self::DECBKM.bits(), 67),                  // DECBKM
        (Self::DECLRMM.bits(), 69),                 // DECLRMM
        (Self::DECSDM.bits(), 80),                  // DECSDM
        (Self::MOUSE_NORMAL.bits(), 1000),          // DEC 1000
        (Self::MOUSE_BUTTON.bits(), 1002),          // DEC 1002
        (Self::MOUSE_ANY.bits(), 1003),             // DEC 1003
        (Self::FOCUS.bits(), 1004),                 // DEC 1004
        (Self::UTF8_MOUSE.bits(), 1005),            // DEC 1005
        (Self::MOUSE_SGR.bits(), 1006),             // DEC 1006
        (Self::ALTERNATE_SCROLL.bits(), 1007),      // DEC 1007
        (Self::MOUSE_URXVT.bits(), 1015),           // DEC 1015
        (Self::MOUSE_PIXEL.bits(), 1016),           // DEC 1016
        (Self::NUMLOCK.bits(), 1035),               // DEC 1035
        (Self::REVERSE_WRAP_EXTENDED.bits(), 1045), // DEC 1045
        (Self::VI.bits(), 1046),                    // DEC 1046
        (Self::ALT_SCREEN_1047.bits(), 1047),       // DEC 1047
        (Self::ALT_SCREEN_1049.bits(), 1049),       // DEC 1049
        (Self::BRACKETED_PASTE.bits(), 2004),       // XTPUSHKEYS
        (Self::SYNC_OUTPUT.bits(), 2026),           // DEC 2026
        (Self::UNICODE_CORE.bits(), 2027),          // DEC 2027
        (Self::COLOR_SCHEME_UPDATES.bits(), 2031),  // XTCOLORMOD
        (Self::IN_BAND_RESIZE.bits(), 2048),        // DEC 2048
    ];

    /// The DEC private-mode parameter (`? Pm h/l` / DECRQM). `0` for ANSI
    /// modes (insert, `CSI 4 h`) and for [`TermFlags::NONE`].
    #[inline]
    pub const fn private_param(self) -> i32 {
        let bits = self.bits();
        let mut i = 0;
        while i < Self::PRIVATE_MODES.len() {
            let (flag_bits, param) = Self::PRIVATE_MODES[i];
            if flag_bits == bits {
                return param;
            }
            i += 1;
        }
        0
    }

    /// Reverse lookup for DECRQM's numeric parameter; unknown →
    /// [`TermFlags::NONE`].
    #[inline]
    pub const fn from_private_param(param: i32) -> Self {
        let mut i = 0;
        while i < Self::PRIVATE_MODES.len() {
            let (flag_bits, p) = Self::PRIVATE_MODES[i];
            if p == param {
                return Self::from_bits_truncate(flag_bits);
            }
            i += 1;
        }
        Self::NONE
    }

    /// Map an ANSI (`CSI Pm h/l`) or DEC private (`CSI ? Pm h/l`) mode number
    /// to its flag. `private` selects the DEC private table; the ANSI table
    /// covers IRM (`4`) and LNM (`20`). This is the one code→flag entry point
    /// for DECSET/DECRST.
    #[inline]
    pub const fn from_mode_code(code: i32, private: bool) -> Self {
        if private {
            Self::from_private_param(code)
        } else {
            match code {
                4 => Self::INSERT,
                20 => Self::LINEFEED,
                _ => Self::NONE,
            }
        }
    }

    /// Look up a tracked mode by its numeric code regardless of the ANSI/DEC
    /// private split — used by DECRQM/XTSAVE/XTRESTORE, which query/save/
    /// restore a mode rather than set a specific ANSI or private form.
    #[inline]
    pub const fn from_any_mode_code(code: i32) -> Self {
        let ansi = Self::from_mode_code(code, false);
        if ansi.is_empty() {
            Self::from_mode_code(code, true)
        } else {
            ansi
        }
    }
}

/// Mouse-tracking + mouse-protocol modes. Entering the alternate screen
/// resets these so a pager (`less`/`man`) starts clean and text selection
/// works; mouse-first apps (nvim/herdr) re-enable them right after `?1049h`.
pub const MOUSE_MASK: TermFlags = TermFlags::MOUSE_X10
    .union(TermFlags::MOUSE_NORMAL)
    .union(TermFlags::MOUSE_BUTTON)
    .union(TermFlags::MOUSE_ANY)
    .union(TermFlags::MOUSE_SGR)
    .union(TermFlags::MOUSE_PIXEL)
    .union(TermFlags::MOUSE_URXVT)
    .union(TermFlags::UTF8_MOUSE);

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(TermFlags::from_private_param(66), TermFlags::DECNKM);
        assert_eq!(TermFlags::from_private_param(67), TermFlags::DECBKM);
        assert_eq!(TermFlags::from_private_param(1035), TermFlags::NUMLOCK);
        // 4 (IRM) is an ANSI mode, not a DEC private mode.
        assert_eq!(TermFlags::from_private_param(4), TermFlags::NONE);
        assert_eq!(TermFlags::from_private_param(999), TermFlags::NONE);
    }

    #[test]
    fn all_private_modes_roundtrip() {
        // Every table entry must invert cleanly in BOTH directions — the
        // single source of truth for the DEC private-mode number ↔ flag pair.
        for &(bits, param) in TermFlags::PRIVATE_MODES {
            let flag = TermFlags::from_bits_truncate(bits);
            assert_eq!(flag.private_param(), param, "bits {bits} → param");
            assert_eq!(
                TermFlags::from_private_param(param),
                flag,
                "param {param} → flag"
            );
        }
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
}
