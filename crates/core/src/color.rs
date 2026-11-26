// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The palette index type used by reply formatting.

use core::fmt;

use compact_str::{CompactString, format_compact};

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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
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
    // The array reads in channel order; `from_be_bytes` puts `R` in the top
    // byte, so the packed value is 0xRRGGBBAA after a single `bswap` (a native
    // bit-cast on big-endian targets) instead of three shifts + two ORs.
    u32::from_be_bytes([clamp_channel(r), clamp_channel(g), clamp_channel(b), 0xff])
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

/// Pack `[R, G, B, A]` (0..1 floats) into `0xRRGGBBAA`.
#[inline]
pub const fn rgba_to_u32(c: [f32; 4]) -> u32 {
    let r = round_channel(c[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = round_channel(c[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = round_channel(c[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = round_channel(c[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// Round a 0..255 float channel to its byte. `f32::round` is `std`-only, so
/// the `no_std` crate rounds with `+0.5` before the truncating cast.
#[inline]
const fn round_channel(v: f32) -> u8 {
    (v + 0.5) as u8
}

/// Unpack `0xRRGGBBAA` into `[R, G, B]`, dropping alpha (the integer
/// counterpart of [`u32_to_rgba`], for luma and table math).
#[inline]
pub const fn u32_to_rgb(c: u32) -> [u8; 3] {
    [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8]
}

/// Unpack `0xRRGGBBAA` into `[R, G, B, A]`.
#[inline]
pub const fn u32_to_rgba(c: u32) -> [f32; 4] {
    [
        ((c >> 24) & 0xff) as f32 / 255.0,
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
    ]
}

/// Blend RGB `a` 50% toward RGB `b` per channel (the SGR 2 faint/dim effect).
#[inline]
pub const fn blend_faint(a: u32, b: u32) -> u32 {
    let [ar, ag, ab] = u32_to_rgb(a);
    let [br, bg, bb] = u32_to_rgb(b);
    rgb_to_u32(
        (ar as i32 + br as i32) / 2,
        (ag as i32 + bg as i32) / 2,
        (ab as i32 + bb as i32) / 2,
    )
}

/// Resolve an xterm 256-color index (`0..=255`) to packed `0xRRGGBBAA`: the 16
/// ANSI colors, the 6×6×6 color cube (`16..=231`), and the grayscale ramp
/// (`232..=255`).
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

/// One channel of the 6×6×6 color cube: level 0 is black, 1..=5 are
/// `55 + level * 40` (the standard xterm ramp: 0, 95, 135, 175, 215, 255).
#[inline]
const fn cube_level(v: u8) -> u8 {
    if v == 0 { 0 } else { 55 + v * 40 }
}

/// Decode an `RRGGBB` hex string (with or without a leading `#`) into three
/// 8-bit channels.
#[inline]
fn decode_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    let bytes = const_hex::decode(s.as_bytes()).ok()?;
    <[u8; 3]>::try_from(bytes.as_slice()).ok()
}

/// Parse a CSS hex color into `[R, G, B, A]`, with or without the leading `#`.
///
/// Accepts `RGB`, `RGBA`, `RRGGBB` and `RRGGBBAA`; the shorthand forms repeat
/// each nibble (`#ff0` is `#ffff00`), and a missing alpha channel is opaque.
#[inline]
pub fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let hex = s.strip_prefix('#').unwrap_or(s).as_bytes();
    let mut color = [b'f'; 8];
    match hex.len() {
        3 | 4 => {
            for (i, &nibble) in hex.iter().enumerate() {
                color[i * 2] = nibble;
                color[i * 2 + 1] = nibble;
            }
        }
        6 | 8 => {
            color[..hex.len()].copy_from_slice(hex);
        }
        _ => return None,
    };
    let [r, g, b, a] = const_hex::const_decode_to_array(&color).ok()?;
    Some([
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ])
}

/// Parse an OSC color (`rgb:RR/GG/BB`, `rgb:RRRR/GGGG/BBBB`, or
/// `#RRGGBB`) into a packed `0xRRGGBBAA` `u32`.
///
/// `rgb:` channels of 1..4 hex digits are scaled to 8 bits (`XParseColor`
/// semantics).
pub fn parse(s: &[u8]) -> Option<u32> {
    let s = core::str::from_utf8(s).ok()?;
    if let Some(rest) = s.strip_prefix("rgb:") {
        let mut parts = rest.split('/');
        let (a, b, c) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        return Some(rgb_to_u32(
            scale_channel(a)? as i32,
            scale_channel(b)? as i32,
            scale_channel(c)? as i32,
        ));
    }
    let [r, g, b] = decode_rgb(s)?;
    Some(rgb_to_u32(r as i32, g as i32, b as i32))
}

/// Scale one `rgb:` channel of 1..=4 hex digits to 8 bits (`XParseColor`).
const fn scale_channel(part: &str) -> Option<u8> {
    let len = part.len();
    if !matches!(len, 1..=4) {
        return None;
    }
    match u32::from_str_radix(part, 16) {
        Ok(v) => Some((v * 255 / ((1 << (4 * len)) - 1)) as u8),
        Err(_) => None,
    }
}

/// Format a bare xterm `rgb:RRRR/GGGG/BBBB` spec from a packed `0xRRGGBBAA`
/// (8-bit channels repeated to 4 hex digits), without the OSC wrapper.
#[inline]
#[must_use]
pub fn rgb_spec(rgb: u32) -> CompactString {
    let [r, g, b] = u32_to_rgb(rgb);
    format_compact!("rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}")
}

/// Format an OSC color reply `ESC]CODE;rgb:RRRR/GGGG/BBBB ESC\` from a
/// `0xRRGGBBAA` value (8-bit channels repeated to 4 hex digits, the xterm
/// convention).
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

/// Format the full `OSC 11` background reply as a fixed 25-byte buffer:
/// `\x1b]11;rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}\x1b\\`
/// (8-bit channels repeated to 4 hex digits, the xterm convention).
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
    fn parse_color_forms() {
        assert_eq!(parse(b"rgb:ff/00/00"), Some(0xff0000ff));
        assert_eq!(parse(b"rgb:ffff/0000/0000"), Some(0xff0000ff));
        assert_eq!(parse(b"#00ff00"), Some(0x00ff00ff));
        assert_eq!(parse(b"nonsense"), None);
    }

    #[test]
    fn parse_scales_16bit_channels() {
        // 0x8080 = 128·257 → scales to 128, not the high byte (also 128).
        assert_eq!(parse(b"rgb:8080/0000/0000"), Some(0x800000ff));
        // 0x0100 ≈ 0.0039 → scales to 0 (high-byte would give 1).
        assert_eq!(parse(b"rgb:0100/0000/0000"), Some(0x000000ff));
        // 3-digit and 1-digit channels scale too.
        assert_eq!(parse(b"rgb:fff/000/000"), Some(0xff0000ff));
        assert_eq!(parse(b"rgb:f/0/0"), Some(0xff0000ff));
        // Malformed channel length is rejected.
        assert_eq!(parse(b"rgb:fffff/0000/0000"), None);
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
        assert_eq!(u32_to_rgb(0x1122_33ff), [0x11, 0x22, 0x33]);
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
    fn parse_hex_color_and_extra_rgb_channel() {
        assert_eq!(parse_hex_color("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex_color("ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex_color("nonsense"), None);

        // Shorthand: each nibble repeats, so `#ff0` is `#ffff00`.
        assert_eq!(parse_hex_color("#ff0"), Some([1.0, 1.0, 0.0, 1.0]));
        assert_eq!(parse_hex_color("#f00"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex_color("#fff"), Some([1.0, 1.0, 1.0, 1.0]));
        // Four digits carry alpha, scaled from the nibble (`f` = 1.0, `0` = 0).
        assert_eq!(parse_hex_color("#fff0"), Some([1.0, 1.0, 1.0, 0.0]));
        assert_eq!(parse_hex_color("#f008"), Some([1.0, 0.0, 0.0, 8.0 / 15.0]));
        // Full alpha form.
        assert_eq!(
            parse_hex_color("#ff000080"),
            Some([1.0, 0.0, 0.0, 128.0 / 255.0])
        );
        // Lengths outside 3/4/6/8 and non-hex digits are rejected.
        assert_eq!(parse_hex_color("#ff"), None);
        assert_eq!(parse_hex_color("#ff00f"), None);
        assert_eq!(parse_hex_color("#ggg"), None);
        // Four `rgb:` channels are rejected.
        assert_eq!(parse(b"rgb:ff/00/00/00"), None);
    }
}
