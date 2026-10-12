// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Mouse event encoding (X10 / SGR).

use smallvec::SmallVec;
use smol_str::{SmolStr, SmolStrBuilder};

bitflags::bitflags! {
    /// Active mouse-tracking configuration, derived from the DEC mouse flags —
    /// the screen-independent core: every screen maps its flags into this and
    /// delegates to [`MouseButton::encode`], so X10/SGR selection and motion
    /// gating live in one place.
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
        /// DEC 1016 — SGR-pixel protocol: coordinates are 1-based pixels, just
        /// like the cell form is 1-based.
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
    /// 0-based cell/pixel coordinates; the emitted protocol is always 1-based
    /// (`xterm`/`herdr` treat the top-left cell *and* pixel as `1;1`). The
    /// button code is the same for press/release; only the final byte differs.
    #[inline]
    pub fn encode_sgr(self, x: u16, y: u16, pressed: bool) -> SmolStr {
        let final_byte = if pressed { 'M' } else { 'm' };
        let (x, y) = (x + 1, y + 1);
        // `Motion` = motion + left button (a drag, `cb=32`);
        // `MotionNoButton` = pure hover (`cb=35`). The button code is used
        // verbatim so ratatui/crossterm can tell Drag(Left) from Moved.
        smol_str::format_smolstr!("\x1b[<{};{};{}{}", self as u8, x, y, final_byte)
    }

    /// Encode this event in X10 style (`CSI M Cb Cx Cy`, single bytes).
    /// `Cx`/`Cy` are RAW bytes (`32 + coord`, truncated) — they may be
    /// non-UTF-8 for coordinates ≥ 96, so this returns bytes, not a string.
    #[inline]
    pub fn encode_x10(self, x: u16, y: u16) -> SmallVec<u8, 8> {
        let (cb, cx, cy) = self.x10_bytes(x, y);
        SmallVec::from([0x1b, b'[', b'M', cb, cx as u8, cy as u8])
    }

    /// Encode this event in DEC 1005 UTF-8 mouse style — X10's `CSI M Cb`
    /// prefix, but `Cx`/`Cy` are UTF-8 encoded instead of truncated single
    /// bytes. Built on the stack via [`SmolStrBuilder`] (no heap `String`).
    #[inline]
    pub fn encode_utf8(self, x: u16, y: u16) -> SmolStr {
        let (cb, cx, cy) = self.x10_bytes(x, y);
        let mut b = SmolStrBuilder::new();
        b.push_str("\x1b[M");
        b.push(cb as char);
        b.push(char::from_u32(cx as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        b.push(char::from_u32(cy as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        b.finish()
    }

    /// Encode a mouse event under the given tracking configuration, or `None`
    /// when tracking is disabled. `x`/`y` are 0-based cell coordinates.
    pub fn encode(
        self,
        tracking: MouseTracking,
        x: u16,
        y: u16,
        pressed: bool,
    ) -> Option<SmallVec<u8, 8>> {
        if !tracking.contains(MouseTracking::ENABLED) {
            return None;
        }
        // X10 (DEC 9) reports only button PRESSES, never releases.
        if tracking.contains(MouseTracking::X10) && !pressed {
            return None;
        }
        if tracking.intersects(MouseTracking::SGR | MouseTracking::PIXEL) {
            return Some(SmallVec::from_slice_copy(
                self.encode_sgr(x, y, pressed).as_bytes(),
            ));
        }
        // Drag motion reports only in any-motion (1003) mode; release is
        // `Cb = 3`.
        if pressed && self == Self::Motion && !tracking.contains(MouseTracking::ANY_MOTION) {
            return None;
        }
        let b = if pressed { self } else { Self::Release };
        if tracking.contains(MouseTracking::UTF8) {
            return Some(SmallVec::from_slice_copy(b.encode_utf8(x, y).as_bytes()));
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
