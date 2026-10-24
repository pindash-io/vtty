import init, { Terminal } from "./pkg/vtty_wasm.js";
import { codeToHtml, isSupportedLanguage } from "@pierre/highlights";
import {
  catppuccinMocha,
  dracula,
  githubDark,
  monokai,
  nord,
  oneDarkPro,
  pierreDark,
  pierreLight,
  tokyoNight,
} from "@pierre/highlights/themes";

const out = document.getElementById("out");
const input = document.getElementById("input");
const themeSelect = document.getElementById("theme");
const langSelect = document.getElementById("lang");
const examplesEl = document.getElementById("examples");

let term;
let current; // the active example

const LIGHT_THEME = pierreLight;

const THEMES = {
  "pierre-dark": pierreDark,
  "github-dark": githubDark,
  "one-dark-pro": oneDarkPro,
  dracula,
  monokai,
  nord,
  "tokyo-night": tokyoNight,
  "catppuccin-mocha": catppuccinMocha,
};

const LANGS = [
  "rust", "bash", "ts", "js", "jsx", "tsx", "python", "go", "c", "cpp",
  "java", "csharp", "css", "html", "markdown", "json", "jsonc", "yaml",
  "toml", "sql", "ruby", "php", "swift", "kotlin", "lua", "zig", "r",
  "haskell", "nix", "dockerfile",
];

let theme = pierreDark;
let lang = "rust";

// `mode: "terminal"` renders the vtty_screen grid (SGR colors); `mode:
// "highlight"` feeds the parsed text to @pierre/highlights as source code.
const EXAMPLES = [
  {
    name: "colors",
    mode: "terminal",
    src:
      "\x1b[31mred\x1b[0m \x1b[32mgreen\x1b[0m \x1b[33myellow\x1b[0m " +
      "\x1b[34mblue\x1b[0m \x1b[35mmagenta\x1b[0m \x1b[36mcyan\x1b[0m\n" +
      "\x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m " +
      "\x1b[9mstrike\x1b[0m \x1b[31;7minverse\x1b[0m\n" +
      "\x1b[38;5;196m256-color red\x1b[0m " +
      "\x1b[48;5;21;38;5;231mblue bg\x1b[0m\n" +
      "\x1b[38;2;255;128;0mtruecolor orange\x1b[0m " +
      "\x1b[48;2;20;40;80mdark bg\x1b[0m\n",
  },
  {
    name: "terminal",
    mode: "terminal",
    src:
      "\x1b[1;32mvtty@demo\x1b[0m:\x1b[1;34m~/vtty\x1b[0m$ ls --color=auto\n" +
      "\x1b[1;34mcrates\x1b[0m  \x1b[1;34mexamples\x1b[0m  \x1b[1;36mCargo.toml\x1b[0m  \x1b[1;36mLICENSE\x1b[0m\n" +
      "\n" +
      "\x1b[1;32mvtty@demo\x1b[0m:\x1b[1;34m~/vtty\x1b[0m$ echo \"hello from vtty\"\n" +
      "hello from vtty\n" +
      "\n" +
      "\x1b[1;32mvtty@demo\x1b[0m:\x1b[1;34m~/vtty\x1b[0m$ git status --short\n" +
      "\x1b[1;33m M\x1b[0m crates/vtty_screen/src/lib.rs\n",
  },
  {
    name: "rust",
    mode: "highlight",
    lang: "rust",
    src: `use vtty::{Parser, Perform};

struct Screen;

impl Perform for Screen {
    fn print_bytes(&mut self, s: &[u8]) {
        // render this ASCII run
    }
}
`,
  },
  {
    name: "json",
    mode: "highlight",
    lang: "json",
    src: `{
  "name": "vtty",
  "version": "0.1.0",
  "features": ["simd", "no_std", "panic-free"]
}
`,
  },
];

