// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! XTGETTCAP terminfo capability table.
//!
//! Answers the xterm `DCS + q <hex cap> ST` query with the terminfo values a
//! terminal advertises. [`Cap::parse`] maps a query name and its aliases to a
//! [`Cap`], [`CapValue`] carries the reply, and [`Reply::termcap`] formats it.

use compact_str::CompactString;

/// The reply value for a capability.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum CapValue {
    /// Unknown or unsupported; the reply is `DCS 0 + r`.
    None,
    /// Boolean, replied as a bare key.
    Bool,
    /// String, replied as `key=value`.
    Text(&'static [u8]),
    /// The column count (`co`).
    Cols,
    /// The row count (`li`).
    Rows,
    /// The OS name (`query-os-name`).
    OsName,
    /// The terminal name (`TN`).
    Tn,
}

/// Fixed text replies for terminfo string capabilities; each const is
/// named after the capability it answers.
#[allow(missing_docs, reason = "const name is the capability name")]
impl CapValue {
    pub const TEXT_CO: Self = Self::Text(b"256");
    pub const TEXT_PA: Self = Self::Text(b"32767");
    pub const TEXT_RGB: Self = Self::Text(b"8/8/8");
    pub const TEXT_IT: Self = Self::Text(b"8");
    pub const TEXT_KITTY: Self = Self::Text(b"yes");
    pub const TEXT_SETRGBF: Self = Self::Text(b"\x1b[38:2:%p1%d:%p2%d:%p3%dm");
    pub const TEXT_SETRGBB: Self = Self::Text(b"\x1b[48:2:%p1%d:%p2%d:%p3%dm");
    pub const TEXT_SETAF: Self = Self::Text(b"\x1b[38;5;%p1%dm");
    pub const TEXT_SETAB: Self = Self::Text(b"\x1b[48;5;%p1%dm");
    pub const TEXT_OP: Self = Self::Text(b"\x1b[39;49m");
    pub const TEXT_OC: Self = Self::Text(b"\x1b]104\x07");
    pub const TEXT_SMULX: Self = Self::Text(b"\x1b[4:%p1%dm");
    pub const TEXT_SETULC: Self =
        Self::Text(b"\x1b[58:2:%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%dm");
    pub const TEXT_SS: Self = Self::Text(b"\x1b[%p1%d q");
    pub const TEXT_SE: Self = Self::Text(b"\x1b[0 q");
    pub const TEXT_CS: Self = Self::Text(b"\x1b]12;%p1%s\x07");
    pub const TEXT_CR: Self = Self::Text(b"\x1b]112\x07");
    pub const TEXT_SYNC: Self = Self::Text(b"\x1b[?2026%?%p1%{1}%-%tl%eh");
    pub const TEXT_E3: Self = Self::Text(b"\x1b[3J");
    pub const TEXT_CLEAR: Self = Self::Text(b"\x1b[H\x1b[2J");
    pub const TEXT_CUP: Self = Self::Text(b"\x1b[%i%p1%d;%p2%dH");
    pub const TEXT_SMKX: Self = Self::Text(b"\x1b[?1h\x1b=");
    pub const TEXT_RMKX: Self = Self::Text(b"\x1b[?1l\x1b>");
    pub const TEXT_SMCUP: Self = Self::Text(b"\x1b[?1049h");
    pub const TEXT_RMCUP: Self = Self::Text(b"\x1b[?1049l");
    pub const TEXT_SMACS: Self = Self::Text(b"\x1b(0");
    pub const TEXT_RMACS: Self = Self::Text(b"\x1b(B");
    pub const TEXT_ACSC: Self = Self::Text(b"``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~");
    pub const TEXT_CIVIS: Self = Self::Text(b"\x1b[?25l");
    pub const TEXT_CNORM: Self = Self::Text(b"\x1b[?25h");
    pub const TEXT_KBS: Self = Self::Text(b"\x7f");
    pub const TEXT_KDCH1: Self = Self::Text(b"\x1b[3~");
    pub const TEXT_KICH1: Self = Self::Text(b"\x1b[2~");
    pub const TEXT_KHOME: Self = Self::Text(b"\x1b[H");
    pub const TEXT_KEND: Self = Self::Text(b"\x1b[F");
    pub const TEXT_KCUU1: Self = Self::Text(b"\x1b[A");
    pub const TEXT_KCUD1: Self = Self::Text(b"\x1b[B");
    pub const TEXT_KCUB1: Self = Self::Text(b"\x1b[D");
    pub const TEXT_KCUF1: Self = Self::Text(b"\x1b[C");
    pub const TEXT_INDN: Self = Self::Text(b"\x1b[%p1%dS");
}

