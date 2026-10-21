// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal browser demo: feed bytes to [`vtty::Parser`] and render the text
//! with basic SGR styling (16/bright colors, bold, italic, underline,
//! strikethrough, inverse) as HTML.

use std::fmt::Write as _;

use vtty::{Parser, Perform};
use wasm_bindgen::prelude::*;

const ANSI: [&str; 8] = [
    "#000000", "#cd0000", "#00cd00", "#cdcd00", "#0000ee", "#cd00cd", "#00cdcd", "#e5e5e5",
];
const BRIGHT: [&str; 8] = [
    "#7f7f7f", "#ff0000", "#00ff00", "#ffff00", "#5c5cff", "#ff00ff", "#00ffff", "#ffffff",
];

/// The current SGR text style.
#[derive(Clone, Copy, PartialEq, Default)]
struct Style {
    fg: Option<&'static str>,
    bg: Option<&'static str>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    inverse: bool,
}

/// A parser plus a stream of styled text runs, exposed to JavaScript.
#[wasm_bindgen]
pub struct Terminal {
    parser: Parser,
    style: Style,
    cur: String,
    segs: Vec<(Style, String)>,
}

#[wasm_bindgen]
impl Terminal {
    /// Create an empty terminal.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Terminal {
        Terminal {
            parser: Parser::new(),
            style: Style::default(),
            cur: String::new(),
            segs: Vec::new(),
        }
    }

    /// Feed a UTF-8 string, which may contain ANSI/VT escape sequences.
    pub fn feed(&mut self, text: &str) {
        let mut parser = std::mem::take(&mut self.parser);
        parser.advance(self, text.as_bytes());
        self.parser = parser;
    }

    /// Reset to an empty buffer.
    pub fn clear(&mut self) {
        self.parser = Parser::new();
        self.style = Style::default();
        self.cur.clear();
        self.segs.clear();
    }

    /// Render the accumulated text as HTML with SGR styling applied.
    pub fn render(&self) -> String {
        let mut segs = self.segs.clone();
        if !self.cur.is_empty() {
            segs.push((self.style, self.cur.clone()));
        }
        let mut html = String::new();
        for (style, text) in segs {
            let text = escape(&text);
            if style == Style::default() {
                html.push_str(&text);
            } else {
                html.push_str(&span(style, &text));
            }
        }
        html
    }
}

impl Terminal {
    fn set_style(&mut self, style: Style) {
        if style == self.style {
            return;
        }
        if !self.cur.is_empty() {
            self.segs.push((self.style, std::mem::take(&mut self.cur)));
        }
        self.style = style;
    }

    fn sgr(&mut self, params: &[i32]) {
        let mut s = if params.is_empty() {
            Style::default()
        } else {
            self.style
        };
        for &p in params {
            match p {
                0 => s = Style::default(),
                1 => s.bold = true,
                3 => s.italic = true,
                4 => s.underline = true,
                7 => s.inverse = true,
                9 => s.strike = true,
                22 => s.bold = false,
                23 => s.italic = false,
                24 => s.underline = false,
                27 => s.inverse = false,
                29 => s.strike = false,
                30..=37 => s.fg = Some(ANSI[(p - 30) as usize]),
                39 => s.fg = None,
                40..=47 => s.bg = Some(ANSI[(p - 40) as usize]),
                49 => s.bg = None,
                90..=97 => s.fg = Some(BRIGHT[(p - 90) as usize]),
                100..=107 => s.bg = Some(BRIGHT[(p - 100) as usize]),
                _ => {}
            }
        }
        self.set_style(s);
    }
}

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        self.cur.push(c);
    }

    fn print_bytes(&mut self, s: &[u8]) {
        // Printable ASCII: one char per byte.
        self.cur.extend(s.iter().map(|&b| b as char));
    }

    fn execute(&mut self, b: u8) {
        match b {
            b'\n' => self.cur.push('\n'),
            b'\t' => self.cur.push('\t'),
            _ => {} // CR and other controls are not meaningful in a text stream
        }
    }

    fn csi_dispatch(&mut self, params: &[i32], _sep: u16, _i: &[u8], final_byte: u8) {
        if final_byte == b'm' {
            self.sgr(params);
        }
    }
}

fn span(s: Style, text: &str) -> String {
    let (mut fg, mut bg) = (s.fg, s.bg);
    if s.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    let mut out = String::from("<span style=\"");
    if let Some(c) = fg {
        let _ = write!(out, "color:{c};");
    }
    if let Some(c) = bg {
        let _ = write!(out, "background-color:{c};");
    }
    if s.bold {
        out.push_str("font-weight:bold;");
    }
    if s.italic {
        out.push_str("font-style:italic;");
    }
    let mut deco: Vec<&str> = Vec::new();
    if s.underline {
        deco.push("underline");
    }
    if s.strike {
        deco.push("line-through");
    }
    if !deco.is_empty() {
        let _ = write!(out, "text-decoration:{};", deco.join(" "));
    }
    out.push_str("\">");
    out.push_str(text);
    out.push_str("</span>");
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
