// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Mouse event encoding (X10 / normal / SGR / urxvt / pixel).
//!
//! Mouse tracking is an xterm extension using DEC private modes
//! 9/1000/1002/1003/1005/1006/1015/1016 — see [xterm ctlseqs, Mouse
//! Tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking).

use compact_str::CompactString;
use smallvec::SmallVec;

/// Inline capacity for a mouse sequence. SGR (`ESC [ < cb ; x ; y M/m`) is
/// the longest at ≤ 19 bytes; X10 (6) and UTF-8 (≤ 11) are shorter. 24 keeps
/// all three on the stack.
const MOUSE_SEQ_INLINE: usize = 24;

bitflags::bitflags! {
    /// Active mouse-tracking configuration, derived from the DEC mouse flags;
    /// screens map their flags into this and delegate to [`MouseButton::encode`].
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MouseTracking: u8 {
        /// Any tracking enabled: DEC 9 (X10), 1000 (press), 1002 (drag), or
        /// 1003 (any-motion).
        const ENABLED = 1 << 0;
        /// DEC 9 — X10 press-only mode: report button presses, but not
        /// releases (1000 and up report both).
        const X10 = 1 << 1;
        /// DEC 1006 — SGR protocol (`CSI < b ; x ; y M/m`).
        const SGR = 1 << 2;
        /// DEC 1003 — any-motion tracking. Required for X10 motion reports.
        const ANY_MOTION = 1 << 3;
        /// DEC 1016 — SGR-pixel protocol (1-based pixel coordinates).
        const PIXEL = 1 << 4;
        /// DEC 1005 — UTF-8 mouse protocol (X10 with UTF-8 coordinates).
        const UTF8 = 1 << 5;
    }
}

impl MouseTracking {
    /// All tracking disabled.
    pub const DEFAULT: Self = Self::empty();
}

/// Mouse button/event codes for X10 and SGR encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum MouseButton {
    /// Left button (button 0).
    Left = 0,
    /// Middle button (button 1).
    Middle = 1,
    /// Right button (button 2).
    Right = 2,
    /// Button release (button code + 3).
    Release = 3,
    /// Motion with the left button held (bit 5 | button 0) — a drag.
    Motion = 32,
    /// Motion with no button held (bit 5 | button 3) — pure hover.
    MotionNoButton = 35,
    /// Wheel scroll up (bit 6 set).
    WheelUp = 64,
    /// Wheel scroll down (bit 6 + 1 set).
    WheelDown = 65,
    /// Wheel scroll left (bit 6 + 2 set).
    WheelLeft = 66,
    /// Wheel scroll right (bit 6 + 3 set).
    WheelRight = 67,
}

impl MouseButton {
    /// X10 `Cb`/`Cx`/`Cy`: button code + 32, and 1-based coordinates + 32.
    #[inline]
    const fn x10_bytes(self, x: u16, y: u16) -> (u8, u16, u16) {
        (
            (self as u8).wrapping_add(32),
            x.wrapping_add(33),
            y.wrapping_add(33),
        )
    }

    /// Encode this event as an SGR (`CSI < … M/m`) sequence. `x`/`y` are
    /// 0-based; the emitted protocol is 1-based. Press/release differ only
    /// in the final byte (`M`/`m`).
    #[inline]
    #[must_use]
    pub fn encode_sgr(self, x: u16, y: u16, pressed: bool) -> CompactString {
        let final_byte = if pressed { 'M' } else { 'm' };
        let (x, y) = (x + 1, y + 1);
        // `Motion` = motion + left button (a drag, `cb=32`);
        // `MotionNoButton` = pure hover (`cb=35`). The button code is used
        // verbatim so ratatui/crossterm can tell Drag(Left) from Moved.
        compact_str::format_compact!("\x1b[<{};{};{}{}", self as u8, x, y, final_byte)
    }

    /// Encode this event in X10 style (`CSI M Cb Cx Cy`, single bytes).
    /// `Cx`/`Cy` are RAW bytes (`32 + coord`, truncated) — they may be
    /// non-UTF-8 for coordinates ≥ 96, so this returns bytes, not a string.
    #[inline]
    #[must_use]
    pub fn encode_x10(self, x: u16, y: u16) -> SmallVec<u8, MOUSE_SEQ_INLINE> {
        let (cb, cx, cy) = self.x10_bytes(x, y);
        SmallVec::from([0x1b, b'[', b'M', cb, cx as u8, cy as u8])
    }

    /// Encode this event in DEC 1005 UTF-8 mouse style — X10's `CSI M Cb`
    /// prefix, but `Cx`/`Cy` are UTF-8 encoded instead of truncated single
    /// bytes. Built inline (no heap `String`).
    #[inline]
    #[must_use]
    pub fn encode_utf8(self, x: u16, y: u16) -> CompactString {
        let (cb, cx, cy) = self.x10_bytes(x, y);
        let mut b = CompactString::new("");
        b.push_str("\x1b[M");
        b.push(cb as char);
        b.push(char::from_u32(cx as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        b.push(char::from_u32(cy as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        b
    }

    /// Encode a mouse event under the given tracking configuration, or `None`
    /// when tracking is disabled. `x`/`y` are 0-based cell coordinates.
    #[must_use]
    pub fn encode(
        self,
        tracking: MouseTracking,
        x: u16,
        y: u16,
        pressed: bool,
    ) -> Option<SmallVec<u8, MOUSE_SEQ_INLINE>> {
        if !tracking.contains(MouseTracking::ENABLED) {
            return None;
        }
        // X10 (DEC 9) reports only button PRESSES, never releases.
        if tracking.contains(MouseTracking::X10) && !pressed {
            return None;
        }
        if tracking.intersects(MouseTracking::SGR | MouseTracking::PIXEL) {
            return Some(SmallVec::from(self.encode_sgr(x, y, pressed).as_bytes()));
        }
        // Drag motion reports only in any-motion (1003) mode; release is
        // `Cb = 3`.
        if pressed && self == Self::Motion && !tracking.contains(MouseTracking::ANY_MOTION) {
            return None;
        }
        let b = if pressed { self } else { Self::Release };
        if tracking.contains(MouseTracking::UTF8) {
            return Some(SmallVec::from(b.encode_utf8(x, y).as_bytes()));
        }
        // X10's single-byte coordinates top out at 222: `32 + coord + 1`
        // must fit in one byte (≤ 255), so a coordinate ≥ 223 would wrap
        // and corrupt the byte stream. Ghostty's X10 encoder drops the
        // event instead (`cell.x > 222 || cell.y > 222` → return).
        if x > 222 || y > 222 {
            return None;
        }
        Some(b.encode_x10(x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            MouseButton::Left.encode_x10(0, 0).as_ref(),
            &b"\x1b[M !!"[..]
        );
        // Coordinate 95 → cx = 128 = 0x80: a RAW non-UTF-8 byte, not the UTF-8
        // encoding of U+0080.
        assert_eq!(
            MouseButton::Left.encode_x10(95, 95).as_ref(),
            &[0x1b, b'[', b'M', b' ', 0x80, 0x80]
        );
    }

    #[test]
    fn mouse_encode_utf8_mode() {
        let utf8 = MouseTracking::ENABLED | MouseTracking::UTF8;
        assert_eq!(
            MouseButton::Left.encode(utf8, 0, 0, true).as_deref(),
            Some(&b"\x1b[M !!"[..])
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
}
