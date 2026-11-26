// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kitty keyboard protocol flags (`CSI > flags u` / `CSI < count u`), plus
//! the winit-free key-encoding model.
//!
//! Reference: [kitty keyboard protocol].
//!
//! [kitty keyboard protocol]: https://sw.kovidgoyal.net/kitty/keyboard-protocol/

use alloc::string::ToString;

use compact_str::{CompactString, ToCompactString, format_compact};

bitflags::bitflags! {
    /// Kitty keyboard protocol flags, reported by `CSI > flags u` and pushed
    /// by `CSI < count u`. Each bit enables a progressive feature tier.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct KittyFlags: u8 {
        /// Disambiguate escape codes (`CSI u`).
        const DISAMBIGUATE = 1;
        /// Report press, repeat and release.
        const REPORT_EVENT_TYPES = 2;
        /// Report alternate keys.
        const REPORT_ALTERNATE_KEYS = 4;
        /// Report all keys as escape sequences.
        const REPORT_ALL_KEYS = 8;
        /// Report associated text.
        const REPORT_ASSOCIATED_TEXT = 16;
    }
}

/// How key events serialize to the PTY; the host picks one from the DEC flags
/// (DECCKM, modifyOtherKeys, kitty).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyEncoding {
    /// Arrows as `CSI A-D`, Ctrl as control bytes, Alt as an ESC prefix.
    #[default]
    Legacy,
    /// DECCKM, arrows as `SS3`.
    ApplicationCursorKeys,
    /// xterm modifyOtherKeys. Carries the mode: 1 is `CSI 1;mod A`, 2 is
    /// `CSI 27;mod;code~`.
    ModifyOtherKeys(u8),
    /// kitty keyboard protocol, `CSI code:mods u`. Carries the progressive-enhancement flags.
    Kitty(u8),
}

/// Key press/repeat/release event type (for kitty `report_events`, flag 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyEventKind {
    /// A key press, the kitty `:1` event type.
    #[default]
    Press,
    /// A held-key repeat, the kitty `:2` event type.
    Repeat,
    /// A key release, the kitty `:3` event type.
    Release,
}

/// The physical location of a key (winit-free). The encoder uses it to
/// distinguish the numeric keypad from the main key row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyLocation {
    /// The key's usual location.
    #[default]
    Standard,
    /// The left-hand variant.
    Left,
    /// The right-hand variant.
    Right,
    /// The numeric keypad.
    Numpad,
}

/// The keypad/backarrow modes the encoder needs (winit-free), read from the
/// screen's DEC flags at encode time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModes {
    /// DECKPAM/DECNKM (`ESC =` / `CSI ?66h`) application keypad.
    pub application_keypad: bool,
    /// DEC 1035 (`CSI ?1035h`): ignore numlock, the keypad is always
    /// numeric, never application.
    pub numlock_ignored: bool,
    /// DECBKM (`CSI ?67h`): Backspace sends `\x08` instead of `\x7f`.
    pub backarrow: bool,
}

impl KeyModes {
    /// The effective application-keypad state: DECNKM on and numlock not
    /// ignored.
    pub const fn keypad_application(&self) -> bool {
        self.application_keypad && !self.numlock_ignored
    }
}

