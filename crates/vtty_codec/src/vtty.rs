// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Codec implementations for the `vtty_core` data types.

use alloc::string::String;
use alloc::vec::Vec;
use smallvec::SmallVec;

use vtty_core::{
    Charset, Charsets, ColorId, ColorSpec, CursorStyle, Event, KittyFlags, MouseButton,
    MouseTracking, Recording, Sgr, SgrEffect, SgrFlags, State, StyleFlags, TermFlags,
    UnderlineStyle,
};
use vtty_status::{ProgramStatus, StatusKind, StatusState};

use crate::error::{DecodeError, EncodeError};
use crate::io::{Read, Write};
use crate::traits::{Decode, Encode};
use crate::varint::{decode_varint, encode_varint};
use crate::{impl_bitflags_codec, impl_tagged_codec};

/// `Event` variant discriminant — the tag byte of the wire format, mirroring
/// `Event`'s declaration order. Private on purpose: the tag is an
/// implementation detail, not part of the public API.
#[repr(u8)]
enum Tag {
    Print = 0,
    PrintBytes = 1,
    Execute = 2,
    Csi = 3,
    Esc = 4,
    DcsHook = 5,
    DcsPut = 6,
    DcsUnhook = 7,
    Osc = 8,
    /// Any other byte — an invalid wire tag.
    None = 255,
}

impl From<u8> for Tag {
    fn from(b: u8) -> Self {
        match b {
            0 => Self::Print,
            1 => Self::PrintBytes,
            2 => Self::Execute,
            3 => Self::Csi,
            4 => Self::Esc,
            5 => Self::DcsHook,
            6 => Self::DcsPut,
            7 => Self::DcsUnhook,
            8 => Self::Osc,
            _ => Self::None,
        }
    }
}

impl Encode for Event {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        match self {
            Self::Print(c) => {
                writer.write_byte(Tag::Print as u8)?;
                c.encode(writer)
            }
            Self::PrintBytes(s) => {
                writer.write_byte(Tag::PrintBytes as u8)?;
                s.encode(writer)
            }
            Self::Execute(b) => {
                writer.write_byte(Tag::Execute as u8)?;
                b.encode(writer)
            }
            Self::CsiDispatch {
                params,
                sep,
                intermediates,
                final_byte,
            } => {
                writer.write_byte(Tag::Csi as u8)?;
                params.encode(writer)?;
                sep.encode(writer)?;
                intermediates.encode(writer)?;
                final_byte.encode(writer)
            }
            Self::EscDispatch {
                intermediates,
                byte,
            } => {
                writer.write_byte(Tag::Esc as u8)?;
                intermediates.encode(writer)?;
                byte.encode(writer)
            }
            Self::DcsHook {
                params,
                intermediates,
                final_byte,
            } => {
                writer.write_byte(Tag::DcsHook as u8)?;
                params.encode(writer)?;
                intermediates.encode(writer)?;
                final_byte.encode(writer)
            }
            Self::DcsPut(b) => {
                writer.write_byte(Tag::DcsPut as u8)?;
                b.encode(writer)
            }
            Self::DcsUnhook => writer.write_byte(Tag::DcsUnhook as u8),
            Self::OscDispatch {
                params,
                bell_terminated,
            } => {
                writer.write_byte(Tag::Osc as u8)?;
                params.encode(writer)?;
                bell_terminated.encode(writer)
            }
            _ => unreachable!("non-exhaustive enum has an unknown variant"),
        }
    }
}

