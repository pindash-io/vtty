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

const EXAMPLES = [
  {
    name: "rust",
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
    name: "terminal",
    lang: "bash",
    src: `$ ls crates/
vtty  vtty_core  vtty_width  vtty_ffi  vtty_status

$ cargo build --release
   Compiling vtty v0.1.0
    Finished release profile [optimized] in 0.42s

$ echo "hello from vtty"
hello from vtty
`,
  },
  {
    name: "json",
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
  out.innerHTML = highlight(term.text());
}

function show(example) {
  lang = example.lang;
  term.clear();
  term.feed(example.src);
  render();
}

function feed(text) {
  term.feed(text);
  render();
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

  for (const name of Object.keys(THEMES)) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    themeSelect.append(option);
  }
  themeSelect.value = "pierre-dark";
  themeSelect.addEventListener("change", () => {
    theme = THEMES[themeSelect.value];
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
