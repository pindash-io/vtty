// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query/response reply formatting (DECRQSS / DECRQM / XTGETTCAP).
//!
//! Reference: <https://vt100.net/emu/dec_ansi_parser>.

use core::fmt::Write as _;

use smol_str::{SmolStr, SmolStrBuilder};

use crate::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

/// Accumulates VT query replies in dispatch order, so a burst of queries
/// (`XTVERSION` + `OSC 10/11` + `OSC 4` + `DA`) is answered in order instead
/// of dropping all but the last.
#[derive(Debug)]
pub struct Reply {
    buf: SmolStrBuilder,
    /// Nothing appended yet — lets `is_empty` skip a `finish()` clone.
    empty: bool,
}

impl Reply {
    /// An empty reply buffer.
    pub const DEFAULT: Self = Self {
        buf: SmolStrBuilder::new(),
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

    /// Append a DECRQSS answer: `DCS {valid} $ r {value} ST`.
    pub fn decrqss(mut self, valid: bool, value: &str) -> Self {
        let _ = write!(
            self.buf,
            "\x1bP{}$r{value}\x1b\\",
            if valid { "1" } else { "0" }
        );
        self.empty = false;
        self
    }

    /// Append an XTGETTCAP answer: `DCS {ok} + r {hex key}[={hex value}] ST`.
    /// `ok` picks `1`/`0`; `value` is omitted for boolean capabilities and
    /// failure messages.
    fn xtgettcap(mut self, ok: bool, key: &[u8], value: Option<&[u8]>) -> Self {
        self.buf.push_str(if ok { "\x1bP1+r" } else { "\x1bP0+r" });
        self.buf.push_str(&const_hex::encode(key));
        if let Some(value) = value {
            self.buf.push('=');
            self.buf.push_str(&const_hex::encode(value));
        }
        self.buf.push_str("\x1b\\");
        self.empty = false;
        self
    }

    /// Append an XTGETTCAP answer: `DCS 1 + r {hex key}={hex value} ST`.
    #[inline]
    pub fn xtgettcap_reply(self, key: &[u8], value: &[u8]) -> Self {
        self.xtgettcap(true, key, Some(value))
    }

    /// Append an XTGETTCAP boolean answer: `DCS 1 + r {hex key} ST` (no
    /// `=value`, per xterm — a bare name means "supported").
    #[inline]
    pub fn xtgettcap_reply_bool(self, key: &[u8]) -> Self {
        self.xtgettcap(true, key, None)
    }

    /// Append an XTGETTCAP failure: `DCS 0 + r {hex msg} ST`.
    #[inline]
    pub fn xtgettcap_error(self, msg: &[u8]) -> Self {
        self.xtgettcap(false, msg, None)
    }

    /// Append an XTGETTCAP answer with a decimal integer value, hex-encoding
    /// the digits directly (no `to_string` allocation).
    pub fn xtgettcap_num(mut self, key: &[u8], n: usize) -> Self {
        self.buf.push_str("\x1bP1+r");
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
        self.buf.push_str("\x1b\\");
        self.empty = false;
        self
    }

    /// Append an SGR reply (`m…`) from a pen's palette ids + style flags.
    /// Starts with `0` (reset-all), like vte. `palette` resolves RGB ids.
    pub fn sgr(mut self, fg: ColorId, bg: ColorId, flags: SgrFlags, palette: &[u32]) -> Self {
        let _ = write!(self.buf, "0");
        if flags.contains(SgrFlags::BOLD) {
            let _ = write!(self.buf, ";1");
        }
        if flags.contains(SgrFlags::ITALIC) {
            let _ = write!(self.buf, ";3");
        }
        if flags.contains(SgrFlags::UNDERLINE) {
            let _ = write!(self.buf, ";4");
        }
        if flags.contains(SgrFlags::INVERSE) {
            let _ = write!(self.buf, ";7");
        }
        push_sgr_color(&mut self.buf, 3, fg, DEFAULT_FG_ID, palette);
        push_sgr_color(&mut self.buf, 4, bg, DEFAULT_BG_ID, palette);
        self.empty = false;
        self
    }

    /// Append a DECRQM answer: `CSI {?}{param};{state}$y`. `state` is the Pm
    /// value per ECMA-48: 0 = not recognized, 1 = set, 2 = reset. `private`
    /// includes the `?` for `CSI ?Ps$p` queries.
    pub fn decrqm(mut self, param: i32, private: bool, state: u8) -> Self {
        let _ = write!(
            self.buf,
            "\x1b[{}{param};{state}$y",
            if private { "?" } else { "" }
        );
        self.empty = false;
        self
    }

    /// Finish and return the accumulated reply.
    #[inline]
    pub fn finish(self) -> SmolStr {
        self.buf.finish()
    }

    /// The accumulated reply text.
    #[doc(hidden)]
    #[inline]
    pub fn as_smol_str(&self) -> SmolStr {
        self.buf.finish()
    }

    /// Builds a reply from raw text (used by the codec crate).
    #[doc(hidden)]
    pub fn from_text(text: &str) -> Self {
        let mut buf = SmolStrBuilder::new();
        buf.push_str(text);
        Self {
            buf,
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
    s: &mut SmolStrBuilder,
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
    let _ = write!(
        s,
        ";{base}8;2;{};{};{}",
        (rgb >> 16) & 0xff,
        (rgb >> 8) & 0xff,
        rgb & 0xff
    );
}