impl Decode for Event {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        match Tag::from(reader.read_byte()?) {
            Tag::Print => Ok(Self::Print(char::decode(reader)?)),
            Tag::PrintBytes => Ok(Self::PrintBytes(Vec::decode(reader)?)),
            Tag::Execute => Ok(Self::Execute(u8::decode(reader)?)),
            Tag::Csi => Ok(Self::CsiDispatch {
                params: SmallVec::decode(reader)?,
                sep: u16::decode(reader)?,
                intermediates: SmallVec::decode(reader)?,
                final_byte: u8::decode(reader)?,
            }),
            Tag::Esc => Ok(Self::EscDispatch {
                intermediates: SmallVec::decode(reader)?,
                byte: u8::decode(reader)?,
            }),
            Tag::DcsHook => Ok(Self::DcsHook {
                params: SmallVec::decode(reader)?,
                intermediates: SmallVec::decode(reader)?,
                final_byte: u8::decode(reader)?,
            }),
            Tag::DcsPut => Ok(Self::DcsPut(u8::decode(reader)?)),
            Tag::DcsUnhook => Ok(Self::DcsUnhook),
            Tag::Osc => Ok(Self::OscDispatch {
                params: SmallVec::decode(reader)?,
                bell_terminated: bool::decode(reader)?,
            }),
            Tag::None => Err(DecodeError::InvalidValue),
        }
    }
}

impl Encode for Recording {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.events.encode(writer)
    }
}

impl Decode for Recording {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(Self {
            events: Vec::decode(reader)?,
        })
    }
}

impl Encode for Sgr {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.params.encode(writer)?;
        self.sep.encode(writer)
    }
}

impl Decode for Sgr {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(Self {
            params: SmallVec::decode(reader)?,
            sep: u16::decode(reader)?,
        })
    }
}

impl Encode for ColorId {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.index().encode(writer)
    }
}

impl Decode for ColorId {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(Self(u16::decode(reader)?))
    }
}

impl_bitflags_codec!(
    SgrFlags => u16,
    StyleFlags => u8,
    TermFlags => u64,
    KittyFlags => u8,
    MouseTracking => u8,
);

impl_tagged_codec!(
    MouseButton,
    0 => MouseButton::Left,
    1 => MouseButton::Middle,
    2 => MouseButton::Right,
    3 => MouseButton::Release,
    32 => MouseButton::Motion,
    35 => MouseButton::MotionNoButton,
    64 => MouseButton::WheelUp,
    65 => MouseButton::WheelDown,
    66 => MouseButton::WheelLeft,
    67 => MouseButton::WheelRight,
);

impl_tagged_codec!(
    UnderlineStyle,
    0 => UnderlineStyle::None,
    1 => UnderlineStyle::Single,
    2 => UnderlineStyle::Double,
    3 => UnderlineStyle::Curly,
    4 => UnderlineStyle::Dotted,
    5 => UnderlineStyle::Dashed,
);

impl_tagged_codec!(
    CursorStyle,
    0 => CursorStyle::None,
    1 => CursorStyle::Block,
    2 => CursorStyle::Underline,
    3 => CursorStyle::Bar,
);

impl Encode for ColorSpec {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        match *self {
            Self::Default => writer.write_byte(0),
            Self::Ansi(v) => {
                writer.write_byte(1)?;
                v.encode(writer)
            }
            Self::Indexed(v) => {
                writer.write_byte(2)?;
                v.encode(writer)
            }
            Self::Rgb(v) => {
                writer.write_byte(3)?;
                v.encode(writer)
            }
        }
    }
}

impl Decode for ColorSpec {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(match reader.read_byte()? {
            0 => Self::Default,
            1 => Self::Ansi(u16::decode(reader)?),
            2 => Self::Indexed(u16::decode(reader)?),
            3 => Self::Rgb(u32::decode(reader)?),
            _ => return Err(DecodeError::InvalidValue),
        })
    }
}

impl Encode for SgrEffect {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        match self {
            Self::Reset => writer.write_byte(0),
            Self::Set(f) => {
                writer.write_byte(1)?;
                f.encode(writer)
            }
            Self::Clear(f) => {
                writer.write_byte(2)?;
                f.encode(writer)
            }
            Self::Underline(s) => {
                writer.write_byte(3)?;
                s.encode(writer)
            }
            Self::Fg(c) => {
                writer.write_byte(4)?;
                c.encode(writer)
            }
            Self::Bg(c) => {
                writer.write_byte(5)?;
                c.encode(writer)
            }
            Self::UnderlineColor(c) => {
                writer.write_byte(6)?;
                c.encode(writer)
            }
            Self::ClearUnderlineColor => writer.write_byte(7),
        }
    }
}

