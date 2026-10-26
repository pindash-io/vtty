// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! **Compact**, **`no_std`**, **panic-free** binary serialization.
//!
//! [`Encode`] turns a value into a **deterministic** byte stream and
//! [`Decode`] turns that stream back into a value.
//!
//! ## Wire format
//!
//! - Integers — fixed-width little-endian.
//! - Floats — IEEE-754 bit patterns, little-endian (`f32`↔`u32`, `f64`↔`u64`).
//! - `bool` — one byte, `0` or `1`.
//! - `char` — its `u32` code point, little-endian.
//! - `()` — zero bytes.
//! - Slices, strings and collections — an LEB128 item/byte count followed by
//!   the items.
//! - Enums — one discriminant byte followed by the variant payload.
//!
//! ## Safety
//!
//! Encoding never allocates or panics. Decoding allocates only in proportion
//! to the bytes actually read, so a bogus length prefix cannot force a large
//! allocation — it fails with [`DecodeError::UnexpectedEof`] instead.

#![no_std]
#![forbid(unsafe_code)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "fixed-width encoding truncates to target-width types where the range is known-safe"
)]

extern crate alloc;

mod decoder;
mod error;
mod impls;
mod io;
mod traits;
mod varint;
mod vtty;

pub use decoder::{DecodeIter, Decoder, decode_from_slice, decode_iter, encode_to_vec};
pub use error::{DecodeError, EncodeError};
pub use io::{Read, SliceReader, SliceWriter, VecWriter, Write};
pub use traits::{Decode, Encode};

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;
    use smallvec::SmallVec;

    use crate::varint::{decode_varint, encode_varint};

    fn ok<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => unreachable!("unexpected error: {error:?}"),
        }
    }

    fn roundtrip<T>(value: &T) -> T
    where
        T: Encode + Decode + PartialEq + core::fmt::Debug,
    {
        let bytes = ok(encode_to_vec(value));
        assert_eq!(
            bytes.len(),
            value.encoded_len(),
            "encoded_len must match encode"
        );
        ok(decode_from_slice(&bytes))
    }

    #[test]
    fn integers_roundtrip() {
        assert_eq!(roundtrip(&0_u8), 0_u8);
        assert_eq!(roundtrip(&u8::MAX), u8::MAX);
        assert_eq!(roundtrip(&0x1234_u16), 0x1234_u16);
        assert_eq!(roundtrip(&0x1234_5678_u32), 0x1234_5678_u32);
        assert_eq!(roundtrip(&u64::MAX), u64::MAX);
        assert_eq!(roundtrip(&-42_i32), -42_i32);
    }

    #[test]
    fn floats_roundtrip() {
        for value in [
            0.0_f32,
            -0.0_f32,
            1.5_f32,
            f32::MIN_POSITIVE,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            let bytes = ok(encode_to_vec(&value));
            assert_eq!(bytes.len(), value.encoded_len(), "f32 encoded_len");
            let decoded = ok(decode_from_slice::<f32>(&bytes));
            assert_eq!(decoded.to_bits(), value.to_bits(), "f32 {value:?}");
        }
        for value in [
            0.0_f64,
            -0.0_f64,
            1.5_f64,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            let bytes = ok(encode_to_vec(&value));
            assert_eq!(bytes.len(), value.encoded_len(), "f64 encoded_len");
            let decoded = ok(decode_from_slice::<f64>(&bytes));
            assert_eq!(decoded.to_bits(), value.to_bits(), "f64 {value:?}");
        }
    }

    #[test]
    fn unit_roundtrip() {
        assert_eq!(roundtrip(&()), ());
        assert_eq!(().encoded_len(), 0);
        assert!(ok(encode_to_vec(&())).is_empty());
    }

    #[test]
    fn option_roundtrip() {
        assert_eq!(roundtrip(&None::<u32>), None);
        assert_eq!(roundtrip(&Some(42_u32)), Some(42_u32));
        assert_eq!(
            roundtrip(&Some(alloc::string::String::from("hi"))),
            Some(alloc::string::String::from("hi"))
        );
    }

    #[test]
    fn scalars_roundtrip() {
        assert!(roundtrip(&true));
        assert!(!roundtrip(&false));
        assert_eq!(roundtrip(&'λ'), 'λ');
        assert_eq!(roundtrip(&'\0'), '\0');
    }

    #[test]
    fn collections_roundtrip() {
        let bytes = vec![0_u8, 1, 2, 254, 255];
        assert_eq!(roundtrip(&bytes), bytes);

        let text = alloc::string::String::from("héllo 世界");
        assert_eq!(roundtrip(&text), text);

        let sv: SmallVec<i32, 6> = SmallVec::from_buf([1, -2, 3]);
        assert_eq!(roundtrip(&sv), sv);
    }

    #[test]
    fn truncated_input() {
        let bytes = ok(encode_to_vec(&0x1234_5678_u32));
        assert_eq!(
            decode_from_slice::<u32>(&bytes[..3]),
            Err(DecodeError::UnexpectedEof)
        );
    }

    #[test]
    fn buffer_too_small() {
        let value = 0x1234_5678_u32;
        let mut buf = [0_u8; 3];
        assert_eq!(
            value.encode(&mut SliceWriter::new(&mut buf)),
            Err(EncodeError::BufferTooSmall)
        );
    }

    #[test]
    fn invalid_bool() {
        assert_eq!(
            decode_from_slice::<bool>(&[2]),
            Err(DecodeError::InvalidValue)
        );
    }

    #[test]
    fn streaming_decoder_chunked() {
        let value = vec![0x0102_0304_u32, 0xffff_ffff];
        let bytes = ok(encode_to_vec(&value));

        let mut decoder = Decoder::<Vec<u32>>::new();
        for (i, _) in bytes.iter().enumerate() {
            let decoded = ok(decoder.advance(&bytes[i..=i]));
            assert_eq!(
                decoded,
                if i + 1 == bytes.len() {
                    Some(value.clone())
                } else {
                    None
                }
            );
        }
    }

    #[test]
    fn streaming_decoder_finish_truncated() {
        let bytes = ok(encode_to_vec(&0x1234_5678_u32));
        let mut decoder = Decoder::<u32>::new();
        assert_eq!(ok(decoder.advance(&bytes[..3])), None);
        assert_eq!(decoder.finish(), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn decode_iter_values() {
        let values = [1_u32, 2, 3];
        let mut bytes = Vec::new();
        for value in &values {
            bytes.extend_from_slice(&ok(encode_to_vec(value)));
        }
        let decoded: Vec<u32> = decode_iter(&bytes).map(ok).collect();
        assert_eq!(decoded, values.to_vec());
    }

    #[test]
    fn decode_iter_truncated_tail() {
        let bytes = ok(encode_to_vec(&0x1234_5678_u32));
        let mut iter = decode_iter::<u32>(&bytes[..3]);
        assert_eq!(iter.next(), Some(Err(DecodeError::UnexpectedEof)));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn varint_boundary() {
        // 128 elements ⇒ LEB128 length is `80 01`.
        let value: Vec<u8> = vec![7; 128];
        let bytes = ok(encode_to_vec(&value));
        assert_eq!(bytes[0], 0x80);
        assert_eq!(bytes[1], 0x01);
        assert_eq!(ok(decode_from_slice::<Vec<u8>>(&bytes)), value);
    }

    #[test]
    fn error_display() {
        assert_eq!(
            EncodeError::BufferTooSmall.to_string(),
            "encode buffer too small"
        );
        assert_eq!(
            DecodeError::UnexpectedEof.to_string(),
            "unexpected end of input"
        );
        assert_eq!(DecodeError::InvalidValue.to_string(), "invalid value");
    }

    #[test]
    fn varint_roundtrip() {
        for value in [0_usize, 1, 127, 128, 16_383, 16_384, usize::MAX] {
            let mut buf = [0_u8; 10];
            let written = {
                let mut writer = SliceWriter::new(&mut buf);
                ok(encode_varint(&mut writer, value));
                writer.position()
            };
            let mut reader = SliceReader::new(&buf[..written]);
            assert_eq!(ok(decode_varint(&mut reader)), value);
        }
    }

    #[test]
    fn varint_overflow() {
        // Ten 0xff continuation bytes overflow a usize varint.
        let bytes = [0xff_u8; 10];
        let mut reader = SliceReader::new(&bytes);
        assert_eq!(decode_varint(&mut reader), Err(DecodeError::InvalidValue));
    }

    #[test]
    fn varint_truncated() {
        // Continuation bytes with no terminator.
        let bytes = [0xff_u8; 3];
        let mut reader = SliceReader::new(&bytes);
        assert_eq!(decode_varint(&mut reader), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn slice_writer_atomic_on_overflow() {
        let mut buf = [0xaa_u8; 4];
        let mut writer = SliceWriter::new(&mut buf);
        ok(writer.write(&[1, 2]));
        assert_eq!(writer.write(&[3, 4, 5]), Err(EncodeError::BufferTooSmall));
        assert_eq!(writer.position(), 2);
        assert_eq!(buf, [1, 2, 0xaa, 0xaa]);
    }

    #[test]
    fn slice_writer_into_written() {
        let mut buf = [0_u8; 8];
        let mut writer = SliceWriter::new(&mut buf);
        ok(writer.write(&[7, 8, 9]));
        assert_eq!(&*writer.into_written(), &[7_u8, 8, 9][..]);
    }

    #[test]
    fn slice_reader_remaining_and_eof() {
        let mut reader = SliceReader::new(&[1_u8, 2, 3]);
        assert_eq!(reader.remaining(), 3);
        assert!(!reader.is_empty());
        ok(reader.read_exact(&mut [0_u8; 3]));
        assert!(reader.is_empty());
        assert_eq!(reader.position(), 3);
    }

    #[test]
    fn read_exact_truncated() {
        let mut reader = SliceReader::new(&[1_u8, 2]);
        let mut buf = [0_u8; 3];
        assert_eq!(reader.read_exact(&mut buf), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn read_byte_eof() {
        let mut reader = SliceReader::new(&[]);
        assert_eq!(reader.read_byte(), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn vec_writer_appends() {
        let mut buf = Vec::new();
        let mut writer = VecWriter::new(&mut buf);
        ok(writer.write(&[1, 2]));
        ok(writer.write_byte(3));
        assert_eq!(writer.len(), 3);
        assert!(!writer.is_empty());
        assert_eq!(writer.as_slice(), &[1_u8, 2, 3][..]);
    }

    #[test]
    fn streaming_decoder_multiple_values() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ok(encode_to_vec(&1_u32)));
        bytes.extend_from_slice(&ok(encode_to_vec(&2_u32)));

        let mut decoder = Decoder::<u32>::new();
        assert_eq!(ok(decoder.advance(&bytes)), Some(1_u32));
        assert_eq!(ok(decoder.advance(&[])), Some(2_u32));
    }

    #[test]
    fn streaming_decoder_invalid() {
        let mut decoder = Decoder::<bool>::new();
        assert_eq!(decoder.advance(&[2]), Err(DecodeError::InvalidValue));
    }

    #[test]
    fn decode_iter_empty() {
        let mut iter = decode_iter::<u32>(&[]);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn decode_iter_invalid_tail() {
        let mut iter = decode_iter::<bool>(&[1, 2]);
        assert_eq!(iter.next(), Some(Ok(true)));
        assert_eq!(iter.next(), Some(Err(DecodeError::InvalidValue)));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn string_rejects_invalid_utf8() {
        // varint(2) + [0xff, 0xfe] is not valid UTF-8.
        let bytes = [0x02_u8, 0xff, 0xfe];
        assert_eq!(
            decode_from_slice::<alloc::string::String>(&bytes),
            Err(DecodeError::InvalidValue)
        );
    }

    #[test]
    fn char_rejects_surrogate() {
        // U+D800 is a surrogate, not a valid `char`.
        let bytes = 0xd800_u32.to_le_bytes();
        assert_eq!(
            decode_from_slice::<char>(&bytes),
            Err(DecodeError::InvalidValue)
        );
    }
}
