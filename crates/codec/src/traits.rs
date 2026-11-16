// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The [`Encode`] and [`Decode`] traits.

use crate::error::{DecodeError, EncodeError};
use crate::io::{CountWriter, Read, Write};

/// A value that can be encoded into a [`Write`] sink.
pub trait Encode {
    /// Encodes `self` into `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::BufferTooSmall`] when `writer` has no room,
    /// or [`EncodeError::UnknownVariant`] when a `#[non_exhaustive]` enum
    /// carries a variant this build does not know.
    fn encode<W: Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<(), EncodeError>;

    /// The exact number of bytes [`Encode::encode`] will write.
    ///
    /// The default encodes into a `CountWriter`; override it only when a
    /// closed-form length is cheaper than re-running the encode logic.
    fn encoded_len(&self) -> usize {
        let mut counter = CountWriter::new();
        // `CountWriter` never fails, so the result is always `Ok`.
        let _ = self.encode(&mut counter);
        counter.len()
    }
}

/// A value that can be decoded from a [`Read`] source.
pub trait Decode: Sized {
    /// Decodes a value from `reader`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::UnexpectedEof`] when the input ends early, or
    /// [`DecodeError::InvalidValue`] when the bytes are not a valid encoding.
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, DecodeError>;
}
