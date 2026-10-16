// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Program Status Protocol (`OSC 7501`) — parse and encode.
//!
//! A program reports what it is doing (idle, working, done, blocked, error,
//! or clear) to the terminal with:
//!
//! ```text
//! ESC ] 7501 ; state=working:app=brew:msg=<base64> ESC \
//! ```
//!
//! The on-wire form is `ESC ] 7501 ; body ESC \`:
//! - `ESC ]` (`0x1b 0x5d`) — the OSC introducer.
//! - `7501` — the program-status protocol number.
//! - `;` — the OSC parameter separator.
//! - `body` — `key=value` pairs joined by `:`.
//! - `ESC \` (`0x1b 0x5c`) — the string terminator.
//!
//! [`ProgramStatus`] parses the body into a typed record;
//! [`ProgramStatus::write_osc`] encodes one back into the full escape sequence.
//!
//! Reference: <https://www.superlogical.com/rex/docs/build/program-status>.

use alloc::string::String;
use alloc::vec::Vec;

use base64::Engine;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use smol_str::SmolStr;

use crate::perform::Perform;

/// Report body length limit (bytes).
const MAX_SEQUENCE: usize = 4096;
/// `key` length limit (bytes).
const MAX_KEY: usize = 16;
/// `msg` limits: on-wire base64, then decoded bytes.
const MAX_MSG_ENCODED: usize = 2732;
const MAX_MSG_DECODED: usize = 2048;
/// `title` limits: on-wire base64, then decoded bytes.
const MAX_TITLE_ENCODED: usize = 256;
const MAX_TITLE_DECODED: usize = 192;
/// `app` length limit (bytes).
const MAX_APP: usize = 32;
/// `id` limits: total length, per segment, and depth.
const MAX_ID: usize = 128;
const MAX_ID_SEGMENT: usize = 32;
const MAX_ID_DEPTH: usize = 8;

// The wire form: `SEQ_OSC` `OSC_NUM` `;` `key=value`… `SEQ_ST`.
/// OSC program-status protocol number.
const OSC_NUM: &[u8] = b"7501";

/// OSC introducer sequence: `ESC ]` (`0x1b 0x5d`).
const SEQ_OSC: &[u8] = b"\x1b]";
/// String terminator sequence: `ESC \` (`0x1b 0x5c`).
const SEQ_ST: &[u8] = b"\x1b\\";

/// Implements `parse` (name → variant) and `as_bytes` (variant → name) for a
/// fieldless enum whose variants each have a fixed lowercase protocol name.
macro_rules! named_bytes {
    ($ty:ident, $($variant:ident => $name:literal),+ $(,)?) => {
        impl $ty {
            fn parse(value: &[u8]) -> Option<Self> {
                Some(match value {
                    $($name => Self::$variant),+,
                    _ => return None,
                })
            }

            fn as_bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => $name),+
                }
            }
        }
    };
}

/// The state a program reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum StatusState {
    /// At rest, waiting for the user's next instruction.
    #[default]
    Idle = 0,
    /// Running. May carry `progress`.
    Working = 1,
    /// Finished a piece of work; the user has not seen the result yet.
    Done = 2,
    /// Cannot continue until the user does something.
    Blocked = 3,
    /// Failed and stopped.
    Error = 4,
    /// Not a state: clears the addressed record (and its children).
    Clear = 5,
}

named_bytes!(
    StatusState,
    Idle => b"idle",
    Working => b"working",
    Done => b"done",
    Blocked => b"blocked",
    Error => b"error",
    Clear => b"clear",
);

/// What a `blocked` program is waiting for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum StatusKind {
    /// Approval to do something.
    #[default]
    Permission = 0,
    /// The user must type an answer.
    Question = 1,
    /// A login, token, or credential.
    Auth = 2,
}

named_bytes!(
    StatusKind,
    Permission => b"permission",
    Question => b"question",
    Auth => b"auth",
);