/// Namespace for terminfo capability names, aliases, and parsing.
#[derive(Clone, Copy, Debug)]
pub struct Cap;

/// Defines the terminfo capability table: one entry per capability
/// generates its `Cap` name const and its `parse` match arm, so the two
/// lists cannot drift apart.
macro_rules! caps {
    ($(
        $name:ident => $bytes:literal => $value:ident $(, [$($alias:ident => $abytes:literal),+ $(,)?])*
    ),+ $(,)?) => {
        impl Cap {
            $(
                #[allow(missing_docs, reason = "capability name and value are self-documenting")]
                pub const $name: &'static [u8] = $bytes;
                $($(
                    #[allow(missing_docs, reason = "capability name and value are self-documenting")]
                    pub const $alias: &'static [u8] = $abytes;
                )+)*
            )+

            /// Parse a query name into a capability (canonical key + value),
            /// accepting the long/short terminfo aliases. Unknown names map to
            /// [`CapValue::None`].
            pub const fn parse(query: &[u8]) -> (&'static [u8], CapValue) {
                match query {
                    $(Self::$name $( $(| Self::$alias)+ )* => (Self::$name, CapValue::$value),)+
                    _ => (Self::NONE, CapValue::None),
                }
            }
        }
    };
}

impl Cap {
    /// Canonical key for an unrecognized capability name.
    pub const NONE: &'static [u8] = b"";
    /// Error key used in `DCS 0 + r` replies for unknown capabilities.
    pub const UNSUPPORTED: &'static [u8] = b"unsupported";
}

caps! {
    TN => b"TN" => Tn, [NAME => b"name"],
    CO => b"Co" => TEXT_CO, [COLORS => b"colors"],
    PA => b"pa" => TEXT_PA, [PAIRS => b"pairs"],
    RGB => b"RGB" => TEXT_RGB,
    TC => b"Tc" => Bool,
    COLS => b"co" => Cols, [COLUMNS => b"cols"],
    ROWS => b"li" => Rows, [LINES => b"lines"],
    IT => b"it" => TEXT_IT,
    AM => b"am" => Bool,
    BCE => b"bce" => Bool,
    CCC => b"ccc" => Bool,
    XN => b"xn" => Bool, [XENL => b"xenl"],
    XT => b"XT" => Bool,
    XF => b"XF" => Bool,
    SIXEL => b"sixel" => Bool,
    ITERM2 => b"iterm2" => Bool,
    KITTY => b"kitty" => TEXT_KITTY, [TK => b"TK"],
    SETRGBF => b"setrgbf" => TEXT_SETRGBF,
    SETRGBB => b"setrgbb" => TEXT_SETRGBB,
    SETAF => b"setaf" => TEXT_SETAF,
    SETAB => b"setab" => TEXT_SETAB,
    OP => b"op" => TEXT_OP,
    OC => b"oc" => TEXT_OC,
    SMULX => b"Smulx" => TEXT_SMULX,
    SETULC => b"Setulc" => TEXT_SETULC,
    SS => b"Ss" => TEXT_SS,
    SE => b"Se" => TEXT_SE,
    CS => b"Cs" => TEXT_CS,
    CR => b"Cr" => TEXT_CR,
    SYNC => b"sync" => TEXT_SYNC,
    E3 => b"E3" => TEXT_E3,
    CLEAR => b"clear" => TEXT_CLEAR, [CL => b"cl"],
    CUP => b"cup" => TEXT_CUP, [CM => b"cm"],
    SMKX => b"smkx" => TEXT_SMKX, [KS => b"ks"],
    RMKX => b"rmkx" => TEXT_RMKX, [KE => b"ke"],
    SMCUP => b"smcup" => TEXT_SMCUP, [TI => b"ti"],
    RMCUP => b"rmcup" => TEXT_RMCUP, [TE => b"te"],
    SMACS => b"smacs" => TEXT_SMACS, [AS => b"as"],
    RMACS => b"rmacs" => TEXT_RMACS, [AE => b"ae"],
    ACSC => b"acsc" => TEXT_ACSC,
    CIVIS => b"civis" => TEXT_CIVIS, [VI => b"vi"],
    CNORM => b"cnorm" => TEXT_CNORM, [VE => b"ve"],
    KBS => b"kbs" => TEXT_KBS,
    KDCH1 => b"kdch1" => TEXT_KDCH1,
    KICH1 => b"kich1" => TEXT_KICH1,
    KHOME => b"khome" => TEXT_KHOME,
    KEND => b"kend" => TEXT_KEND,
    KCUU1 => b"kcuu1" => TEXT_KCUU1,
    KCUD1 => b"kcud1" => TEXT_KCUD1,
    KCUB1 => b"kcub1" => TEXT_KCUB1,
    KCUF1 => b"kcuf1" => TEXT_KCUF1,
    INDN => b"indn" => TEXT_INDN,
    QUERY_OS_NAME => b"query-os-name" => OsName,
}

