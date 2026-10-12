// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Mouse event encoding (X10 / SGR).

use smol_str::{SmolStr, SmolStrBuilder};

/// Active mouse-tracking configuration, derived from the DEC mouse flags.
///
/// This is the screen-independent core of mouse reporting: every terminal
/// screen (cell, line) maps its own mode flags into this and delegates to
/// [`MouseButton::encode`], so the X10/SGR selection and motion-gating rules
/// live in exactly one place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseTracking {
    /// Any tracking enabled: DEC 9 (X10), 1000 (press), 1002 (drag), or
    /// 1003 (any-motion).
    pub enabled: bool,
    /// DEC 9 — X10 press-only mode: report button presses, but not
    /// releases (1000 and up report both).
    pub x10: bool,
    /// DEC 1006 — SGR protocol (`CSI < b ; x ; y M/m`).
    pub sgr: bool,
    /// DEC 1003 — any-motion tracking. Required for X10 motion reports.
    pub any_motion: bool,
    /// DEC 1016 — SGR-pixel protocol: coordinates are 1-based pixels, just
    /// like the cell form is 1-based.
    pub pixel: bool,
    /// DEC 1005 — UTF-8 mouse protocol (X10 with UTF-8 coordinates).
    pub utf8: bool,
}

/// Mouse button/event codes for X10 and SGR encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
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
    /// Encode this event as an SGR (`CSI < … M/m`) escape sequence.
    /// `x`/`y` are 0-based cell or pixel coordinates; the emitted protocol
    /// is always 1-based (xterm/herdr treat the top-left cell *and* pixel as
    /// `1;1`). The button code is the same for press and release (0/1/2);
    /// only the final byte differs (`M` press / `m` release).
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
    pub fn encode_x10(self, x: u16, y: u16) -> smallvec::SmallVec<[u8; 8]> {
        let cb = (self as u8).wrapping_add(32);
        let cx = x.wrapping_add(1).wrapping_add(32) as u8;
        let cy = y.wrapping_add(1).wrapping_add(32) as u8;
        smallvec::smallvec![0x1b, b'[', b'M', cb, cx, cy]
    }

    /// Encode this event in DEC 1005 UTF-8 mouse style — X10's `CSI M Cb`
    /// prefix, but `Cx`/`Cy` are UTF-8 encoded instead of truncated single
    /// bytes. Built on the stack via [`SmolStrBuilder`] (no heap `String`).
    #[inline]
    pub fn encode_utf8(self, x: u16, y: u16) -> SmolStr {
        let cb = (self as u8).wrapping_add(32);
        let cx = x.wrapping_add(1).wrapping_add(32);
        let cy = y.wrapping_add(1).wrapping_add(32);
        let mut b = SmolStrBuilder::new();
        b.push_str("\x1b[M");
        b.push(cb as char);
        b.push(char::from_u32(cx as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        b.push(char::from_u32(cy as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        b.finish()
    }

    /// Encode a mouse event under the given tracking configuration, or `None`
    /// when tracking is disabled. `x`/`y` are 0-based cell coordinates.
    ///
    /// The X10/SGR selection, the generic X10 release code, and the
    /// any-motion gating for X10 motion are all handled here.
    pub fn encode(
        self,
        tracking: MouseTracking,
        x: u16,
        y: u16,
        pressed: bool,
    ) -> Option<smallvec::SmallVec<[u8; 8]>> {
        if !tracking.enabled {
            return None;
        }
        // X10 (DEC 9) reports only button PRESSES, never releases.
        if tracking.x10 && !pressed {
            return None;
        }
        if tracking.sgr || tracking.pixel {
            return Some(smallvec::SmallVec::from_slice(
                self.encode_sgr(x, y, pressed).as_bytes(),
            ));
        }
        // X10 encodes release as `Cb = 3` (button 0 released) and only
        // reports motion in any-motion (1003) mode.
        let b = if pressed { self } else { Self::Release };
        if b == Self::Motion && !tracking.any_motion {
            return None;
        }
        if tracking.utf8 {
            Some(smallvec::SmallVec::from_slice(
                b.encode_utf8(x, y).as_bytes(),
            ))
        } else {
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
}
