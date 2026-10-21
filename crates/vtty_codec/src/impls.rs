// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Built-in implementations for primitives and standard containers.

use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::error::{DecodeError, EncodeError};
use crate::io::{Read, Write};
use crate::traits::{Decode, Encode};
use crate::varint::{decode_varint, encode_varint, varint_len};

macro_rules! impl_fixed_int {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Encode for $ty {
                fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
                    writer.write(&self.to_le_bytes())
                }

                fn encoded_len(&self) -> usize {
                    core::mem::size_of::<$ty>()
                }
            }

            impl Decode for $ty {
                fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
                    let mut buf = [0_u8; core::mem::size_of::<$ty>()];
                    reader.read_exact(&mut buf)?;
                    Ok(<$ty>::from_le_bytes(buf))
                }
            }
        )+
    };
}

impl_fixed_int!(u8, u16, u32, u64, i32);

impl Encode for bool {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        writer.write_byte(u8::from(*self))
    }

    fn encoded_len(&self) -> usize {
        1
    }
}

impl Decode for bool {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        match reader.read_byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::InvalidValue),
        }
    }
}

impl Encode for char {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        u32::from(*self).encode(writer)
    }

    fn encoded_len(&self) -> usize {
        size_of::<u32>()
    }
}

impl Decode for char {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Self::from_u32(u32::decode(reader)?).ok_or(DecodeError::InvalidValue)
    }
}

impl Encode for f32 {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.to_bits().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        size_of::<u32>()
    }
}

impl Decode for f32 {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(Self::from_bits(u32::decode(reader)?))
    }
}

impl Encode for f64 {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.to_bits().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        size_of::<u64>()
    }
}

impl Decode for f64 {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Ok(Self::from_bits(u64::decode(reader)?))
    }
}

impl Encode for () {
    fn encode<W: Write + ?Sized>(&self, _writer: &mut W) -> Result<(), EncodeError> {
        Ok(())
    }

    fn encoded_len(&self) -> usize {
        0
    }
}

impl Decode for () {
    fn decode<R: Read + ?Sized>(_reader: &mut R) -> Result<Self, DecodeError> {
        Ok(())
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        match self {
            Self::None => false.encode(writer),
            Self::Some(value) => {
                true.encode(writer)?;
                value.encode(writer)
            }
        }
    }

    fn encoded_len(&self) -> usize {
        1 + match self {
            Self::None => 0,
            Self::Some(value) => value.encoded_len(),
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        if bool::decode(reader)? {
            T::decode(reader).map(Self::Some)
        } else {
            Ok(Self::None)
        }
    }
}

impl<T: Encode> Encode for [T] {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        encode_varint(writer, self.len())?;
        for item in self {
            item.encode(writer)?;
        }
        Ok(())
    }

    fn encoded_len(&self) -> usize {
        varint_len(self.len()) + self.iter().map(|item| item.encoded_len()).sum::<usize>()
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.as_slice().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        self.as_slice().encoded_len()
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        let len = decode_varint(reader)?;
        let mut vec = Self::new();
        for _ in 0..len {
            vec.push(T::decode(reader)?);
        }
        Ok(vec)
    }
}

impl<T: Encode, const N: usize> Encode for SmallVec<T, N> {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.as_ref().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        self.as_ref().encoded_len()
    }
}

impl<T: Decode, const N: usize> Decode for SmallVec<T, N> {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        let len = decode_varint(reader)?;
        let mut vec = Self::new();
        for _ in 0..len {
            vec.push(T::decode(reader)?);
        }
        Ok(vec)
    }
}

impl Encode for str {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.as_bytes().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        self.as_bytes().encoded_len()
    }
}

impl Encode for String {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.as_str().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        self.as_str().encoded_len()
    }
}

impl Decode for String {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        Self::from_utf8(Vec::<u8>::decode(reader)?).map_err(|_| DecodeError::InvalidValue)
    }
}

impl Encode for SmolStr {
    fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.as_str().encode(writer)
    }

    fn encoded_len(&self) -> usize {
        self.as_str().encoded_len()
    }
}

impl Decode for SmolStr {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
        String::decode(reader).map(Self::from)
    }
}

/// Encodes a `bitflags` value as its underlying integer bits, losslessly
/// (`from_bits_retain` keeps bits the current build does not know about).
#[macro_export]
macro_rules! impl_bitflags_codec {
    ($($ty:ty => $bits:ty),+ $(,)?) => {
        $(
            impl Encode for $ty {
                fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
                    self.bits().encode(writer)
                }

                fn encoded_len(&self) -> usize {
                    size_of::<$bits>()
                }
            }

            impl Decode for $ty {
                fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
                    Ok(Self::from_bits_retain(<$bits>::decode(reader)?))
                }
            }
        )+
    };
}

/// Encodes a fieldless enum as one discriminant byte. The tag is written
/// next to each variant so the on-wire mapping is auditable in one place;
/// decoding rejects unknown tags.
#[macro_export]
macro_rules! impl_tagged_codec {
    ($ty:ty, $($tag:literal => $variant:path),+ $(,)?) => {
        impl Encode for $ty {
            fn encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
                match *self {
                    $($variant => writer.write_byte($tag),)+
                    _ => unreachable!("non-exhaustive enum has an unknown variant"),
                }
            }

            fn encoded_len(&self) -> usize {
                1
            }
        }

        impl Decode for $ty {
            fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError> {
                match u8::decode(reader)? {
                    $($tag => Ok($variant),)+
                    _ => Err(DecodeError::InvalidValue),
                }
            }
        }
    };
}
