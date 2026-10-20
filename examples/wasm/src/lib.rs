// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A complete browser terminal demo built on the `vtty` parser.
//!
//! `Terminal` is a minimal character-cell screen: it implements
//! [`Perform`](vtty::Perform) so [`Parser::advance`](vtty::Parser::advance)
//! drives it directly (zero intermediate `Event`s), tracks a cursor, SGR
//! colors/styles, cursor movement, erase, and OSC window titles, then renders
//! itself to an HTML string for the browser.
//!
//! Build it with `wasm-pack build --target web` (or `./build.sh`) and serve
//! `www/`. See `www/index.html` + `www/main.js` for the frontend.

use std::fmt::Write as _;

use vtty::{Parser, Perform, StyleFlags};
use wasm_bindgen::prelude::*;
use web_sys::CanvasRenderingContext2d;

/// Default foreground (`#d0d0d0`) and background (`#000000`), packed 0xRRGGBB.
const DEFAULT_FG: u32 = 0xd0d0d0;
const DEFAULT_BG: u32 = 0x000000;
/// Selection highlight background.
const SELECTION_BG: u32 = 0x264f78;

/// The 16 ANSI palette (0-7 standard, 8-15 bright).
const ANSI16: [u32; 16] = [
    0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5, 0x7f7f7f,
    0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
];

/// One screen cell.
#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: u32,
    bg: u32,
    style: StyleFlags,
    inverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            style: StyleFlags::empty(),
            inverse: false,
        }
    }
}

/// Saved cursor state for DECSC (`ESC 7`) / DECRC (`ESC 8`) and
/// `CSI s` / `CSI u`.
#[derive(Clone, Copy)]
struct Saved {
    row: usize,
    col: usize,
    fg: u32,
    bg: u32,
    style: StyleFlags,
    inverse: bool,
}

/// A normalized (start ≤ end, row-major) inclusive cell selection.
#[derive(Clone, Copy)]
struct Sel {
    start: (usize, usize),
    end: (usize, usize),
}

impl Sel {
    fn contains(&self, r: usize, c: usize) -> bool {
        let (sr, sc) = self.start;
        let (er, ec) = self.end;
        (r > sr || (r == sr && c >= sc)) && (r < er || (r == er && c <= ec))
    }
}

/// A character-cell terminal screen exposed to JavaScript.
#[wasm_bindgen]
pub struct Terminal {
    parser: Parser,
    rows: usize,
    cols: usize,
    cells: Vec<Cell>,
    row: usize,
    col: usize,
    fg: u32,
    bg: u32,
    style: StyleFlags,
    inverse: bool,
    autowrap: bool,
    cursor_visible: bool,
    saved: Option<Saved>,
    sel: Option<Sel>,
    title: String,
}

#[wasm_bindgen]
impl Terminal {
    /// Create a `rows` × `cols` blank terminal.
    #[wasm_bindgen(constructor)]
    pub fn new(rows: u32, cols: u32) -> Terminal {
        let rows = (rows.max(1)) as usize;
        let cols = (cols.max(1)) as usize;
        Terminal {
            parser: Parser::new(),
            rows,
            cols,
            cells: vec![Cell::default(); rows * cols],
            row: 0,
            col: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            style: StyleFlags::empty(),
            inverse: false,
            autowrap: true,
            cursor_visible: true,
            saved: None,
            sel: None,
            title: String::new(),
        }
    }

    /// Feed a UTF-8 string of bytes to the parser. Any ANSI/VT sequences it
    /// contains are executed against the screen.
    pub fn feed(&mut self, text: &str) {
        // New output invalidates any selection (matches real terminals).
        self.sel = None;
        // `advance` takes `&mut self` for the sink and `&mut self` for the
        // parser; split the borrow by swapping the parser out for the call.
        let mut parser = std::mem::take(&mut self.parser);
        parser.advance(self, text.as_bytes());
        self.parser = parser;
    }

    /// Hard-reset the screen (and parser) to a blank state.
    pub fn reset(&mut self) {
        self.parser = Parser::new();
        self.reset_screen();
    }

    pub fn rows(&self) -> u32 {
        self.rows as u32
    }

