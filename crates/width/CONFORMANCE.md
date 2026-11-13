# Width model conformance: taxonomy, model, engine

`vtty_width` maps a string to per-grapheme cell widths via one function,
`cell_widths: Σ* → ℕ*`. The width model follows UTS #51 as of **Unicode
17.0.0** — ICU4X 2.3.0 bakes the per-codepoint properties, `parley_emoji`
0.12.0 supplies the version-agnostic DFA. This document classifies the
authoritative data files, states the model formally, and describes the
analytical engine (`examples/conformance.rs`) that checks every sequence —
plus the non-emoji width rules (combining/spacing marks, Hangul, canonical
composition, and the cell↔char walkers) that the same crate owns.

## 1. Classification & summary

Four Unicode 17.0.0 data files, six sequence classes, three deliberate
terminal divergences.

| data file | classes | expected signature | result |
|---|---|---|---|
| `emoji-sequences.txt` | Basic_Emoji | `[2]` or `[2,0]` | 502/502 |
| | Emoji_Keycap_Sequence | `[2,0,0]` | 12/12 |
| | RGI_Emoji_Flag_Sequence | `[2,0]` | 259/259 |
| | RGI_Emoji_Modifier_Sequence | `[2,0]` | 665/665 |
| | RGI_Emoji_Tag_Sequence | `[2,0,…]` | 3/3 |
| `emoji-zwj-sequences.txt` | RGI_Emoji_ZWJ_Sequence | `[2,0,…]` | 1614/1614 |
| `emoji-test.txt` | fully-qualified | `[2,0,…]` | 3944/3944 |
| | minimally-qualified | `[2,0,…]` | 1029/1029 |
| | unqualified | `[2]` (`Emoji=Yes`) / `[2]` (CJK) / `[2,0,…]` (ZWJ) / `[1,0]` (keycap) | 243/243 |
| | component | `[2]` (hair) / `[1]` (skin tone) | 9/9 |
| `emoji-variation-sequences.txt` | text style | `[1,0]` | 371/371 |
| | emoji style | `[2,0]` | 359/371 + 12 keycap |

**~9,000 sequences, 0 mismatches** once the three terminal overrides below are
accounted for.

### The three divergence classes

| class | predicate | signature | justification |
|---|---|---|---|
| **K₁ — keycap-incomplete** | `s = c VS16`, `c ∈ {# * 0-9}` (no `U+20E3`) | `[1,0]` | a keycap base is `Emoji` but not `ExtPict`; Ghostty drops a lone VS16 after it. The complete `c VS16 U+20E3` widens normally. |
| **K₂ — keycap-no-VS16** | `s = c U+20E3`, `c ∈ {# * 0-9}` | `[1,0]` | without the VS16 the keycap is text presentation. |
| **S — lone skin tone** | `s = c`, `c ∈ {1F3FB..1F3FF}` | `[1]` | a skin-tone modifier is a 0-width combining mark; a LONE one is one cell. |

These are deliberate, Ghostty-aligned terminal overrides, not defects.

## 2. Width model

Let `w(s) = [w₁, …, wₙ] = cell_widths(s)`. A sequence **collapses** into one
2-cell grapheme when:

```math
\mathsf{collapse}(s) \;\equiv\; w_1 = 2 \;\land\; \forall i > 1:\ w_i = 0
```

A grapheme is one emoji member, optionally with a skin tone; a ZWJ sequence
joins members:

```math
\mathsf{member} = \mathsf{Emoji}\ [\mathsf{SkinTone}], \qquad
\mathsf{ZWJ}(s) = \mathsf{member}\ (\mathsf{ZWJ}\ \mathsf{member})^*
```

VS16/VS15 select the presentation style; the lead width combines presentation
with collapse:

```math
\sigma(\mathsf{VS16}) = \mathsf{Emoji},\qquad
\sigma(\mathsf{VS15}) = \mathsf{Text},\qquad
w_1 =
\begin{cases}
2 & \sigma = \mathsf{Emoji} \lor \mathsf{collapse}(s) \\
1 & \sigma = \mathsf{Text}
\end{cases}
```