/// The encoders' shared per-key facts. The common shapes store one fact and
/// derive the kitty form from it; irregular keys carry both explicitly.
#[derive(Clone, Copy, Debug)]
pub enum KeySpec {
    /// Cursor key, legacy `SS3` or `CSI <l>`.
    Cursor(char),
    /// Home/End, legacy `CSI <l>`.
    CsiLetter(char),
    /// Editing and function key, legacy `CSI <n>~`.
    Csi(u8),
    /// Fixed legacy bytes plus an explicit kitty form.
    Fixed {
        /// kitty `(code, trailer)` form.
        kitty: (u32, char),
        /// legacy byte sequence.
        bytes: &'static str,
    },
    /// F13-F20, kitty PUA code and a legacy `CSI <csi>~`.
    Pua {
        /// kitty PUA keycode.
        kitty: u32,
        /// legacy CSI parameter.
        csi: u8,
    },
    /// kitty fallback code, legacy raw byte.
    Fallback {
        /// kitty fallback keycode.
        code: u32,
        /// legacy raw byte.
        bytes: &'static str,
    },
    /// kitty-only functional key.
    KittyOnly(u32),
    /// Legacy-only key (`ContextMenu`).
    LegacyOnly(&'static str),
}

bitflags::bitflags! {
    /// Keyboard modifier state, winit-free: the bits equal the xterm/kitty
    /// modifier contributions, so [`Modifiers::encoded`] is the wire bitfield
    /// minus the leading `1` (no-modifiers) bit.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Modifiers: u8 {
        /// Shift.
        const SHIFT = 0b0001;
        /// Alt.
        const ALT = 0b0010;
        /// Control.
        const CTRL = 0b0100;
        /// Super (OS).
        const SUPER = 0b1000;
    }
}

impl Modifiers {
    /// The xterm/kitty modifier bitfield: 1 = none, then shift = +1,
    /// alt = +2, ctrl = +4, super = +8.
    #[must_use]
    pub const fn encoded(self) -> u8 {
        1 + self.bits()
    }
    /// Whether shift is held.
    #[must_use]
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }
    /// Whether alt is held.
    #[must_use]
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }
    /// Whether control is held.
    #[must_use]
    pub const fn ctrl(self) -> bool {
        self.contains(Self::CTRL)
    }
    /// Whether super (the OS/window key) is held.
    #[must_use]
    pub const fn super_key(self) -> bool {
        self.contains(Self::SUPER)
    }
}

/// A terminal key, winit-free: the single representation the encoders need.
#[derive(Clone, Copy, Debug)]
pub enum Key {
    /// A character key; the legacy encoder sends its UTF-8.
    Character(char),
    /// Backspace, `0x7f` (or `0x08` under DECBKM); kitty key code 127.
    Backspace,
    /// Tab, `\t`; kitty key code 9.
    Tab,
    /// A functional key.
    Named(KeySpec),
    /// A keypad key. `legacy` is `Some` for the keys the legacy encoder sends as keypad.
    Keypad {
        /// Kitty key code.
        kitty: u32,
        /// Legacy SS3 symbol + numerical-mode char, when legacy treats this
        /// key as keypad.
        legacy: Option<(char, char)>,
        /// Legacy fallback spec for functional keypad keys (arrows/editing).
        spec: Option<KeySpec>,
    },
    /// A modifier key under `report_all_keys`. `legacy` carries the bytes for keys that have them.
    Modifier {
        /// Kitty key code.
        kitty: u32,
        /// Legacy byte sequence, when the key has one.
        legacy: Option<&'static str>,
    },
}

impl Key {
    /// The [`KeySpec`] a legacy encoder should use, or `None` for character
    /// keys, keypad SS3 keys, and modifiers.
    const fn legacy_spec(&self) -> Option<&KeySpec> {
        match self {
            Self::Named(spec) => Some(spec),
            Self::Keypad { spec, .. } => spec.as_ref(),
            _ => None,
        }
    }
}

