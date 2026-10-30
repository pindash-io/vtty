// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Top-level helpers and streaming/iterator decoders.

use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use crate::error::{DecodeError, EncodeError};
use crate::io::{SliceReader, VecWriter};
use crate::traits::{Decode, Encode};

/// Encodes `value` into a freshly allocated `Vec<u8>`.
pub fn encode_to_vec<T: Encode + ?Sized>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::with_capacity(value.encoded_len());
    value.encode(&mut VecWriter::new(&mut buf))?;
    Ok(buf)
}

/// Decodes a value from `bytes`.
pub fn decode_from_slice<T: Decode>(bytes: &[u8]) -> Result<T, DecodeError> {
    T::decode(&mut SliceReader::new(bytes))
}

/// A streaming, push-based decoder: feed bytes incrementally and receive the
/// value once its complete encoding has arrived.
///
/// Unlike [`Decode::decode`], which needs the whole buffer up front, this
/// retains partial input across calls. Each feed re-decodes the buffered
/// prefix, so prefer a few large chunks over many one-byte feeds.
pub struct Decoder<T> {
    buf: Vec<u8>,
    marker: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for Decoder<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("buf", &self.buf)
            .finish_non_exhaustive()
    }
}

impl<T> Decoder<T> {
    /// An empty decoder.
    pub const fn new() -> Self {
        Self {
            buf: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<T> Default for Decoder<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Decode> Decoder<T> {
    /// Feeds `chunk`, returning the decoded value once it is complete.
    ///
    /// `Ok(None)` means more bytes are needed; truncation is not an error.
    /// `Err` is returned only for an invalid encoding.
    pub fn advance(&mut self, chunk: &[u8]) -> Result<Option<T>, DecodeError> {
        self.buf.extend_from_slice(chunk);
        let mut reader = SliceReader::new(&self.buf);
        match T::decode(&mut reader) {
            Ok(value) => {
                self.buf.drain(..reader.position());
                Ok(Some(value))
            }
            Err(DecodeError::UnexpectedEof) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Decodes the final buffered value, erroring on a truncated stream.
    pub fn finish(self) -> Result<T, DecodeError> {
        let mut reader = SliceReader::new(&self.buf);
        let value = T::decode(&mut reader)?;
        if reader.position() != self.buf.len() {
            return Err(DecodeError::UnexpectedEof);
        }
        Ok(value)
    }
}

/// An iterator that decodes a sequence of values from a byte slice.
///
/// [`DecodeIter::next`] returns `None` at the end of the slice, `Some(Ok(T))`
/// for each complete value, and stops after the first `Some(Err(..))` (e.g. a
/// truncated trailing value).
pub struct DecodeIter<'a, T> {
    reader: SliceReader<'a>,
    done: bool,
    marker: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for DecodeIter<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodeIter")
            .field("reader", &self.reader)
            .field("done", &self.done)
            .finish()
    }
}

impl<'a, T: Decode> DecodeIter<'a, T> {
    /// Decodes values from `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            reader: SliceReader::new(bytes),
            done: false,
            marker: PhantomData,
        }
    }
}

impl<T: Decode> Iterator for DecodeIter<'_, T> {
    type Item = Result<T, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.reader.is_empty() {
            return None;
        }
        match T::decode(&mut self.reader) {
            Ok(value) => Some(Ok(value)),
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

/// Iterates over values decoded from `bytes`.
pub fn decode_iter<T: Decode>(bytes: &[u8]) -> DecodeIter<'_, T> {
    DecodeIter::new(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DecodeError;
    use alloc::vec;

    #[test]
    fn decoder_advance_incremental() {
        // `Vec<u8>` encodes as varint len + bytes: [2, b'a', b'b'].
        let mut dec = Decoder::<Vec<u8>>::new();
        assert_eq!(dec.advance(&[2]), Ok(None));
        assert_eq!(dec.advance(b"a"), Ok(None));
        assert_eq!(dec.advance(b"b"), Ok(Some(vec![b'a', b'b'])));
        assert!(dec.buf.is_empty());
    }

    #[test]
    fn decoder_advance_rejects_invalid() {
        // `bool` rejects any byte other than 0 or 1.
        let mut dec = Decoder::<bool>::new();
        assert_eq!(dec.advance(&[2]), Err(DecodeError::InvalidValue));
    }

    #[test]
    fn decoder_finish_errors_on_truncated() {
        assert_eq!(
            Decoder::<Vec<u8>>::new().finish(),
            Err(DecodeError::UnexpectedEof)
        );
        // A length prefix promising more bytes than arrived.
        let mut dec = Decoder::<Vec<u8>>::new();
        assert_eq!(dec.advance(&[3, b'a']), Ok(None));
        assert_eq!(dec.finish(), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn decode_iter_stops_after_truncated_tail() {
        // One complete value, then a `Vec<u8>` whose length promises more
        // bytes than remain.
        let bytes = [1, b'a', 3, b'b'];
        let mut it = decode_iter::<Vec<u8>>(&bytes);
        assert_eq!(it.next(), Some(Ok(vec![b'a'])));
        assert_eq!(it.next(), Some(Err(DecodeError::UnexpectedEof)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn decoder_default_and_debug() {
        use alloc::string::String;
        use core::fmt::Write as _;

        let dec = Decoder::<u8>::default();
        assert_eq!(dec.buf.len(), 0);
        let mut s = String::new();
        let _ = write!(&mut s, "{dec:?}");
        assert!(s.contains("Decoder"));

        let it = decode_iter::<u8>(b"");
        let mut s = String::new();
        let _ = write!(&mut s, "{it:?}");
        assert!(s.contains("DecodeIter"));
    }

    #[test]
    fn decoder_finish_rejects_trailing_bytes() {
        let mut dec = Decoder::<u8>::new();
        dec.buf.extend_from_slice(&[5, 6]);
        assert_eq!(dec.finish(), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn decoder_finish_succeeds_on_complete_buffer() {
        let mut dec = Decoder::<u8>::new();
        dec.buf.extend_from_slice(&[5]);
        assert_eq!(dec.finish(), Ok(5));
    }
}