    pub fn cols(&self) -> u32 {
        self.cols as u32
    }

    pub fn cursor_row(&self) -> u32 {
        self.row as u32
    }

    pub fn cursor_col(&self) -> u32 {
        self.col as u32
    }

    /// Set the selection to the inclusive cell range `(r0,c0)..=(r1,c1)`;
    /// the endpoints are normalized so `start ≤ end` in row-major order.
    pub fn select(&mut self, r0: u32, c0: u32, r1: u32, c1: u32) {
        let r0 = (r0 as usize).min(self.rows - 1);
        let c0 = (c0 as usize).min(self.cols - 1);
        let r1 = (r1 as usize).min(self.rows - 1);
        let c1 = (c1 as usize).min(self.cols - 1);
        let (start, end) = if (r0, c0) <= (r1, c1) {
            ((r0, c0), (r1, c1))
        } else {
            ((r1, c1), (r0, c0))
        };
        self.sel = Some(Sel { start, end });
    }

    /// Select the word around `(row, col)` — the maximal run of non-space
    /// cells on that row.
    pub fn select_word(&mut self, row: u32, col: u32) {
        let row = (row as usize).min(self.rows - 1);
        let col = (col as usize).min(self.cols - 1);
        if self.cells[row * self.cols + col].ch == ' ' {
            self.sel = Some(Sel { start: (row, col), end: (row, col) });
            return;
        }
        let mut c0 = col;
        while c0 > 0 && self.cells[row * self.cols + c0 - 1].ch != ' ' {
            c0 -= 1;
        }
        let mut c1 = col;
        while c1 + 1 < self.cols && self.cells[row * self.cols + c1 + 1].ch != ' ' {
            c1 += 1;
        }
        self.sel = Some(Sel { start: (row, c0), end: (row, c1) });
    }

    /// Select an entire row (trailing spaces excluded).
    pub fn select_line(&mut self, row: u32) {
        let row = (row as usize).min(self.rows - 1);
        let mut c1 = self.cols - 1;
        while c1 > 0 && self.cells[row * self.cols + c1].ch == ' ' {
            c1 -= 1;
        }
        self.sel = Some(Sel { start: (row, 0), end: (row, c1) });
    }

    /// Drop the current selection.
    pub fn clear_selection(&mut self) {
        self.sel = None;
    }

    /// The selected text: trailing spaces trimmed per line, lines joined
    /// with `\n`. Empty when there is no selection.
    pub fn selected_text(&self) -> String {
        let Some(sel) = self.sel else {
            return String::new();
        };
        let mut out = String::new();
        for r in sel.start.0..=sel.end.0 {
            let c0 = if r == sel.start.0 { sel.start.1 } else { 0 };
            let c1 = if r == sel.end.0 { sel.end.1 } else { self.cols - 1 };
            let mut line = String::with_capacity(c1 - c0 + 1);
            for c in c0..=c1 {
                line.push(self.cells[r * self.cols + c].ch);
            }
            while line.ends_with(' ') {
                line.pop();
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&line);
        }
        out
    }

    /// The OSC 0/2 window title, if one was set.
    pub fn title(&self) -> String {
        self.title.clone()
    }

    /// Render the screen as HTML: one `<div class="trow">` per row, styled
    /// `<span>`s per run of identically-styled cells.
    pub fn render_html(&self) -> String {
        let mut html = String::with_capacity(self.rows * self.cols * 16);
        for r in 0..self.rows {
            html.push_str("<div class=\"trow\">");
            let mut c = 0;
            while c < self.cols {
                let idx = r * self.cols + c;
                let cell = self.cells[idx];
                let is_cursor = self.cursor_visible && r == self.row && c == self.col;

                // The cursor is a one-cell run with inverted colors.
                if is_cursor {
                    let ch = escape_char(cell.ch);
                    html.push_str(&self.span(&cell, &ch, true));
                    c += 1;
                    continue;
                }

                // Group the run of identically-styled cells that follows.
                let mut end = c + 1;
                while end < self.cols {
                    if self.cursor_visible && r == self.row && end == self.col {
                        break;
                    }
                    let next = self.cells[r * self.cols + end];
                    if next.fg != cell.fg
                        || next.bg != cell.bg
                        || next.style != cell.style
                        || next.inverse != cell.inverse
                    {
                        break;
                    }
                    end += 1;
                }

                let mut text = String::with_capacity(end - c);
                for &cell in &self.cells[idx..r * self.cols + end] {
                    text.push_str(&escape_char(cell.ch));
                }
                html.push_str(&self.span(&cell, &text, false));
                c = end;
            }
            html.push_str("</div>");
        }
        html
    }