impl Decode for SgrEffect {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(match reader.read_byte()? {
            0 => Self::Reset,
            1 => Self::Set(SgrFlags::decode(reader)?),
            2 => Self::Clear(SgrFlags::decode(reader)?),
            3 => Self::Underline(UnderlineStyle::decode(reader)?),
            4 => Self::Fg(ColorSpec::decode(reader)?),
            5 => Self::Bg(ColorSpec::decode(reader)?),
            6 => Self::UnderlineColor(ColorSpec::decode(reader)?),
            7 => Self::ClearUnderlineColor,
            _ => return Err(DecodeError::InvalidValue),
        })
    }
}

impl_tagged_codec!(
    Charset,
    0 => Charset::Ascii,
    1 => Charset::Dec,
    2 => Charset::British,
);

impl Encode for Charsets {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        for g in 0..4 {
            self.get(g).encode(writer)?;
        }
        Ok(())
    }
}

impl Decode for Charsets {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        let mut charsets = Self::ascii();
        for g in 0..4 {
            charsets.designate(g, Charset::decode(reader)?);
        }
        Ok(charsets)
    }
}

impl_tagged_codec!(
    StatusState,
    0 => StatusState::Idle,
    1 => StatusState::Working,
    2 => StatusState::Done,
    3 => StatusState::Blocked,
    4 => StatusState::Error,
    5 => StatusState::Clear,
);

impl_tagged_codec!(
    StatusKind,
    0 => StatusKind::Permission,
    1 => StatusKind::Question,
    2 => StatusKind::Auth,
);

impl Encode for ProgramStatus {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.state.encode(writer)?;
        self.id.encode(writer)?;
        self.kind.encode(writer)?;
        self.progress.encode(writer)?;
        self.app.encode(writer)?;
        self.title.encode(writer)?;
        self.msg.encode(writer)
    }
}

impl Decode for ProgramStatus {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(Self {
            state: StatusState::decode(reader)?,
            id: Option::decode(reader)?,
            kind: Option::decode(reader)?,
            progress: Option::decode(reader)?,
            app: Option::decode(reader)?,
            title: Option::decode(reader)?,
            msg: Option::decode(reader)?,
        })
    }
}

impl Encode for State {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.text.encode(writer)?;
        encode_varint(writer, self.cursor)?;
        encode_varint(writer, self.line_start)
    }
}

impl Decode for State {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        let text = String::decode(reader)?;
        let cursor = decode_varint(reader)?;
        let line_start = decode_varint(reader)?;
        if line_start > cursor || cursor > text.len() {
            return Err(DecodeError::InvalidValue);
        }
        Ok(Self {
            text,
            cursor,
            line_start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use core::fmt;
    use vtty_core::Perform;

    use crate::{decode_from_slice, decode_iter, encode_to_vec};

    fn ok<T, E: fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => unreachable!("unexpected error: {error:?}"),
        }
    }

    fn roundtrip<T>(value: &T) -> T
    where
        T: Encode + Decode + PartialEq + fmt::Debug,
    {
        let bytes = ok(encode_to_vec(value));
        assert_eq!(bytes.len(), value.encoded_len());
        ok(decode_from_slice(&bytes))
    }

    #[test]
    fn event_roundtrip() {
        let events = vec![
            Event::Print('λ'),
            Event::PrintBytes(b"hello".to_vec()),
            Event::Execute(0x1b),
            Event::CsiDispatch {
                params: SmallVec::from_buf([1, 31]),
                sep: 0,
                intermediates: SmallVec::from_buf(*b" "),
                final_byte: b'm',
            },
            Event::EscDispatch {
                intermediates: SmallVec::new(),
                byte: b'7',
            },
            Event::DcsHook {
                params: SmallVec::new(),
                intermediates: SmallVec::new(),
                final_byte: b'q',
            },
            Event::DcsPut(b'x'),
            Event::DcsUnhook,
            Event::OscDispatch {
                params: SmallVec::from_buf([b"0".to_vec(), b"title".to_vec()]),
                bell_terminated: false,
            },
        ];
        assert_eq!(roundtrip(&events), events);
    }

