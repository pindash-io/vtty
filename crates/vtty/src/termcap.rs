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

impl Cap {
    /// `b"TN"`.
    pub const TN: &'static [u8] = b"TN";
    /// `b"Co"`.
    pub const CO: &'static [u8] = b"Co";
    /// `b"pa"`.
    pub const PA: &'static [u8] = b"pa";
    /// `b"RGB"`.
    pub const RGB: &'static [u8] = b"RGB";
    /// `b"Tc"` (truecolor boolean).
    pub const TC: &'static [u8] = b"Tc";
    /// `b"co"`.
    pub const COLS: &'static [u8] = b"co";
    /// `b"li"`.
    pub const ROWS: &'static [u8] = b"li";
    /// `b"it"`.
    pub const IT: &'static [u8] = b"it";
    /// `b"am"`.
    pub const AM: &'static [u8] = b"am";
    /// `b"bce"`.
    pub const BCE: &'static [u8] = b"bce";
    /// `b"ccc"`.
    pub const CCC: &'static [u8] = b"ccc";
    /// `b"xenl"`.
    pub const XENL: &'static [u8] = b"xenl";
    /// `b"xn"`.
    pub const XN: &'static [u8] = b"xn";
    /// `b"XT"`.
    pub const XT: &'static [u8] = b"XT";
    /// `b"XF"`.
    pub const XF: &'static [u8] = b"XF";
    /// `b"sixel"`.
    pub const SIXEL: &'static [u8] = b"sixel";
    /// `b"iterm2"`.
    pub const ITERM2: &'static [u8] = b"iterm2";
    /// `b"kitty"`.
    pub const KITTY: &'static [u8] = b"kitty";
    /// `b"setaf"`.
    pub const SETAF: &'static [u8] = b"setaf";
    /// `b"setab"`.
    pub const SETAB: &'static [u8] = b"setab";
    /// `b"op"`.
    pub const OP: &'static [u8] = b"op";
    /// `b"oc"`.
    pub const OC: &'static [u8] = b"oc";
    /// `b"Smulx"`.
    pub const SMULX: &'static [u8] = b"Smulx";
    /// `b"Setulc"` (set underline color).
    pub const SETULC: &'static [u8] = b"Setulc";
    /// `b"Ss"`.
    pub const SS: &'static [u8] = b"Ss";
    /// `b"Se"`.
    pub const SE: &'static [u8] = b"Se";
    /// `b"Cs"`.
    pub const CS: &'static [u8] = b"Cs";
    /// `b"Cr"`.
    pub const CR: &'static [u8] = b"Cr";
    /// `b"E3"`.
    pub const E3: &'static [u8] = b"E3";
    /// `b"clear"`.
    pub const CLEAR: &'static [u8] = b"clear";
    /// `b"cup"`.
    pub const CUP: &'static [u8] = b"cup";
    /// `b"smkx"`.
    pub const SMKX: &'static [u8] = b"smkx";
    /// `b"rmkx"`.
    pub const RMKX: &'static [u8] = b"rmkx";
    /// `b"smcup"`.
    pub const SMCUP: &'static [u8] = b"smcup";
    /// `b"rmcup"`.
    pub const RMCUP: &'static [u8] = b"rmcup";
    /// `b"smacs"`.
    pub const SMACS: &'static [u8] = b"smacs";
    /// `b"rmacs"`.
    pub const RMACS: &'static [u8] = b"rmacs";
    /// `b"civis"`.
    pub const CIVIS: &'static [u8] = b"civis";
    /// `b"cnorm"`.
    pub const CNORM: &'static [u8] = b"cnorm";
    /// `b"kbs"`.
    pub const KBS: &'static [u8] = b"kbs";
    /// `b"kdch1"`.
    pub const KDCH1: &'static [u8] = b"kdch1";
    /// `b"kich1"`.
    pub const KICH1: &'static [u8] = b"kich1";
    /// `b"khome"`.
    pub const KHOME: &'static [u8] = b"khome";
    /// `b"kend"`.
    pub const KEND: &'static [u8] = b"kend";
    /// `b"kcuu1"`.
    pub const KCUU1: &'static [u8] = b"kcuu1";
    /// `b"kcud1"`.
    pub const KCUD1: &'static [u8] = b"kcud1";
    /// `b"kcub1"`.
    pub const KCUB1: &'static [u8] = b"kcub1";
    /// `b"kcuf1"`.
    pub const KCUF1: &'static [u8] = b"kcuf1";
    /// `b"indn"`.
    pub const INDN: &'static [u8] = b"indn";
    /// `b"query-os-name"`.
    pub const QUERY_OS_NAME: &'static [u8] = b"query-os-name";
    /// `b""`.
    pub const NONE: &'static [u8] = b"";
    /// `b"unsupported"`.
    pub const UNSUPPORTED: &'static [u8] = b"unsupported";
    /// `b"setrgbf"`.
    pub const SETRGBF: &'static [u8] = b"setrgbf";
    /// `b"setrgbb"`.
    pub const SETRGBB: &'static [u8] = b"setrgbb";
    /// `b"sync"`.
    pub const SYNC: &'static [u8] = b"sync";
    /// `b"acsc"`.
    pub const ACSC: &'static [u8] = b"acsc";
    /// `b"name"`.
    pub const NAME: &'static [u8] = b"name";
    /// `b"colors"`.
    pub const COLORS: &'static [u8] = b"colors";
    /// `b"pairs"`.
    pub const PAIRS: &'static [u8] = b"pairs";
    /// `b"cols"`.
    pub const COLUMNS: &'static [u8] = b"cols";
    /// `b"lines"`.
    pub const LINES: &'static [u8] = b"lines";
    /// `b"TK"`.
    pub const TK: &'static [u8] = b"TK";
    /// `b"cl"`.
    pub const CL: &'static [u8] = b"cl";
    /// `b"cm"`.
    pub const CM: &'static [u8] = b"cm";
    /// `b"ks"`.
    pub const KS: &'static [u8] = b"ks";
    /// `b"ke"`.
    pub const KE: &'static [u8] = b"ke";
    /// `b"ti"`.
    pub const TI: &'static [u8] = b"ti";
    /// `b"te"`.
    pub const TE: &'static [u8] = b"te";
    /// `b"as"`.
    pub const AS: &'static [u8] = b"as";
    /// `b"ae"`.
    pub const AE: &'static [u8] = b"ae";
    /// `b"vi"`.
    pub const VI: &'static [u8] = b"vi";
    /// `b"ve"`.
    pub const VE: &'static [u8] = b"ve";

