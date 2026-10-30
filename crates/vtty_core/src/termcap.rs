// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! XTGETTCAP terminfo capability table.
//!
//! Answers the `DCS + q <hex cap> ST` capability query (xterm XTGETTCAP) with
//! the terminal's advertised terminfo values. The reply formatting lives in
//! [`Reply`](crate::Reply); this module maps each query name to a [`Cap`]
//! variant (handling aliases) and that variant to its canonical reply key +
//! value — mirroring Rio's `misc/rio.termcap`/`rio.terminfo` (the subset real
//! apps probe: nvim `RGB`/`setrgbf`/`Smulx`/`Ss`/`sync`, fish
//! `query-os-name`, tmux `setaf`/`setab`).

/// OS name for the non-standard `query-os-name` capability. `cfg!` is
/// `no_std`-safe (unlike `std::env::consts::OS`).
const OS_NAME: &[u8] = if cfg!(target_os = "macos") {
    b"macos"
} else if cfg!(target_os = "linux") {
    b"linux"
} else if cfg!(target_os = "windows") {
    b"windows"
} else if cfg!(target_os = "freebsd") {
    b"freebsd"
} else {
    b"unknown"
};

/// The reply value for a capability.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum CapValue {
    /// Unknown / unsupported — reply is the `DCS 0 + r` error.
    None,
    /// Boolean capability — reply is a bare `key` (no `=value`).
    Bool,
    /// String capability — reply is `key=value`.
    Text(&'static [u8]),
    /// The terminal column count (`co`).
    Cols,
    /// The terminal row count (`li`).
    Rows,
    /// The OS name (`query-os-name`).
    OsName,
}

