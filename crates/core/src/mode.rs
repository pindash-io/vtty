// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Terminal mode flags: ANSI `CSI Ps h/l` (SM/RM) and DEC private `CSI ? Pm
//! h/l` (DECSET/DECRST), plus `CSI ? Ps $ p` (DECRQM). Private-mode numbers
//! follow the [DECronomicon table] and [xterm's ctlseqs].
//!
//! [DECronomicon table]: https://vt100.net/emu/dec_private_modes.html
//! [xterm's ctlseqs]: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html

/// Define the DEC private-mode mapping from one list of `Flag => number`
/// pairs: [`TermFlags::private_param`] and [`TermFlags::from_private_param`],
/// so the flag ↔ number mapping cannot drift apart.
macro_rules! private_modes {
    ($($flag:ident => $param:literal),+ $(,)?) => {
        /// The DEC private-mode parameter (`? Pm h/l` / DECRQM). `0` for
        /// ANSI modes (insert, `CSI 4 h`) and for [`TermFlags::NONE`].
        #[inline]
        pub const fn private_param(self) -> i32 {
            match self {
                $(Self::$flag => $param,)+
                _ => 0,
            }
        }

        /// Reverse lookup for DECRQM's numeric parameter; unknown →
        /// [`TermFlags::NONE`].
        #[inline]
        pub const fn from_private_param(param: i32) -> Self {
            match param {
                $($param => Self::$flag,)+
                _ => Self::NONE,
            }
        }
    };
}

/// Mouse-tracking + protocol modes; reset on alt-screen entry so pagers
/// start clean.
pub const MOUSE_MASK: TermFlags = TermFlags::MOUSE_X10
    .union(TermFlags::MOUSE_NORMAL)
    .union(TermFlags::MOUSE_BUTTON)
    .union(TermFlags::MOUSE_ANY)
    .union(TermFlags::MOUSE_SGR)
    .union(TermFlags::MOUSE_PIXEL)
    .union(TermFlags::MOUSE_URXVT)
    .union(TermFlags::UTF8_MOUSE);

