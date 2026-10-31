// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The palette index type used by reply formatting.

use core::fmt;
use core::ops::Deref;

/// A palette index: `0..=255` the 256-color table, `256`/`257` default
/// fg/bg, `258+` RGB truecolors inserted on demand.
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
        // The shared 0..=255 clamp (xterm clamps out-of-range indexed colors).
        Self(clamp_channel(v) as u16)
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

// ── RGB packing / unpacking ──────────────────────────────────────────

/// Pack three 0..255 channels into RGB8 (`0x00RRGGBB`).
#[inline]
pub const fn rgb(r: i32, g: i32, b: i32) -> u32 {
    // Little-endian bytes lay out [B, G, R, 0] — a single `from_le_bytes`
    // (a native u32 bit-cast on LE targets) instead of two shifts + two ORs.
    u32::from_le_bytes([clamp_channel(b), clamp_channel(g), clamp_channel(r), 0])
}

/// Clamp one 0..255 channel. `i32::clamp` is not const-stable (`Ord` as a
/// const trait is still nightly), so the clamp is an explicit branch.
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

/// Convert packed 0xRRGGBB to [R, G, B, 1.0].
#[inline]
pub const fn u32_to_rgba(rgb: u32) -> [f32; 4] {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        1.0,
    ]
}

/// Convert packed 0xRRGGBB to its three 8-bit channels (the integer
/// counterpart of [`u32_to_rgba`], for luma/table math).
#[inline]
pub const fn u32_to_rgb(rgb: u32) -> [u8; 3] {
    [
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    ]
}

