// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! DEC/ANSI terminal mode flags for `CSI Ps h/l` (set/reset) and
//! `CSI ? Ps $ p` (DECRQM).
//!
//! Reference: <https://vt100.net/emu/dec_ansi_parser>.

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
    /// The DEC private-mode parameter (`? Pm h/l` / DECRQM). `0` for the
    /// ANSI mode (insert, `CSI 4 h`) and for [`TermFlags::NONE`].
    ///
    /// The match arms are the flag bit values, which must stay in sync with
    /// the `bitflags!` definitions above.
    #[inline]
    pub const fn private_param(self) -> i32 {
        match self.bits() {
            1 => 0,              // INSERT (ANSI mode 4)
            2 => 6,              // ORIGIN (DECOM)
            4 => 7,              // WRAPAROUND (DECAWM)
            8 => 0,              // LINEFEED (ANSI LNM 20, not a DEC private mode)
            16 => 25,            // CURSOR_VISIBLE (DECTCEM)
            32 => 2004,          // BRACKETED_PASTE (XTPUSHKEYS)
            64 => 9,             // MOUSE_X10
            128 => 1000,         // MOUSE_NORMAL
            256 => 1002,         // MOUSE_BUTTON
            512 => 1003,         // MOUSE_ANY
            1024 => 1,           // CURSOR_KEYS (DECCKM)
            2048 => 5,           // REVERSE (DECSCNM)
            4096 => 1006,        // MOUSE_SGR
            8192 => 3,           // DECCOLM
            16384 => 47,         // ALT_SCREEN
            32768 => 1049,       // ALT_SCREEN_1049
            65536 => 2026,       // SYNC_OUTPUT
            131072 => 2031,      // COLOR_SCHEME_UPDATES
            262144 => 1004,      // FOCUS
            524288 => 1005,      // UTF8_MOUSE
            1048576 => 1007,     // ALTERNATE_SCROLL
            2097152 => 1046,     // VI
            4194304 => 45,       // REVERSE_WRAP
            8388608 => 69,       // DECLRMM
            16777216 => 66,      // DECNKM
            8589934592 => 67,    // DECBKM
            17179869184 => 1035, // NUMLOCK
            33554432 => 2027,    // UNICODE_CORE
            67108864 => 1016,    // MOUSE_PIXEL
            134217728 => 80,     // DECSDM
            268435456 => 1015,   // MOUSE_URXVT
            536870912 => 2048,   // IN_BAND_RESIZE
            1073741824 => 1045,  // REVERSE_WRAP_EXTENDED
            2147483648 => 1047,  // ALT_SCREEN_1047
            4294967296 => 12,    // CURSOR_BLINK
            _ => 0,
        }
    }

    /// Reverse lookup for DECRQM's numeric parameter; unknown →
    /// [`TermFlags::NONE`].
    #[inline]
    pub const fn from_private_param(param: i32) -> Self {
        match param {
            1 => Self::CURSOR_KEYS,
            // Tracked for DECRQM fidelity, not applied: DECCOLM (?3) would
            // resize the grid, but the host owns the window width.
            3 => Self::DECCOLM,
            5 => Self::REVERSE,
            6 => Self::ORIGIN,
            7 => Self::WRAPAROUND,
            9 => Self::MOUSE_X10,
            12 => Self::CURSOR_BLINK,
            25 => Self::CURSOR_VISIBLE,
            45 => Self::REVERSE_WRAP,
            47 => Self::ALT_SCREEN,
            66 => Self::DECNKM,
            67 => Self::DECBKM,
            1035 => Self::NUMLOCK,
            69 => Self::DECLRMM,
            80 => Self::DECSDM,
            1000 => Self::MOUSE_NORMAL,
            1002 => Self::MOUSE_BUTTON,
            1003 => Self::MOUSE_ANY,
            1004 => Self::FOCUS,
            1005 => Self::UTF8_MOUSE,
            1006 => Self::MOUSE_SGR,
            1007 => Self::ALTERNATE_SCROLL,
            1015 => Self::MOUSE_URXVT,
            1016 => Self::MOUSE_PIXEL,
            1045 => Self::REVERSE_WRAP_EXTENDED,
            1046 => Self::VI,
            1047 => Self::ALT_SCREEN_1047,
            1049 => Self::ALT_SCREEN_1049,
            2004 => Self::BRACKETED_PASTE,
            2026 => Self::SYNC_OUTPUT,
            2027 => Self::UNICODE_CORE,
            2031 => Self::COLOR_SCHEME_UPDATES,
            2048 => Self::IN_BAND_RESIZE,
            _ => Self::NONE,
        }
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
