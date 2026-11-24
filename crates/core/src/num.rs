// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Decimal formatting for the on-wire encoders.

/// Write `v` as decimal into the tail of `buf`, returning those digits.
///
/// At most `buf.len()` low-order digits are written, so a buffer sized for the
/// largest value a caller produces never truncates.
#[inline]
pub fn write_decimal(v: u32, buf: &mut [u8]) -> &[u8] {
    if buf.is_empty() {
        return buf;
    }
    let mut i = buf.len();
    let mut n = v;
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 || i == 0 {
            return &buf[i..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_every_digit_width() {
        let mut buf = [0_u8; 10];
        assert_eq!(write_decimal(0, &mut buf), b"0");
        assert_eq!(write_decimal(7, &mut buf), b"7");
        assert_eq!(write_decimal(40, &mut buf), b"40");
        assert_eq!(write_decimal(100, &mut buf), b"100");
        assert_eq!(write_decimal(255, &mut buf), b"255");
        assert_eq!(write_decimal(65_535, &mut buf), b"65535");
        assert_eq!(write_decimal(u32::MAX, &mut buf), b"4294967295");
    }

    #[test]
    fn truncates_to_the_buffer_instead_of_panicking() {
        let mut buf = [0_u8; 3];
        assert_eq!(write_decimal(1000, &mut buf), b"000");
        let mut empty: [u8; 0] = [];
        assert_eq!(write_decimal(7, &mut empty), b"");
    }
}
