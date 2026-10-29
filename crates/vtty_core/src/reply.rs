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

/// Accumulates VT query replies in dispatch order, so a burst of queries
/// (`XTVERSION` + `OSC 10/11` + `OSC 4` + `DA`) is answered in order instead
/// of dropping all but the last.
#[derive(Debug)]
#[must_use]
pub struct Reply {
    buf: CompactString,
    /// Nothing appended yet — lets `is_empty` answer without touching `buf`.
    empty: bool,
}

impl Reply {
    /// An empty reply buffer.
    pub const DEFAULT: Self = Self {
        buf: CompactString::const_new(""),
        empty: true,
    };

    /// An empty reply buffer.
    #[inline]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// Whether nothing has been appended yet.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Crate-internal mutable access to the append buffer. Marks the reply
    /// non-empty — the DA builders in `conformance.rs` append through this.
    #[inline]
    pub(crate) fn buffer_mut(&mut self) -> &mut CompactString {
        self.empty = false;
        &mut self.buf
    }

    /// Terminate a DCS/OSC string with `ST` and mark the reply non-empty.
    #[inline]
    fn terminate(&mut self) {
        self.buf.push_str(ST);
        self.empty = false;
    }

    /// Build a DECRQSS answer: `DCS {valid} $ r {value} ST`.
    pub fn decrqss(valid: bool, value: &str) -> Self {
        let mut this = Self::default();
        this.buf
            .push_str(if valid { DECRQSS_OK } else { DECRQSS_ERR });
        this.buf.push_str(value);
        this.terminate();
        this
    }

    /// Append an XTGETTCAP answer to `self`: `DCS {ok} + r {hex key}[={hex
    /// value}] ST`. `ok` picks `1`/`0`; `value` is omitted for boolean
    /// capabilities and failure messages.
    pub(crate) fn xtgettcap(&mut self, ok: bool, key: &[u8], value: Option<&[u8]>) {
        self.buf
            .push_str(if ok { XTGETTCAP_OK } else { XTGETTCAP_ERR });
        self.buf.push_str(&const_hex::encode(key));
        if let Some(value) = value {
            self.buf.push('=');
            self.buf.push_str(&const_hex::encode(value));
        }
        self.terminate();
    }

    /// Build an XTGETTCAP answer: `DCS 1 + r {hex key}={hex value} ST`.
    #[inline]
    pub fn xtgettcap_reply(key: &[u8], value: &[u8]) -> Self {
        let mut this = Self::default();
        this.xtgettcap(true, key, Some(value));
        this
    }

    /// Build an XTGETTCAP boolean answer: `DCS 1 + r {hex key} ST` (no
    /// `=value`, per xterm — a bare name means "supported").
    #[inline]
    pub fn xtgettcap_reply_bool(key: &[u8]) -> Self {
        let mut this = Self::default();
        this.xtgettcap(true, key, None);
        this
    }

    /// Build an XTGETTCAP failure: `DCS 0 + r {hex msg} ST`.
    #[inline]
    pub fn xtgettcap_error(msg: &[u8]) -> Self {
        let mut this = Self::default();
        this.xtgettcap(false, msg, None);
        this
    }

