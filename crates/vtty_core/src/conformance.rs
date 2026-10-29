// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! DEC conformance and identity: DECSCL, Device Attributes (DA1/DA2/DA3), and
//! the `$`-family DCS reports (DECTSR / DECCTR / DECCIR / DECTABSR).
//!
//! Reference: EK-VT520-RM §5.

#[cfg(feature = "reply")]
use core::fmt::Write as _;

#[cfg(feature = "reply")]
/// DECTSR answer prefix — `DCS 1 $ s` (Terminal State Report). The final is
/// a lowercase `s` (the EK-VT510-RM errata on vt100.net corrects the printed
/// uppercase `S`).
const DECTSR_PREFIX: &str = "\x1bP1$s";
#[cfg(feature = "reply")]
/// DECCTR answer prefix — `DCS 2 $ s` (Color Table Report).
const DECCTR_PREFIX: &str = "\x1bP2$s";
#[cfg(feature = "reply")]
/// DECCIR answer prefix — `DCS 1 $ u` (Cursor Information Report).
const DECCIR_PREFIX: &str = "\x1bP1$u";
#[cfg(feature = "reply")]
/// DECTABSR answer prefix — `DCS 2 $ u` (Tab Stop Report).
const DECTABSR_PREFIX: &str = "\x1bP2$u";

/// DECSCL conformance level. `Ps1` selects the VT operating level (the
/// factory default for the VT500 family is [`ConformanceLevel::Vt500`] with
/// 8-bit controls); `Ps2` (`0`/`1`/`2`) selects 7-bit vs 8-bit C1 controls
/// and is handled separately (S7C1T / S8C1T).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConformanceLevel {
    /// VT100 mode (`CSI 61 " p`).
    Vt100 = 61,
    /// VT200 mode (`CSI 62 " p`).
    Vt200 = 62,
    /// VT300 mode (`CSI 63 " p`).
    Vt300 = 63,
    /// VT400 mode (`CSI 64 " p`).
    Vt400 = 64,
    /// VT500 mode (`CSI 65 " p`).
    #[default]
    Vt500 = 65,
}

impl ConformanceLevel {
    /// The DECSCL `Ps1` parameter value (`61..=65`).
    #[inline]
    pub const fn param(self) -> i32 {
        self as i32
    }

    /// Parse a DECSCL `Ps1` parameter. Returns `None` for `0` (DECSCL no-op)
    /// or any value outside `61..=65`.
    #[inline]
    pub const fn from_param(p: i32) -> Option<Self> {
        match p {
            61 => Some(Self::Vt100),
            62 => Some(Self::Vt200),
            63 => Some(Self::Vt300),
            64 => Some(Self::Vt400),
            65 => Some(Self::Vt500),
            _ => None,
        }
    }
}

#[cfg(feature = "reply")]
impl crate::reply::Reply {
    /// Append `{prefix}{payload} ST` (a DCS/OSC string) to `self`.
    fn dcs_string(&mut self, prefix: &str, payload: &str) {
        let buf = self.buffer_mut();
        buf.push_str(prefix);
        buf.push_str(payload);
        buf.push_str(crate::reply::ST);
    }

    /// Build a DA1 (Primary Device Attributes) answer:
    /// `CSI ? {a0};{a1};… c`. The first attribute is the service class
    /// (`65` = VT500, `64` = VT400, `62` = VT220, `1` = VT100); the rest are
    /// supported-extension codes (EK-VT520-RM §5-9).
    pub fn da1(attrs: &[u32]) -> Self {
        let mut this = Self::default();
        let buf = this.buffer_mut();
        buf.push_str("\x1b[?");
        join_display(buf, attrs, ';');
        buf.push('c');
        this
    }

    /// Build a DA2 (Secondary Device Attributes) answer:
    /// `CSI > {id};{firmware};{keyboard} c` (EK-VT520-RM §5-11). `id` is the
    /// terminal identification code (`64` = VT520, `65` = color VT525),
    /// `firmware` the version (`10` = V1.0, `20` = V2.0), and `keyboard`
    /// `0` (STD) or `1` (PC).
    pub fn da2(id: u32, firmware: u32, keyboard: u32) -> Self {
        let mut this = Self::default();
        let _ = write!(this.buffer_mut(), "\x1b[>{id};{firmware};{keyboard}c");
        this
    }