    /// Parse a query name into a capability (canonical key + value),
    /// accepting the long/short terminfo aliases. Unknown names map to
    /// [`CapValue::None`].
    pub fn parse(query: &[u8]) -> (&'static [u8], CapValue) {
        match query {
            Self::TN | Self::NAME => (Self::TN, CapValue::TEXT_TN),
            Self::CO | Self::COLORS => (Self::CO, CapValue::TEXT_CO),
            Self::PA | Self::PAIRS => (Self::PA, CapValue::TEXT_PA),
            Self::RGB => (Self::RGB, CapValue::TEXT_RGB),
            Self::TC => (Self::TC, CapValue::Bool),
            Self::COLS | Self::COLUMNS => (Self::COLS, CapValue::Cols),
            Self::ROWS | Self::LINES => (Self::ROWS, CapValue::Rows),
            Self::IT => (Self::IT, CapValue::TEXT_IT),
            Self::AM => (Self::AM, CapValue::Bool),
            Self::BCE => (Self::BCE, CapValue::Bool),
            Self::CCC => (Self::CCC, CapValue::Bool),
            Self::XENL | Self::XN => (Self::XN, CapValue::Bool),
            Self::XT => (Self::XT, CapValue::Bool),
            Self::XF => (Self::XF, CapValue::Bool),
            Self::SIXEL => (Self::SIXEL, CapValue::Bool),
            Self::ITERM2 => (Self::ITERM2, CapValue::Bool),
            Self::KITTY | Self::TK => (Self::KITTY, CapValue::TEXT_KITTY),
            Self::SETRGBF => (Self::SETRGBF, CapValue::TEXT_SETRGBF),
            Self::SETRGBB => (Self::SETRGBB, CapValue::TEXT_SETRGBB),
            Self::SETAF => (Self::SETAF, CapValue::TEXT_SETAF),
            Self::SETAB => (Self::SETAB, CapValue::TEXT_SETAB),
            Self::OP => (Self::OP, CapValue::TEXT_OP),
            Self::OC => (Self::OC, CapValue::TEXT_OC),
            Self::SMULX => (Self::SMULX, CapValue::TEXT_SMULX),
            Self::SETULC => (Self::SETULC, CapValue::TEXT_SETULC),
            Self::SS => (Self::SS, CapValue::TEXT_SS),
            Self::SE => (Self::SE, CapValue::TEXT_SE),
            Self::CS => (Self::CS, CapValue::TEXT_CS),
            Self::CR => (Self::CR, CapValue::TEXT_CR),
            Self::SYNC => (Self::SYNC, CapValue::TEXT_SYNC),
            Self::E3 => (Self::E3, CapValue::TEXT_E3),
            Self::CLEAR | Self::CL => (Self::CLEAR, CapValue::TEXT_CLEAR),
            Self::CUP | Self::CM => (Self::CUP, CapValue::TEXT_CUP),
            Self::SMKX | Self::KS => (Self::SMKX, CapValue::TEXT_SMKX),
            Self::RMKX | Self::KE => (Self::RMKX, CapValue::TEXT_RMKX),
            Self::SMCUP | Self::TI => (Self::SMCUP, CapValue::TEXT_SMCUP),
            Self::RMCUP | Self::TE => (Self::RMCUP, CapValue::TEXT_RMCUP),
            Self::SMACS | Self::AS => (Self::SMACS, CapValue::TEXT_SMACS),
            Self::RMACS | Self::AE => (Self::RMACS, CapValue::TEXT_RMACS),
            Self::ACSC => (Self::ACSC, CapValue::TEXT_ACSC),
            Self::CIVIS | Self::VI => (Self::CIVIS, CapValue::TEXT_CIVIS),
            Self::CNORM | Self::VE => (Self::CNORM, CapValue::TEXT_CNORM),
            Self::KBS => (Self::KBS, CapValue::TEXT_KBS),
            Self::KDCH1 => (Self::KDCH1, CapValue::TEXT_KDCH1),
            Self::KICH1 => (Self::KICH1, CapValue::TEXT_KICH1),
            Self::KHOME => (Self::KHOME, CapValue::TEXT_KHOME),
            Self::KEND => (Self::KEND, CapValue::TEXT_KEND),
            Self::KCUU1 => (Self::KCUU1, CapValue::TEXT_KCUU1),
            Self::KCUD1 => (Self::KCUD1, CapValue::TEXT_KCUD1),
            Self::KCUB1 => (Self::KCUB1, CapValue::TEXT_KCUB1),
            Self::KCUF1 => (Self::KCUF1, CapValue::TEXT_KCUF1),
            Self::INDN => (Self::INDN, CapValue::TEXT_INDN),
            Self::QUERY_OS_NAME => (Self::QUERY_OS_NAME, CapValue::OsName),
            _ => (Self::NONE, CapValue::None),
        }
    }
}
impl crate::Reply {
    /// Answer an XTGETTCAP capability query (`DCS + q <hex cap> ST`),
    /// appending the `DCS … ST` reply. The query may pack several
    /// capabilities, semicolon-separated (nvim/herdr ask for
    /// `Tc;RGB;setrgbf;setrgbb` in one request); each is answered in order.
    pub fn termcap(self, cap: &[u8], cols: usize, rows: usize) -> Self {
        let mut reply = self;
        for part in cap.split(|&b| b == b';') {
            if part.is_empty() {
                continue;
            }
            let (key, value) = Cap::parse(part);
            reply = match value {
                CapValue::None => reply.xtgettcap_error(Cap::UNSUPPORTED),
                CapValue::Bool => reply.xtgettcap_reply_bool(key),
                CapValue::Text(v) => reply.xtgettcap_reply(key, v),
                CapValue::Cols => reply.xtgettcap_num(key, cols),
                CapValue::Rows => reply.xtgettcap_num(key, rows),
                CapValue::OsName => reply.xtgettcap_reply(key, OS_NAME),
            };
        }
        reply
    }
}
