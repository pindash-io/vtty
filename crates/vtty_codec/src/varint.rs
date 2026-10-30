// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! LEB128 length prefixes.

use crate::error::{DecodeError, EncodeError};
use crate::io::{Read, Write};

/// LEB128-encodes `value` into `writer`.
pub(crate) fn encode_varint<W: Write + ?Sized>(
    writer: &mut W,
    mut value: usize,
) -> Result<(), EncodeError> {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            return writer.write_byte(byte);
        }
        writer.write_byte(byte | 0x80)?;
    }
}

/// LEB128-decodes a `usize` from `reader`.
pub(crate) fn decode_varint<R: Read + ?Sized>(reader: &mut R) -> Result<usize, DecodeError> {
    let mut value = 0_usize;
    let mut shift = 0_u32;
    loop {
        let byte = reader.read_byte()?;
        if shift >= usize::BITS {
            return Err(DecodeError::InvalidValue);
        }
        let payload = usize::from(byte & 0x7f);
        if payload > (usize::MAX >> shift) {
            return Err(DecodeError::InvalidValue);
        }
        value |= payload << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}