    #[test]
    fn recording_roundtrip() {
        let recording = Recording {
            events: vec![Event::Print('a'), Event::Execute(b'\n')],
        };
        assert_eq!(roundtrip(&recording), recording);
    }

    #[test]
    fn invalid_tag() {
        let bytes = ok(encode_to_vec(&Event::Print('a')));
        let mut bad = bytes.clone();
        bad[0] = 0xff;
        assert_eq!(
            decode_from_slice::<Event>(&bad),
            Err(DecodeError::InvalidValue)
        );
    }

    #[test]
    fn decode_iter_events() {
        let events = vec![Event::Print('a'), Event::Execute(b'\n'), Event::Print('λ')];
        let mut bytes = Vec::new();
        for event in &events {
            bytes.extend_from_slice(&ok(encode_to_vec(event)));
        }
        let decoded: Vec<Event> = decode_iter(&bytes).map(ok).collect();
        assert_eq!(decoded, events);
    }

    #[test]
    fn sequence_primitives_roundtrip() {
        assert_eq!(roundtrip(&ColorId(0)), ColorId(0));
        assert_eq!(
            roundtrip(&(SgrFlags::BOLD | SgrFlags::UNDERLINE)),
            SgrFlags::BOLD | SgrFlags::UNDERLINE
        );
        assert_eq!(roundtrip(&StyleFlags::all()), StyleFlags::all());
        assert_eq!(roundtrip(&TermFlags::ALT_SCREEN), TermFlags::ALT_SCREEN);
        assert_eq!(roundtrip(&KittyFlags::all()), KittyFlags::all());
        assert_eq!(roundtrip(&MouseTracking::SGR), MouseTracking::SGR);
        assert_eq!(roundtrip(&CursorStyle::Bar), CursorStyle::Bar);
        assert_eq!(roundtrip(&UnderlineStyle::Curly), UnderlineStyle::Curly);
        assert_eq!(roundtrip(&MouseButton::WheelRight), MouseButton::WheelRight);
        assert_eq!(roundtrip(&Charset::Dec), Charset::Dec);

        let mut charsets = Charsets::ascii();
        charsets.designate(1, Charset::Dec);
        assert_eq!(roundtrip(&charsets), charsets);
    }

    #[test]
    fn invalid_enum_tag() {
        assert_eq!(
            decode_from_slice::<CursorStyle>(&[0xff]),
            Err(DecodeError::InvalidValue)
        );
        assert_eq!(
            decode_from_slice::<Charset>(&[9]),
            Err(DecodeError::InvalidValue)
        );
    }

    #[test]
    fn state_roundtrip() {
        let mut s = State::new();
        s.print('中');
        s.execute(b'\n');
        s.print_bytes(b"ab");

        let bytes = ok(encode_to_vec(&s));
        assert_eq!(bytes.len(), s.encoded_len());
        assert_eq!(ok(decode_from_slice::<State>(&bytes)), s);
    }

    #[test]
    fn state_decode_rejects_invalid_offsets() {
        assert_eq!(
            decode_from_slice::<State>(&[0x01, b'a', 0x00, 0x01]),
            Err(DecodeError::InvalidValue)
        );
    }

    #[test]
    fn program_status_roundtrip() {
        let status = ProgramStatus {
            state: StatusState::Blocked,
            id: Some("build/test".into()),
            kind: Some(StatusKind::Permission),
            progress: Some(40),
            app: Some("terraform".into()),
            title: Some("US East".into()),
            msg: Some("Deploying v2".into()),
        };
        assert_eq!(roundtrip(&status), status);
    }
}
