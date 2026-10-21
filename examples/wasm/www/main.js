import init, { Terminal } from "./pkg/vtty_wasm.js";

const out = document.getElementById("out");
const examples = document.getElementById("examples");
const input = document.getElementById("input");

let term;

const EXAMPLES = [
  ["fg 16 colors", "\x1b[30m 30 \x1b[31m 31 \x1b[32m 32 \x1b[33m 33 \x1b[34m 34 \x1b[35m 35 \x1b[36m 36 \x1b[37m 37 \x1b[0m\n"],
  ["fg bright", "\x1b[90m 90 \x1b[91m 91 \x1b[92m 92 \x1b[93m 93 \x1b[94m 94 \x1b[95m 95 \x1b[96m 96 \x1b[97m 97 \x1b[0m\n"],
  ["bg 16 colors", "\x1b[40m 40 \x1b[41m 41 \x1b[42m 42 \x1b[43m 43 \x1b[44m 44 \x1b[45m 45 \x1b[46m 46 \x1b[47m 47 \x1b[0m\n"],
  ["bg bright", "\x1b[100m 100 \x1b[101m 101 \x1b[102m 102 \x1b[103m 103 \x1b[104m 104 \x1b[105m 105 \x1b[106m 106 \x1b[107m 107 \x1b[0m\n"],
  ["styles", "\x1b[1mbold\x1b[0m  \x1b[3mitalic\x1b[0m  \x1b[4munderline\x1b[0m  \x1b[9mstrike\x1b[0m  \x1b[31m\x1b[7minverse\x1b[0m\n"],
  ["combined SGR", "\x1b[1;4;36mbold + underline + cyan\x1b[0m\n"],
  ["partial reset", "\x1b[1;31mbold red\x1b[22m red only\x1b[0m default\n"],
  ["CRLF + UTF-8", "line 1\r\nline 2\r\n水永泳 🚀\n"],
];

const DEMO = EXAMPLES.map(([, bytes]) => bytes).join("");

/** Turn the user-facing `\x1b` / `\n` / `\t` text into real bytes. */
function interpret(text) {
  return text
    .replace(/\\x1b/g, "\x1b")
    .replace(/\\n/g, "\n")
    .replace(/\\t/g, "\t");
}

function render() {
  out.innerHTML = term.render();
}

function feed(text) {
  term.feed(text);
  render();
}

async function main() {
  await init();
  term = new Terminal();

  for (const [name, bytes] of EXAMPLES) {
    const button = document.createElement("button");
    button.className = "example";
    button.textContent = name;
    button.addEventListener("click", () => {
      term.clear();
      feed(bytes);
    });
    examples.append(button);
  }

  feed(DEMO);
}

document.getElementById("demo").addEventListener("click", () => {
  term.clear();
  feed(DEMO);
});
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