impl crate::Reply {
    /// Build an XTGETTCAP capability query answer (`DCS + q <hex cap> ST`).
    #[must_use]
    pub fn termcap(
        cap: &[u8],
        cols: usize,
        rows: usize,
        tn: &[u8],
        os_name: &[u8],
    ) -> CompactString {
        use crate::reply::{xtgettcap, xtgettcap_num_append};

        let mut buf = CompactString::default();
        for part in cap.split(|&b| b == b';') {
            if part.is_empty() {
                continue;
            }
            let (key, value) = Cap::parse(part);
            match value {
                CapValue::None => {
                    xtgettcap(&mut buf, false, Cap::UNSUPPORTED, None);
                }
                CapValue::Bool => xtgettcap(&mut buf, true, key, None),
                CapValue::Text(v) => xtgettcap(&mut buf, true, key, Some(v)),
                CapValue::Cols => xtgettcap_num_append(&mut buf, key, cols),
                CapValue::Rows => xtgettcap_num_append(&mut buf, key, rows),
                CapValue::OsName if os_name.is_empty() => {
                    xtgettcap(&mut buf, false, Cap::UNSUPPORTED, None);
                }
                CapValue::OsName => {
                    xtgettcap(&mut buf, true, key, Some(os_name));
                }
                CapValue::Tn => xtgettcap(&mut buf, true, key, Some(tn)),
            };
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reply;

    #[test]
    fn parse_aliases_resolve_to_canonical() {
        let (key, value) = Cap::parse(b"colors");
        assert_eq!(key, Cap::CO);
        assert!(matches!(value, CapValue::Text(v) if v == &b"256"[..]));

        let (key, value) = Cap::parse(b"cl");
        assert_eq!(key, Cap::CLEAR);
        assert!(matches!(value, CapValue::Text(v) if v == &b"\x1b[H\x1b[2J"[..]));

        let (key, value) = Cap::parse(b"nope");
        assert_eq!(key, Cap::NONE);
        assert!(matches!(value, CapValue::None));
    }

    #[test]
    fn termcap_replies_for_each_value_kind() {
        assert_eq!(
            Reply::termcap(b"RGB", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r524742=382f382f38\x1b\\"
        );
        assert_eq!(
            Reply::termcap(b"am", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r616d\x1b\\"
        );
        assert_eq!(
            Reply::termcap(b"co", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r636f=3830\x1b\\"
        );
        assert_eq!(
            Reply::termcap(b"li", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r6c69=3234\x1b\\"
        );
        assert_eq!(
            Reply::termcap(b"nope", 80, 24, b"jetfish", b"macos"),
            "\x1bP0+r756e737570706f72746564\x1b\\"
        );
    }

    #[test]
    fn termcap_os_name_answers_hex_os() {
        assert_eq!(
            Reply::termcap(b"query-os-name", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r71756572792d6f732d6e616d65=6d61636f73\x1b\\"
        );
        // No OS name to advertise: answer the capability as unsupported.
        assert_eq!(
            Reply::termcap(b"query-os-name", 80, 24, b"jetfish", b""),
            "\x1bP0+r756e737570706f72746564\x1b\\"
        );
    }

    #[test]
    fn termcap_answers_packed_query_in_order() {
        assert_eq!(
            Reply::termcap(b"RGB;am;co", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r524742=382f382f38\x1b\\\x1bP1+r616d\x1b\\\x1bP1+r636f=3830\x1b\\"
        );
    }

    #[test]
    fn termcap_skips_empty_parts() {
        assert_eq!(
            Reply::termcap(b";;RGB;;", 80, 24, b"jetfish", b"macos"),
            "\x1bP1+r524742=382f382f38\x1b\\"
        );
    }
}