    /// Render the screen as plain text (trailing spaces trimmed).
    pub fn plain_text(&self) -> String {
        let mut out = String::with_capacity(self.rows * (self.cols + 1));
        for r in 0..self.rows {
            let mut line = String::with_capacity(self.cols);
            for c in 0..self.cols {
                line.push(self.cells[r * self.cols + c].ch);
            }
            while line.ends_with(' ') {
                line.pop();
            }
            out.push_str(&line);
            if r + 1 < self.rows {
                out.push('\n');
            }
        }
        out
    }

    /// Draw the screen onto a 2D canvas context. `cell_w`/`cell_h` are the
    /// pixel size of one character cell; `cursor_on` is the blink phase from
    /// JavaScript (the cursor is only drawn when both DECTCEM is on and
    /// `cursor_on` is true).
    pub fn draw(
        &self,
        ctx: &CanvasRenderingContext2d,
        cell_w: f64,
        cell_h: f64,
        cursor_on: bool,
    ) {
        let text_y = cell_h * 0.5;
        let underline_y = cell_h - 2.0;
        let strike_y = cell_h * 0.5 - 0.5;

        ctx.set_text_baseline("middle");
        ctx.set_text_align("left");

        // Track the font string so it is only set when it actually changes.
        let mut font: Option<(bool, bool)> = None; // (bold, italic)

        for r in 0..self.rows {
            for c in 0..self.cols {
                let cell = self.cells[r * self.cols + c];
                let (mut fg, mut bg) = (cell.fg, cell.bg);
                if cell.inverse {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let is_cursor = self.cursor_visible && cursor_on && r == self.row && c == self.col;
                if is_cursor {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if self.is_selected(r, c) {
                    bg = SELECTION_BG;
                }

                let x = c as f64 * cell_w;
                let y = r as f64 * cell_h;

                ctx.set_fill_style_str(&js_color(bg));
                ctx.fill_rect(x, y, cell_w, cell_h);

                if cell.ch != ' ' {
                    let bold = cell.style.contains(StyleFlags::BOLD);
                    let italic = cell.style.contains(StyleFlags::ITALIC);
                    if font != Some((bold, italic)) {
                        ctx.set_font(&font_css(bold, italic));
                        font = Some((bold, italic));
                    }
                    ctx.set_fill_style_str(&js_color(fg));
                    let _ = ctx.fill_text(&cell.ch.to_string(), x, y + text_y);
                }

                // Decorations: one thin rect each.
                if cell.style.contains(StyleFlags::UNDERLINE) {
                    ctx.set_fill_style_str(&js_color(fg));
                    ctx.fill_rect(x, y + underline_y, cell_w, 1.0);
                }
                if cell.style.contains(StyleFlags::STRIKETHROUGH) {
                    ctx.set_fill_style_str(&js_color(fg));
                    ctx.fill_rect(x, y + strike_y, cell_w, 1.0);
                }
                if cell.style.contains(StyleFlags::OVERLINE) {
                    ctx.set_fill_style_str(&js_color(fg));
                    ctx.fill_rect(x, y, cell_w, 1.0);
                }
            }
        }
    }
}

// ── Internal screen helpers (not exported) ────────────────────────────

impl Terminal {
    fn is_selected(&self, r: usize, c: usize) -> bool {
        self.sel.is_some_and(|s| s.contains(r, c))
    }

    fn span(&self, cell: &Cell, text: &str, cursor: bool) -> String {
        let (mut fg, mut bg) = (cell.fg, cell.bg);
        if cell.inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cursor {
            std::mem::swap(&mut fg, &mut bg);
        }

        let mut s = String::with_capacity(text.len() + 96);
        if cursor {
            s.push_str("<span class=\"cur\" style=\"");
        } else {
            s.push_str("<span style=\"");
        }
        let _ = write!(s, "color:#{fg:06x};background-color:#{bg:06x};");
        if cell.style.contains(StyleFlags::BOLD) {
            s.push_str("font-weight:bold;");
        }
        if cell.style.contains(StyleFlags::ITALIC) {
            s.push_str("font-style:italic;");
        }
        let mut deco: Vec<&str> = Vec::new();
        if cell.style.contains(StyleFlags::UNDERLINE) {
            deco.push("underline");
        }
        if cell.style.contains(StyleFlags::STRIKETHROUGH) {
            deco.push("line-through");
        }
        if cell.style.contains(StyleFlags::OVERLINE) {
            deco.push("overline");
        }
        if !deco.is_empty() {
            let _ = write!(s, "text-decoration:{};", deco.join(" "));
        }
        s.push_str("\">");
        s.push_str(text);
        s.push_str("</span>");
        s
    }

    fn write_char(&mut self, ch: char) {
        let idx = self.row * self.cols + self.col;
        self.cells[idx] = Cell {
            ch,
            fg: self.fg,
            bg: self.bg,
            style: self.style,
            inverse: self.inverse,
        };
        if self.col + 1 < self.cols {
            self.col += 1;
        } else if self.autowrap {
            self.col = 0;
            self.line_feed();
        }
    }

    fn line_feed(&mut self) {
        if self.row + 1 < self.rows {
            self.row += 1;
        } else {
            self.scroll_up();
        }
    }

    fn scroll_up(&mut self) {
        for r in 0..self.rows - 1 {
            for c in 0..self.cols {
                self.cells[r * self.cols + c] = self.cells[(r + 1) * self.cols + c];
            }
        }
        for c in 0..self.cols {
            self.cells[(self.rows - 1) * self.cols + c] = Cell::default();
        }
    }

    fn reset_screen(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
        self.row = 0;
        self.col = 0;
        self.fg = DEFAULT_FG;
        self.bg = DEFAULT_BG;
        self.style = StyleFlags::empty();
        self.inverse = false;
        self.autowrap = true;
        self.cursor_visible = true;
        self.saved = None;
        self.sel = None;
        self.title.clear();
    }

    fn reset_sgr(&mut self) {
        self.fg = DEFAULT_FG;
        self.bg = DEFAULT_BG;
        self.style = StyleFlags::empty();
        self.inverse = false;
    }

    fn save_cursor(&mut self) {
        self.saved = Some(Saved {
            row: self.row,
            col: self.col,
            fg: self.fg,
            bg: self.bg,
            style: self.style,
            inverse: self.inverse,
        });
    }

    fn restore_cursor(&mut self) {
        if let Some(s) = self.saved {
            self.row = s.row.min(self.rows - 1);
            self.col = s.col.min(self.cols - 1);
            self.fg = s.fg;
            self.bg = s.bg;
            self.style = s.style;
            self.inverse = s.inverse;
        }
    }

    fn move_up(&mut self, n: i32) {
        self.row = self.row.saturating_sub(n.max(1) as usize);
    }

    fn move_down(&mut self, n: i32) {
        self.row = (self.row + n.max(1) as usize).min(self.rows - 1);
    }

    fn move_right(&mut self, n: i32) {
        self.col = (self.col + n.max(1) as usize).min(self.cols - 1);
    }

    fn move_left(&mut self, n: i32) {
        self.col = self.col.saturating_sub(n.max(1) as usize);
    }

    fn cup(&mut self, params: &[i32]) {
        let row = n(params, 0);
        let col = n(params, 1);
        self.row = ((row - 1) as usize).min(self.rows - 1);
        self.col = ((col - 1) as usize).min(self.cols - 1);
    }

    fn erase_display(&mut self, mode: i32) {
        match mode {
            1 => {
                // Clear from screen start through the cursor.
                for r in 0..self.row {
                    self.erase_row(r, 0, self.cols);
                }
                self.erase_row(self.row, 0, self.col + 1);
            }
            2 | 3 => {
                // Clear the whole screen (3 = including scrollback, none here).
                for r in 0..self.rows {
                    self.erase_row(r, 0, self.cols);
                }
            }
            _ => {
                // Clear from the cursor to the end of the screen.
                self.erase_row(self.row, self.col, self.cols);
                for r in self.row + 1..self.rows {
                    self.erase_row(r, 0, self.cols);
                }
            }
        }
    }

    fn erase_line(&mut self, mode: i32) {
        match mode {
            1 => self.erase_row(self.row, 0, self.col + 1),
            2 => self.erase_row(self.row, 0, self.cols),
            _ => self.erase_row(self.row, self.col, self.cols),
        }
    }

    fn erase_chars(&mut self, n: i32) {
        let start = self.col;
        let end = (start + n.max(1) as usize).min(self.cols);
        self.erase_row(self.row, start, end);
    }

    fn erase_row(&mut self, row: usize, start: usize, end: usize) {
        for c in start..end {
            self.cells[row * self.cols + c] = Cell::default();
        }
    }

    fn set_mode(&mut self, intermediates: &[u8], params: &[i32], enable: bool) {
        if !intermediates.contains(&b'?') {
            return;
        }
        for &p in params {
            match p {
                7 => self.autowrap = enable,   // DECAWM
                25 => self.cursor_visible = enable, // DECTCEM
                _ => {}
            }
        }
    }

    fn sgr(&mut self, params: &[i32]) {
        if params.is_empty() {
            self.reset_sgr();
            return;
        }
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.reset_sgr(),
                1 => self.style |= StyleFlags::BOLD,
                2 => {} // faint: ignored for the demo
                3 => self.style |= StyleFlags::ITALIC,
                4 | 21 => self.style |= StyleFlags::UNDERLINE,
                7 => self.inverse = true,
                9 => self.style |= StyleFlags::STRIKETHROUGH,
                22 => self.style.remove(StyleFlags::BOLD),
                23 => self.style.remove(StyleFlags::ITALIC),
                24 => self.style.remove(StyleFlags::UNDERLINE),
                27 => self.inverse = false,
                29 => self.style.remove(StyleFlags::STRIKETHROUGH),
                30..=37 => self.fg = ANSI16[(params[i] - 30) as usize],
                38 => i = self.sgr_extended(params, i, true),
                39 => self.fg = DEFAULT_FG,
                40..=47 => self.bg = ANSI16[(params[i] - 40) as usize],
                48 => i = self.sgr_extended(params, i, false),
                49 => self.bg = DEFAULT_BG,
                53 => self.style |= StyleFlags::OVERLINE,
                55 => self.style.remove(StyleFlags::OVERLINE),
                90..=97 => self.fg = ANSI16[(params[i] - 82) as usize],
                100..=107 => self.bg = ANSI16[(params[i] - 92) as usize],
                _ => {}
            }
            i += 1;
        }
    }