bitflags::bitflags! {
    /// Settable terminal modes tracked by a screen, as a single flag set.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct TermFlags: u64 {
        /// No mode, from `from_private_param` for an unknown code.
        const NONE = 0;
        /// IRM (`CSI 4`) insert/replace.
        const INSERT = 1 << 0;
        /// LNM (`CSI 20`) linefeed/newline.
        const LINEFEED = 1 << 3;
        /// DECCKM (`CSI ?1`) application cursor keys.
        const CURSOR_KEYS = 1 << 10;
        /// DECCOLM (`CSI ?3`) 132 columns, tracked only.
        const DECCOLM = 1 << 13;
        /// DECSCNM (`CSI ?5`) reverse video.
        const REVERSE_COLORS = 1 << 11;
        /// DECOM (`CSI ?6`) origin at the scroll margin.
        const ORIGIN = 1 << 1;
        /// DECAWM (`CSI ?7`) autowrap.
        const WRAPAROUND = 1 << 2;
        /// X10 mouse reporting.
        const MOUSE_X10 = 1 << 6;
        /// DEC 12 (`CSI ?12`) blinking cursor.
        const CURSOR_BLINK = 1 << 32;
        /// DECTCEM (`CSI ?25`) cursor visible.
        const CURSOR_VISIBLE = 1 << 4;
        /// DEC 45 (`CSI ?45`) reverse wrap-around.
        const REVERSE_WRAP = 1 << 22;
        /// DEC 47 (`CSI ?47`) alternate screen buffer.
        const ALT_SCREEN = 1 << 14;
        /// DECNKM (`CSI ?66`) application keypad.
        const KEYPAD_KEYS = 1 << 24;
        /// DECBKM (`CSI ?67`) Backspace sends 0x08.
        const BACKARROW_KEY_MODE = 1 << 33;
        /// DEC 1035 (`CSI ?1035`) ignore numlock on the keypad.
        const NUMLOCK = 1 << 34;
        /// DECLRMM (`CSI ?69`) left/right margins.
        const ENABLE_LEFT_AND_RIGHT_MARGIN = 1 << 23;
        /// DEC 80 (`CSI ?80`) Sixel display mode, tracked only.
        const DECSDM = 1 << 27;
        /// DEC 1000 (`CSI ?1000`) press tracking.
        const MOUSE_NORMAL = 1 << 7;
        /// DEC 1002 (`CSI ?1002`) drag tracking.
        const MOUSE_BUTTON = 1 << 8;
        /// DEC 1003 (`CSI ?1003`) any-motion tracking.
        const MOUSE_ANY = 1 << 9;
        /// DEC 1004 (`CSI ?1004`) focus in/out.
        const FOCUS = 1 << 18;
        /// DEC 1005 (`CSI ?1005`) UTF-8 mouse.
        const UTF8_MOUSE = 1 << 19;
        /// DEC 1006 (`CSI ?1006`) SGR mouse.
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
        /// DEC 1047 — alternate screen that clears on entry but does not
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
    private_modes! {
        CURSOR_KEYS => 1,                    // DECCKM
        DECCOLM => 3,                        // DECCOLM
        REVERSE_COLORS => 5,                 // DECSCNM
        ORIGIN => 6,                         // DECOM
        WRAPAROUND => 7,                     // DECAWM
        MOUSE_X10 => 9,                      // DEC 9
        CURSOR_BLINK => 12,                  // DEC 12
        CURSOR_VISIBLE => 25,                // DECTCEM
        REVERSE_WRAP => 45,                  // DEC 45
        ALT_SCREEN => 47,                    // DEC 47
        KEYPAD_KEYS => 66,                   // DECNKM
        BACKARROW_KEY_MODE => 67,            // DECBKM
        ENABLE_LEFT_AND_RIGHT_MARGIN => 69,  // DECLRMM
        DECSDM => 80,                        // DECSDM
        MOUSE_NORMAL => 1000,                // DEC 1000
        MOUSE_BUTTON => 1002,                // DEC 1002
        MOUSE_ANY => 1003,                   // DEC 1003
        FOCUS => 1004,                       // DEC 1004
        UTF8_MOUSE => 1005,                  // DEC 1005
        MOUSE_SGR => 1006,                   // DEC 1006
        ALTERNATE_SCROLL => 1007,            // DEC 1007
        MOUSE_URXVT => 1015,                 // DEC 1015
        MOUSE_PIXEL => 1016,                 // DEC 1016
        NUMLOCK => 1035,                     // DEC 1035
        REVERSE_WRAP_EXTENDED => 1045,       // DEC 1045
        VI => 1046,                          // DEC 1046
        ALT_SCREEN_1047 => 1047,             // DEC 1047
        ALT_SCREEN_1049 => 1049,             // DEC 1049
        BRACKETED_PASTE => 2004,             // XTPUSHKEYS
        SYNC_OUTPUT => 2026,                 // DEC 2026
        UNICODE_CORE => 2027,                // DEC 2027
        COLOR_SCHEME_UPDATES => 2031,        // XTCOLORMOD
        IN_BAND_RESIZE => 2048,              // DEC 2048
    }

    /// Map an ANSI or DEC private mode number to its flag; `private` selects
    /// the table (ANSI covers IRM 4 and LNM 20).
    #[inline]
    pub const fn from_mode_code(code: i32, private: bool) -> Self {
        if private {
            return Self::from_private_param(code);
        }
        match code {
            4 => Self::INSERT,
            20 => Self::LINEFEED,
            _ => Self::NONE,
        }
    }

    /// The ANSI/DEC mode number and `?` form for this flag — the inverse of
    /// [`Self::from_mode_code`]. `None` for [`Self::NONE`] and untracked
    /// flags.
    #[inline]
    pub const fn mode_code(self) -> Option<(i32, bool)> {
        Some(match self {
            Self::INSERT => (4, false),
            Self::LINEFEED => (20, false),
            _ => {
                let code = self.private_param();
                if code == 0 {
                    return None;
                }
                (code, true)
            }
        })
    }

    /// Look up a mode by code regardless of the ANSI/DEC split
    /// (DECRQM/XTSAVE/XTRESTORE).
    #[inline]
    pub const fn from_any_mode_code(code: i32) -> Self {
        let ansi = Self::from_mode_code(code, false);
        if !ansi.is_empty() {
            return ansi;
        }
        Self::from_mode_code(code, true)
    }
}

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
        assert_eq!(TermFlags::from_private_param(66), TermFlags::KEYPAD_KEYS);
        assert_eq!(
            TermFlags::from_private_param(67),
            TermFlags::BACKARROW_KEY_MODE
        );
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
