#!/usr/bin/env bash
# Build the C FFI example: the vtty_ffi library plus term_demo.
set -euo pipefail
cd "$(dirname "$0")"

echo ">> building vtty_ffi (release)…"
cargo build --release -p vtty_ffi

libs=(-lvtty_ffi)
# forkpty lives in libutil on Linux, in libc on macOS/BSD.
case "$(uname -s)" in
  Linux) libs+=(-lutil) ;;
esac

echo ">> compiling term_demo…"
cc -O2 term_demo.c -I ../../crates/vtty_ffi -L ../../target/release "${libs[@]}" -o term_demo

echo ">> built ./term_demo — run it, or pipe commands: printf 'ls\\nexit\\n' | ./term_demo"
