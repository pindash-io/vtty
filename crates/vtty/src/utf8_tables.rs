//! Compile-time UTF-8→UTF-32 transcode lookup tables.
//!
//! Computed by `const fn` from simdutf's `shufutf8`/`utf8bigindex` algorithm
//! — no table data is checked in. A 12-bit end-of-code-point mask (`bit i =
//! byte i ends a code point`) yields code-point sizes (gaps between set bits),
//! classified as `case12` (6×1-2-byte), `case123` (4×1-3-byte), or `case1234`
//! (3×1-4-byte); otherwise an error sentinel `(209, 12)`. `UTF8BIGINDEX[mask]`
//! maps to `(shuffle index, consumed bytes)`; `SHUFUTF8[idx]` is the shuffle
//! that gathers each code point's bytes into one lane.

/// 209 × 16-byte shuffle patterns.
pub(crate) const SHUFUTF8: [[u8; 16]; 209] = build_shufutf8();

/// 4096 × (shuffle index, consumed bytes).
pub(crate) const UTF8BIGINDEX: [[u8; 2]; 4096] = build_utf8bigindex();

/// Lexicographic rank of a 6-tuple of 1..=2 (indices 0..=63).
const fn rank6(s: [u8; 6]) -> u8 {
    ((s[0] - 1) << 5)
        + ((s[1] - 1) << 4)
        + ((s[2] - 1) << 3)
        + ((s[3] - 1) << 2)
        + ((s[4] - 1) << 1)
        + (s[5] - 1)
}

/// Lexicographic rank of a 4-tuple of 1..=3 (indices 64..=144).
const fn rank4(s: [u8; 4]) -> u8 {
    64 + (s[0] - 1) * 27 + (s[1] - 1) * 9 + (s[2] - 1) * 3 + (s[3] - 1)
}

/// Lexicographic rank of a 3-tuple of 1..=4 (indices 145..=208).
const fn rank3(s: [u8; 3]) -> u8 {
    145 + (s[0] - 1) * 16 + (s[1] - 1) * 4 + (s[2] - 1)
}

/// Code-point sizes (gaps between end-of-code-point bits) for a 12-bit mask.
/// The unused tail is zero-padded.
const fn codepoint_sizes(mask: u32) -> [u8; 12] {
    let mut sizes = [0_u8; 12];
    let mut n = 0_u8;
    let mut prev = 0_u8;
    let mut first = true;
    let mut i = 0_u8;
    while i < 12 {
        if (mask >> i) & 1 == 1 {
            sizes[n as usize] = if first { i + 1 } else { i - prev };
            prev = i;
            first = false;
            n += 1;
        }
        i += 1;
    }
    sizes
}

