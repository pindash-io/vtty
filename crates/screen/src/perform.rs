// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The `Perform` adapter: the parser's event stream translated into `Screen`
//! mutations.

use alloc::string::String;

use vtty_core::Perform;

use crate::Screen;

impl Screen {
    fn csi(&mut self, params: &[i32], private: bool, f: u8) {
        let n = |i: usize| params.get(i).copied().unwrap_or(1).max(1) as u32;
        if private {
            // DEC private sequences. `CSI ? r` is DECPCTERM, not a DECSTBM
            // form; the screen does not implement it, so it is ignored.
            match f {
                b'h' => self.set_mode(params),   // DECSET
                b'l' => self.reset_mode(params), // DECRST
                b'J' => self.erase_in_display(params.first().copied().unwrap_or(0), true), // DECSED
                b'K' => self.erase_in_line(params.first().copied().unwrap_or(0), true), // DECSEL
                _ => {}
            }
            return;
        }
        match f {
            b'A' => self.cursor_up(n(0)),      // CUU
            b'B' => self.cursor_down(n(0)),    // CUD
            b'C' => self.cursor_forward(n(0)), // CUF
            b'D' => self.cursor_back(n(0)),    // CUB
            b'E' => {
                // CNL
                self.cursor_down(n(0));
                self.active.cursor.col = 0;
            }
            b'F' => {
                // CPL
                self.cursor_up(n(0));
                self.active.cursor.col = 0;
            }
            b'G' => self.cha(n(0)),              // CHA
            b'd' => self.vpa(n(0)),              // VPA
            b'H' | b'f' => self.cup(n(0), n(1)), // CUP / HVP
            b'J' => self.erase_in_display(params.first().copied().unwrap_or(0), false), // ED
            b'K' => {
                self.erase_in_line(params.first().copied().unwrap_or(0), false);
            } // EL
            b'L' => self.insert_lines(n(0)),     // IL
            b'M' => self.delete_lines(n(0)),     // DL
            b'P' => self.delete_chars(n(0)),     // DCH
            b'@' => self.insert_chars(n(0)),     // ICH
            b'X' => self.erase_chars(n(0)),      // ECH
            b'S' => self.scroll_n(false, n(0)),  // SU
            b'T' => self.scroll_n(true, n(0)),   // SD
            b'h' => self.apply_ansi_modes(params, true), // SM
            b'l' => self.apply_ansi_modes(params, false), // RM
            b'm' => self.sgr(params),            // SGR
            b'g' => self.tbc(params),            // TBC
            b'r' => self.set_margins(params),    // DECSTBM
            _ => {}
        }
    }
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn print_bytes(&mut self, s: &[u8]) {
        self.put_ascii(s);
    }

    fn print_codepoints(&mut self, codepoints: &[u32]) {
        // Segment the run into grapheme clusters so ZWJ emoji families write
        // as one cell instead of separate glyphs.
        let text: String = codepoints
            .iter()
            .filter_map(|&cp| char::from_u32(cp))
            .collect();
        for grapheme in vtty_grapheme::graphemes(&text) {
            self.put_cluster(grapheme.as_str(&text));
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => self.linefeed(), // LF / VT / FF
            b'\r' => {
                // CR
                self.clear_wrap();
                self.active.cursor.col = 0;
            }
            b'\t' => {
                // HT
                self.clear_wrap();
                self.active.cursor.col = self.next_tab_stop();
            }
            0x08 => {
                // BS
                self.clear_wrap();
                self.active.cursor.col = self.active.cursor.col.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &[i32], _sep: u16, intermediates: &[u8], f: u8) {
        // DECSTR (`CSI ! p`): soft reset. The `!` intermediate distinguishes
        // it from the DEC private (`?`) sequences handled by `csi`.
        if intermediates.first() == Some(&b'!') && f == b'p' {
            self.soft_reset();
            return;
        }
        // DECSCA (`CSI Ps " q`): protect attribute for subsequent cells.
        if intermediates.first() == Some(&b'"') && f == b'q' {
            self.decsca(params);
            return;
        }
        // VT400 rectangular-area operations (`CSI … $ x/v/z/{/r/t`).
        if intermediates.first() == Some(&b'$') {
            match f {
                b'x' => self.decfra(params),  // DECFRA
                b'v' => self.deccra(params),  // DECCRA
                b'z' => self.decera(params),  // DECERA
                b'{' => self.decsera(params), // DECSERA
                b'r' => self.deccara(params), // DECCARA
                b't' => self.decrara(params), // DECRARA
                _ => {}
            }
            return;
        }
        let private = intermediates.first() == Some(&b'?');
        self.csi(params, private, f);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        // DECALN is the only escape sequence the screen acts on that carries
        // an intermediate byte; everything else uses a bare final byte.
        if matches!(intermediates, [b'#']) && byte == b'8' {
            self.decaln();
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.save_cursor(),    // DECSC
            b'8' => self.restore_cursor(), // DECRC
            b'D' => self.linefeed(),       // IND
            b'H' => self.hts(),            // HTS
            b'M' => self.reverse_index(),  // RI
            b'E' => {
                // NEL
                self.active.cursor.col = 0;
                self.linefeed();
            }
            b'c' => self.reset(), // RIS
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        match params.first().copied() {
            Some(b"0") | Some(b"1") | Some(b"2") => {
                if let Some(t) = params.get(1) {
                    self.title = String::from_utf8_lossy(t).into_owned();
                }
            }
            Some(b"9") => self.osc_progress(params),
            _ => {}
        }
    }
}
