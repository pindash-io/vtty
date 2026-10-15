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
