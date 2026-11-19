// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Single source of truth for the parser's resource limits.
//!
//! These bounds are the code-side counterpart of the "Resource limits" table
//! in `ARCHITECTURE.md`. Keeping them in one module — rather than scattered
//! across `parser.rs` and `utf8.rs` — means the two cannot drift apart. Each
//! constant documents why the bound exists and what happens when it is
//! exceeded.

/// Maximum buffered OSC payload, in bytes.
///
/// A hostile program can otherwise stream a gigabyte into `ESC ] … ST` and
/// OOM the parser; xterm.js caps OSC/DCS at `PAYLOAD_LIMIT = 10000000` and
/// aborts the sequence, so we bound here too. 1 MiB comfortably fits the
/// largest real OSC (`OSC 52` clipboard ≈ 128 KiB). On overflow the whole
/// OSC is discarded.
pub(crate) const MAX_OSC_PAYLOAD: usize = 1_048_576;

/// Maximum number of OSC parameters.
///
/// vte/rio-vt cap at 16; the remaining payload is dropped.
pub(crate) const MAX_OSC_PARAMS: usize = 16;

/// Maximum number of CSI/DCS parameters.
///
/// vte/rio-vt cap at 32, xterm at 16; past this the parameters are dropped.
pub(crate) const MAX_PARAM_COUNT: usize = 32;

/// Maximum CSI/DCS parameter value.
///
/// DEC VT500 params are 16-bit (`0..=65535`); `i32` holds the clamped range
/// plus the `-1` empty-param sentinel.
pub(crate) const MAX_PARAM_VALUE: i32 = 65535;

/// Maximum number of CSI/DCS intermediates.
///
/// The DEC spec caps intermediates (private markers plus `0x20..0x2f`) at
/// two; extras are dropped.
pub(crate) const MAX_INTERMEDIATES: usize = 2;

/// Maximum number of continuation bytes in a UTF-8 code point.
///
/// A full code point is at most `1 + MAX_UTF8_CONTINUATIONS = 4` bytes — the
/// "UTF-8 partial: 4 bytes" row in `ARCHITECTURE.md`.
pub(crate) const MAX_UTF8_CONTINUATIONS: usize = 3;

/// Maximum SIMD UTF-8 decode window, in bytes.
///
/// The window is additionally capped at the next `ESC` (a safe boundary that
/// must reach the escape state rather than be dispatched as `execute`) and at
/// a code-point boundary.
#[cfg(any(feature = "simd", test))]
pub(crate) const SIMD_UTF8_CHUNK: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    /// The "Resource limits" table in `ARCHITECTURE.md` must stay in lockstep
    /// with this module. Every documented value is derived from a constant
    /// here, so changing one without the other fails this test.
    #[test]
    fn resource_limits_match_architecture_table() {
        let doc = include_str!("../ARCHITECTURE.md");

        assert_limit(doc, "OSC payload", &format_mib(MAX_OSC_PAYLOAD));
        assert_limit(doc, "OSC params", &alloc::format!("{}", MAX_OSC_PARAMS));
        assert_limit(
            doc,
            "CSI/DCS params",
            &alloc::format!("{}", MAX_PARAM_COUNT),
        );
        assert_limit(
            doc,
            "intermediates",
            &alloc::format!("{}", MAX_INTERMEDIATES),
        );
        assert_limit(
            doc,
            "UTF-8 partial",
            &alloc::format!("{} bytes", MAX_UTF8_CONTINUATIONS + 1),
        );
        assert_limit(
            doc,
            "SIMD UTF-8 window",
            &alloc::format!("{} bytes, capped at `ESC`", SIMD_UTF8_CHUNK),
        );
    }

    /// Assert the "limit" cell for `resource` equals `expected`.
    fn assert_limit(doc: &str, resource: &str, expected: &str) {
        assert_eq!(
            limit_cell(doc, resource),
            Some(expected),
            "`{resource}` limit in ARCHITECTURE.md drifted from (or is missing in) limits.rs",
        );
    }

    /// Extract the "limit" column cell for `resource` from the markdown table.
    fn limit_cell<'a>(doc: &'a str, resource: &str) -> Option<&'a str> {
        for line in doc.lines() {
            let line = line.trim_start();
            if !line.starts_with('|') {
                continue;
            }
            let mut cells = line.split('|');
            let _ = cells.next(); // empty cell before the leading `|`
            let name = cells.next()?.trim();
            let limit = cells.next()?.trim();
            if name == resource {
                return Some(limit);
            }
        }
        None
    }

    /// Format a byte count the way the table does (`1 MiB`).
    fn format_mib(bytes: usize) -> String {
        alloc::format!("{} MiB", bytes / (1 << 20))
    }
}
