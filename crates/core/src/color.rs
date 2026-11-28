// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The palette index type used by reply formatting.

use core::fmt;

use compact_str::CompactString;
use compact_str::format_compact;

/// Reserved palette index for the default foreground (SGR 39).
pub const DEFAULT_FG_ID: ColorId = ColorId(256);

/// Reserved palette index for the default background (SGR 49).
pub const DEFAULT_BG_ID: ColorId = ColorId(257);

/// A palette index: `0..=255` the 256-color table, `256`/`257` default
/// fg/bg, `258+` RGB truecolors inserted on demand.
///
/// # Example
///
/// ```
/// use vtty_core::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, indexed_rgb};
///
/// let red = ColorId(1);
/// assert_eq!(red.index(), 1);
/// // Palette index 1 resolves to xterm red:
/// assert_eq!(indexed_rgb(red.index() as u8), 0xcd0000ff);
///
/// // The two reserved default slots:
/// assert_eq!(DEFAULT_FG_ID, ColorId(256));
/// assert_eq!(DEFAULT_BG_ID, ColorId(257));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColorId(pub u16);

impl ColorId {
    /// The underlying `u16` index.
    #[inline]
    pub const fn index(self) -> u16 {
        self.0
    }

    /// Clamp `n` to the xterm 256-colour range.
    #[inline]
    pub const fn from_index(n: i32) -> Self {
        Self(clamp_channel(n) as u16)
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

/// SGR indexed-color parameters clamp to the 256-color table, as xterm does.
impl From<i32> for ColorId {
    #[inline]
    fn from(v: i32) -> Self {
        Self::from_index(v)
    }
}

impl fmt::Debug for ColorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug as the raw `u16` so replay snapshots (which formatted the old
        // `u16` alias) stay byte-identical.
        self.0.fmt(f)
    }
}

impl fmt::Display for ColorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

// Forward only: a reverse impl would collide with `u16 == u16`.
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

impl_partial_eq!(i32);
impl_partial_eq!(usize);

impl PartialEq<u16> for ColorId {
    #[inline]
    fn eq(&self, other: &u16) -> bool {
        self.0 == *other
    }
}

/// Deref to the `u16` index so `id < 8`-style comparisons read naturally.
impl core::ops::Deref for ColorId {
    type Target = u16;

