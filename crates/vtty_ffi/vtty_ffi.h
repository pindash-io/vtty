/* Copyright © Fangdun Tsai <fundon@pindash.io>
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 *
 * vtty_ffi.h — raw C ABI over the byte-level `vtty` VT parser.
 *
 * All offsets (cursor, text length, line ranges) are BYTE offsets into the
 * UTF-8 buffer returned by vt_text. vt_text is valid UTF-8 but NOT
 * NUL-terminated; use vt_text_len for the length.
 *
 * The state is an opaque pointer (vt_state). Create with vt_new, release
 * with vt_free, reuse a pointer with vt_reset.
 */
#ifndef VTTY_FFI_H
#define VTTY_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define VTTY_FFI_VERSION_MAJOR 0
#define VTTY_FFI_VERSION_MINOR 1
#define VTTY_FFI_VERSION_PATCH 0

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque parser + screen state. */
typedef struct vt_state vt_state;

/* ABI version as (major << 16) | (minor << 8) | patch. */
uint32_t vt_version(void);

/* Allocate a fresh state. Free with vt_free. Never returns NULL. */
vt_state *vt_new(void);

/* Release a state allocated by vt_new. */
void vt_free(vt_state *state);

/* Reset a state to empty (text, cursor, and partial parse state cleared).
 * The pointer stays valid for reuse. */
void vt_reset(vt_state *state);

/* Feed `len` bytes to the parser. */
void vt_advance(vt_state *state, const uint8_t *bytes, size_t len);

/* The accumulated text (valid UTF-8, NOT NUL-terminated). */
const char *vt_text(const vt_state *state);

/* Byte length of vt_text. */
size_t vt_text_len(const vt_state *state);

/* Current cursor as a BYTE offset into vt_text. */
size_t vt_cursor(const vt_state *state);

/* Number of lines. Empty = 0 lines; a trailing '\n' counts as one empty line. */
size_t vt_line_count(const vt_state *state);

/* 0-based row (line index) of the cursor. */
size_t vt_cursor_row(const vt_state *state);

/* 0-based BYTE column of the cursor within its line (byte offset from the
 * line start — not a terminal cell column). */
size_t vt_cursor_col(const vt_state *state);

/* Fill *start and *end with the byte range of line `line` (0-based) into
 * vt_text. *end is exclusive (points at the line's '\n', or the buffer end).
 * Returns false when `line` is out of range (leaves the start/end pointers
 * untouched). */
bool vt_line_range(const vt_state *state, size_t line, size_t *start, size_t *end);

#ifdef __cplusplus
}
#endif

#endif /* VTTY_FFI_H */