    /// Handle SGR `38`/`48` extended colors (`5;n` indexed or `2;r;g;b`
    /// truecolor, `;` and `:` separated). Returns the index of the last
    /// consumed parameter (the caller's `i += 1` advances past it).
    fn sgr_extended(&mut self, params: &[i32], i: usize, is_fg: bool) -> usize {
        let mut j = i + 1;
        if j >= params.len() {
            return i;
        }
        let (color, last) = match params[j] {
            5 => {
                j += 1;
                if j >= params.len() {
                    return i;
                }
                (color256(params[j]), j)
            }
            2 => {
                j += 1;
                // Skip the optional empty colorspace marker (`38:2::r:g:b`).
                while j < params.len() && params[j] == -1 {
                    j += 1;
                }
                if j + 2 >= params.len() {
                    return i;
                }
                (vtty::rgb(params[j], params[j + 1], params[j + 2]), j + 2)
            }
            _ => return i,
        };
        if is_fg {
            self.fg = color;
        } else {
            self.bg = color;
        }
        last
    }
}

// ── Perform: the parser's callback contract ───────────────────────────

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        self.write_char(c);
    }

    fn print_bytes(&mut self, s: &[u8]) {
        // Printable ASCII (`0x20..=0x7e`) is one char per byte by contract.
        for &b in s {
            self.write_char(b as char);
        }
    }

    fn execute(&mut self, b: u8) {
        match b {
            0x08 => self.col = self.col.saturating_sub(1), // BS
            0x09 => {
                // TAB: next multiple of 8.
                self.col = ((self.col / 8) + 1) * 8;
                self.col = self.col.min(self.cols - 1);
            }
            0x0a..=0x0c => self.line_feed(), // LF / VT / FF
            0x0d => self.col = 0,                   // CR
            _ => {}                                 // BEL etc.
        }
    }

    fn csi_dispatch(&mut self, params: &[i32], _sep: u16, intermediates: &[u8], final_byte: u8) {
        match final_byte {
            b'm' => self.sgr(params),
            b'A' => self.move_up(n(params, 0)),
            b'B' => self.move_down(n(params, 0)),
            b'C' => self.move_right(n(params, 0)),
            b'D' => self.move_left(n(params, 0)),
            b'E' => {
                self.col = 0;
                self.line_feed();
            } // CNL
            b'F' => {
                self.col = 0;
                self.move_up(n(params, 0));
            } // CPL
            b'G' | b'`' => {
                self.col = (n(params, 0) as usize - 1).min(self.cols - 1);
            } // CHA
            b'd' => {
                self.row = (n(params, 0) as usize - 1).min(self.rows - 1);
            } // VPA
            b'H' | b'f' => self.cup(params),
            b'J' => self.erase_display(params.first().copied().unwrap_or(0)),
            b'K' => self.erase_line(params.first().copied().unwrap_or(0)),
            b'X' => self.erase_chars(n(params, 0)),
            b'h' => self.set_mode(intermediates, params, true),
            b'l' => self.set_mode(intermediates, params, false),
            b's' => self.save_cursor(),
            b'u' => self.restore_cursor(),
            _ => {} // e.g. DECSTBM `r` — ignored for the demo
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], byte: u8) {
        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'c' => self.reset_screen(), // RIS
            b'D' => self.line_feed(),    // IND
            b'E' => {
                self.col = 0;
                self.line_feed();
            } // NEL
            b'M' => self.row = self.row.saturating_sub(1), // RI
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell: bool) {
        let Some(first) = params.first() else {
            return;
        };
        if (*first == b"0" || *first == b"2")
            && let Some(title) = params.get(1)
        {
            self.title = String::from_utf8_lossy(title).into_owned();
        }
    }
}

