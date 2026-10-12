// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The callback interface the byte parser dispatches to.

/// Sink for the byte parser's decoded output. [`crate::Parser::advance`]
/// calls these as it decodes; every method has a no-op default, so a screen
/// overrides only what it cares about. [`crate::Event`] is the owned,
/// trait-free alternative.
pub trait Perform {
    /// A single printable non-ASCII code point (UTF-8 decoded).
    fn print(&mut self, _c: char) {}

    /// A contiguous run of printable ASCII (`0x20..=0x7e`) — the bulk hot
    /// path. The bytes are valid UTF-8 by construction; convert with
    /// `std::str::from_utf8_unchecked` without re-validating.
    fn print_bytes(&mut self, _s: &[u8]) {}

    /// A contiguous run of pre-decoded printable code points — the non-ASCII
    /// bulk hot path, never C0/C1 controls. The default loops over
    /// [`print`](Perform::print); bulk cell writers can specialize to skip
    /// per-char dispatch.
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        for &cp in codepoints {
            let c = char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER);
            self.print(c);
        }
    }

    /// A C0/C1 control byte (e.g. `\n`, `\t`, `\x1b`).
    fn execute(&mut self, _byte: u8) {}

    /// A CSI sequence's final byte, with collected parameters and
    /// intermediates (private markers `?` `>` `=` `<` + `0x20..0x2f`).
    /// Params are `i32`, clamped to `0..=65535`; an empty colon param is the
    /// `-1` sentinel. `sep` records `:` separators: bit `i` is set when param
    /// `i` is followed by a colon (`4:2` vs `4;2`). Default: ignore.
    fn csi_dispatch(&mut self, _params: &[i32], _sep: u16, _intermediates: &[u8], _final_byte: u8) {
    }

    /// An escape sequence's final byte. Default: ignore.
    fn esc_dispatch(&mut self, _intermediates: &[u8], _byte: u8) {}

    /// A device-control string header arrived. Parameters are `i32` (see
    /// [`csi_dispatch`](Perform::csi_dispatch)). Default: ignore.
    fn hook(&mut self, _params: &[i32], _intermediates: &[u8], _final_byte: u8) {}

    /// A device-control string payload byte arrived. Default: ignore.
    fn put(&mut self, _byte: u8) {}

    /// A contiguous DCS payload run (no terminators). Bulk form of
    /// [`put`](Perform::put); the default loops over it. Cross-byte decoders
    /// (e.g. sixel) can keep the per-byte form.
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
