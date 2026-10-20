#!/usr/bin/env bash
# Build the wasm package and serve the demo.
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack not found. Install it: https://rustwasm.github.io/wasm-pack/installer/"
  exit 1
fi

echo ">> building wasm (release)…"
wasm-pack build --target web --out-dir www/pkg --release

echo ">> serving http://localhost:8000"
python3 -m http.server 8000 --directory www
