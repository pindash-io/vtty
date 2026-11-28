// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query/response reply formatting (DECRQSS / DECRQM / XTGETTCAP and the
//! `$`-family DCS reports). See the [DECronomicon CSI/DCS tables] and
//! EK-VT520-RM §5.
//!
//! [DECronomicon CSI/DCS tables]: https://vt100.net/emu/ctrlseq_dec.html

use alloc::string::ToString;
use core::fmt::Write as _;

use compact_str::CompactString;
use compact_str::format_compact;

use crate::u32_to_array;
use crate::{ColorId, SgrFlags};
use crate::{DEFAULT_BG_ID, DEFAULT_FG_ID};

/// DCS/OSC string terminator (`ESC \`).
pub(crate) const ST: &str = "\x1b\\";

/// XTGETTCAP answer prefix (`DCS 1 + r`), valid.
const XTGETTCAP_OK: &str = "\x1bP1+r";
/// XTGETTCAP answer prefix (`DCS 0 + r`), unsupported.
const XTGETTCAP_ERR: &str = "\x1bP0+r";

/// DECRQSS answer prefix (`DCS 1 $ r`), valid.
const DECRQSS_OK: &str = "\x1bP1$r";
/// DECRQSS answer prefix (`DCS 0 $ r`), invalid.
const DECRQSS_ERR: &str = "\x1bP0$r";

/// Namespace for VT query reply builders.
///
/// Each associated function returns a finished reply as a
/// [`CompactString`], ready to write back to the PTY. The screen accumulates
/// a burst of replies by concatenating them in dispatch order. The report
/// formats follow the DEC manuals (DECRQSS/DECRQM/XTGETTCAP).
///
/// # Example
///
/// ```
/// use vtty_core::Reply;
///
/// // DECRQSS answer for "what is the SGR state?":
/// assert_eq!(Reply::decrqss(true, "0").as_str(), "\x1bP1$r0\x1b\\");
///
/// // An OSC 11 background-color report (red):
/// assert_eq!(
///     Reply::osc_color(11, 0xff0000ff).as_str(),
///     "\x1b]11;rgb:ffff/0000/0000\x1b\\",
/// );
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Reply;

impl Reply {
    /// Build a DECRQSS answer: `DCS {valid} $ r {value} ST`.
    #[must_use]
    pub fn decrqss(valid: bool, value: &str) -> CompactString {
        format_compact!(
            "{}{value}\x1b\\",
            if valid { DECRQSS_OK } else { DECRQSS_ERR }
        )
    }

    /// Build an XTGETTCAP answer: `DCS 1 + r {hex key}={hex value} ST`.
    #[inline]
    #[must_use]
    pub fn xtgettcap_reply(key: &[u8], value: &[u8]) -> CompactString {
        xtgettcap_build(true, key, Some(value))
    }

    /// Build an XTGETTCAP boolean answer: `DCS 1 + r {hex key} ST` (no
    /// `=value`, per xterm, where a bare name means "supported").
    #[inline]
    #[must_use]
    pub fn xtgettcap_reply_bool(key: &[u8]) -> CompactString {
        xtgettcap_build(true, key, None)
    }

    /// Build an XTGETTCAP failure: `DCS 0 + r {hex msg} ST`.
    #[inline]
    #[must_use]
    pub fn xtgettcap_error(msg: &[u8]) -> CompactString {
        xtgettcap_build(false, msg, None)
    }

    /// Build an XTGETTCAP answer with a decimal integer value.
    #[inline]
    #[must_use]
    pub fn xtgettcap_num(key: &[u8], n: usize) -> CompactString {
        xtgettcap_num_build(key, n)
    }

    /// Build an SGR reply (`m…`) from a pen's palette ids + style flags.
    #[must_use]
    pub fn sgr(fg: ColorId, bg: ColorId, flags: SgrFlags, palette: &[u32]) -> CompactString {
        let mut buf = CompactString::default();
        buf.push_str("0");
        for (_, code) in [
            (SgrFlags::BOLD, ";1"),
            (SgrFlags::ITALIC, ";3"),
            (SgrFlags::UNDERLINE, ";4"),
            (SgrFlags::INVERSE, ";7"),
        ]
        .into_iter()
        .filter(|(flag, _)| flags.contains(*flag))
        {
            buf.push_str(code);
        }
        push_sgr_color(&mut buf, 3, fg, DEFAULT_FG_ID, palette);
        push_sgr_color(&mut buf, 4, bg, DEFAULT_BG_ID, palette);
        buf
    }

    /// Build an OSC color reply (`ESC]CODE;rgb:… ESC\`) from `0xRRGGBBAA`.
    #[inline]
    #[must_use]
    pub fn osc_color(code: u16, rgb: u32) -> CompactString {
        crate::color::format_osc_color_reply(code, rgb)
    }

    /// Build an `OSC 4` palette-entry reply
    /// `ESC]4;N;rgb:RRRR/GGGG/BBBB ESC\`.
    #[inline]
    #[must_use]
    pub fn osc_palette(index: usize, rgb: u32) -> CompactString {
        crate::color::format_osc_palette_reply(index, rgb)
    }

    /// Build the fixed `OSC 11` background reply `ESC]11;rgb:… ESC\`.
    #[inline]
    #[must_use]
    pub fn osc_background(c: [f32; 4]) -> CompactString {
        // Pure ASCII, so validation cannot fail: stay strict rather than splice
        // U+FFFD into a broken reply.
        CompactString::from_utf8(crate::color::format_osc_background(c)).unwrap_or_default()
    }

