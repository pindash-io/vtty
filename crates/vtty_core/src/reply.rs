// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query/response reply formatting (DECRQSS / DECRQM / XTGETTCAP and the
//! `$`-family DCS reports).
//!
//! Query/response pairs follow the [DECronomicon CSI/DCS
//! tables](https://vt100.net/emu/ctrlseq_dec.html); the individual report
//! formats (DECRPSS, DECTSR, DECCTR, DECCIR, DECTABSR) follow EK-VT520-RM §5.

use core::fmt::Write as _;

use compact_str::CompactString;

use crate::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags, u32_to_rgb};

/// DCS/OSC string terminator (`ESC \`).
pub(crate) const ST: &str = "\x1b\\";

/// XTGETTCAP answer prefix — `DCS 1 + r` (supported/valid).
const XTGETTCAP_OK: &str = "\x1bP1+r";
/// XTGETTCAP answer prefix — `DCS 0 + r` (unsupported/error).
const XTGETTCAP_ERR: &str = "\x1bP0+r";

/// DECRQSS answer prefix — `DCS 1 $ r` (valid).
const DECRQSS_OK: &str = "\x1bP1$r";
/// DECRQSS answer prefix — `DCS 0 $ r` (invalid).
const DECRQSS_ERR: &str = "\x1bP0$r";

/// Namespace for VT query reply builders. Each associated function returns
/// the finished reply as a [`CompactString`]; the screen accumulates a burst
/// of replies by concatenating them in dispatch order.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reply;

impl Reply {
    /// Build a DECRQSS answer: `DCS {valid} $ r {value} ST`.
    pub fn decrqss(valid: bool, value: &str) -> CompactString {
        let mut buf = CompactString::default();
        buf.push_str(if valid { DECRQSS_OK } else { DECRQSS_ERR });
        buf.push_str(value);
        terminate(&mut buf);
        buf
    }

    /// Build an XTGETTCAP answer: `DCS 1 + r {hex key}={hex value} ST`.
    #[inline]
    pub fn xtgettcap_reply(key: &[u8], value: &[u8]) -> CompactString {
        xtgettcap_build(true, key, Some(value))
    }

    /// Build an XTGETTCAP boolean answer: `DCS 1 + r {hex key} ST` (no
    /// `=value`, per xterm — a bare name means "supported").
    #[inline]
    pub fn xtgettcap_reply_bool(key: &[u8]) -> CompactString {
        xtgettcap_build(true, key, None)
    }

    /// Build an XTGETTCAP failure: `DCS 0 + r {hex msg} ST`.
    #[inline]
    pub fn xtgettcap_error(msg: &[u8]) -> CompactString {
        xtgettcap_build(false, msg, None)
    }

    /// Build an XTGETTCAP answer with a decimal integer value.
    #[inline]
    pub fn xtgettcap_num(key: &[u8], n: usize) -> CompactString {
        xtgettcap_num_build(key, n)
    }

    /// Build an SGR reply (`m…`) from a pen's palette ids + style flags.
    /// Starts with `0` (reset-all), like vte. `palette` resolves RGB ids.
    pub fn sgr(fg: ColorId, bg: ColorId, flags: SgrFlags, palette: &[u32]) -> CompactString {
        let mut buf = CompactString::default();
        buf.push_str("0");
        for (flag, code) in [
            (SgrFlags::BOLD, ";1"),
            (SgrFlags::ITALIC, ";3"),
            (SgrFlags::UNDERLINE, ";4"),
            (SgrFlags::INVERSE, ";7"),
        ] {
            if flags.contains(flag) {
                buf.push_str(code);
            }
        }
        push_sgr_color(&mut buf, 3, fg, DEFAULT_FG_ID, palette);
        push_sgr_color(&mut buf, 4, bg, DEFAULT_BG_ID, palette);
        buf
    }

