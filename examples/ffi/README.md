# vtty — C FFI example

A minimal interactive terminal built on the `vtty_ffi` C ABI: keystrokes are
forwarded to a shell running on a pseudo-terminal, and the shell's output is
fed through the vtty parser and printed.

```
examples/ffi/
├── build.sh      # build vtty_ffi and compile term_demo
└── term_demo.c   # the C demo
```

The ABI header `vtty_ffi.h` lives in `crates/vtty_ffi/`.

## Build & run

```bash
./build.sh                          # build the library + compile term_demo
./term_demo                         # interactive (Ctrl-D to exit)
printf 'ls\nexit\n' | ./term_demo   # or pipe commands
```