/// A parsed OSC 7501 report.
///
/// A report without `id` addresses the root record; each report replaces its
/// record completely.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramStatus {
    /// The reported state. Required by the protocol.
    pub state: StatusState,
    /// Optional record path (`a/b/c`); absent means the root record.
    pub id: Option<SmolStr>,
    /// Why the program is `blocked`; ignored for other states.
    pub kind: Option<StatusKind>,
    /// Progress in `0..=100`; absent means indeterminate.
    pub progress: Option<u8>,
    /// Stable program identifier (`cargo`, `terraform`, …).
    pub app: Option<SmolStr>,
    /// Short human-readable label.
    pub title: Option<SmolStr>,
    /// One human-readable line about what the record is doing.
    pub msg: Option<SmolStr>,
}

impl ProgramStatus {
    /// Feature-detection query: `ESC ] 7501 ; ? ESC \` — the `?` asks the
    /// terminal whether it supports the protocol.
    pub const QUERY: &[u8] = b"\x1b]7501;?\x1b\\";

    /// Parses an OSC 7501 report body (the `key=value` pairs).
    ///
    /// Returns `None` if the report is invalid and must be discarded whole:
    /// missing/invalid `state`, malformed `id`, any limit exceeded, bad
    /// base64, or control characters in a decoded `msg`/`title`.
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() > MAX_SEQUENCE {
            return None;
        }
        let mut state = None;
        let mut id = None;
        let mut kind = None;
        let mut progress = None;
        let mut app = None;
        let mut title = None;
        let mut msg = None;

        // Malformed pairs are skipped (`split_pair` → `None`); a hard error in
        // a recognized field (`?`/`return None`) discards the whole report.
        for (key, value) in payload.split(|&b| b == b':').filter_map(split_pair) {
            match key {
                b"state" => state = StatusState::parse(value),
                b"id" => id = Some(parse_id(value)?),
                b"kind" => kind = StatusKind::parse(value),
                b"progress" => progress = parse_progress(value),
                b"app" => {
                    if value.is_empty() || !value.iter().all(|&b| is_ident_byte(b)) {
                        app = None; // outside the character set: treated as absent
                    } else if value.len() > MAX_APP {
                        return None; // limit exceeded
                    } else {
                        app = core::str::from_utf8(value).ok().map(SmolStr::from);
                    }
                }
                b"title" => {
                    title = Some(parse_text(value, MAX_TITLE_ENCODED, MAX_TITLE_DECODED)?);
                }
                b"msg" => {
                    msg = Some(parse_text(value, MAX_MSG_ENCODED, MAX_MSG_DECODED)?);
                }
                _ => {} // unknown key
            }
        }

        let state = state?;
        // `kind` only applies to blocked; `progress` only to working/blocked.
        let kind = if state == StatusState::Blocked {
            kind
        } else {
            None
        };
        let progress = if matches!(state, StatusState::Working | StatusState::Blocked) {
            progress
        } else {
            None
        };

        Some(Self {
            state,
            id,
            kind,
            progress,
            app,
            title,
            msg,
        })
    }

    /// Parses a report from an OSC dispatch's params (`params[0] == "7501"`).
    pub fn from_osc_params(params: &[&[u8]]) -> Option<Self> {
        if params.first().is_none_or(|p| *p != OSC_NUM) {
            return None;
        }
        Self::parse(params.get(1)?)
    }

    /// The full `OSC 7501 ; pairs ST` sequence.
    pub fn to_osc(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write_osc(&mut out);
        out
    }

    /// Appends the full `OSC 7501 ; pairs ST` sequence to `out`.
    pub fn write_osc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(SEQ_OSC);
        out.extend_from_slice(OSC_NUM);
        out.push(b';'); // OSC parameter separator
        push_field(out, b"state", self.state.as_bytes());
        if let Some(id) = &self.id {
            push_field(out, b"id", id.as_bytes());
        }
        if let Some(kind) = self.kind {
            push_field(out, b"kind", kind.as_bytes());
        }
        if let Some(progress) = self.progress {
            let mut buf = [0_u8; 3];
            push_field(out, b"progress", write_decimal(progress, &mut buf));
        }
        if let Some(app) = &self.app {
            push_field(out, b"app", app.as_bytes());
        }
        if let Some(title) = &self.title {
            push_field(out, b"title", B64.encode(title.as_bytes()).as_bytes());
        }
        if let Some(msg) = &self.msg {
            push_field(out, b"msg", B64.encode(msg.as_bytes()).as_bytes());
        }
        out.extend_from_slice(SEQ_ST);
    }
}