    /// Build a DA3 (Tertiary Device Attributes) answer:
    /// `DCS ! | {hex} ST` — the terminal unit ID as hex byte pairs
    /// (EK-VT520-RM §5-12).
    pub fn da3(unit_id: &[u8]) -> Self {
        let mut this = Self::default();
        this.dcs_string("\x1bP!|", &const_hex::encode(unit_id));
        this
    }

    /// Build a DECTSR (Terminal State Report) answer:
    /// `DCS 1 $ s {device_attrs} ST`. The payload is the terminal's
    /// device-attribute string (the DA1/DA2/DA3 response bodies), per
    /// EK-VT520-RM §5-160.
    #[inline]
    pub fn dectsr(device_attrs: &str) -> Self {
        let mut this = Self::default();
        this.dcs_string(DECTSR_PREFIX, device_attrs);
        this
    }

    /// Build an XTVERSION answer: `DCS > | {name} {version} ST` (the
    /// `CSI > q` version query, xterm-style).
    pub fn xtversion(name: &str, version: &str) -> Self {
        let mut this = Self::default();
        let buf = this.buffer_mut();
        buf.push_str("\x1bP>|");
        buf.push_str(name);
        buf.push(' ');
        buf.push_str(version);
        buf.push_str(crate::reply::ST);
        this
    }

    /// Build a DECCTR (Color Table Report) answer:
    /// `DCS 2 $ s {Pc;2;r;g;b / …} ST`. Each entry is `(color_number, red,
    /// green, blue)` with 8-bit RGB (0-255), scaled to the DEC 0-100
    /// coordinate range (rounding to nearest).
    pub fn decctr(entries: &[(u8, u8, u8, u8)]) -> Self {
        let mut this = Self::default();
        let buf = this.buffer_mut();
        buf.push_str(DECCTR_PREFIX);
        for (i, &(idx, r, g, b)) in entries.iter().enumerate() {
            if i > 0 {
                buf.push('/');
            }
            let _ = write!(
                buf,
                "{idx};2;{};{};{}",
                dec_percent(r),
                dec_percent(g),
                dec_percent(b),
            );
        }
        buf.push_str(crate::reply::ST);
        this
    }

    /// Build a DECCIR (Cursor Information Report) answer:
    /// `DCS 1 $ u {payload} ST`. The payload is the cursor state (position,
    /// visual attributes, origin mode, and active G0–G3 charsets), which a
    /// screen builds; see EK-VT520-RM §5-30.
    #[inline]
    pub fn deccir(payload: &str) -> Self {
        let mut this = Self::default();
        this.dcs_string(DECCIR_PREFIX, payload);
        this
    }

    /// Build a DECTABSR (Tab Stop Report) answer:
    /// `DCS 2 $ u {col/col/…} ST`, one decimal column number per tab stop.
    pub fn dectabsr(cols: &[u32]) -> Self {
        let mut this = Self::default();
        let buf = this.buffer_mut();
        buf.push_str(DECTABSR_PREFIX);
        join_display(buf, cols, '/');
        buf.push_str(crate::reply::ST);
        this
    }
}

#[cfg(feature = "reply")]
/// Append `items` to `buf`, separated by `sep`.
fn join_display<T: core::fmt::Display>(
    buf: &mut compact_str::CompactString,
    items: &[T],
    sep: char,
) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            buf.push(sep);
        }
        let _ = write!(buf, "{item}");
    }
}

