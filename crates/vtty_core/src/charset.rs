// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! G0/G1 charset designation (`ESC ( 0` = DEC Special Graphics).
//!
//! SCS (Select Character Set) follows ECMA-35 (ISO 2022); the GL/GR byte
//! treatment follows the [DEC parser state
//! diagram](https://vt100.net/emu/dec_ansi_parser). The DEC Special Graphics
//! glyph table is from EK-VT520-RM (SCS).

/// DEC Special Graphics translation for `0x5f..=0x7e`, used when G0/G1 is
/// designated to charset `0` (`ESC ( 0` / `ESC ) 0`).
#[inline]
pub const fn dec_special_graphics(c: char) -> char {
    match c {
        '_' => ' ',
        '`' => '◆',
        'a' => '▒',
        'b' => '␉',
        'c' => '␌',
        'd' => '␍',
        'e' => '␊',
        'f' => '°',
        'g' => '±',
        'h' => '␤',
        'i' => '␋',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => c,
    }
}

/// A designated charset for G0/G1 (`ESC ( 0` = DEC Special Graphics).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Charset {
    /// US-ASCII (the default).
    Ascii,
    /// DEC Special Graphics — maps ASCII `0x5f..=0x7e` to line-drawing and
    /// special glyphs (`─│┌┐└┘` etc.), which vim/tmux use to draw borders.
    Dec,
    /// British (`ESC ( A`) — maps `#` to `£`.
    British,
}

impl Charset {
    /// Translate a char through the designated charset.
    #[inline]
    pub const fn map(self, c: char) -> char {
        match self {
            Self::Ascii => c,
            Self::Dec => match c {
                '\u{5f}'..='\u{7e}' => dec_special_graphics(c),
                _ => c,
            },
            Self::British => {
                if c == '#' {
                    '£'
                } else {
                    c
                }
            }
        }
    }
}

/// The four ISO 2022 designated character sets (G0, G1, G2, G3), selected by
/// the `ESC ( / ) / * / +` designation sequences and the SO/SI/SS2/SS3 shift
/// controls. A terminal carries these as one unit: DECSC/DECRC save and
/// restore all four together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Charsets([Charset; 4]);

impl Charsets {
    /// All slots designated to US-ASCII (the power-on state).
    pub const fn ascii() -> Self {
        Self([Charset::Ascii; 4])
    }

    /// Designate slot `g` to `charset`; a code past the last slot clamps to
    /// G3 (the last slot).
    pub fn designate(&mut self, g: usize, charset: Charset) {
        self.0[g.min(3)] = charset;
    }

    /// Read slot `g`; panics for an out-of-range slot (`g > 3`).
    pub const fn get(self, g: usize) -> Charset {
        assert!(g < 4, "charset slot must be 0..=3");
        self.0[g]
    }

    /// The G0 set (the default, and SI `0x0F` target).
    pub const fn g0(self) -> Charset {
        self.0[0]
    }

    /// The G1 set (SO `0x0E` shift target).
    pub const fn g1(self) -> Charset {
        self.0[1]
    }

    /// The G2 set (SS2 `0x8E` single-shift target).
    pub const fn g2(self) -> Charset {
        self.0[2]
    }

    /// The G3 set (SS3 `0x8F` single-shift target).
    pub const fn g3(self) -> Charset {
        self.0[3]
    }
}

impl Default for Charsets {
    fn default() -> Self {
        Self::ascii()
    }
}

impl core::ops::Index<usize> for Charsets {
    type Output = Charset;

    fn index(&self, g: usize) -> &Charset {
        &self.0[g]
    }
}

impl core::ops::IndexMut<usize> for Charsets {
    fn index_mut(&mut self, g: usize) -> &mut Charset {
        &mut self.0[g]
    }
}