/// Extension for [`Perform`] sinks that want typed OSC 7501 reports.
pub trait PerformProgramStatus: Perform {
    /// Called with each valid OSC 7501 report.
    fn program_status(&mut self, _status: ProgramStatus) {}

    /// Routes one OSC dispatch.
    ///
    /// Call this first in [`Perform::osc_dispatch`]. Returns `true` when the
    /// params were a valid OSC 7501 report (and [`Self::program_status`] was
    /// called).
    fn route_osc_7501(&mut self, params: &[&[u8]]) -> bool {
        if let Some(status) = ProgramStatus::from_osc_params(params) {
            self.program_status(status);
            true
        } else {
            false
        }
    }
}

/// A byte allowed in a `value` (`[A-Za-z0-9_.,+/=-]`).
fn is_value_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
}

/// A byte allowed in an `app` or `id` segment (`[A-Za-z0-9_.+-]`).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-')
}

/// Splits a `key=value` pair at the first `=`. Returns `None` for a malformed
/// pair: no `=`, an empty/over-long/non-lowercase key, or a forbidden value
/// byte.
fn split_pair(pair: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut kv = pair.splitn(2, |&b| b == b'=');
    let key = kv.next()?;
    let value = kv.next()?;
    if key.is_empty() || key.len() > MAX_KEY {
        return None;
    }
    if !key.iter().all(u8::is_ascii_lowercase) {
        return None;
    }
    if !value.iter().all(|&b| is_value_byte(b)) {
        return None;
    }
    Some((key, value))
}

/// Parses a decimal `progress` value in `0..=100`.
fn parse_progress(value: &[u8]) -> Option<u8> {
    if value.is_empty() || value.len() > 3 || !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n = core::str::from_utf8(value).ok()?.parse::<u8>().ok()?;
    (n <= 100).then_some(n)
}

/// Parses an `id` path (`a/b/c`), enforcing the length, segment and depth
/// limits.
fn parse_id(value: &[u8]) -> Option<SmolStr> {
    if value.is_empty() || value.len() > MAX_ID {
        return None;
    }
    let depth = value.iter().filter(|&&b| b == b'/').count() + 1;
    if depth > MAX_ID_DEPTH {
        return None;
    }
    for segment in value.split(|&b| b == b'/') {
        if segment.is_empty() || segment.len() > MAX_ID_SEGMENT {
            return None;
        }
        if !segment.iter().all(|&b| is_ident_byte(b)) {
            return None;
        }
    }
    Some(SmolStr::from(core::str::from_utf8(value).ok()?))
}

/// A control-character class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    /// C0 controls: `U+0000..=U+001F`.
    C0,
    /// DEL: `U+007F`.
    Del,
    /// C1 controls: `U+0080..=U+009F`.
    C1,
}

impl Control {
    /// Classifies `c` as a control character, or `None`.
    fn of(c: char) -> Option<Self> {
        Some(match u32::from(c) {
            0x00..=0x1f => Self::C0,
            0x7f => Self::Del,
            0x80..=0x9f => Self::C1,
            _ => return None,
        })
    }
}

