// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The callback interface the byte parser dispatches to.

/// Sink for the byte parser's decoded output.
///
/// [`crate::Parser::advance`] calls these methods as it decodes a VT byte
/// stream; every method has a no-op default, so a terminal screen only
/// overrides the events it cares about. The [`crate::Event`] enum is the
/// owned, trait-free alternative to this callback interface.
pub trait Perform {
    /// A single printable non-ASCII code point (UTF-8 decoded).
    fn print(&mut self, _c: char) {}

    /// A contiguous run of printable ASCII (0x20..=0x7e) — the bulk hot path.
    /// The bytes are guaranteed valid UTF-8 by construction; implementations
    /// may convert with `std::str::from_utf8_unchecked` (or `push_str` via a
    /// `&str` view) without re-validating.
    fn print_bytes(&mut self, _s: &[u8]) {}

    /// A contiguous run of pre-decoded printable code points — the non-ASCII
    /// bulk hot path. The code points are guaranteed valid `char`s (decoded
    /// from validated UTF-8) and are never C0/C1 controls. The default
    /// iterates and calls [`print`](Perform::print), so overriding is
    /// optional; implementations that bulk-write cells can specialize to skip
    /// per-char trait dispatch (rio-vt parity).
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        for &cp in codepoints {
            let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
            self.print(c);
        }
    }

    /// A C0/C1 control byte (e.g. `\n`, `\t`, `\x1b`).
    fn execute(&mut self, _byte: u8) {}

    /// A CSI sequence's final byte, with collected parameters and
    /// intermediates (private markers `?` `>` `=` `<` + the `0x20..0x2f`
    /// range). Parameters are `i32`, clamped to `0..=65535`; an empty colon
    /// parameter is the `-1` sentinel. `sep` records the `:` (colon)
    /// separators: bit `i` is set when parameter `i` is followed by a colon
    /// (so `4:2` and `4;2` are distinguishable). Default: ignore.
    fn csi_dispatch(&mut self, _params: &[i32], _sep: u16, _intermediates: &[u8], _final_byte: u8) {
    }

    /// An escape sequence's final byte. Default: ignore.
    fn esc_dispatch(&mut self, _intermediates: &[u8], _byte: u8) {}

    /// A device-control string header arrived. Parameters are `i32` (see
    /// [`csi_dispatch`](Perform::csi_dispatch)). Default: ignore.
    fn hook(&mut self, _params: &[i32], _intermediates: &[u8], _final_byte: u8) {}

    /// A device-control string payload byte arrived. Default: ignore.
    fn put(&mut self, _byte: u8) {}

    /// A contiguous device-control string payload run (no terminators or
    /// ignored bytes). Bulk form of [`put`](Perform::put); the default loops
    /// over [`put`](Perform::put). Implementations with cross-byte state
    /// (e.g. a sixel decoder) can keep the per-byte form.
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
