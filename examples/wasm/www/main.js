import init, { Terminal } from "./pkg/vtty_wasm.js";

const canvas = document.getElementById("term");
const ctx = canvas.getContext("2d");
const titleEl = document.getElementById("title");
const dimsEl = document.getElementById("dims");
const inputEl = document.getElementById("input");

const ROWS = 24;
const COLS = 96;
const FONT = "13px ui-monospace, Menlo, Consolas, monospace";

let term;
let cellW;
let cellH;
let cursorOn = true;
let dpr = 1;

// Selection state (mouse-driven).
let selecting = false;
let moved = false;
let anchor = null;
let clickCount = 0;
let clickTimer = null;

/** Turn the user-facing "\x1b" / "\n" / "\r" / "\t" text into real bytes. */
function interpret(text) {
  return text
    .replace(/\\x1b|\\x1B|\\u001b/g, "\x1b")
    .replace(/\\n/g, "\n")
    .replace(/\\r/g, "\r")
    .replace(/\\t/g, "\t");
}

function render() {
  // Map CSS-pixel cell coordinates to the HiDPI backing store.
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  term.draw(ctx, cellW, cellH, cursorOn);
  titleEl.textContent = term.title() || "vtty web demo";
  dimsEl.textContent = `${term.rows()}×${term.cols()}`;
}

function feed(text) {
  term.feed(text);
  render();
}

// ── Demo: a colorful ANSI payload exercising SGR, cursor, erase, OSC ──

function demo() {
  const R = "\x1b[0m";
  feed(
    // OSC window title
    "\x1b]0;vtty wasm demo\x1b\\" +
      "\x1b[2J\x1b[H" +
      // Bold header + underline
      "\x1b[1;4;36m  vtty · zero-allocation DEC VT500 parser  \x1b[0m\r\n" +
      "\x1b[38;5;240m  ─────────────────────────────────────────  \x1b[0m\r\n\r\n" +
      // 16-color palette
      "\x1b[1m  Palette\x1b[0m\r\n" +
      Array.from({ length: 16 }, (_, i) => `\x1b[48;5;${i}m  `).join("") +
      R +
      "\r\n\r\n" +
      // 256-color gradient
      "\x1b[1m  Gradient (256-color)\x1b[0m\r\n" +
      Array.from({ length: 32 }, (_, i) => `\x1b[48;5;${16 + i * 7}m `).join("") +
      R +
      "\r\n\r\n" +
      // Truecolor
      "\x1b[1m  Truecolor (38;2;r;g;b)\x1b[0m\r\n" +
      "\x1b[38;2;255;120;80m  red-ish\x1b[0m  " +
      "\x1b[38;2;120;255;140m  green-ish\x1b[0m  " +
      "\x1b[38;2;120;170;255m  blue-ish\x1b[0m\r\n\r\n" +
      // Styles
      "\x1b[1m  Styles\x1b[0m\r\n" +
      "  \x1b[1mbold\x1b[0m  \x1b[3mitalic\x1b[0m  \x1b[4munderline\x1b[0m  " +
      "\x1b[9mstrike\x1b[0m  \x1b[53moverline\x1b[0m  \x1b[7minverse\x1b[0m  " +
      "\x1b[5mblink (title)\x1b[0m\r\n\r\n" +
      // Cursor movement + box drawing
      "\x1b[1;33m  Cursor / box drawing\x1b[0m\r\n" +
      "  ┌──────────────┐\r\n" +
      "  │  \x1b[32mhello\x1b[33m, \x1b[35mvtty\x1b[33m  │\r\n" +
      "  └──────────────┘\r\n\r\n" +
      // Save/restore cursor (ESC 7 / ESC 8)
      "\x1b[38;5;208m  (cursor saved with ESC 7, moved, restored with ESC 8)\x1b[0m" +
      "\x1b7\x1b[10C\x1b[31mMARK\x1b8\r\n"
  );
}

// ── Code block: a syntax-highlighted snippet rendered via SGR ──