/// The full ISO 2022 SCS (Select Character Set) state: the four designated
/// G-sets plus the GL locking-shift slot and the SS2/SS3 single shift. One
/// `Copy` value shared by the cell and line screens so the active-charset
/// selection lives in ONE place (teken's `t_scs_gl`/`t_curscs` over `t_scs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scs {
    /// The four designated G-sets (G0..=G3).
    pub charsets: Charsets,
    /// The GL locking-shift slot (ECMA-35 LS0..=LS3): 0..=3.
    pub gl: u8,
    /// SS2/SS3 single shift for the NEXT printable char only.
    pub single_shift: Option<u8>,
}

impl Scs {
    /// The power-on state: all G-sets ASCII, GL on G0, no single shift.
    pub const fn ascii() -> Self {
        Self {
            charsets: Charsets::ascii(),
            gl: 0,
            single_shift: None,
        }
    }

    /// Designate slot `g` to `charset` (delegates to [`Charsets::designate`]).
    pub fn designate(&mut self, g: usize, charset: Charset) {
        self.charsets.designate(g, charset);
    }

    /// Set the GL locking-shift slot (LS0..=LS3).
    pub fn shift(&mut self, g: u8) {
        self.gl = g.min(3);
    }

    /// The active charset: an SS2/SS3 single shift wins for one char, then the
    /// GL locking shift, otherwise G0.
    pub fn active(&self) -> Charset {
        match self.single_shift {
            Some(2) => self.charsets.g2(),
            Some(3) => self.charsets.g3(),
            _ => match self.gl {
                1 => self.charsets.g1(),
                2 => self.charsets.g2(),
                3 => self.charsets.g3(),
                _ => self.charsets.g0(),
            },
        }
    }

    /// Translate a char through the active charset.
    pub fn map(&self, c: char) -> char {
        self.active().map(c)
    }
}

impl Default for Scs {
    fn default() -> Self {
        Self::ascii()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec_special_graphics_translation() {
        assert_eq!(dec_special_graphics('_'), ' ');
        assert_eq!(Charset::Dec.map('_'), ' ');
        assert_eq!(Charset::Dec.map('q'), '─');
        assert_eq!(Charset::Dec.map('x'), '│');
        assert_eq!(Charset::Dec.map('a'), '▒');
        assert_eq!(Charset::Dec.map('~'), '·');
        // Outside the mapped range is a passthrough.
        assert_eq!(Charset::Dec.map('A'), 'A');
    }

    #[test]
    fn charset_map() {
        assert_eq!(Charset::Ascii.map('q'), 'q');
        assert_eq!(Charset::Dec.map('q'), '─');
        assert_eq!(Charset::Dec.map('λ'), 'λ');
    }

    #[test]
    fn charsets_get() {
        let mut sets = Charsets::ascii();
        sets.designate(0, Charset::Dec);
        sets.designate(3, Charset::British);
        assert_eq!(sets.get(0), Charset::Dec);
        assert_eq!(sets.get(3), Charset::British);
    }

    #[test]
    #[should_panic(expected = "charset slot must be 0..=3")]
    fn charsets_get_out_of_range_panics() {
        Charsets::ascii().get(4);
    }
}

#[cfg(test)]
mod scs_tests {
    use super::*;

    #[test]
    fn scs_ls2_ls3_lock_and_map() {
        let mut s = Scs::ascii();
        s.designate(2, Charset::Dec); // G2 = DEC Special Graphics
        s.designate(3, Charset::British); // G3 = British
        s.shift(2); // LS2
        assert_eq!(s.map('q'), '─');
        s.shift(3); // LS3
        assert_eq!(s.map('#'), '£');
        s.shift(0); // LS0
        assert_eq!(s.map('q'), 'q');
    }

    #[test]
    fn scs_single_shift_wins_for_one_char() {
        let mut s = Scs::ascii();
        s.designate(2, Charset::Dec);
        s.single_shift = Some(2);
        assert_eq!(s.map('q'), '─');
        s.single_shift = None;
        assert_eq!(s.map('q'), 'q');
    }
}
