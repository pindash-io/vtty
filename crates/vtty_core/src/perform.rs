// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The callback interface the byte parser dispatches to.
//!
//! Reference: <https://vt100.net/emu/dec_ansi_parser>.

/// Sink for the byte parser's decoded output. `Parser::advance` calls these
/// as it decodes; every method has a no-op default, so a screen overrides
/// only what it cares about. [`Event`](crate::Event) is the owned,
/// trait-free alternative.
pub trait Perform {
    /// A single printable non-ASCII code point (UTF-8 decoded).
    fn print(&mut self, _c: char) {}

    /// A contiguous run of printable ASCII (`0x20..=0x7e`) — display text,
    /// the bulk hot path. Valid UTF-8 by construction, so implementations
    /// may `std::str::from_utf8_unchecked` without re-validating.
    fn print_bytes(&mut self, _s: &[u8]) {}

    /// A contiguous run of pre-decoded printable code points (never C0/C1).
    /// Defaults to per-code-point [`print`](Perform::print); bulk writers
    /// can specialize.
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        for &cp in codepoints {
            let c = char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER);
            self.print(c);
        }
    }

    /// A C0/C1 control byte (e.g. `\n`, `\t`, `\x1b`).
    fn execute(&mut self, _byte: u8) {}

    /// A CSI sequence's final byte, with parameters and intermediates
    /// (private markers `?` `>` `=` `<` + `0x20..0x2f`). Params are `i32`
    /// (clamped to `0..=65535`); an empty colon param is the `-1` sentinel.
    /// `sep` marks colon-separated params: bit `i` = param `i` followed by
    /// `:` (`4:2` vs `4;2`). Default: ignore.
    fn csi_dispatch(&mut self, _params: &[i32], _sep: u16, _intermediates: &[u8], _final_byte: u8) {
    }

    /// An escape sequence's final byte. Default: ignore.
    fn esc_dispatch(&mut self, _intermediates: &[u8], _byte: u8) {}

    /// A device-control string header arrived. Parameters are `i32` (see
    /// [`csi_dispatch`](Perform::csi_dispatch)). Default: ignore.
    fn hook(&mut self, _params: &[i32], _intermediates: &[u8], _final_byte: u8) {}

    /// A device-control string payload byte arrived. Raw — arbitrary, not
    /// necessarily UTF-8 (Sixel raster, kitty graphics, base64). Default:
    /// ignore.
    fn put(&mut self, _byte: u8) {}

    /// A contiguous DCS payload run — raw bytes. Defaults to per-byte
    /// [`put`](Perform::put); cross-byte decoders (e.g. sixel) can override.
    fn put_slice(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.put(byte);
        }
    }

    /// The device-control string ended. Default: ignore.
    fn unhook(&mut self) {}

    /// An operating-system command ended (params split on `;`). Default: ignore.
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
}
