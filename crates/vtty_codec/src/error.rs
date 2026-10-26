// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Encoding/decoding error types.

use core::fmt;

/// An error produced while encoding a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// The destination buffer is too small to hold the value.
    BufferTooSmall,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall => f.write_str("encode buffer too small"),
        }
    }
}

impl core::error::Error for EncodeError {}

/// An error produced while decoding a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The input ended before the value was complete.
    UnexpectedEof,
    /// The input contained an undecodable value (bad enum tag, invalid
    /// `bool`/`char`, malformed varint, invalid UTF-8, …).
    InvalidValue,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of input"),
            Self::InvalidValue => f.write_str("invalid value"),
        }
    }
}

impl core::error::Error for DecodeError {}