function highlight(src) {
  return new TextDecoder().decode(
    codeToHtml(src, {
      lang: isSupportedLanguage(lang) ? lang : "plain",
      themes: { light: LIGHT_THEME, dark: theme },
      defaultColor: "light-dark()",
    })
  );
}

function render() {
  out.innerHTML =
    current.mode === "terminal" ? term.render() : highlight(term.text());
}

function show(example) {
  current = example;
  if (example.lang) {
    lang = example.lang;
  }
  term.clear();
  term.feed(example.src);
  render();
}

function feed(text) {
  term.feed(text);
  render();
}

/** Parse `#rrggbb` / `rrggbb` to a packed 0xRRGGBB, or `null`. */
function parseHex(c) {
  const m = /^#?([0-9a-f]{6})$/i.exec(c || "");
  return m ? parseInt(m[1], 16) : null;
}

/** Derive the terminal colors from a highlight theme and apply them. */
function applyTerminalTheme(t) {
  const s = t.style || {};
  const fg =
    parseHex(s["editor.foreground"] ?? s.text ?? s.foreground) ?? 0xffffff;
  const bg =
    parseHex(s["editor.background"] ?? s.background) ?? 0x000000;
  const red = parseHex(s["terminal.ansi.red"]) ?? 0xff5555;
  const green = parseHex(s["terminal.ansi.green"]) ?? 0x50fa7b;
  const blue = parseHex(s["terminal.ansi.blue"]) ?? 0xbd93f9;
  term.set_theme(fg, bg, red, green, blue);
}

let charW = 8.4;
let charH = 21;

/** Measure the monospace cell size once (the grid must fill `out` exactly). */
function measure() {
  const probe = document.createElement("span");
  probe.style.cssText = "position:absolute;visibility:hidden;white-space:pre;";
  probe.textContent = "0";
  out.appendChild(probe);
  charW = probe.getBoundingClientRect().width;
  charH = probe.getBoundingClientRect().height;
  probe.remove();
}

/** Resize the terminal grid to fill the `out` element. */
function fit() {
  const rect = out.getBoundingClientRect();
  const cols = Math.max(1, Math.floor(rect.width / charW));
  const rows = Math.max(1, Math.floor(rect.height / charH));
  term.resize(cols, rows);
  if (current) render();
}

/** Turn the user-facing `\x1b` / `\n` / `\t` text into real bytes. */
function interpret(text) {
  return text
    .replace(/\\x1b/g, "\x1b")
    .replace(/\\n/g, "\n")
    .replace(/\\t/g, "\t");
}

async function main() {
  await init();
  term = new Terminal();
  applyTerminalTheme(theme);
  measure();
  fit();
  window.addEventListener("resize", fit);

  for (const name of Object.keys(THEMES)) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    themeSelect.append(option);
  }
  themeSelect.value = "pierre-dark";
  themeSelect.addEventListener("change", () => {
    theme = THEMES[themeSelect.value];
    applyTerminalTheme(theme);
    render();
  });

  for (const name of LANGS) {
    if (!isSupportedLanguage(name)) continue;
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    langSelect.append(option);
  }
  langSelect.value = "rust";
  langSelect.addEventListener("change", () => {
    lang = langSelect.value;
    render();
  });

  document.getElementById("scheme").addEventListener("click", () => {
    out.style.colorScheme = out.style.colorScheme === "dark" ? "light" : "dark";
  });

  for (const example of EXAMPLES) {
    const button = document.createElement("button");
    button.className = "example";
    button.textContent = example.name;
    button.addEventListener("click", () => show(example));
    examplesEl.append(button);
  }

  show(EXAMPLES[0]);
}

document.getElementById("demo").addEventListener("click", () => show(EXAMPLES[0]));
document.getElementById("clear").addEventListener("click", () => {
  term.clear();
  render();
});
document.getElementById("feed").addEventListener("click", () => {
  feed(interpret(input.value));
  input.value = "";
});
input.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    feed(interpret(input.value));
    input.value = "";
  }
});

main();