    #[inline]
    fn deref(&self) -> &u16 {
        &self.0
    }
}

/// Pack three 0..255 channels into opaque RGBA8 (`0xRRGGBBFF`).
#[inline]
pub const fn rgb_to_u32(r: i32, g: i32, b: i32) -> u32 {
    // Channel order plus a big-endian read packs `0xRRGGBBAA`, no shifts.
    u32::from_be_bytes([clamp_channel(r), clamp_channel(g), clamp_channel(b), 0xff])
}

/// Unpack `0xRRGGBBAA` into `[R, G, B]`, dropping alpha (the integer
/// counterpart of [`u32_to_rgba`], for luma and table math).
#[inline]
pub const fn u32_to_array(c: u32) -> [u8; 3] {
    let [r, g, b, _] = c.to_be_bytes();
    [r, g, b]
}

/// Pack `[R, G, B, A]` (0..1 floats) into `0xRRGGBBAA`.
#[inline]
pub const fn rgba_to_u32(c: [f32; 4]) -> u32 {
    let r = round_channel(c[0].clamp(0.0, 1.0) * 255.0);
    let g = round_channel(c[1].clamp(0.0, 1.0) * 255.0);
    let b = round_channel(c[2].clamp(0.0, 1.0) * 255.0);
    let a = round_channel(c[3].clamp(0.0, 1.0) * 255.0);
    u32::from_be_bytes([r, g, b, a])
}

/// Unpack `0xRRGGBBAA` into `[R, G, B, A]`.
#[inline]
pub const fn u32_to_rgba(c: u32) -> [f32; 4] {
    let [r, g, b, a] = c.to_be_bytes();
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

/// Blend RGB `a` 50% toward RGB `b` per channel (the SGR 2 faint/dim effect).
#[inline]
pub const fn blend_faint(a: u32, b: u32) -> u32 {
    let [ar, ag, ab] = u32_to_array(a);
    let [br, bg, bb] = u32_to_array(b);
    rgb_to_u32(
        (ar as i32 + br as i32) / 2,
        (ag as i32 + bg as i32) / 2,
        (ab as i32 + bb as i32) / 2,
    )
}

/// Resolve an xterm 256-color index to packed `0xRRGGBBAA`: ANSI, cube, ramp.
#[inline]
pub const fn indexed_rgb(n: u8) -> u32 {
    match n {
        0 => 0x000000ff,
        1 => 0xcd0000ff,
        2 => 0x00cd00ff,
        3 => 0xcdcd00ff,
        4 => 0x0000eeff,
        5 => 0xcd00cdff,
        6 => 0x00cdcdff,
        7 => 0xe5e5e5ff,
        8 => 0x7f7f7fff,
        9 => 0xff0000ff,
        10 => 0x00ff00ff,
        11 => 0xffff00ff,
        12 => 0x5c5cffff,
        13 => 0xff00ffff,
        14 => 0x00ffffff,
        15 => 0xffffffff,
        16..=231 => {
            let v = n - 16;
            let r = v / 36;
            let g = (v % 36) / 6;
            let b = v % 6;
            rgb_to_u32(
                cube_level(r) as i32,
                cube_level(g) as i32,
                cube_level(b) as i32,
            )
        }
        232..=255 => {
            let v = 8 + (n - 232) * 10;
            rgb_to_u32(v as i32, v as i32, v as i32)
        }
    }
}

/// Parse a CSS hex color into `[R, G, B, A]` bytes: `RGB`, `RGBA`, `RRGGBB` or
/// `RRGGBBAA`, with or without `#`, shorthand repeating each nibble.
#[inline]
pub const fn hex_to_rgba_array(s: &str) -> Option<[u8; 4]> {
    let hex = match s.as_bytes() {
        [b'#', rest @ ..] => rest,
        bytes => bytes,
    };
    // Shorthand repeats each nibble; 3/6-digit alpha stays `ff`.
    let mut color = [b'f'; 8];
    let mut i = 0;
    match hex.len() {
        3 | 4 => {
            while i < hex.len() {
                color[i * 2] = hex[i];
                color[i * 2 + 1] = hex[i];
                i += 1;
            }
        }
        6 | 8 => {
            while i < hex.len() {
                color[i] = hex[i];
                i += 1;
            }
        }
        _ => return None,
    }
    let Ok(rgba) = const_hex::const_decode_to_array(&color) else {
        return None;
    };
    Some(rgba)
}

/// Decode a CSS hex color into `[R, G, B]`, dropping any alpha: the shapes are
/// those of [`hex_to_rgba_array`].
#[inline]
pub const fn hex_to_rgb_array(s: &str) -> Option<[u8; 3]> {
    let Some([r, g, b, _]) = hex_to_rgba_array(s) else {
        return None;
    };
    Some([r, g, b])
}

/// Parse a CSS hex color into packed `0xRRGGBBAA`; the shapes are those of
/// [`hex_to_rgba_array`].
///
/// ```
/// use vtty_core::hex_to_rgba_u32;
///
/// assert_eq!(hex_to_rgba_u32("#f00"), Some(0xff0000ff));
/// assert_eq!(hex_to_rgba_u32("#ff000080"), Some(0xff000080));
/// assert_eq!(hex_to_rgba_u32("#ff"), None);
/// ```
#[inline]
pub const fn hex_to_rgba_u32(s: &str) -> Option<u32> {
    let Some(rgba) = hex_to_rgba_array(s) else {
        return None;
    };
    Some(u32::from_be_bytes(rgba))
}

/// Parse a CSS hex color into `[R, G, B, A]`; the shapes are those of
/// [`hex_to_rgba_u32`].
#[inline]
pub const fn hex_to_rgba(s: &str) -> Option<[f32; 4]> {
    let Some(color) = hex_to_rgba_u32(s) else {
        return None;
    };
    Some(u32_to_rgba(color))
}

/// Parse an OSC color parameter into a packed `0xRRGGBBAA` `u32`.
///
/// Accepts the X11 `rgb:R/G/B` form, whose channels of 1..4 hex digits are
/// scaled to 8 bits, and the CSS hex forms of [`hex_to_rgba_u32`].
pub const fn osc_to_rgba_u32(s: &[u8]) -> Option<u32> {
    // `rgb:RR/GG/BB` and `rgb:RRRR/GGGG/BBBB`: exactly three `/`-separated
    // channels, each scaled by [`scale_channel`].
    if let [b'r', b'g', b'b', b':', rest @ ..] = s {
        let Some((a, rest)) = split_channel(rest) else {
            return None;
        };
        let Some((b, c)) = split_channel(rest) else {
            return None;
        };
        // A fourth channel is not an X11 colour.
        let mut i = 0;
        while i < c.len() {
            if c[i] == b'/' {
                return None;
            }
            i += 1;
        }
        let (Some(a), Some(b), Some(c)) = (scale_channel(a), scale_channel(b), scale_channel(c))
        else {
            return None;
        };
        return Some(rgb_to_u32(a as i32, b as i32, c as i32));
    }
    // A hex colour is the same text CSS accepts.
    let Ok(hex) = core::str::from_utf8(s) else {
        return None;
    };
    hex_to_rgba_u32(hex)
}

/// Format a bare xterm `rgb:RRRR/GGGG/BBBB` spec from a packed `0xRRGGBBAA`
/// (8-bit channels repeated to 4 hex digits), without the OSC wrapper.
#[inline]
#[must_use]
pub fn rgb_spec(rgb: u32) -> CompactString {
    let [r, g, b] = u32_to_array(rgb);
    format_compact!("rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}")
}

/// Format an OSC color reply (`ESC]CODE;rgb:… ESC\`) from a `0xRRGGBBAA` value.
#[inline]
#[must_use]
pub fn format_osc_color_reply(code: u16, rgb: u32) -> CompactString {
    format_compact!("\x1b]{code};{}\x1b\\", rgb_spec(rgb))
}

/// Format an `OSC 4` palette-entry reply `ESC]4;N;rgb:RRRR/GGGG/BBBB ESC\`.
#[inline]
#[must_use]
pub fn format_osc_palette_reply(index: usize, rgb: u32) -> CompactString {
    format_compact!("\x1b]4;{index};{}\x1b\\", rgb_spec(rgb))
}

/// Format the `OSC 11` reply as a fixed 25-byte buffer.
#[inline]
#[must_use]
pub const fn format_osc_background(c: [f32; 4]) -> [u8; 25] {
    let r = round_channel(c[0].clamp(0.0, 1.0) * 255.0);
    let g = round_channel(c[1].clamp(0.0, 1.0) * 255.0);
    let b = round_channel(c[2].clamp(0.0, 1.0) * 255.0);
    let [rh, rl, gh, gl, bh, bl] =
        *const_hex::const_encode::<3, false>(&[r, g, b]).as_byte_array::<6>();
    [
        0x1b, b']', b'1', b'1', b';', b'r', b'g', b'b', b':', rh, rl, rh, rl, b'/', gh, gl, gh, gl,
        b'/', bh, bl, bh, bl, 0x1b, b'\\',
    ]
}

/// Clamp one 0..255 channel.
#[inline]
const fn clamp_channel(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

/// Round a 0..255 float channel to its byte. `f32::round` is `std`-only, so
/// the `no_std` crate rounds with `+0.5` before the truncating cast.
#[inline]
const fn round_channel(v: f32) -> u8 {
    (v + 0.5) as u8
}

/// One channel of the 6×6×6 color cube: level 0 is black, 1..=5 are
/// `55 + level * 40` (the standard xterm ramp: 0, 95, 135, 175, 215, 255).
#[inline]
const fn cube_level(v: u8) -> u8 {
    if v == 0 { 0 } else { 55 + v * 40 }
}

/// Split a byte slice at its first `/`, returning the two halves.
const fn split_channel(part: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut i = 0;
    while i < part.len() {
        if part[i] == b'/' {
            let (head, tail) = part.split_at(i);
            let (_, rest) = tail.split_at(1);
            return Some((head, rest));
        }
        i += 1;
    }
    None
}

/// Scale one `rgb:` channel of 1..=4 hex digits to 8 bits (`XParseColor`).
const fn scale_channel(part: &[u8]) -> Option<u8> {
    let len = part.len();
    if !matches!(len, 1..=4) {
        return None;
    }
    let mut v = 0_u32;
    let mut i = 0;
    while i < len {
        let digit = match part[i] {
            d @ b'0'..=b'9' => d - b'0',
            d @ (b'a'..=b'f' | b'A'..=b'F') => d.to_ascii_lowercase() - b'a' + 10,
            _ => return None,
        };
        v = v * 16 + digit as u32;
        i += 1;
    }
    Some((v * 255 / ((1 << (4 * len)) - 1)) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_clamps_channels() {
        assert_eq!(rgb_to_u32(255, 0, 0), 0xff0000ff);
        assert_eq!(rgb_to_u32(0, 300, 0), 0x00ff00ff);
        assert_eq!(rgb_to_u32(-1, 0, 0), 0x000000ff);
    }

    #[test]
    fn indexed_rgb_covers_ansi_cube_and_grayscale() {
        // The 16 ANSI colors.
        assert_eq!(indexed_rgb(0), 0x000000ff);
        assert_eq!(indexed_rgb(1), 0xcd0000ff);
        assert_eq!(indexed_rgb(15), 0xffffffff);
        // The 6×6×6 cube: 16 is black, 21 is blue, 196 is red.
        assert_eq!(indexed_rgb(16), 0x000000ff);
        assert_eq!(indexed_rgb(21), 0x0000ffff);
        assert_eq!(indexed_rgb(196), 0xff0000ff);
        // The grayscale ramp: 232 → 0x08, 255 → 0xee.
        assert_eq!(indexed_rgb(232), 0x080808ff);
        assert_eq!(indexed_rgb(255), 0xeeeeeeff);
    }

    #[test]
    fn osc_to_rgba_u32_forms() {
        assert_eq!(osc_to_rgba_u32(b"rgb:ff/00/00"), Some(0xff0000ff));
        assert_eq!(osc_to_rgba_u32(b"rgb:ffff/0000/0000"), Some(0xff0000ff));
        // Hex forms are accepted too, in every CSS shape.
        assert_eq!(osc_to_rgba_u32(b"#00ff00"), Some(0x00ff00ff));
        assert_eq!(osc_to_rgba_u32(b"#0f0"), Some(0x00ff00ff));
        assert_eq!(osc_to_rgba_u32(b"#00ff0080"), Some(0x00ff0080));
        assert_eq!(osc_to_rgba_u32(b"nonsense"), None);
    }

    #[test]
    fn osc_to_rgba_u32_is_const() {
        const RED: Option<u32> = osc_to_rgba_u32(b"rgb:ff/00/00");
        assert_eq!(RED, Some(0xff00_00ff));
        const HASH: Option<u32> = osc_to_rgba_u32(b"#00ff00");
        assert_eq!(HASH, Some(0x00ff_00ff));
        // Uppercase hex and 1..=4 digit channels.
        assert_eq!(osc_to_rgba_u32(b"rgb:FF/0/00"), Some(0xff00_00ff));
        // Wrong channel count, non-hex digits and empty channels.
        assert_eq!(osc_to_rgba_u32(b"rgb:ff/00"), None);
        assert_eq!(osc_to_rgba_u32(b"rgb:ff/00/00/00"), None);
        assert_eq!(osc_to_rgba_u32(b"rgb:ff/00/"), None);
        assert_eq!(osc_to_rgba_u32(b"rgb:/00/00"), None);
        assert_eq!(osc_to_rgba_u32(b"rgb:gg/00/00"), None);
        assert_eq!(osc_to_rgba_u32(b"rgb:fffff/00/00"), None);
        // Anything else is not an OSC colour.
        assert_eq!(osc_to_rgba_u32(b"nonsense"), None);
        assert_eq!(osc_to_rgba_u32(b""), None);
    }

    #[test]
    fn osc_to_rgba_u32_scales_16bit_channels() {
        // 0x8080 = 128·257 → scales to 128, not the high byte (also 128).
        assert_eq!(osc_to_rgba_u32(b"rgb:8080/0000/0000"), Some(0x800000ff));
        // 0x0100 ≈ 0.0039 → scales to 0 (high-byte would give 1).
        assert_eq!(osc_to_rgba_u32(b"rgb:0100/0000/0000"), Some(0x000000ff));
        // 3-digit and 1-digit channels scale too.
        assert_eq!(osc_to_rgba_u32(b"rgb:fff/000/000"), Some(0xff0000ff));
        assert_eq!(osc_to_rgba_u32(b"rgb:f/0/0"), Some(0xff0000ff));
        // Malformed channel length is rejected.
        assert_eq!(osc_to_rgba_u32(b"rgb:fffff/0000/0000"), None);
    }

    #[test]
    fn osc_background_reply_is_fixed_size_and_wrapped() {
        // [1.0, 0.988235, 0.988235, 1.0] ≈ 0xff/0xfc/0xfc.
        let reply = format_osc_background([1.0, 0.988235, 0.988235, 1.0]);
        assert_eq!(reply.len(), 25);
        assert_eq!(&reply, b"\x1b]11;rgb:ffff/fcfc/fcfc\x1b\\");
    }

    #[test]
    fn color_id_conversions_and_comparisons() {
        assert_eq!(ColorId::from(300_i32), ColorId(255));
        assert_eq!(ColorId::from(-1_i32), ColorId(0));
        assert_eq!(ColorId::from(7_i32), ColorId(7));
        assert_eq!(ColorId::from(7_u16), ColorId(7));
        assert_eq!(ColorId::from(7_usize), ColorId(7));
        assert_eq!(ColorId(7).index(), 7);
        assert_eq!(*ColorId(7), 7_u16);
        assert!(ColorId(4) == 4_i32);
        assert!(ColorId(4) == 4_usize);
        assert!(ColorId(4) == 4_u16);
    }

    #[test]
    fn rgba_round_trips_and_faint_blend() {
        assert_eq!(u32_to_array(0x1122_33ff), [0x11, 0x22, 0x33]);
        assert_eq!(u32_to_rgba(0x0000_00ff), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(u32_to_rgba(0xffffffff), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(u32_to_rgba(0x0000_0080)[3], 128.0 / 255.0);
        assert_eq!(rgba_to_u32([1.0, 0.0, 0.0, 1.0]), 0xff0000ff);
        assert_eq!(rgba_to_u32([1.0, 0.0, 0.0, 0.0]), 0xff000000);
        assert_eq!(blend_faint(0x000000ff, 0xffffffff), 0x7f7f7fff);
    }

    #[test]
    fn indexed_rgb_cube_mid_levels_and_gray() {
        assert_eq!(indexed_rgb(16 + 2 * 36 + 2 * 6 + 2), 0x878787ff);
        assert_eq!(indexed_rgb(243), 0x767676ff);
    }

    #[test]
    fn color_id_debug_matches_raw_index() {
        use alloc::string::String;
        use core::fmt::Write as _;
        let mut s = String::new();
        let _ = write!(&mut s, "{:?}", ColorId(7));
        assert_eq!(s, "7");
    }

    #[test]
    fn hex_to_rgba_array_is_the_layer_below() {
        const RED: Option<[u8; 4]> = hex_to_rgba_array("#f00");
        assert_eq!(RED, Some([0xff, 0, 0, 0xff]));
        assert_eq!(hex_to_rgba_array("ff0000"), RED);
        // Shorthand repeats its nibble; `#fff0` is white with alpha `00`.
        assert_eq!(hex_to_rgba_array("#fff0"), Some([0xff, 0xff, 0xff, 0x00]));
        assert_eq!(hex_to_rgba_array("#f008"), Some([0xff, 0, 0, 0x88]));
        assert_eq!(
            hex_to_rgba_array("#11223344"),
            Some([0x11, 0x22, 0x33, 0x44])
        );
        assert_eq!(hex_to_rgba_array("#"), None);
        assert_eq!(hex_to_rgba_array("#ff"), None);
        assert_eq!(hex_to_rgba_array("nonsense"), None);
        // The packed and float forms are the same channels.
        assert_eq!(hex_to_rgba_u32("#11223344"), Some(0x1122_3344));
        assert_eq!(hex_to_rgba("#11223344"), Some(u32_to_rgba(0x1122_3344)));
    }

    #[test]
    fn hex_to_rgba_u32_shapes() {
        const RED: Option<u32> = hex_to_rgba_u32("#f00");
        assert_eq!(RED, Some(0xff0000ff));
        // Shorthand repeats nibbles; 6/8-digit pass through; missing alpha = `ff`.
        assert_eq!(hex_to_rgba_u32("#ff0"), Some(0xffff00ff));
        assert_eq!(hex_to_rgba_u32("ff0000"), Some(0xff0000ff));
        assert_eq!(hex_to_rgba_u32("#fff0"), Some(0xffff_ff00));
        assert_eq!(hex_to_rgba_u32("#f008"), Some(0xff000088));
        assert_eq!(hex_to_rgba_u32("#11223344"), Some(0x1122_3344));
        assert_eq!(hex_to_rgba_u32("#"), None);
        assert_eq!(hex_to_rgba_u32("#ff"), None);
        assert_eq!(hex_to_rgba_u32("#gg0000"), None);
        // The float form is the same value, unpacked.
        assert_eq!(hex_to_rgba("#ff000080"), Some(u32_to_rgba(0xff00_0080)));
    }

    #[test]
    fn hex_to_rgba_is_const() {
        const RED: Option<[f32; 4]> = hex_to_rgba("#f00");
        assert_eq!(RED, Some([1.0, 0.0, 0.0, 1.0]));
        const SHORTHAND_ALPHA: Option<[f32; 4]> = hex_to_rgba("#fff0");
        assert_eq!(SHORTHAND_ALPHA, Some([1.0, 1.0, 1.0, 0.0]));
        const FULL: Option<[f32; 4]> = hex_to_rgba("00ff0080");
        assert_eq!(FULL, Some([0.0, 1.0, 0.0, 128.0 / 255.0]));
        // The six- and three-digit forms are opaque: alpha stays `ff`.
        assert_eq!(hex_to_rgba("#ff0").map(|c| c[3]), Some(1.0));
        assert_eq!(hex_to_rgba("#ff0000").map(|c| c[3]), Some(1.0));
    }

    #[test]
    fn hex_to_rgb_array_is_const_and_drops_alpha() {
        const RED: Option<[u8; 3]> = hex_to_rgb_array("#ff0000");
        assert_eq!(RED, Some([0xff, 0, 0]));
        assert_eq!(hex_to_rgb_array("00ff7f"), Some([0, 0xff, 0x7f]));
        assert_eq!(hex_to_rgb_array("#FF0000"), Some([0xff, 0, 0]));
        // Shorthand repeats each nibble.
        assert_eq!(hex_to_rgb_array("#f00"), Some([0xff, 0, 0]));
        assert_eq!(hex_to_rgb_array("0af"), Some([0, 0xaa, 0xff]));
        // Alpha-bearing forms keep only the three channels.
        assert_eq!(hex_to_rgb_array("#ff000080"), Some([0xff, 0, 0]));
        assert_eq!(hex_to_rgb_array("#f008"), Some([0xff, 0, 0]));
        assert_eq!(hex_to_rgb_array("#ff000"), None);
        assert_eq!(hex_to_rgb_array("#ff00000"), None);
        assert_eq!(hex_to_rgb_array("#gg0000"), None);
        assert_eq!(hex_to_rgb_array(""), None);
        assert_eq!(hex_to_rgb_array("#"), None);
    }

    #[test]
    fn hex_to_rgba_and_extra_rgb_channel() {
        assert_eq!(hex_to_rgba("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(hex_to_rgba("ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(hex_to_rgba("nonsense"), None);

        // Shorthand: each nibble repeats, so `#ff0` is `#ffff00`.
        assert_eq!(hex_to_rgba("#ff0"), Some([1.0, 1.0, 0.0, 1.0]));
        assert_eq!(hex_to_rgba("#f00"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(hex_to_rgba("#fff"), Some([1.0, 1.0, 1.0, 1.0]));
        // Four digits carry alpha, scaled from the nibble (`f` = 1.0, `0` = 0).
        assert_eq!(hex_to_rgba("#fff0"), Some([1.0, 1.0, 1.0, 0.0]));
        assert_eq!(hex_to_rgba("#f008"), Some([1.0, 0.0, 0.0, 8.0 / 15.0]));
        // Full alpha form.
        assert_eq!(
            hex_to_rgba("#ff000080"),
            Some([1.0, 0.0, 0.0, 128.0 / 255.0])
        );
        // Lengths outside 3/4/6/8 and non-hex digits are rejected.
        assert_eq!(hex_to_rgba("#ff"), None);
        assert_eq!(hex_to_rgba("#ff00f"), None);
        assert_eq!(hex_to_rgba("#ggg"), None);
        // Four `rgb:` channels are rejected.
        assert_eq!(osc_to_rgba_u32(b"rgb:ff/00/00/00"), None);
    }
}