/// Encode a key event into the bytes to write to the PTY, under the given
/// [`KeyEncoding`]. This is the winit-free entry point; the host maps its
/// native keys/modifiers into [`Key`] + [`Modifiers`] first.
///
/// # Example
///
/// ```
/// use vtty_core::{Key, KeyEncoding, KeyEventKind, KeyModes, Modifiers, encode_key};
///
/// // A plain character key in legacy mode:
/// assert_eq!(
///     encode_key(
///         &Key::Character('a'),
///         Modifiers::empty(),
///         KeyEncoding::Legacy,
///         KeyEventKind::Press,
///         KeyModes::default(),
///         None,
///     ),
///     Some("a".into()),
/// );
///
/// // Backspace follows the DECBKM backarrow mode:
/// assert_eq!(
///     encode_key(
///         &Key::Backspace,
///         Modifiers::empty(),
///         KeyEncoding::Legacy,
///         KeyEventKind::Press,
///         KeyModes { backarrow: true, ..KeyModes::default() },
///         None,
///     ),
///     Some("\x08".into()),
/// );
/// ```
#[must_use]
pub fn encode_key(
    key: &Key,
    mods: Modifiers,
    encoding: KeyEncoding,
    kind: KeyEventKind,
    modes: KeyModes,
    text: Option<&str>,
) -> Option<CompactString> {
    if kind == KeyEventKind::Release {
        if let KeyEncoding::Kitty(flags) = encoding {
            return encode_kitty(key, mods, flags, kind, text);
        }
        return None;
    }
    match encoding {
        KeyEncoding::Legacy => encode_legacy(key, false, mods, modes, text),
        KeyEncoding::ApplicationCursorKeys => encode_legacy(key, true, mods, modes, text),
        KeyEncoding::ModifyOtherKeys(mode) => {
            encode_modify_other_keys(key, mode, mods, modes, text)
        }
        KeyEncoding::Kitty(flags) => encode_kitty(key, mods, flags, kind, text),
    }
}

/// The kitty fallback form for Enter/Tab/Backspace/Space: the raw byte unless
/// a modifier, an event field, or `report_all_keys` forces the CSI-u form.
fn kitty_fallback(
    code: u32,
    bytes: &'static str,
    has_mods: bool,
    event: Option<u32>,
    flags: u8,
    encoded_mods: u32,
) -> CompactString {
    if has_mods || event.is_some() || flags & 8 != 0 {
        kitty_csi(code, 'u', encoded_mods, event)
    } else {
        bytes.into()
    }
}

/// Kitty keyboard protocol encoding (`CSI code:mods u`).
fn encode_kitty(
    key: &Key,
    mods: Modifiers,
    flags: u8,
    kind: KeyEventKind,
    text: Option<&str>,
) -> Option<CompactString> {
    // Releases are reported only when report_events (flag 2) is set.
    if kind == KeyEventKind::Release && flags & 2 == 0 {
        return None;
    }

    let encoded_mods = u32::from(mods.encoded());
    let has_mods = encoded_mods != 1;

    // report_events (flag 2): repeat/release append `:event` (2/3).
    let event = match (flags & 2 != 0, kind) {
        (true, KeyEventKind::Repeat) => Some(2_u32),
        (true, KeyEventKind::Release) => Some(3_u32),
        _ => None,
    };

    let encoded = match key {
        Key::Character(ch) => {
            encode_kitty_text(*ch, flags, mods, has_mods, encoded_mods, event, text)?
        }
        Key::Keypad { kitty, .. } => kitty_csi(*kitty, 'u', encoded_mods, event),
        Key::Backspace => kitty_fallback(127, "\x7f", has_mods, event, flags, encoded_mods),
        Key::Tab => kitty_fallback(9, "\t", has_mods, event, flags, encoded_mods),
        Key::Named(KeySpec::Fallback { code, bytes }) => {
            kitty_fallback(*code, bytes, has_mods, event, flags, encoded_mods)
        }
        Key::Modifier { kitty, .. } if flags & 8 != 0 => {
            kitty_csi(*kitty, 'u', encoded_mods, event)
        }
        Key::Named(spec) if let Some((code, trailer)) = kitty_form(*spec) => {
            kitty_csi(code, trailer, encoded_mods, event)
        }
        _ => return None,
    };
    Some(encoded)
}