#[cfg(feature = "reply")]
/// Scale an 8-bit RGB channel (0-255) to the DEC 0-100 coordinate range used
/// by DECCTR / DECAC / DECATC, rounding to nearest.
#[inline]
const fn dec_percent(v: u8) -> u32 {
    (v as u32 * 100 + 127) / 255
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_roundtrip() {
        assert_eq!(
            ConformanceLevel::from_param(61),
            Some(ConformanceLevel::Vt100)
        );
        assert_eq!(
            ConformanceLevel::from_param(62),
            Some(ConformanceLevel::Vt200)
        );
        assert_eq!(
            ConformanceLevel::from_param(63),
            Some(ConformanceLevel::Vt300)
        );
        assert_eq!(
            ConformanceLevel::from_param(64),
            Some(ConformanceLevel::Vt400)
        );
        assert_eq!(
            ConformanceLevel::from_param(65),
            Some(ConformanceLevel::Vt500)
        );
        assert_eq!(ConformanceLevel::from_param(0), None);
        assert_eq!(ConformanceLevel::from_param(60), None);
        assert_eq!(ConformanceLevel::from_param(66), None);
        assert_eq!(ConformanceLevel::Vt100.param(), 61);
        assert_eq!(ConformanceLevel::Vt200.param(), 62);
        assert_eq!(ConformanceLevel::Vt300.param(), 63);
        assert_eq!(ConformanceLevel::Vt400.param(), 64);
        assert_eq!(ConformanceLevel::Vt500.param(), 65);
        // Factory default.
        assert_eq!(ConformanceLevel::default(), ConformanceLevel::Vt500);
    }

    #[cfg(feature = "reply")]
    #[test]
    fn da_reply_formats() {
        use crate::reply::Reply;

        // DA1: service class + extensions (EK-VT520-RM §5-9 example shape).
        assert_eq!(
            Reply::da1(&[65, 1, 2, 7, 9, 12, 18, 19, 21, 23])
                .finish(),
            "\x1b[?65;1;2;7;9;12;18;19;21;23c"
        );
        // DA2: id;firmware;keyboard (EK-VT520-RM §5-11 example).
        assert_eq!(Reply::da2(64, 20, 1).finish(), "\x1b[>64;20;1c");
        // DA3: unit ID as hex byte pairs (EK-VT520-RM §5-12 example).
        assert_eq!(
            Reply::da3(&[0x00, 0x01, 0x02, 0x05]).finish(),
            "\x1bP!|00010205\x1b\\"
        );
    }

    #[cfg(feature = "reply")]
    #[test]
    fn xtversion_reply_format() {
        use crate::reply::Reply;

        assert_eq!(
            Reply::xtversion("jetfish", "0.1").finish(),
            "\x1bP>|jetfish 0.1\x1b\\"
        );
    }

    #[cfg(feature = "reply")]
    #[test]
    fn dectsr_reply_format() {
        use crate::reply::Reply;

        // Lowercase `s` final (EK-VT510-RM errata).
        assert_eq!(
            Reply::dectsr("?64;1;2;7;8;9;15;18;21c").finish(),
            "\x1bP1$s?64;1;2;7;8;9;15;18;21c\x1b\\"
        );
        assert_eq!(Reply::dectsr("").finish(), "\x1bP1$s\x1b\\");
    }

    #[cfg(feature = "reply")]
    #[test]
    fn decctr_reply_format() {
        use crate::reply::Reply;

        // 8-bit RGB scales to 0-100: 255→100, 0→0, 128→50.
        assert_eq!(
            Reply::decctr(&[(0, 255, 0, 0), (7, 0, 128, 255)])
                .finish(),
            "\x1bP2$s0;2;100;0;0/7;2;0;50;100\x1b\\"
        );
        assert_eq!(Reply::decctr(&[]).finish(), "\x1bP2$s\x1b\\");
    }

    #[cfg(feature = "reply")]
    #[test]
    fn deccir_reply_format() {
        use crate::reply::Reply;

        assert_eq!(
            Reply::deccir("1;3;7;0;0;0;0").finish(),
            "\x1bP1$u1;3;7;0;0;0;0\x1b\\"
        );
    }

    #[cfg(feature = "reply")]
    #[test]
    fn dectabsr_reply_format() {
        use crate::reply::Reply;

        // The EK-VT520-RM §5-157 example: stops at columns 9, 17, 25, …
        assert_eq!(
            Reply::dectabsr(&[9, 17, 25, 33, 41, 49, 57, 65, 73])
                .finish(),
            "\x1bP2$u9/17/25/33/41/49/57/65/73\x1b\\"
        );
        assert_eq!(Reply::dectabsr(&[]).finish(), "\x1bP2$u\x1b\\");
    }
}
