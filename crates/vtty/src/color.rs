// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The palette index type used by reply formatting.

use core::fmt;
use core::ops::Deref;

/// A palette index: 0..=255 are the dynamic 256-color table, 256/257 are the
/// reserved default foreground/background, and 258+ are RGB truecolors
/// inserted on demand.
///
/// `#[repr(transparent)]` over the `u16` field gives it the exact u16 ABI
/// (2 bytes, align 2) — `#[repr(u16)]` itself is only valid on fieldless
/// enums, not structs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ColorId(pub u16);

impl ColorId {
    /// The underlying `u16` index.
    #[inline]
    pub const fn index(self) -> u16 {
        self.0
    }
}

impl From<u16> for ColorId {
    #[inline]
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<usize> for ColorId {
    #[inline]
    fn from(v: usize) -> Self {
        Self(v as u16)
    }
}

/// SGR indexed-color parameters clamp to the 256-color table (xterm clamps
/// out-of-range indexed colors; values ≥ 256 are malformed).
impl From<i32> for ColorId {
    #[inline]
    fn from(v: i32) -> Self {
        Self(if v < 0 {
            0
        } else if v > 255 {
            255
        } else {
            v as u16
        })
    }
}

impl fmt::Debug for ColorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Transparent: debug as the raw u16 so replay snapshots (which
        // formatted the old `u16` alias) stay byte-identical.
        self.0.fmt(f)
    }
}

impl fmt::Display for ColorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

// Comparisons against raw integer literals (`id == 4`, `id == 4u16`) so call
// sites read naturally. Only the forward direction: reverse impls would
// collide with `core`'s `u16 == u16` and make inference ambiguous.
macro_rules! impl_partial_eq {
    ($t:ty) => {
        impl PartialEq<$t> for ColorId {
            #[inline]
            fn eq(&self, other: &$t) -> bool {
                self.0 as $t == *other
            }
        }
    };
}
impl PartialEq<u16> for ColorId {
    #[inline]
    fn eq(&self, other: &u16) -> bool {
        self.0 == *other
    }
}
impl_partial_eq!(i32);
impl_partial_eq!(usize);

/// `ColorId` derefs to its `u16` index so comparisons (`id == 4`, `id < 8`)
/// and formatting keep reading naturally without `.0` everywhere.
impl Deref for ColorId {
    type Target = u16;

    #[inline]
    fn deref(&self) -> &u16 {
        &self.0
    }
}

/// Reserved palette index for the default foreground (SGR 39).
pub const DEFAULT_FG_ID: ColorId = ColorId(256);
/// Reserved palette index for the default background (SGR 49).
pub const DEFAULT_BG_ID: ColorId = ColorId(257);