/// kitty CSI formatter: `CSI <code>[;<mods>[:<event>]] <trailer>`.
fn kitty_csi(code: u32, trailer: char, encoded_mods: u32, event: Option<u32>) -> CompactString {
    let has_mods = encoded_mods != 1;
    let omit_code = code == 1 && !has_mods && event.is_none();
    match (has_mods, event, omit_code) {
        (false, None, true) => format_compact!("\x1b[{trailer}"),
        (false, None, false) => format_compact!("\x1b[{code}{trailer}"),
        (true, None, _) => {
            format_compact!("\x1b[{code};{encoded_mods}{trailer}")
        }
        (false, Some(e), _) => format_compact!("\x1b[{code};1:{e}{trailer}"),
        (true, Some(e), _) => {
            format_compact!("\x1b[{code};{encoded_mods}:{e}{trailer}")
        }
    }
}

/// Encode a kitty text key (`report_alternates` / `report_text` /
/// `report_events`).
fn encode_kitty_text(
    ch: char,
    flags: u8,
    mods: Modifiers,
    has_mods: bool,
    encoded_mods: u32,
    event: Option<u32>,
    text: Option<&str>,
) -> Option<CompactString> {
    use core::fmt::Write as _;
    let embed_text = flags & 16 != 0 && text.is_some_and(|t| !t.is_empty());

    // A text key with only shift (or no modifier) stays text.
    if flags & 8 == 0
        && !embed_text
        && event.is_none()
        && !mods.alt()
        && !mods.ctrl()
        && !mods.super_key()
    {
        return Some(if mods.shift() {
            shifted_text(ch, text)
        } else {
            ch.to_string().into()
        });
    }

    let mut b = CompactString::default();
    let _ = write!(b, "\x1b[{}", ch as u32);

    // report_alternates (flag 4): `:shifted:base`.
    if flags & 4 != 0 && has_mods {
        if mods.shift() {
            let shifted = shifted_char(ch, text)?;
            let _ = write!(b, ":{}:{}", shifted as u32, ch as u32);
        } else {
            let _ = write!(b, "::{}", ch as u32);
        }
    }

    // `;<mods>[:<event>]`.
    if has_mods || event.is_some() || embed_text {
        b.push(';');
        if has_mods {
            let _ = write!(b, "{encoded_mods}");
        } else if event.is_some() {
            b.push('1');
        }
        if let Some(e) = event {
            let _ = write!(b, ":{e}");
        }
    }

    // report_text (flag 16): `;<codepoints>`.
    if embed_text && let Some(text) = text {
        for (i, c) in text.chars().enumerate() {
            b.push(if i == 0 { ';' } else { ':' });
            let _ = write!(b, "{}", c as u32);
        }
    }

    b.push('u');
    Some(b)
}

/// The shifted character of a Shift+key: the event `text`'s first char, else
/// the uppercase base.
fn shifted_char(ch: char, text: Option<&str>) -> Option<char> {
    text.and_then(|t| t.chars().next())
        .or_else(|| ch.to_uppercase().next())
}

/// The shifted text of a Shift+key: the shifted char, else the base itself.
fn shifted_text(ch: char, text: Option<&str>) -> CompactString {
    shifted_char(ch, text).unwrap_or(ch).to_compact_string()
}

/// The kitty functional `(code, trailer)`, or `None` for fallback/legacy-only
/// keys.
const fn kitty_form(spec: KeySpec) -> Option<(u32, char)> {
    Some(match spec {
        KeySpec::Cursor(l) | KeySpec::CsiLetter(l) => (1, l),
        KeySpec::Csi(n) => (n as u32, '~'),
        KeySpec::Fixed { kitty, .. } => kitty,
        KeySpec::Pua { kitty, .. } => (kitty, 'u'),
        KeySpec::KittyOnly(code) => (code, 'u'),
        KeySpec::Fallback { .. } | KeySpec::LegacyOnly(_) => return None,
    })
}