Expected signatures by class:

- `Basic`, `Flag`, `Modifier`, `Tag`, `ZWJ` — `collapse(s)`
- `Keycap` — `[2,0,0]`
- `TextStyle` (`c VS15`) — `[1,0]`
- `EmojiStyle` (`c VS16`) — `[2,0]`

Every continuation is 0-width (`w_i = 0` for `i > 1`): skin tones, ZWJ,
selectors, keycap-ends, and tag chars never add cells.

### Combining vs spacing look-alike characters (non-emoji)

A combining mark is `Grapheme_Extend` / `Default_Ignorable` → 0 cells; the
SPACING look-alike characters are `Sk` modifiers and keep their 1-cell wcwidth — they
are NOT marks and must not attach:

| char | width | with base `e` / ` ` |
|---|---|---|
| U+0301 COMBINING ACUTE (Mn) | 0 | `e\u{301} = [1,0]`, ` \u{301} = [1,0]` |
| U+00B4 ACUTE ACCENT (Sk) | 1 | `e\u{00B4} = [1,1]`, ` \u{00B4} = [1,1]` |
| U+0060 GRAVE ACCENT (Sk) | 1 | `e\u{60} = [1,1]`, ` \u{60} = [1,1]` |

Locked by `vtty_width::tests::combining_vs_spacing_acute`.

### Mark attachment (lone vs space base vs visible base)

The whole-text DFA's attachment rule (`finalize_lead`, the `has_base` /
`space_base` / `followed_by_mark` triple):

| text | `cell_widths` | rule |
|---|---|---|
| `e\u{301}` | `[1, 0]` | a mark after a visible base ATTACHES |
| ` \u{301}` | `[1, 0]` | a SINGLE mark after a SPACE attaches (the space is a base) |
| ` \u{301}\u{301}` | `[1, 1, 0]` | two marks after a SPACE: the FIRST is STANDALONE so the second has a visible base |
| `1\u{323}\u{323}` | `[1, 0, 0]` | two marks after a VISIBLE base attach (the base is already visible) |
| `❤\u{fe0f}\u{301}\u{301}` | `[2, 0, 0, 0]` | marks after a VS16-widened emoji attach (the emoji is visible) |

Locked by `vtty_width::tests::mark_after_space_followed_by_mark_is_standalone`.

Format controls U+200B ZWSP / U+200C ZWNJ are `Default_Ignorable` → 0 cells
(the engine's `cell_spans` uses U+200B as an internal grapheme breaker to
stop parley composing `e`+U+0301 into a precomposed `é`).

### Canonical composition (grapheme-breaker decision)

`composes_precomposed(base, mark)` — does `base` + `mark` have a Unicode
canonical precomposed form (`e`+U+0301 → `é`)? The renderer inserts the ZWSP
grapheme break ONLY when this is true; a non-composing base (`◌`, symbols,
CJK) keeps its grapheme intact so the GPOS mark anchor still positions the
mark over the base. Scoped to the combining diacritical marks U+0300..=U+036F
(the `ccmp` block); emoji selectors and Hangul Jamo are excluded.

Locked by `vtty_width::tests::composes_precomposed_scopes_to_diacritics`.

### CJK width mode (East Asian Width Ambiguous → 2)

`cell_width` packs an extra `AMBIGUOUS` bit (bit 8 of the `u16` `Properties`,
baked by `build.rs` for the EAW "A" set, and via `CodePointMapData` on the ICU
runtime path). When the global `cjk_width()` flag is set (`set_cjk_width(true)`,
wired from `JETFISH_CJK_WIDTH`), a width-1 Ambiguous char becomes 2 cells:

| text | default (`cjk_width = false`) | CJK mode (`true`) |
|---|---|---|
| `⑩` U+2469 (A) | `[1]` | `[2]` |
| `①` U+2460 (A) | `[1]` | `[2]` |
| `a` | `[1]` | `[1]` |