/// Number of set bits in the 12-bit mask.
const fn codepoint_count(mask: u32) -> u8 {
    let mut n = 0_u8;
    let mut i = 0_u8;
    while i < 12 {
        if (mask >> i) & 1 == 1 {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Classify a 12-bit mask into `(shuffle index, consumed bytes)`.
const fn classify(mask: u32) -> (u8, u8) {
    let sizes = codepoint_sizes(mask);
    let len = codepoint_count(mask);
    if len >= 6
        && sizes[0] <= 2
        && sizes[1] <= 2
        && sizes[2] <= 2
        && sizes[3] <= 2
        && sizes[4] <= 2
        && sizes[5] <= 2
    {
        let s = [sizes[0], sizes[1], sizes[2], sizes[3], sizes[4], sizes[5]];
        return (rank6(s), s[0] + s[1] + s[2] + s[3] + s[4] + s[5]);
    }
    if len >= 4 && sizes[0] <= 3 && sizes[1] <= 3 && sizes[2] <= 3 && sizes[3] <= 3 {
        let s = [sizes[0], sizes[1], sizes[2], sizes[3]];
        return (rank4(s), s[0] + s[1] + s[2] + s[3]);
    }
    if len >= 3 && sizes[0] <= 4 && sizes[1] <= 4 && sizes[2] <= 4 {
        let s = [sizes[0], sizes[1], sizes[2]];
        return (rank3(s), s[0] + s[1] + s[2]);
    }
    (209, 12)
}

/// Six code points of 1..=2 bytes → six u16 lanes (12 shuffle entries).
const fn shuf_case12(s: [u8; 6]) -> [u8; 16] {
    let mut out = [0_u8; 16];
    let mut pos: u8 = 0;
    let mut i = 0_usize;
    while i < 6 {
        if s[i] == 1 {
            out[2 * i] = pos;
            out[2 * i + 1] = 0xff;
            pos += 1;
        } else {
            out[2 * i] = pos + 1;
            out[2 * i + 1] = pos;
            pos += 2;
        }
        i += 1;
    }
    out
}

/// Four code points of 1..=3 bytes → four u32 lanes.
const fn shuf_case123(s: [u8; 4]) -> [u8; 16] {
    let mut out = [0_u8; 16];
    let mut pos: u8 = 0;
    let mut i = 0_usize;
    while i < 4 {
        match s[i] {
            1 => {
                out[4 * i] = pos;
                out[4 * i + 1] = 0xff;
                out[4 * i + 2] = 0xff;
                out[4 * i + 3] = 0xff;
                pos += 1;
            }
            2 => {
                out[4 * i] = pos + 1;
                out[4 * i + 1] = pos;
                out[4 * i + 2] = 0xff;
                out[4 * i + 3] = 0xff;
                pos += 2;
            }
            _ => {
                out[4 * i] = pos + 2;
                out[4 * i + 1] = pos + 1;
                out[4 * i + 2] = pos;
                out[4 * i + 3] = 0xff;
                pos += 3;
            }
        }
        i += 1;
    }
    out
}

/// Three code points of 1..=4 bytes → three u32 lanes.
const fn shuf_case1234(s: [u8; 3]) -> [u8; 16] {
    let mut out = [0_u8; 16];
    let mut pos: u8 = 0;
    let mut i = 0_usize;
    while i < 3 {
        match s[i] {
            1 => {
                out[4 * i] = pos;
                out[4 * i + 1] = 0xff;
                out[4 * i + 2] = 0xff;
                out[4 * i + 3] = 0xff;
                pos += 1;
            }
            2 => {
                out[4 * i] = pos + 1;
                out[4 * i + 1] = pos;
                out[4 * i + 2] = 0xff;
                out[4 * i + 3] = 0xff;
                pos += 2;
            }
            3 => {
                out[4 * i] = pos + 2;
                out[4 * i + 1] = pos + 1;
                out[4 * i + 2] = pos;
                out[4 * i + 3] = 0xff;
                pos += 3;
            }
            _ => {
                out[4 * i] = pos + 3;
                out[4 * i + 1] = pos + 2;
                out[4 * i + 2] = pos + 1;
                out[4 * i + 3] = pos;
                pos += 4;
            }
        }
        i += 1;
    }
    out
}

/// Build `SHUFUTF8`: 64 case12 + 81 case123 + 64 case1234 patterns, each in
/// lexicographic order.
const fn build_shufutf8() -> [[u8; 16]; 209] {
    let mut table = [[0_u8; 16]; 209];
    let mut idx = 0_usize;

    // case12: 6-tuples of 1..=2.
    let mut a = 1_u8;
    while a <= 2 {
        let mut b = 1_u8;
        while b <= 2 {
            let mut c = 1_u8;
            while c <= 2 {
                let mut d = 1_u8;
                while d <= 2 {
                    let mut e = 1_u8;
                    while e <= 2 {
                        let mut f = 1_u8;
                        while f <= 2 {
                            table[idx] = shuf_case12([a, b, c, d, e, f]);
                            idx += 1;
                            f += 1;
                        }
                        e += 1;
                    }
                    d += 1;
                }
                c += 1;
            }
            b += 1;
        }
        a += 1;
    }

    // case123: 4-tuples of 1..=3.
    let mut a = 1_u8;
    while a <= 3 {
        let mut b = 1_u8;
        while b <= 3 {
            let mut c = 1_u8;
            while c <= 3 {
                let mut d = 1_u8;
                while d <= 3 {
                    table[idx] = shuf_case123([a, b, c, d]);
                    idx += 1;
                    d += 1;
                }
                c += 1;
            }
            b += 1;
        }
        a += 1;
    }

    // case1234: 3-tuples of 1..=4.
    let mut a = 1_u8;
    while a <= 4 {
        let mut b = 1_u8;
        while b <= 4 {
            let mut c = 1_u8;
            while c <= 4 {
                table[idx] = shuf_case1234([a, b, c]);
                idx += 1;
                c += 1;
            }
            b += 1;
        }
        a += 1;
    }

    table
}

/// Build `UTF8BIGINDEX`: classify every 12-bit mask.
const fn build_utf8bigindex() -> [[u8; 2]; 4096] {
    let mut table = [[0_u8; 2]; 4096];
    let mut mask = 0_u32;
    while mask < 4096 {
        let (idx, consumed) = classify(mask);
        table[mask as usize] = [idx, consumed];
        mask += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent reference: the Python generator's algorithm with the
    /// dedup + sort done by a `BTreeSet` instead of const-fn ranking.
    fn ref_sizes(mask: u32) -> Vec<usize> {
        let mut v = Vec::new();
        let mut old = -1_isize;
        for i in 0..12 {
            if (mask >> i) & 1 == 1 {
                v.push((i as isize - old) as usize);
                old = i as isize;
            }
        }
        v
    }

    fn ref_shuf12(sizes: &[usize]) -> [u8; 16] {
        let mut out = [0_u8; 16];
        let mut pos: u8 = 0;
        for (i, &s) in sizes.iter().enumerate() {
            if s == 1 {
                out[2 * i] = pos;
                out[2 * i + 1] = 0xff;
                pos += 1;
            } else {
                out[2 * i] = pos + 1;
                out[2 * i + 1] = pos;
                pos += 2;
            }
        }
        out
    }

    fn ref_shuf123(sizes: &[usize]) -> [u8; 16] {
        let mut out = [0_u8; 16];
        let mut pos: u8 = 0;
        for (i, &s) in sizes.iter().enumerate() {
            match s {
                1 => {
                    out[4 * i] = pos;
                    out[4 * i + 1] = 0xff;
                    out[4 * i + 2] = 0xff;
                    out[4 * i + 3] = 0xff;
                    pos += 1;
                }
                2 => {
                    out[4 * i] = pos + 1;
                    out[4 * i + 1] = pos;
                    out[4 * i + 2] = 0xff;
                    out[4 * i + 3] = 0xff;
                    pos += 2;
                }
                _ => {
                    out[4 * i] = pos + 2;
                    out[4 * i + 1] = pos + 1;
                    out[4 * i + 2] = pos;
                    out[4 * i + 3] = 0xff;
                    pos += 3;
                }
            }
        }
        out
    }

    fn ref_shuf1234(sizes: &[usize]) -> [u8; 16] {
        let mut out = [0_u8; 16];
        let mut pos: u8 = 0;
        for (i, &s) in sizes.iter().enumerate() {
            match s {
                1 => {
                    out[4 * i] = pos;
                    out[4 * i + 1] = 0xff;
                    out[4 * i + 2] = 0xff;
                    out[4 * i + 3] = 0xff;
                    pos += 1;
                }
                2 => {
                    out[4 * i] = pos + 1;
                    out[4 * i + 1] = pos;
                    out[4 * i + 2] = 0xff;
                    out[4 * i + 3] = 0xff;
                    pos += 2;
                }
                3 => {
                    out[4 * i] = pos + 2;
                    out[4 * i + 1] = pos + 1;
                    out[4 * i + 2] = pos;
                    out[4 * i + 3] = 0xff;
                    pos += 3;
                }
                _ => {
                    out[4 * i] = pos + 3;
                    out[4 * i + 1] = pos + 2;
                    out[4 * i + 2] = pos + 1;
                    out[4 * i + 3] = pos;
                    pos += 4;
                }
            }
        }
        out
    }

    #[test]
    fn tables_match_simdutf_reference() {
        use std::collections::{BTreeSet, HashMap};

        let mut c12: BTreeSet<Vec<usize>> = BTreeSet::new();
        let mut c123: BTreeSet<Vec<usize>> = BTreeSet::new();
        let mut c1234: BTreeSet<Vec<usize>> = BTreeSet::new();
        for x in 0..4096_u32 {
            let s = ref_sizes(x);
            if s.len() >= 6 && s[..6].iter().all(|&v| v <= 2) {
                c12.insert(s[..6].to_vec());
            } else if s.len() >= 4 && s[..4].iter().all(|&v| v <= 3) {
                c123.insert(s[..4].to_vec());
            } else if s.len() >= 3 && s[..3].iter().all(|&v| v <= 4) {
                c1234.insert(s[..3].to_vec());
            }
        }

        let patterns: Vec<Vec<usize>> = c12
            .iter()
            .chain(c123.iter())
            .chain(c1234.iter())
            .cloned()
            .collect();
        assert_eq!(patterns.len(), 209, "three cases must total 209 patterns");
        let index: HashMap<Vec<usize>, usize> = patterns
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i))
            .collect();

        for (i, p) in patterns.iter().enumerate() {
            let expected = if p.len() == 6 {
                ref_shuf12(p)
            } else if p.len() == 4 {
                ref_shuf123(p)
            } else {
                ref_shuf1234(p)
            };
            assert_eq!(SHUFUTF8[i], expected, "shufutf8[{i}] for {p:?}");
        }

        for x in 0..4096_u32 {
            let s = ref_sizes(x);
            let (idx, consumed) = if s.len() >= 6 && s[..6].iter().all(|&v| v <= 2) {
                (index[&s[..6].to_vec()], s[..6].iter().sum())
            } else if s.len() >= 4 && s[..4].iter().all(|&v| v <= 3) {
                (index[&s[..4].to_vec()], s[..4].iter().sum())
            } else if s.len() >= 3 && s[..3].iter().all(|&v| v <= 4) {
                (index[&s[..3].to_vec()], s[..3].iter().sum())
            } else {
                (209, 12)
            };
            assert_eq!(
                UTF8BIGINDEX[x as usize],
                [idx as u8, consumed as u8],
                "utf8bigindex[0x{x:03x}]"
            );
        }
    }
}
