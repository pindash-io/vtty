// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Browser demo: feed bytes to [`vtty::Parser`], drive a
//! [`vtty_screen::Screen`] grid, and render the styled cells as HTML.

use std::fmt::Write as _;

use vtty::{ColorId, Parser, SgrFlags};
use vtty_screen::Screen;
use wasm_bindgen::prelude::*;

/// Demo grid size.
const COLS: u32 = 80;
const ROWS: u32 = 24;

/// A parser + terminal screen, exposed to JavaScript.
#[wasm_bindgen]
pub struct Terminal {
    parser: Parser,
    screen: Screen,
}

#[wasm_bindgen]
impl Terminal {
    /// Create an empty 80×24 terminal.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Terminal {
        Terminal {
            parser: Parser::with_simd(),
            screen: Screen::new(COLS, ROWS),
        }
    }

    /// Feed a UTF-8 string, which may contain ANSI/VT escape sequences.
    pub fn feed(&mut self, text: &str) {
        let mut parser = std::mem::take(&mut self.parser);
        // Mimic a PTY's ONLCR: translate LF to CRLF (normalizing any existing
        // CRLF first). The screen's LF keeps the column, so a bare `\n` would
        // otherwise indent every following line to the previous line's end.
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        parser.advance(&mut self.screen, text.as_bytes());
        self.parser = parser;
    }

    /// Reset to a blank screen, keeping the current size and theme.
    pub fn clear(&mut self) {
        self.parser = Parser::with_simd();
        self.screen.reset();
    }

    /// Resize the grid (reflowing existing content) to `cols × rows`.
    pub fn resize(&mut self, cols: u32, rows: u32) {
        self.screen.resize(cols, rows);
    }

    /// Apply terminal colors derived from a highlight theme: the default
    /// fg/bg plus the three ANSI primaries (red, green, blue). The rest of
    /// the 16-color palette is derived by 50% mixing.
    pub fn set_theme(&mut self, fg: u32, bg: u32, red: u32, green: u32, blue: u32) {
        let yellow = vtty::blend_faint(red, green);
        let magenta = vtty::blend_faint(red, blue);
        let cyan = vtty::blend_faint(green, blue);
        let gray = vtty::blend_faint(bg, fg);
        let bright = |c: u32| vtty::blend_faint(c, fg);
        let palette = [
            bg,
            red,
            green,
            yellow,
            blue,
            magenta,
            cyan,
            fg,
            gray,
            bright(red),
            bright(green),
            bright(yellow),
            bright(blue),
            bright(magenta),
            bright(cyan),
            fg,
        ];
        self.screen.set_palette(palette);
        self.screen.set_default_colors(fg, bg);
    }

    /// The grid as plain text, one line per row (trailing empty rows trimmed).
    pub fn text(&self) -> String {
        let mut lines = self.screen.lines();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// The grid as HTML with SGR styling (16/256/truecolor, bold, italic,
    /// underline, strikethrough, inverse).
    pub fn render(&self) -> String {
        // Fill the whole viewport with the theme's background.
        let bg = self.screen.color_rgb(vtty::DEFAULT_BG_ID).unwrap_or(0);
        let mut html = format!("<pre class=\"term\" style=\"background:#{bg:06x}\">");
        let mut last_nonempty = self.screen.rows() as i64 - 1;
        while last_nonempty >= 0 && self.row_is_blank(last_nonempty as u32) {
            last_nonempty -= 1;
        }
        for row in 0..=last_nonempty.max(0) as u32 {
            if row > 0 {
                html.push('\n');
            }
            html.push_str(&self.render_row(row));
        }
        html.push_str("</pre>");
        html
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal {
    /// Whether every cell in the row is blank (a default-styled space). This
    /// matches `render_row`'s trimming, so `render` doesn't drop rows that
    /// contain only colored spaces (e.g. a wrapped background run).
    fn row_is_blank(&self, row: u32) -> bool {
        (0..self.screen.cols()).all(|col| is_blank(self.screen.cell(row, col)))
    }

    /// Render one row as inline-styled `<span>`s, trimming trailing blanks.
    fn render_row(&self, row: u32) -> String {
        let mut out = String::new();
        let mut prev: Option<(SgrFlags, ColorId, ColorId)> = None;
        let mut last = self.screen.cols() as i64 - 1;
        while last >= 0 && is_blank(self.screen.cell(row, last as u32)) {
            last -= 1;
        }
        if last < 0 {
            return out;
        }
        for col in 0..=last as u32 {
            let Some(cell) = self.screen.cell(row, col) else {
                continue;
            };
            if cell.is_continuation() {
                continue;
            }
            let state = (cell.sgr, cell.fg, cell.bg);
            if prev != Some(state) {
                if prev.is_some() {
                    out.push_str("</span>");
                }
                out.push_str(&span_open(cell, &self.screen));
                prev = Some(state);
            }
            out.push_str(&escape_char(cell.ch));
            for &mark in cell.marks() {
                out.push_str(&escape_char(mark));
            }
        }
        if prev.is_some() {
            out.push_str("</span>");
        }
        out
    }
}

/// True when a cell is blank (space, default attributes) — such cells are
/// trimmed from the end of a rendered row.
fn is_blank(cell: Option<&vtty_screen::Cell>) -> bool {
    cell.is_none_or(vtty_screen::Cell::is_blank)
}

/// The opening `<span style="…">` for a cell's SGR attributes.
fn span_open(cell: &vtty_screen::Cell, screen: &Screen) -> String {
    let mut fg = screen.color_rgb(cell.fg);
    let mut bg = screen.color_rgb(cell.bg);
    if cell.sgr.contains(SgrFlags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    let mut s = String::from("<span style=\"");
    if let Some(c) = fg {
        let _ = write!(s, "color:#{c:06x};");
    }
    if let Some(c) = bg {
        let _ = write!(s, "background-color:#{c:06x};");
    }
    if cell.sgr.contains(SgrFlags::BOLD) {
        s.push_str("font-weight:bold;");
    }
    if cell.sgr.contains(SgrFlags::ITALIC) {
        s.push_str("font-style:italic;");
    }
    let mut deco = Vec::new();
    if cell.sgr.contains(SgrFlags::UNDERLINE) {
        deco.push("underline");
    }
    if cell.sgr.contains(SgrFlags::STRIKETHROUGH) {
        deco.push("line-through");
    }
    if !deco.is_empty() {
        let _ = write!(s, "text-decoration:{};", deco.join(" "));
    }
    s.push_str("\">");
    s
}

/// Escape one char for HTML.
fn escape_char(c: char) -> String {
    match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        _ => c.to_string(),
    }
}