    /// Build an OSC color reply `ESC]CODE;rgb:RRRR/GGGG/BBBB ESC\` from a
    /// 0xRRGGBB value (8-bit channels repeated to 4 hex digits, the xterm
    /// convention).
    #[inline]
    pub fn osc_color(code: u16, rgb: u32) -> CompactString {
        crate::color::format_osc_color_reply(code, rgb)
    }

    /// Build an `OSC 4` palette-entry reply
    /// `ESC]4;N;rgb:RRRR/GGGG/BBBB ESC\`.
    #[inline]
    pub fn osc_palette(index: usize, rgb: u32) -> CompactString {
        crate::color::format_osc_palette_reply(index, rgb)
    }

    /// Build the fixed `OSC 11` background reply `ESC]11;rgb:… ESC\`.
    #[inline]
    pub fn osc_background(c: [f32; 4]) -> CompactString {
        // The 25-byte buffer is pure ASCII (ESC + hex digits + ST).
        let fixed = crate::color::format_osc_background(c);
        CompactString::from(core::str::from_utf8(&fixed).unwrap_or_default())
    }

    /// Build a DECRQM answer: `CSI {?}{param};{state}$y`. `state` is the Pm
    /// value per ECMA-48: 0 = not recognized, 1 = set, 2 = reset. `private`
    /// includes the `?` for `CSI ?Ps$p` queries.
    pub fn decrqm(param: i32, private: bool, state: u8) -> CompactString {
        let mut buf = CompactString::default();
        buf.push_str("\x1b[");
        if private {
            buf.push('?');
        }
        let _ = write!(buf, "{param};{state}$y");
        buf
    }
}

/// Terminate a DCS/OSC string with `ST`.
#[inline]
fn terminate(buf: &mut CompactString) {
    buf.push_str(ST);
}

/// Append an XTGETTCAP answer: `DCS {ok} + r {hex key}[={hex value}] ST`.
/// `ok` picks `1`/`0`; `value` is omitted for boolean capabilities and
/// failure messages.
pub(crate) fn xtgettcap(buf: &mut CompactString, ok: bool, key: &[u8], value: Option<&[u8]>) {
    buf.push_str(if ok { XTGETTCAP_OK } else { XTGETTCAP_ERR });
    buf.push_str(&const_hex::encode(key));
    if let Some(value) = value {
        buf.push('=');
        buf.push_str(&const_hex::encode(value));
    }
    terminate(buf);
}

/// Append an XTGETTCAP decimal-integer answer, hex-encoding the digits
/// directly (no `to_string` allocation).
pub(crate) fn xtgettcap_num_append(buf: &mut CompactString, key: &[u8], n: usize) {
    buf.push_str(XTGETTCAP_OK);
    buf.push_str(&const_hex::encode(key));
    buf.push('=');
    // Decimal digits → their hex pairs: '0'..'9' = 0x30..0x39 → "30".."39".
    let mut digits = [0_u8; 20];
    let mut i = digits.len();
    let mut n = n;
    loop {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &d in &digits[i..] {
        buf.push('3');
        buf.push(d as char);
    }
    terminate(buf);
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

/// Append an SGR color to `s`: `;{base}9` (default), `;{base}8;5;n`
/// (indexed), or `;{base}8;2;r;g;b` (RGB, resolved from the palette).
/// `base` is 3 for fg, 4 for bg; `default_id` is the matching default color.
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
    let Some(rgb) = palette.get(id.0 as usize).copied() else {
        let _ = write!(s, ";{base}9");
        return;
    };
    let [r, g, b] = u32_to_rgb(rgb);
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
            Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::empty(), &pal),
            "0;39;49"
        );
        assert_eq!(
            Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::BOLD, &pal),
            "0;1;39;49"
        );
        // 5 = indexed, 258 = an RGB id (0x112233) in the palette.
        let mut pal = [0_u32; 259];
        pal[258] = 0x112233;
        assert_eq!(
            Reply::sgr(ColorId(5), ColorId(258), SgrFlags::empty(), &pal),
            "0;38;5;5;48;2;17;34;51"
        );
    }
}