It matches `wcwidth` under a CJK locale and Ghostty's `cjk_width`; the default
keeps Ambiguous at 1 so the grid stays in sync with the shell's non-CJK
`wcwidth`, and the renderer's Enclosed-Alphanumerics scale-down keeps the ink
inside the 1-cell box.

Locked by `vtty_width::tests::cjk_width_makes_ambiguous_wide`.

### Hangul jamo and syllable widths
L jamo (U+1100) is 2 cells; V/T jamo (U+1160..=U+11FF) are zero-width
continuations that compose onto a preceding L/LV. A LONE V/T jamo occupies 1
cell (the lone-mark fix). Precomposed syllables (U+AC00..=U+D7A3) are 2 cells.

| text | `cell_widths` |
|---|---|
| `U+1100` (L) | `[2]` |
| `U+1161` (V) | `[1]` (lone) |
| `U+1100 U+1161` (L+V) | `[2, 0]` |
| `U+1100 U+1161 U+11A8` (L+V+T) | `[2, 0, 0]` |
| `가` (LV) / `각` (LVT) | `[2]` |

Locked by `vtty_width::tests::hangul_jamo_and_syllable_widths`.

### Cell↔char walkers

Three char-INDEX walkers over the precomputed `cell_widths` vector — the
renderer's run char-range boundaries (floor/ceil/end):

- `cell_col_char_from_widths(w, col)` — the glyph STARTING at (or containing)
  `col`, skipping 0-width continuations (floor).
- `cell_col_char_ceil_from_widths(w, col)` — the first glyph AT or past `col`
  (ceil; a 0-width char at the boundary IS included).
- `cell_col_char_end_from_widths(w, col)` — one past the glyph ENDING at
  `col`, including its trailing 0-width continuations. The boundary is
  INCLUSIVE (`cell + w >= col`, not `>`), so a glyph whose cell range ENDS
  exactly at `col` is included (the 2-cell ZWJ-family run-end fix).

These own every cell↔char boundary in one place, so the grid width (not the
font advance) propagates through the renderer's run layout.

The three terminal overrides (K₁/K₂/S, §1) narrow the catalogue:

```math
\begin{aligned}
\mathsf{K_1}(s) &\equiv \exists\, c \in \{ \#,\ *,\ 0..9 \}:\ s = c\ \mathsf{VS16}
  & w &= [1,0] \\
\mathsf{K_2}(s) &\equiv \exists\, c \in \{ \#,\ *,\ 0..9 \}:\ s = c\ U{+}20E3
  & w &= [1,0] \\
\mathsf{S}(s)   &\equiv \exists\, c \in [U{+}1F3FB, U{+}1F3FF]:\ s = c
  & w &= [1]
\end{aligned}
```

## 3. Analytical engine

`examples/conformance.rs` runs all four files in one pass and prints the
classification table plus any mismatch. For each entry it:

1. parses the codepoint field, expanding `CP..CP` ranges;
2. classifies by file, sequence type, and qualification;
3. applies `cell_widths`;
4. checks the signature against the three overrides.

```bash
cargo run --release -p vtty_width --example conformance -- \
  /tmp/emoji-test.txt \
  /tmp/emoji-zwj-sequences.txt \
  /tmp/emoji-variation-sequences.txt \
  /tmp/emoji-sequences.txt
```

The per-file examples (`emoji_conformance`, `zwj_conformance`,
`variation_conformance`, `sequences_conformance`) remain the focused
single-file checks.

## 4. Data sources

```bash
U=https://www.unicode.org/Public/17.0.0
curl -sL $U/emoji/emoji-test.txt              -o /tmp/emoji-test.txt
curl -sL $U/emoji/emoji-zwj-sequences.txt     -o /tmp/emoji-zwj-sequences.txt
curl -sL $U/ucd/emoji/emoji-variation-sequences.txt -o /tmp/emoji-variation-sequences.txt
curl -sL $U/emoji/emoji-sequences.txt         -o /tmp/emoji-sequences.txt
```