/// Convert [R, G, B, A] (0..1 floats) to packed 0xRRGGBB.
#[inline]
pub const fn rgba_to_u32(c: [f32; 4]) -> u32 {
    let r = round_channel(c[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = round_channel(c[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = round_channel(c[2].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 16) | (g << 8) | b
}

/// Round a 0..255 float channel to its byte. `f32::round` is `std`-only, so
/// the `no_std` crate rounds with `+0.5` before the truncating cast.
#[inline]
const fn round_channel(v: f32) -> u8 {
    (v + 0.5) as u8
}

/// Blend RGB `a` 50% toward RGB `b` per channel (the SGR 2 faint/dim effect).
#[inline]
pub const fn blend_faint(a: u32, b: u32) -> u32 {
    let [ar, ag, ab] = u32_to_rgb(a);
    let [br, bg, bb] = u32_to_rgb(b);
    rgb(
        (ar as i32 + br as i32) / 2,
        (ag as i32 + bg as i32) / 2,
        (ab as i32 + bb as i32) / 2,
    )
}

// ── xterm 256-color table ────────────────────────────────────────────

/// Resolve an xterm 256-color index (`0..=255`) to packed `0xRRGGBB`: the 16
/// ANSI colors, the 6×6×6 color cube (`16..=231`), and the grayscale ramp
/// (`232..=255`).
#[inline]
pub const fn indexed_rgb(n: u8) -> u32 {
    match n {
        0 => 0x000000,
        1 => 0xcd0000,
        2 => 0x00cd00,
        3 => 0xcdcd00,
        4 => 0x0000ee,
        5 => 0xcd00cd,
        6 => 0x00cdcd,
        7 => 0xe5e5e5,
        8 => 0x7f7f7f,
        9 => 0xff0000,
        10 => 0x00ff00,
        11 => 0xffff00,
        12 => 0x5c5cff,
        13 => 0xff00ff,
        14 => 0x00ffff,
        15 => 0xffffff,
        16..=231 => {
            let v = n - 16;
            let r = v / 36;
            let g = (v % 36) / 6;
            let b = v % 6;
            rgb(
                cube_level(r) as i32,
                cube_level(g) as i32,
                cube_level(b) as i32,
            )
        }
        232..=255 => {
            let v = 8 + (n - 232) * 10;
            rgb(v as i32, v as i32, v as i32)
        }
    }
}

/// One channel of the 6×6×6 color cube: level 0 is black, 1..=5 are
/// `55 + level * 40` (the standard xterm ramp: 0, 95, 135, 175, 215, 255).
#[inline]
const fn cube_level(v: u8) -> u8 {
    if v == 0 { 0 } else { 55 + v * 40 }
}

// ── OSC color parsing ─────────────────────────────────────────────────

/// Decode an `RRGGBB` hex string (with or without a leading `#`) into three
/// 8-bit channels.
#[inline]
fn decode_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    let bytes = const_hex::decode(s.as_bytes()).ok()?;
    <[u8; 3]>::try_from(bytes.as_slice()).ok()
}

/// Parse an RRGGBB / RGB hex string into [R, G, B, 1.0].
#[inline]
pub fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let [r, g, b] = decode_rgb(s)?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

/// Parse an OSC color (`rgb:RR/GG/BB`, `rgb:RRRR/GGGG/BBBB`, or
/// `#RRGGBB`) into a 0xRRGGBB `u32`.
///
/// `rgb:` channels of 1..4 hex digits are scaled to 8 bits (`XParseColor`
/// semantics), matching Ghostty and Zellij.
pub fn parse(s: &[u8]) -> Option<u32> {
    let s = core::str::from_utf8(s).ok()?;
    if let Some(rest) = s.strip_prefix("rgb:") {
        let mut parts = rest.split('/');
        let (a, b, c) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        return Some(rgb(
            scale_channel(a)? as i32,
            scale_channel(b)? as i32,
            scale_channel(c)? as i32,
        ));
    }
    if s.starts_with('#') {
        let [r, g, b] = decode_rgb(s)?;
        return Some(rgb(r as i32, g as i32, b as i32));
    }
    None
}

/// Scale one `rgb:` channel of 1..=4 hex digits to 8 bits (`XParseColor`).
fn scale_channel(part: &str) -> Option<u8> {
    let len = part.len();
    if !(1..=4).contains(&len) {
        return None;
    }
    let v = u32::from_str_radix(part, 16).ok()?;
    Some((v * 255 / ((1 << (4 * len)) - 1)) as u8)
}

// ── OSC color replies ─────────────────────────────────────────────────

/// Format an OSC color reply `ESC]CODE;rgb:RRRR/GGGG/BBBB ESC\` from a
/// 0xRRGGBB value (8-bit channels repeated to 4 hex digits, the xterm
/// convention).
#[inline]
#[must_use]
pub fn format_osc_color_reply(code: u16, rgb: u32) -> compact_str::CompactString {
    let [r, g, b] = u32_to_rgb(rgb);
    // Direct `{:02x}` hex (each channel doubled to 4 digits) — no intermediate
    // `const_hex::encode` heap strings; `format_compact!` writes inline.
    compact_str::format_compact!(
        "\x1b]{code};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}\x1b\\"
    )
}

/// Format an `OSC 4` palette-entry reply `ESC]4;N;rgb:RRRR/GGGG/BBBB ESC\`.
#[inline]
#[must_use]
pub fn format_osc_palette_reply(index: usize, rgb: u32) -> compact_str::CompactString {
    let [r, g, b] = u32_to_rgb(rgb);
    compact_str::format_compact!(
        "\x1b]4;{index};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}\x1b\\"
    )
}

/// Format the full `OSC 11` background reply as a fixed 25-byte buffer:
/// `\x1b]11;rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}\x1b\\`
/// (8-bit channels repeated to 4 hex digits, the xterm convention
/// lazygit/others expect).
#[inline]
#[must_use]
pub const fn format_osc_background(c: [f32; 4]) -> [u8; 25] {
    let r = round_channel(c[0].clamp(0.0, 1.0) * 255.0);
    let g = round_channel(c[1].clamp(0.0, 1.0) * 255.0);
    let b = round_channel(c[2].clamp(0.0, 1.0) * 255.0);
    let [rh, rl] = *const_hex::const_encode::<1, false>(&[r]).as_byte_array::<2>();
    let [gh, gl] = *const_hex::const_encode::<1, false>(&[g]).as_byte_array::<2>();
    let [bh, bl] = *const_hex::const_encode::<1, false>(&[b]).as_byte_array::<2>();
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
        assert_eq!(rgb(255, 0, 0), 0xff0000);
        assert_eq!(rgb(0, 300, 0), 0x00ff00);
        assert_eq!(rgb(-1, 0, 0), 0x000000);
    }

    #[test]
    fn indexed_rgb_covers_ansi_cube_and_grayscale() {
        // The 16 ANSI colors.
        assert_eq!(indexed_rgb(0), 0x000000);
        assert_eq!(indexed_rgb(1), 0xcd0000);
        assert_eq!(indexed_rgb(15), 0xffffff);
        // The 6×6×6 cube: 16 is black, 21 is blue, 196 is red.
        assert_eq!(indexed_rgb(16), 0x000000);
        assert_eq!(indexed_rgb(21), 0x0000ff);
        assert_eq!(indexed_rgb(196), 0xff0000);
        // The grayscale ramp: 232 → 0x08, 255 → 0xee.
        assert_eq!(indexed_rgb(232), 0x080808);
        assert_eq!(indexed_rgb(255), 0xeeeeee);
    }

    #[test]
    fn parse_color_forms() {
        assert_eq!(parse(b"rgb:ff/00/00"), Some(0xff0000));
        assert_eq!(parse(b"rgb:ffff/0000/0000"), Some(0xff0000));
        assert_eq!(parse(b"#00ff00"), Some(0x00ff00));
        assert_eq!(parse(b"nonsense"), None);
    }

    #[test]
    fn parse_scales_16bit_channels() {
        // 0x8080 = 128·257 → scales to 128, not the high byte (also 128).
        assert_eq!(parse(b"rgb:8080/0000/0000"), Some(0x800000));
        // 0x0100 ≈ 0.0039 → scales to 0 (high-byte would give 1).
        assert_eq!(parse(b"rgb:0100/0000/0000"), Some(0x000000));
        // 3-digit and 1-digit channels scale too.
        assert_eq!(parse(b"rgb:fff/000/000"), Some(0xff0000));
        assert_eq!(parse(b"rgb:f/0/0"), Some(0xff0000));
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
        assert_eq!(u32_to_rgb(0x112233), [0x11, 0x22, 0x33]);
        assert_eq!(u32_to_rgba(0x000000), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(u32_to_rgba(0xffffff), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(rgba_to_u32([1.0, 0.0, 0.0, 1.0]), 0xff0000);
        assert_eq!(blend_faint(0x000000, 0xffffff), 0x7f7f7f);
    }

    #[test]
    fn indexed_rgb_cube_mid_levels_and_gray() {
        assert_eq!(indexed_rgb(16 + 2 * 36 + 2 * 6 + 2), 0x878787);
        assert_eq!(indexed_rgb(243), 0x767676);
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
        // Four `rgb:` channels are rejected.
        assert_eq!(parse(b"rgb:ff/00/00/00"), None);
    }
}