function code() {
  const R = "\x1b[0m", K = "\x1b[34m", S = "\x1b[32m", C = "\x1b[90m",
        N = "\x1b[36m", T = "\x1b[33m";
  feed(
    "\x1b[2J\x1b[H" +
      "\x1b[1;4;36m  code block · a vtty Perform impl  \x1b[0m\r\n\r\n" +
      C + "  // A zero-allocation screen" + R + "\r\n" +
      K + "use" + R + " vtty::{Parser, Perform};\r\n\r\n" +
      K + "struct" + R + " Screen;\r\n\r\n" +
      K + "impl" + R + " Perform " + K + "for" + R + " Screen {\r\n" +
      "    " + K + "fn" + R + " " + T + "print_bytes" + R +
      "(&" + K + "mut self" + R + ", s: &[" + N + "u8" + R + "]) {\r\n" +
      "        " + C + "// render this ASCII run" + R + "\r\n" +
      "    }\r\n" +
      "}\r\n\r\n" +
      K + "fn" + R + " " + T + "main" + R + "() {\r\n" +
      "    " + K + "let" + R + " " + K + "mut" + R + " parser = Parser::" + T + "new" + R + "();\r\n" +
      "    parser.advance(&" + K + "mut" + R + " screen, " + S + "\"\\x1b[1;31mhi\"" + R + ");\r\n" +
      "}\r\n"
  );
}

async function main() {
  await init();
  term = new Terminal(ROWS, COLS);

  // Measure one cell in CSS pixels, then size the backing store at the
  // device pixel ratio (2x on retina) so glyphs stay crisp.
  dpr = window.devicePixelRatio || 1;
  ctx.font = FONT;
  const metrics = ctx.measureText("M");
  cellW = Math.ceil(metrics.width);
  cellH = Math.ceil(13 * 1.35);

  const cssW = COLS * cellW;
  const cssH = ROWS * cellH;
  canvas.width = Math.round(cssW * dpr);
  canvas.height = Math.round(cssH * dpr);
  canvas.style.width = `${cssW}px`;
  canvas.style.height = `${cssH}px`;

  demo();
  render();
}

document.getElementById("send").addEventListener("click", () => {
  const v = inputEl.value;
  if (v) feed(interpret(v));
  inputEl.value = "";
});

inputEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    const v = inputEl.value;
    if (v) feed(interpret(v));
    inputEl.value = "";
  }
});

document.getElementById("demo").addEventListener("click", demo);
document.getElementById("code").addEventListener("click", code);
document.getElementById("clear").addEventListener("click", () => {
  feed("\x1b[2J\x1b[H");
});
document.getElementById("reset").addEventListener("click", () => {
  term.reset();
  render();
});

// ── Selection: drag to select, double-click word, triple-click line ──

function cellAt(e) {
  const rect = canvas.getBoundingClientRect();
  const col = Math.min(COLS - 1, Math.max(0, Math.floor((e.clientX - rect.left) / cellW)));
  const row = Math.min(ROWS - 1, Math.max(0, Math.floor((e.clientY - rect.top) / cellH)));
  return { row, col };
}

canvas.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  const { row, col } = cellAt(e);

  clickCount += 1;
  clearTimeout(clickTimer);
  clickTimer = setTimeout(() => (clickCount = 0), 400);

  if (clickCount === 1) {
    selecting = true;
    moved = false;
    anchor = { row, col };
    term.select(row, col, row, col);
  } else if (clickCount === 2) {
    selecting = false;
    term.select_word(row, col);
  } else {
    selecting = false;
    term.select_line(row);
  }
  render();
});

window.addEventListener("mousemove", (e) => {
  if (!selecting) return;
  const { row, col } = cellAt(e);
  if (anchor.row !== row || anchor.col !== col) moved = true;
  term.select(anchor.row, anchor.col, row, col);
  render();
});

window.addEventListener("mouseup", () => {
  if (!selecting) return;
  selecting = false;
  if (!moved) term.clear_selection(); // a plain click clears the selection
  render();
});

// Ctrl/Cmd+C copies the selection; Escape clears it.
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    term.clear_selection();
    render();
    return;
  }
  if (!(e.metaKey || e.ctrlKey)) return;
  if (e.key.toLowerCase() !== "c") return;
  if (document.activeElement === inputEl) return; // let the input keep its own copy
  const text = term.selected_text();
  if (text) {
    e.preventDefault();
    navigator.clipboard.writeText(text).catch(() => {});
  }
});

// Cursor blink: toggle the phase and repaint.
setInterval(() => {
  cursorOn = !cursorOn;
  render();
}, 550);

main();
