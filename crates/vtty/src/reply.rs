// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query/response reply formatting (DECRQSS / DECRQM / XTGETTCAP).

use alloc::string::String;
use core::fmt::Write as _;

use smol_str::SmolStr;

use crate::hex::push_hex;
use crate::{ColorId, DEFAULT_BG_ID, DEFAULT_FG_ID, SgrFlags};

/// Accumulates VT query replies into one buffer, in dispatch order, so a
/// burst of queries (asciinema's XTVERSION + OSC 10/11 + OSC 4 + DA probe)
/// is answered in order instead of dropping all but the last.
///
/// The free functions below are one-shot conveniences over [`Self::new`] +
/// one append + [`Self::finish`].
#[derive(Default, Debug)]
pub struct Reply {
    buf: String,
}

impl Reply {
    /// An empty reply buffer.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing has been appended yet.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Append a DECRQSS answer: `DCS {valid} $ r {value} ST`.
    pub fn decrqss(mut self, valid: bool, value: &str) -> Self {
        let _ = write!(
            self.buf,
            "\x1bP{}$r{value}\x1b\\",
            if valid { "1" } else { "0" }
        );
        self
    }

    /// Append an XTGETTCAP answer: `DCS {ok} + r {hex key}[={hex value}] ST`.
    /// `ok` picks `1`/`0`; `value` is omitted for boolean capabilities and
    /// failure messages.
    fn xtgettcap(mut self, ok: bool, key: &[u8], value: Option<&[u8]>) -> Self {
        self.buf.push_str(if ok { "\x1bP1+r" } else { "\x1bP0+r" });
        push_hex(&mut self.buf, key);
        if let Some(value) = value {
            self.buf.push('=');
            push_hex(&mut self.buf, value);
        }
        self.buf.push_str("\x1b\\");
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
        push_hex(&mut self.buf, key);
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
        push_sgr_color(&mut self.buf, 3, fg, palette);
        push_sgr_color(&mut self.buf, 4, bg, palette);
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
        self
    }

    /// Finish and return the accumulated reply.
    #[inline]
    pub fn finish(self) -> SmolStr {
        SmolStr::new(self.buf)
    }
}

/// Append an SGR color to `s`: `;{base}9` (default), `;{base}8;5;n`
/// (indexed), or `;{base}8;2;r;g;b` (RGB, resolved from the palette).
/// `base` is 3 for fg, 4 for bg.
fn push_sgr_color(s: &mut String, base: u8, id: ColorId, palette: &[u32]) {
    match id {
        DEFAULT_FG_ID | DEFAULT_BG_ID => {
            let _ = write!(s, ";{base}9");
        }
        id if id.0 <= 255 => {
            let _ = write!(s, ";{base}8;5;{id}");
        }
        _ => {
            let rgb = palette.get(id.0 as usize).copied().unwrap_or(0);
            let _ = write!(
                s,
                ";{base}8;2;{};{};{}",
                (rgb >> 16) & 0xff,
                (rgb >> 8) & 0xff,
                rgb & 0xff
            );
        }
    }
}