    /// Append an XTGETTCAP decimal-integer answer to `self`, hex-encoding
    /// the digits directly (no `to_string` allocation).
    pub(crate) fn xtgettcap_num_append(&mut self, key: &[u8], n: usize) {
        self.buf.push_str(XTGETTCAP_OK);
        self.buf.push_str(&const_hex::encode(key));
        self.buf.push('=');
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
            self.buf.push('3');
            self.buf.push(d as char);
        }
        self.terminate();
    }

    /// Build an XTGETTCAP answer with a decimal integer value.
    #[inline]
    pub fn xtgettcap_num(key: &[u8], n: usize) -> Self {
        let mut this = Self::default();
        this.xtgettcap_num_append(key, n);
        this
    }

    /// Build an SGR reply (`m…`) from a pen's palette ids + style flags.
    /// Starts with `0` (reset-all), like vte. `palette` resolves RGB ids.
    pub fn sgr(fg: ColorId, bg: ColorId, flags: SgrFlags, palette: &[u32]) -> Self {
        let mut this = Self::default();
        let buf = this.buffer_mut();
        buf.push_str("0");
        if flags.contains(SgrFlags::BOLD) {
            buf.push_str(";1");
        }
        if flags.contains(SgrFlags::ITALIC) {
            buf.push_str(";3");
        }
        if flags.contains(SgrFlags::UNDERLINE) {
            buf.push_str(";4");
        }
        if flags.contains(SgrFlags::INVERSE) {
            buf.push_str(";7");
        }
        push_sgr_color(buf, 3, fg, DEFAULT_FG_ID, palette);
        push_sgr_color(buf, 4, bg, DEFAULT_BG_ID, palette);
        this
    }

    /// Build an OSC color reply `ESC]CODE;rgb:RRRR/GGGG/BBBB ESC\` from a
    /// 0xRRGGBB value (8-bit channels repeated to 4 hex digits, the xterm
    /// convention).
    #[inline]
    pub fn osc_color(code: u16, rgb: u32) -> Self {
        let mut this = Self::default();
        this.buffer_mut()
            .push_str(&crate::color::format_osc_color_reply(code, rgb));
        this
    }

    /// Build an `OSC 4` palette-entry reply
    /// `ESC]4;N;rgb:RRRR/GGGG/BBBB ESC\`.
    #[inline]
    pub fn osc_palette(index: usize, rgb: u32) -> Self {
        let mut this = Self::default();
        this.buffer_mut()
            .push_str(&crate::color::format_osc_palette_reply(index, rgb));
        this
    }

    /// Build the fixed `OSC 11` background reply `ESC]11;rgb:… ESC\`.
    #[inline]
    pub fn osc_background(c: [f32; 4]) -> Self {
        let fixed = crate::color::format_osc_background(c);
        // The 25-byte buffer is pure ASCII (ESC + hex digits + ST).
        let mut this = Self::default();
        this.buffer_mut()
            .push_str(core::str::from_utf8(&fixed).unwrap_or_default());
        this
    }

    /// Build a DECRQM answer: `CSI {?}{param};{state}$y`. `state` is the Pm
    /// value per ECMA-48: 0 = not recognized, 1 = set, 2 = reset. `private`
    /// includes the `?` for `CSI ?Ps$p` queries.
    pub fn decrqm(param: i32, private: bool, state: u8) -> Self {
        let mut this = Self::default();
        let buf = this.buffer_mut();
        buf.push_str("\x1b[");
        if private {
            buf.push('?');
        }
        let _ = write!(buf, "{param};{state}$y");
        this
    }

    /// Finish and return the accumulated reply.
    #[inline]
    pub fn finish(self) -> CompactString {
        self.buf
    }

    /// The accumulated reply text.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.buf.as_str()
    }

    /// Builds a reply from raw text (used by the codec crate).
    #[doc(hidden)]
    pub fn from_text(text: &str) -> Self {
        Self {
            buf: CompactString::from(text),
            empty: text.is_empty(),
        }
    }
}

impl Default for Reply {
    fn default() -> Self {
        Self::DEFAULT
    }
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
        assert_eq!(Reply::decrqm(25, true, 1).finish(), "\x1b[?25;1$y");
        assert_eq!(Reply::decrqm(25, true, 2).finish(), "\x1b[?25;2$y");
        assert_eq!(Reply::decrqm(25, true, 0).finish(), "\x1b[?25;0$y");
        assert_eq!(Reply::decrqm(4, false, 2).finish(), "\x1b[4;2$y");
    }

    #[test]
    fn decrqss_reply_format() {
        assert_eq!(Reply::decrqss(true, "m").finish(), "\x1bP1$rm\x1b\\");
        assert_eq!(Reply::decrqss(false, "x").finish(), "\x1bP0$rx\x1b\\");
    }

    #[test]
    fn sgr_reply_format() {
        let pal = [0_u32; 256];
        assert_eq!(
            Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::empty(), &pal,)
                .finish(),
            "0;39;49"
        );
        assert_eq!(
            Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::BOLD, &pal)
                .finish(),
            "0;1;39;49"
        );
        // 5 = indexed, 258 = an RGB id (0x112233) in the palette.
        let mut pal = [0_u32; 259];
        pal[258] = 0x112233;
        assert_eq!(
            Reply::sgr(ColorId(5), ColorId(258), SgrFlags::empty(), &pal)
                .finish(),
            "0;38;5;5;48;2;17;34;51"
        );
    }

    #[test]
    fn reply_is_empty() {
        assert!(Reply::new().is_empty());
        assert!(!Reply::decrqss(true, "m").is_empty());
        assert!(!Reply::decrqm(25, true, 1).is_empty());
        assert!(
            !Reply::sgr(DEFAULT_FG_ID, DEFAULT_BG_ID, SgrFlags::empty(), &[])
                .is_empty()
        );
        assert!(!Reply::xtgettcap_reply(b"TN", b"xterm").is_empty());
        assert!(!Reply::xtgettcap_num(b"co", 80).is_empty());
        assert!(!Reply::dectabsr(&[8]).is_empty());
    }
}