/// Encode a numeric-keypad key: SS3 `ESC O [<mod>]<sym>` in application mode,
/// the plain char in numerical mode.
fn encode_keypad(
    sym: char,
    plain: char,
    mods: Modifiers,
    modes: KeyModes,
) -> Option<CompactString> {
    let xmod = mods.encoded();
    if modes.keypad_application() {
        if xmod == 1 {
            Some(format_compact!("\x1bO{sym}"))
        } else {
            Some(format_compact!("\x1bO{xmod}{sym}"))
        }
    } else {
        Some(format_compact!("{plain}"))
    }
}

/// The unmodified legacy byte sequence for a [`KeySpec`].
fn legacy_spec_bytes(spec: &KeySpec, app_cursor: bool) -> Option<CompactString> {
    Some(match spec {
        KeySpec::Cursor(letter) => legacy_arrow(*letter, app_cursor),
        KeySpec::CsiLetter(letter) => format_compact!("\x1b[{letter}"),
        KeySpec::Csi(code) => format_compact!("\x1b[{code}~"),
        KeySpec::Pua { csi, .. } => format_compact!("\x1b[{csi}~"),
        KeySpec::Fixed { bytes, .. }
        | KeySpec::Fallback { bytes, .. }
        | KeySpec::LegacyOnly(bytes) => (*bytes).into(),
        KeySpec::KittyOnly(_) => return None,
    })
}

/// The unmodified legacy byte sequence for a key.
fn legacy_base(
    key: &Key,
    app_cursor: bool,
    modes: KeyModes,
    text: Option<&str>,
) -> Option<CompactString> {
    Some(match key {
        Key::Character(ch) => match text {
            Some(t) if !t.is_empty() => t.into(),
            _ => ch.to_compact_string(),
        },
        Key::Backspace => {
            if modes.backarrow {
                "\x08".into()
            } else {
                "\x7f".into()
            }
        }
        Key::Tab => "\t".into(),
        Key::Named(_) | Key::Keypad { .. } => legacy_spec_bytes(key.legacy_spec()?, app_cursor)?,
        Key::Modifier {
            legacy: Some(bytes),
            ..
        } => (*bytes).into(),
        Key::Modifier { .. } => return None,
    })
}

/// The xterm modifyOtherKeys cursor letter for a Ctrl+cursor key
/// (`Ctrl+Left` → `CSI 1;5D`): the kitty code-1 trailer.
const fn cursor_key_letter(key: &Key) -> Option<char> {
    match key.legacy_spec() {
        Some(KeySpec::Cursor(l) | KeySpec::CsiLetter(l)) => Some(*l),
        _ => None,
    }
}

/// Map an ASCII byte to its Ctrl control character (0x00-0x1F), if any.
/// Follows vt100.net Table 3-5.
#[must_use]
pub const fn legacy_ctrl(byte: u8) -> Option<char> {
    Some(match byte {
        b' ' | b'2' | b'@' => '\x00',
        b'a'..=b'z' => (byte - b'a' + 1) as char,
        b'A'..=b'Z' => (byte - b'A' + 1) as char,
        b'3' | b'[' => '\x1b',
        b'4' | b'\\' => '\x1c',
        b'5' | b']' => '\x1d',
        b'6' | b'^' | b'~' => '\x1e',
        b'7' | b'/' | b'_' | b'?' => '\x1f',
        b'8' => '\x7f',
        b'0' | b'1' | b'9' => byte as char,
        _ => return None,
    })
}

/// DECCKM normal- vs application-mode arrow sequence (`CSI A` / `SS3 A`).
fn legacy_arrow(letter: char, app_cursor: bool) -> CompactString {
    if app_cursor {
        format_compact!("\x1bO{letter}")
    } else {
        format_compact!("\x1b[{letter}")
    }
}