// ── Free helpers ──────────────────────────────────────────────────────

/// Get a CSI parameter with DEC's "absent or 0 means default" semantics.
fn n(params: &[i32], i: usize) -> i32 {
    match params.get(i) {
        Some(&v) if v > 0 => v,
        _ => 1,
    }
}

/// Map an xterm 256-color index to packed 0xRRGGBB.
fn color256(n: i32) -> u32 {
    let n = n.clamp(0, 255) as u32;
    match n {
        0..=15 => ANSI16[n as usize],
        16..=231 => {
            let n = n - 16;
            let (r, g, b) = (n / 36, (n / 6) % 6, n % 6);
            let scale = |v: u32| if v == 0 { 0 } else { 55 + v * 40 };
            (scale(r) << 16) | (scale(g) << 8) | scale(b)
        }
        _ => {
            let v = 8 + (n - 232) * 10;
            (v << 16) | (v << 8) | v
        }
    }
}

/// Escape one character for embedding in HTML text content.
fn escape_char(c: char) -> String {
    match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        _ => c.to_string(),
    }
}

/// Packed 0xRRGGBB → `#rrggbb` CSS color string.
fn js_color(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

/// Canvas font string for a given (bold, italic) style.
fn font_css(bold: bool, italic: bool) -> String {
    let mut f = String::from("13px ui-monospace, Menlo, Consolas, monospace");
    match (bold, italic) {
        (true, true) => f.insert_str(0, "italic bold "),
        (true, false) => f.insert_str(0, "bold "),
        (false, true) => f.insert_str(0, "italic "),
        (false, false) => {}
    }
    f
}