/// Decodes a base64 `title`/`msg` value, enforcing the encoded/decoded limits
/// and rejecting control characters.
fn parse_text(value: &[u8], max_encoded: usize, max_decoded: usize) -> Option<SmolStr> {
    if value.len() > max_encoded {
        return None;
    }
    let decoded = B64.decode(value).ok()?;
    if decoded.len() > max_decoded {
        return None;
    }
    let text = String::from_utf8(decoded).ok()?;
    // Refuse control characters (C0, DEL, C1).
    if text.chars().any(|c| Control::of(c).is_some()) {
        return None;
    }
    Some(SmolStr::from(text))
}

/// Standard base64; decoding accepts optional padding (per the protocol).
const B64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Appends `:key=value` to `out`.
fn push_field(out: &mut Vec<u8>, key: &[u8], value: &[u8]) {
    out.push(b':');
    out.extend_from_slice(key);
    out.push(b'=');
    out.extend_from_slice(value);
}

/// Writes `n` (`0..=255`) as a decimal into `buf`, returning the written part.
fn write_decimal(mut n: u8, buf: &mut [u8; 3]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + n % 10;
        n /= 10;
        if n == 0 {
            return &buf[i..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(payload: &[u8]) -> ProgramStatus {
        match ProgramStatus::parse(payload) {
            Some(status) => status,
            None => unreachable!("failed to parse"),
        }
    }

    #[test]
    fn parse_minimal() {
        let status = parsed(b"state=working");
        assert_eq!(status.state, StatusState::Working);
        assert_eq!(status.id, None);
        assert_eq!(status.kind, None);
        assert_eq!(status.progress, None);
        assert_eq!(status.app, None);
        assert_eq!(status.title, None);
        assert_eq!(status.msg, None);
    }

    #[test]
    fn parse_full() {
        let status = parsed(
            b"state=blocked:kind=permission:id=build/test:progress=40:app=terraform:title=SGVsbG8=:msg=QXBwcm92ZQ==",
        );
        assert_eq!(status.state, StatusState::Blocked);
        assert_eq!(status.kind, Some(StatusKind::Permission));
        assert_eq!(status.id.as_deref(), Some("build/test"));
        assert_eq!(status.progress, Some(40));
        assert_eq!(status.app.as_deref(), Some("terraform"));
        assert_eq!(status.title.as_deref(), Some("Hello"));
        assert_eq!(status.msg.as_deref(), Some("Approve"));
    }

    #[test]
    fn parse_requires_state() {
        assert_eq!(ProgramStatus::parse(b"app=brew"), None);
    }

    #[test]
    fn parse_invalid_state() {
        assert_eq!(ProgramStatus::parse(b"state=bogus"), None);
    }

    #[test]
    fn parse_skips_malformed_and_unknown() {
        let status = parsed(b"state=idle:=bad:unknown=1:app=brew");
        assert_eq!(status.state, StatusState::Idle);
        assert_eq!(status.app.as_deref(), Some("brew"));
    }

    #[test]
    fn parse_repeated_key_last_wins() {
        let status = parsed(b"state=working:state=done");
        assert_eq!(status.state, StatusState::Done);
    }

    #[test]
    fn parse_kind_ignored_without_blocked() {
        let status = parsed(b"state=working:kind=auth");
        assert_eq!(status.kind, None);
    }

    #[test]
    fn parse_progress_out_of_range() {
        let status = parsed(b"state=working:progress=101");
        assert_eq!(status.progress, None);
    }

    #[test]
    fn parse_id_invalid_discards_report() {
        assert_eq!(ProgramStatus::parse(b"state=working:id=a//b"), None);
    }

    #[test]
    fn parse_app_invalid_charset_absent() {
        let status = parsed(b"state=idle:app=foo/bar");
        assert_eq!(status.app, None);
    }

    #[test]
    fn parse_msg_control_char_discards_report() {
        // "a\nb" contains a control character.
        let msg = B64.encode(b"a\nb");
        let mut payload = Vec::from(b"state=done:msg=".as_slice());
        payload.extend_from_slice(msg.as_bytes());
        assert_eq!(ProgramStatus::parse(&payload), None);
    }

    #[test]
    fn split_pair_valid() {
        assert_eq!(
            split_pair(b"state=idle"),
            Some((&b"state"[..], &b"idle"[..]))
        );
        // `=` in the value (base64 padding) stays in the value.
        assert_eq!(split_pair(b"msg=Zm9v="), Some((&b"msg"[..], &b"Zm9v="[..])));
    }

    #[test]
    fn split_pair_invalid() {
        assert_eq!(split_pair(b"no_equals"), None);
        assert_eq!(split_pair(b"=value"), None); // empty key
        assert_eq!(split_pair(b"State=idle"), None); // non-lowercase key
        assert_eq!(split_pair(b"key=va lue"), None); // space is not a value byte
    }

    #[test]
    fn control_classification() {
        assert_eq!(Control::of('\0'), Some(Control::C0));
        assert_eq!(Control::of('\u{1f}'), Some(Control::C0));
        assert_eq!(Control::of('\u{7f}'), Some(Control::Del));
        assert_eq!(Control::of('\u{80}'), Some(Control::C1));
        assert_eq!(Control::of('\u{9f}'), Some(Control::C1));
        assert_eq!(Control::of('a'), None);
        assert_eq!(Control::of('\u{a0}'), None);
    }

    #[test]
    fn write_decimal_digits() {
        let mut buf = [0_u8; 3];
        assert_eq!(write_decimal(0, &mut buf), b"0");
        assert_eq!(write_decimal(7, &mut buf), b"7");
        assert_eq!(write_decimal(40, &mut buf), b"40");
        assert_eq!(write_decimal(100, &mut buf), b"100");
        assert_eq!(write_decimal(255, &mut buf), b"255");
    }

    #[test]
    fn from_osc_params() {
        let params: &[&[u8]] = &[b"7501", b"state=idle"];
        assert_eq!(
            ProgramStatus::from_osc_params(params),
            Some(ProgramStatus::default())
        );
        let bad: &[&[u8]] = &[b"9999", b"state=idle"];
        assert_eq!(ProgramStatus::from_osc_params(bad), None);
    }

    #[test]
    fn write_osc_roundtrip() {
        let status = ProgramStatus {
            state: StatusState::Working,
            id: Some(SmolStr::from("build/test")),
            kind: None,
            progress: Some(40),
            app: Some(SmolStr::from("terraform")),
            title: Some(SmolStr::from("US East")),
            msg: Some(SmolStr::from("Deploying v2")),
        };
        let osc = status.to_osc();
        assert!(osc.starts_with(b"\x1b]7501;"));
        assert!(osc.ends_with(b"\x1b\\"));
        // Body is between the 7-byte prefix and the 2-byte ST.
        let body = &osc[7..osc.len() - 2];
        assert_eq!(parsed(body), status);
    }

    struct StatusSink {
        status: Option<ProgramStatus>,
    }

    impl Perform for StatusSink {}

    impl PerformProgramStatus for StatusSink {
        fn program_status(&mut self, status: ProgramStatus) {
            self.status = Some(status);
        }
    }

    #[test]
    fn route_osc_7501() {
        let mut sink = StatusSink { status: None };
        let params: &[&[u8]] = &[b"7501", b"state=working:app=brew"];
        assert!(sink.route_osc_7501(params));
        let status = match sink.status {
            Some(status) => status,
            None => unreachable!("expected a routed status"),
        };
        assert_eq!(status.state, StatusState::Working);
        assert_eq!(status.app.as_deref(), Some("brew"));

        // A non-7501 OSC is not routed.
        let mut sink = StatusSink { status: None };
        let params: &[&[u8]] = &[b"0", b"title"];
        assert!(!sink.route_osc_7501(params));
        assert_eq!(sink.status, None);
    }
}