/// Apply legacy modifiers (Shift/Alt/Ctrl) to an unmodified base sequence.
fn legacy_modified(base: CompactString, key: &Key, mods: Modifiers) -> Option<CompactString> {
    let ctrl = mods.ctrl();
    let alt = mods.alt();
    let shift = mods.shift();

    // Shift+Tab is the BackTab (`CSI Z`), not a shifted tab byte.
    if shift && !ctrl && !alt && matches!(key, Key::Tab) {
        return Some("\x1b[Z".into());
    }

    // Shift/Alt (without Ctrl) on cursor keys keep the xterm modifyOtherKeys
    // form (`CSI 1;2A` Shift+Up, `CSI 1;3A` Alt+Up).
    if !ctrl
        && (shift || alt)
        && let Some(letter) = cursor_key_letter(key)
    {
        return Some(format_compact!("\x1b[1;{}{letter}", mods.encoded()));
    }

    if !ctrl && !alt {
        return Some(base);
    }

    let ctrl_char = if ctrl && !(shift && base.as_bytes() != *b"@") {
        match base.as_bytes() {
            [b] if matches!(b, ..0x20 | 0x7f) => Some(*b as char),
            [b] => legacy_ctrl(*b),
            _ => None,
        }
    } else {
        None
    };

    let mut out = CompactString::default();
    if alt {
        out.push('\x1b');
    }
    match ctrl_char {
        Some(c) => out.push(c),
        None if ctrl => {
            let xmod = mods.encoded();
            if let Some(letter) = cursor_key_letter(key) {
                out.push_str(&format_compact!("\x1b[1;{xmod}{letter}"));
            } else {
                let cp = base.chars().next()? as u32;
                out.push_str(&format_compact!("\x1b[{cp};{xmod}u"));
            }
        }
        None => out.push_str(&base),
    }
    Some(out)
}

/// Legacy encoding (DECCKM-aware): arrows, named keys, Ctrl → control chars,
/// Alt → ESC prefix.
fn encode_legacy(
    key: &Key,
    app_cursor: bool,
    mods: Modifiers,
    modes: KeyModes,
    text: Option<&str>,
) -> Option<CompactString> {
    if let Key::Keypad {
        legacy: Some((sym, plain)),
        ..
    } = key
    {
        return encode_keypad(*sym, *plain, mods, modes);
    }
    let base = legacy_base(key, app_cursor, modes, text)?;
    legacy_modified(base, key, mods)
}

/// xterm modifyOtherKeys: encode arrow/home/end as `CSI 1;<mod><letter>` and
/// editing keys as `CSI <code>;<mod>~`. Unmodified keys fall back to legacy.
fn encode_modify_other_keys(
    key: &Key,
    mode: u8,
    mods: Modifiers,
    modes: KeyModes,
    text: Option<&str>,
) -> Option<CompactString> {
    if let Key::Keypad {
        legacy: Some((sym, plain)),
        ..
    } = key
    {
        return encode_keypad(*sym, *plain, mods, modes);
    }
    // xterm modifier param: 1 (none), 2 shift, 3 alt, 4 shift+alt, 5 ctrl, …
    // xterm has no super bit, so mask it out.
    let xmod = mods.encoded() & 0b111;
    if xmod == 1 {
        return encode_legacy(key, false, mods, modes, text);
    }
    if mode == 2 {
        // Format 2 (`CSI 27;mod;code~`): the key's Unicode/control code, with
        // BS/HT/CR/ESC/DEL special-cased.
        let code = match key {
            Key::Backspace => 8_u32,
            Key::Tab => 9,
            // Enter is the only `Fallback` with code 13.
            Key::Named(KeySpec::Fallback { code: 13, .. }) => 13,
            // Escape is the only `Fixed` with kitty code 27.
            Key::Named(KeySpec::Fixed { kitty: (27, _), .. }) => 27,
            // Delete is the only `Csi(3)`.
            Key::Named(KeySpec::Csi(3)) => 127,
            Key::Character(c) => *c as u32,
            _ => return encode_modify_other_keys_mode1(key, xmod, mods, text),
        };
        return Some(format_compact!("\x1b[27;{xmod};{code}~"));
    }
    encode_modify_other_keys_mode1(key, xmod, mods, text)
}