    /// Build a DECRQM answer; `state` is ECMA-48's 0/1/2 and `private` adds
    /// the `?`.
    #[must_use]
    pub fn decrqm(param: i32, private: bool, state: u8) -> CompactString {
        format_compact!("\x1b[{}{param};{state}$y", if private { "?" } else { "" })
    }
}

/// Terminate a DCS/OSC string with `ST`.
#[inline]
fn terminate(buf: &mut CompactString) {
    buf.push_str(ST);
}

/// Append an XTGETTCAP answer; `value` is omitted for booleans and failures.
pub(crate) fn xtgettcap(buf: &mut CompactString, ok: bool, key: &[u8], value: Option<&[u8]>) {
    buf.push_str(if ok { XTGETTCAP_OK } else { XTGETTCAP_ERR });
    buf.push_str(&const_hex::encode(key));
    if let Some(value) = value {
        buf.push('=');
        buf.push_str(&const_hex::encode(value));
    }
    terminate(buf);
}

/// Append an XTGETTCAP decimal-integer answer, hex-encoding the digits of
/// the decimal form (same bytes as [`xtgettcap`] with a numeric value).
pub(crate) fn xtgettcap_num_append(buf: &mut CompactString, key: &[u8], n: usize) {
    xtgettcap(buf, true, key, Some(n.to_string().as_bytes()));
}

/// Build an XTGETTCAP answer in a fresh buffer.
#[inline]
fn xtgettcap_build(ok: bool, key: &[u8], value: Option<&[u8]>) -> CompactString {
    let mut buf = CompactString::default();
    xtgettcap(&mut buf, ok, key, value);
    buf
}

/// Build an XTGETTCAP decimal-integer answer in a fresh buffer.
#[inline]
fn xtgettcap_num_build(key: &[u8], n: usize) -> CompactString {
    let mut buf = CompactString::default();
    xtgettcap_num_append(&mut buf, key, n);
    buf
}

/// Append an SGR color (`;{base}9`, `;{base}8;5;n` or `;{base}8;2;r;g;b`).
fn push_sgr_color(
    s: &mut CompactString,
    base: u8,
    id: ColorId,
    default_id: ColorId,
    palette: &[u32],
) {
    if id == default_id {
        let _ = write!(s, ";{base}9");
        return;
    }
    if id.0 <= 255 {
        let _ = write!(s, ";{base}8;5;{id}");
        return;
    }
    let Some(rgb) = palette.get(id.0 as usize) else {
        let _ = write!(s, ";{base}9");
        return;
    };
    let [r, g, b] = u32_to_array(*rgb);
    let _ = write!(s, ";{base}8;2;{r};{g};{b}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrqm_reply_format() {
        assert_eq!(Reply::decrqm(25, true, 1), "\x1b[?25;1$y");
        assert_eq!(Reply::decrqm(25, true, 2), "\x1b[?25;2$y");
        assert_eq!(Reply::decrqm(25, true, 0), "\x1b[?25;0$y");
        assert_eq!(Reply::decrqm(4, false, 2), "\x1b[4;2$y");
    }

    #[test]
    fn decrqss_reply_format() {
        assert_eq!(Reply::decrqss(true, "m"), "\x1bP1$rm\x1b\\");
        assert_eq!(Reply::decrqss(false, "x"), "\x1bP0$rx\x1b\\");
    }

    #[test]
    fn sgr_reply_format() {
        let pal = [0_u32; 256];
        assert_eq!(
            Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::DEFAULT, &pal),
            "0;39;49"
        );
        assert_eq!(
            Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::BOLD, &pal),
            "0;1;39;49"
        );
        // 5 = indexed, 258 = an RGB id (0x112233ff) in the palette.
        let mut pal = [0_u32; 259];
        pal[258] = 0x112233ff;
        assert_eq!(
            Reply::sgr(ColorId(5), ColorId(258), SgrFlags::DEFAULT, &pal),
            "0;38;5;5;48;2;17;34;51"
        );
    }

    #[test]
    fn sgr_rgb_id_out_of_palette_falls_back_to_default() {
        let pal = [0_u32; 256];
        assert_eq!(
            Reply::sgr(ColorId(258), DEFAULT_BG_ID, SgrFlags::DEFAULT, &pal),
            "0;39;49"
        );
    }

    #[test]
    fn xtgettcap_reply_formats() {
        assert_eq!(
            Reply::xtgettcap_reply(b"RGB", b"8/8/8"),
            "\x1bP1+r524742=382f382f38\x1b\\"
        );
        assert_eq!(Reply::xtgettcap_reply_bool(b"am"), "\x1bP1+r616d\x1b\\");
        assert_eq!(Reply::xtgettcap_error(b"nope"), "\x1bP0+r6e6f7065\x1b\\");
        assert_eq!(Reply::xtgettcap_num(b"co", 80), "\x1bP1+r636f=3830\x1b\\");
    }

    #[test]
    fn osc_reply_formats() {
        assert_eq!(
            Reply::osc_color(10, 0x112233ff),
            "\x1b]10;rgb:1111/2222/3333\x1b\\"
        );
        assert_eq!(
            Reply::osc_palette(4, 0x112233ff),
            "\x1b]4;4;rgb:1111/2222/3333\x1b\\"
        );
        assert_eq!(
            Reply::osc_background([1.0, 0.0, 0.0, 1.0]),
            "\x1b]11;rgb:ffff/0000/0000\x1b\\"
        );
    }
}
