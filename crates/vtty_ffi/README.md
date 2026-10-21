# vtty_ffi

<!-- cargo-rdme start -->

Raw C ABI over `vtty`'s byte-cursor state (`vtty::State`). Declared in
`vtty_ffi.h`.

## Usage

```c
#include "vtty_ffi.h"

vt_state *s = vt_new();
vt_advance(s, (const uint8_t *)"hello\nworld", 11);

size_t rows = vt_line_count(s);     // 2
size_t len  = vt_text_len(s);       // 11
size_t start, end;
vt_line_range(s, 1, &start, &end);  // start=6, end=11

vt_free(s);
```

API: `vt_version` · `vt_new` · `vt_free` · `vt_reset` · `vt_advance` ·
`vt_flush` · `vt_text` · `vt_text_len` · `vt_cursor` · `vt_line_count` ·
`vt_cursor_row` · `vt_cursor_col` · `vt_line_range`.

Build: `cargo build -p vtty_ffi` (cdylib + staticlib).

## Example

`examples/ffi/term_demo.c` spawns a shell on a pseudo-terminal and runs
it as a minimal interactive terminal (keystrokes in, shell output out).
See the file's header for build instructions.

<!-- cargo-rdme end -->