/// xterm modifyOtherKeys format 1 (`CSI 1;mod <letter>` / `CSI code;mod~`).
fn encode_modify_other_keys_mode1(
    key: &Key,
    xmod: u8,
    mods: Modifiers,
    text: Option<&str>,
) -> Option<CompactString> {
    match key.legacy_spec() {
        Some(KeySpec::Cursor(l) | KeySpec::CsiLetter(l)) => {
            return Some(format_compact!("\x1b[1;{xmod}{l}"));
        }
        Some(KeySpec::Csi(n @ (2 | 3 | 5 | 6))) => {
            return Some(format_compact!("\x1b[{n};{xmod}~"));
        }
        _ => {}
    }
    encode_legacy(key, false, mods, KeyModes::default(), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_flag_bits() {
        assert_eq!(KittyFlags::DISAMBIGUATE.bits(), 1);
        assert_eq!(KittyFlags::REPORT_EVENT_TYPES.bits(), 2);
        assert_eq!(KittyFlags::REPORT_ALTERNATE_KEYS.bits(), 4);
        assert_eq!(KittyFlags::REPORT_ALL_KEYS.bits(), 8);
        assert_eq!(KittyFlags::REPORT_ASSOCIATED_TEXT.bits(), 16);
    }

    #[test]
    fn kitty_text_params_are_code_points() {
        // `CSI <code point>[:shifted:base][;mods[:event]][;text] u`: the
        // parameters are decimal code points, never the character itself.
        let all = KittyFlags::DISAMBIGUATE
            | KittyFlags::REPORT_EVENT_TYPES
            | KittyFlags::REPORT_ALTERNATE_KEYS
            | KittyFlags::REPORT_ALL_KEYS
            | KittyFlags::REPORT_ASSOCIATED_TEXT;
        let encode = |flags: KittyFlags, mods: Modifiers, text: Option<&'static str>| {
            encode_key(
                &Key::Character('a'),
                mods,
                KeyEncoding::Kitty(flags.bits()),
                KeyEventKind::Press,
                KeyModes::default(),
                text,
            )
            .map(|s| s.to_string())
        };

        assert_eq!(
            encode(all, Modifiers::SHIFT, Some("A")).as_deref(),
            Some("\x1b[97:65:97;2;65u")
        );
        assert_eq!(
            encode(
                KittyFlags::DISAMBIGUATE
                    | KittyFlags::REPORT_ALTERNATE_KEYS
                    | KittyFlags::REPORT_ASSOCIATED_TEXT,
                Modifiers::CTRL,
                Some("a")
            )
            .as_deref(),
            Some("\x1b[97::97;5;97u")
        );
        // No modifier reports an empty field rather than dropping it.
        assert_eq!(
            encode(all, Modifiers::empty(), Some("a")).as_deref(),
            Some("\x1b[97;;97u")
        );
        // Without `report_all_keys` or embedded text the key stays plain text.
        assert_eq!(
            encode(
                KittyFlags::DISAMBIGUATE | KittyFlags::REPORT_EVENT_TYPES,
                Modifiers::empty(),
                None
            )
            .as_deref(),
            Some("a")
        );
    }

    #[test]
    fn kitty_from_bits_truncate() {
        let f = KittyFlags::from_bits_truncate(0b11);
        assert!(f.contains(KittyFlags::DISAMBIGUATE));
        assert!(f.contains(KittyFlags::REPORT_EVENT_TYPES));
        assert!(!f.contains(KittyFlags::REPORT_ALL_KEYS));
        // Unknown bits (e.g. 32) are dropped.
        assert_eq!(KittyFlags::from_bits_truncate(32), KittyFlags::empty());
    }
}