impl CapValue {
    /// Text value for `TN`.
    pub const TEXT_TN: Self = Self::Text(b"jetfish");
    /// Text value for `CO`.
    pub const TEXT_CO: Self = Self::Text(b"256");
    /// Text value for `PA`.
    pub const TEXT_PA: Self = Self::Text(b"32767");
    /// Text value for `RGB`.
    pub const TEXT_RGB: Self = Self::Text(b"8/8/8");
    /// Text value for `IT`.
    pub const TEXT_IT: Self = Self::Text(b"8");
    /// Text value for `KITTY`.
    pub const TEXT_KITTY: Self = Self::Text(b"yes");
    /// Text value for `SETRGBF`.
    pub const TEXT_SETRGBF: Self = Self::Text(b"\x1b[38:2:%p1%d:%p2%d:%p3%dm");
    /// Text value for `SETRGBB`.
    pub const TEXT_SETRGBB: Self = Self::Text(b"\x1b[48:2:%p1%d:%p2%d:%p3%dm");
    /// Text value for `SETAF`.
    pub const TEXT_SETAF: Self = Self::Text(b"\x1b[38;5;%p1%dm");
    /// Text value for `SETAB`.
    pub const TEXT_SETAB: Self = Self::Text(b"\x1b[48;5;%p1%dm");
    /// Text value for `OP`.
    pub const TEXT_OP: Self = Self::Text(b"\x1b[39;49m");
    /// Text value for `OC`.
    pub const TEXT_OC: Self = Self::Text(b"\x1b]104\x07");
    /// Text value for `SMULX`.
    pub const TEXT_SMULX: Self = Self::Text(b"\x1b[4:%p1%dm");
    /// Text value for `SETULC` (set underline color, one packed RGB param).
    pub const TEXT_SETULC: Self =
        Self::Text(b"\x1b[58:2:%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%dm");
    /// Text value for `SS`.
    pub const TEXT_SS: Self = Self::Text(b"\x1b[%p1%d q");
    /// Text value for `SE`.
    pub const TEXT_SE: Self = Self::Text(b"\x1b[0 q");
    /// Text value for `CS`.
    pub const TEXT_CS: Self = Self::Text(b"\x1b]12;%p1%s\x07");
    /// Text value for `CR`.
    pub const TEXT_CR: Self = Self::Text(b"\x1b]112\x07");
    /// Text value for `SYNC`.
    pub const TEXT_SYNC: Self = Self::Text(b"\x1b[?2026%?%p1%{1}%-%tl%eh");
    /// Text value for `E3`.
    pub const TEXT_E3: Self = Self::Text(b"\x1b[3J");
    /// Text value for `CLEAR`.
    pub const TEXT_CLEAR: Self = Self::Text(b"\x1b[H\x1b[2J");
    /// Text value for `CUP`.
    pub const TEXT_CUP: Self = Self::Text(b"\x1b[%i%p1%d;%p2%dH");
    /// Text value for `SMKX`.
    pub const TEXT_SMKX: Self = Self::Text(b"\x1b[?1h\x1b=");
    /// Text value for `RMKX`.
    pub const TEXT_RMKX: Self = Self::Text(b"\x1b[?1l\x1b>");
    /// Text value for `SMCUP`.
    pub const TEXT_SMCUP: Self = Self::Text(b"\x1b[?1049h");
    /// Text value for `RMCUP`.
    pub const TEXT_RMCUP: Self = Self::Text(b"\x1b[?1049l");
    /// Text value for `SMACS`.
    pub const TEXT_SMACS: Self = Self::Text(b"\x1b(0");
    /// Text value for `RMACS`.
    pub const TEXT_RMACS: Self = Self::Text(b"\x1b(B");
    /// Text value for `ACSC`.
    pub const TEXT_ACSC: Self = Self::Text(b"``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~");
    /// Text value for `CIVIS`.
    pub const TEXT_CIVIS: Self = Self::Text(b"\x1b[?25l");
    /// Text value for `CNORM`.
    pub const TEXT_CNORM: Self = Self::Text(b"\x1b[?25h");
    /// Text value for `KBS`.
    pub const TEXT_KBS: Self = Self::Text(b"\x7f");
    /// Text value for `KDCH1`.
    pub const TEXT_KDCH1: Self = Self::Text(b"\x1b[3~");
    /// Text value for `KICH1`.
    pub const TEXT_KICH1: Self = Self::Text(b"\x1b[2~");
    /// Text value for `KHOME`.
    pub const TEXT_KHOME: Self = Self::Text(b"\x1b[H");
    /// Text value for `KEND`.
    pub const TEXT_KEND: Self = Self::Text(b"\x1b[F");
    /// Text value for `KCUU1`.
    pub const TEXT_KCUU1: Self = Self::Text(b"\x1b[A");
    /// Text value for `KCUD1`.
    pub const TEXT_KCUD1: Self = Self::Text(b"\x1b[B");
    /// Text value for `KCUB1`.
    pub const TEXT_KCUB1: Self = Self::Text(b"\x1b[D");
    /// Text value for `KCUF1`.
    pub const TEXT_KCUF1: Self = Self::Text(b"\x1b[C");
    /// Text value for `INDN`.
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
            pub fn parse(query: &[u8]) -> (&'static [u8], CapValue) {
                match query {
                    $(Self::$name $( $(| Self::$alias)+ )* => (Self::$name, CapValue::$value),)+
                    _ => (Self::NONE, CapValue::None),
                }
            }
        }
    };
}

impl Cap {
    /// `b""`.
    pub const NONE: &'static [u8] = b"";
    /// `b"unsupported"`.
    pub const UNSUPPORTED: &'static [u8] = b"unsupported";
}

caps! {
    TN => b"TN" => TEXT_TN, [NAME => b"name"],
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
    /// The query may pack several capabilities, semicolon-separated (nvim/herdr
    /// ask for `Tc;RGB;setrgbf;setrgbb` in one request); each is answered in
    /// order.
    pub fn termcap(cap: &[u8], cols: usize, rows: usize) -> compact_str::CompactString {
        use crate::reply::{xtgettcap, xtgettcap_num_append};

        let mut buf = compact_str::CompactString::default();
        for part in cap.split(|&b| b == b';') {
            if part.is_empty() {
                continue;
            }
            let (key, value) = Cap::parse(part);
            match value {
                CapValue::None => xtgettcap(&mut buf, false, Cap::UNSUPPORTED, None),
                CapValue::Bool => xtgettcap(&mut buf, true, key, None),
                CapValue::Text(v) => xtgettcap(&mut buf, true, key, Some(v)),
                CapValue::Cols => xtgettcap_num_append(&mut buf, key, cols),
                CapValue::Rows => xtgettcap_num_append(&mut buf, key, rows),
                CapValue::OsName => xtgettcap(&mut buf, true, key, Some(OS_NAME)),
            };
        }
        buf
    }
}
