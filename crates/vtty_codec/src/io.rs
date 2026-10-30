// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal byte sink/source abstractions and their slice/vec adapters.

use alloc::vec::Vec;

use crate::error::{DecodeError, EncodeError};

/// A minimal byte sink for encoding (a `no_std` subset of `std::io::Write`).
pub trait Write {
    /// Appends `buf` to the sink.
    fn write(&mut self, buf: &[u8]) -> Result<(), EncodeError>;

    /// Appends a single byte to the sink.
    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<(), EncodeError> {
        self.write(&[byte])
    }
}

/// A minimal byte source for decoding (a `no_std` subset of `std::io::Read`).
pub trait Read {
    /// Copies up to `buf.len()` bytes into `buf`, returning how many were
    /// copied.
    ///
    /// `Ok(0)` means end of input.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, DecodeError>;

    /// Reads a single byte.
    #[inline]
    fn read_byte(&mut self) -> Result<u8, DecodeError> {
        let mut byte = [0_u8; 1];
        match self.read(&mut byte)? {
            1 => Ok(byte[0]),
            _ => Err(DecodeError::UnexpectedEof),
        }
    }

    /// Reads exactly `buf.len()` bytes.
    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), DecodeError> {
        let mut filled = 0;
        while filled < buf.len() {
            let n = self.read(&mut buf[filled..])?;
            if n == 0 {
                return Err(DecodeError::UnexpectedEof);
            }
            filled += n;
        }
        Ok(())
    }
}

/// Encodes into a caller-provided fixed-size buffer.
///
/// Each [`Write::write`] is atomic: a write that would run past the end fails
/// with [`EncodeError::BufferTooSmall`] and leaves the buffer unchanged.
#[derive(Debug)]
pub struct SliceWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> SliceWriter<'a> {
    /// Wraps `buf`, starting at offset `0`.
    pub const fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// The number of bytes written so far.
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// The number of bytes that can still be written.
    pub const fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// The written prefix of the buffer.
    pub fn into_written(self) -> &'a mut [u8] {
        &mut self.buf[..self.pos]
    }
}

impl Write for SliceWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<(), EncodeError> {
        let end = self
            .pos
            .checked_add(buf.len())
            .ok_or(EncodeError::BufferTooSmall)?;
        let dst = self
            .buf
            .get_mut(self.pos..end)
            .ok_or(EncodeError::BufferTooSmall)?;
        dst.copy_from_slice(buf);
        self.pos = end;
        Ok(())
    }
}

/// Decodes from a caller-provided byte slice.
#[derive(Debug)]
pub struct SliceReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> SliceReader<'a> {
    /// Wraps `buf`, starting at offset `0`.
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// The number of bytes consumed so far.
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// The number of bytes left to read.
    pub const fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Whether all bytes have been consumed.
    pub const fn is_empty(&self) -> bool {
        self.pos == self.buf.len()
    }
}

impl Read for SliceReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, DecodeError> {
        let n = buf.len().min(self.buf.len() - self.pos);
        buf[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Encodes into an owned `Vec<u8>`, growing it as needed.
#[derive(Debug)]
pub struct VecWriter<'a> {
    buf: &'a mut Vec<u8>,
}

impl<'a> VecWriter<'a> {
    /// Wraps `buf`, appending to it.
    pub const fn new(buf: &'a mut Vec<u8>) -> Self {
        Self { buf }
    }

    /// The number of bytes written so far.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// The encoded bytes written so far.
    pub fn as_slice(&self) -> &[u8] {
        self.buf.as_slice()
    }
}

impl Write for VecWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<(), EncodeError> {
        self.buf.extend_from_slice(buf);
        Ok(())
    }
}

/// A sink that counts bytes without storing them, used to compute
/// [`crate::Encode::encoded_len`].
#[derive(Debug, Default)]
pub(crate) struct CountWriter {
    len: usize,
}

impl CountWriter {
    /// An empty counter.
    pub(crate) const fn new() -> Self {
        Self { len: 0 }
    }

    /// The number of bytes written so far.
    pub(crate) const fn len(&self) -> usize {
        self.len
    }
}

impl Write for CountWriter {
    fn write(&mut self, buf: &[u8]) -> Result<(), EncodeError> {
        self.len += buf.len();
        Ok(())
    }
}
