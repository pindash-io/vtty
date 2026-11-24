# Width model conformance: taxonomy, model, engine

`vtty_width` maps a string to per-code-point cell widths via one function,
`cell_widths: Σ* → ℕ*`. The model follows UTS #51 as of **Unicode 17.0.0** —
ICU4X 2.3.0 bakes the per-codepoint properties, `parley_emoji` 0.13.0 supplies
the version-agnostic DFA.

The formal statement of the model — Layer 1 `w₁`, Layer 2 `w₂`, Layer 3 `W`,
the binding of every symbol to its code item, and the invariants that bind
callers — is [`WIDTH-MODEL.md`](WIDTH-MODEL.md). This document covers what that
model has to agree with: the authoritative data files, the deliberate terminal
divergences from them, and the engine that checks every sequence.

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
| | emoji style | `[2,0]` | 359/359 |
| | emoji style, keycap base | `[1,0]` (K₁) | 12/12 |

**≈9,000 sequences, 0 mismatches** once the three terminal overrides below are
accounted for.

### The three divergence classes

| class | predicate | signature | justification |
|---|---|---|---|
| **K₁ — keycap-incomplete** | `s = c VS16`, `c ∈ {# * 0-9}` (no `U+20E3`) | `[1,0]` | a keycap base is `Emoji` but not `ExtPict`, so a lone VS16 after it is a stray. The complete `c VS16 U+20E3` widens to `[2,0,0]`. |
| **K₂ — keycap-no-VS16** | `s = c U+20E3`, `c ∈ {# * 0-9}` | `[1,0]` | without the VS16 the keycap is text presentation. |
| **S — lone skin tone** | `s = c`, `c ∈ {1F3FB..1F3FF}` | `[1]` | a skin-tone modifier is a 0-width combining mark; a lone one is one cell. |

These are deliberate, Ghostty-aligned terminal overrides, not defects. They are
the `ρ` keycap arm and the `w₁` skin-tone arm of the formal model.

### Two more locked behaviours

| rule | signature | locked by |
|---|---|---|
| halfwidth sound marks never continue a run (Invariant 1) | `❤ﾞ` = `[1,1]`, not `[1,0]` | `tests::halfwidth_sound_mark_does_not_attach_to_emoji_base` |
| a selector attaches per `ExtPict`, widens per `Emoji` | `vs_base` ≠ `vs_target` | `tests::emoji_predicates_classify_selectors_and_bases` |

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
- `EmojiStyle` (`c VS16`) — `[2,0]`, except the keycap bases (K₁)

Every continuation in a collapsing sequence is 0-width: skin tones, ZWJ,
selectors, keycap-ends, and tag chars never add cells.

### Combining vs spacing look-alike characters (non-emoji)

A combining mark is `Grapheme_Extend` / `Default_Ignorable` → 0 cells; the
SPACING look-alike characters are `Sk` modifiers and keep their 1-cell
wcwidth — they are not marks and must not attach:

| char | width | with base `e` / ` ` |
|---|---|---|
| U+0301 COMBINING ACUTE (Mn) | 0 | `e\u{301} = [1,0]`, ` \u{301} = [1,0]` |
| U+00B4 ACUTE ACCENT (Sk) | 1 | `e\u{00B4} = [1,1]`, ` \u{00B4} = [1,1]` |
| U+0060 GRAVE ACCENT (Sk) | 1 | `e\u{60} = [1,1]`, ` \u{60} = [1,1]` |

Locked by `tests::combining_vs_spacing_acute`.

### Mark attachment (lone vs space base vs visible base)

The whole-text DFA's attachment rule (`emoji::finalize_lead`, the `has_base` /
`space_base` / `followed_by_mark` triple):

| text | `cell_widths` | rule |
|---|---|---|
| `e\u{301}` | `[1, 0]` | a mark after a visible base attaches |
| ` \u{301}` | `[1, 0]` | a single mark after a space attaches (the space is a base) |
| ` \u{301}\u{301}` | `[1, 1, 0]` | two marks after a space: the first is standalone so the second has a visible base |
| `1\u{323}\u{323}` | `[1, 0, 0]` | two marks after a visible base attach (the base is already visible) |
| `❤\u{fe0f}\u{301}\u{301}` | `[2, 0, 0, 0]` | marks after a VS16-widened emoji attach (the emoji is visible) |

Locked by `tests::mark_after_space_followed_by_mark_is_standalone`.

Format controls U+200B ZWSP / U+200C ZWNJ are `Default_Ignorable` → 0 cells.
They do **not** continue an emoji run: the continuation clause keys on
`GCB ∈ {Extend, SpacingMark}`, and a format control is `GCB=Control`.

### Canonical composition (grapheme-breaker decision)

`composes_precomposed(base, mark)` — does `base` + `mark` have a Unicode
canonical precomposed form (`e`+U+0301 → `é`)? A renderer inserts the ZWSP
grapheme break only when this is true; a non-composing base (`◌`, symbols,
CJK) keeps its grapheme intact so the GPOS mark anchor still positions the mark
over the base. Scoped to the combining diacritical marks U+0300..=U+036F (the
`ccmp` block); emoji selectors and Hangul jamo are excluded.

Locked by `tests::composes_precomposed_scopes_to_diacritics`.

### CJK width mode (East Asian Width Ambiguous → 2)

`cell_width` packs an extra `AMBIGUOUS` bit (bit 8 of the `u16` `Properties`,
baked by `build.rs` for the EAW "A" set, and via `CodePointMapData` on the ICU
runtime path). When the global `cjk_width()` flag is set (`set_cjk_width(true)`),
a width-1 Ambiguous char becomes 2 cells:

| text | default (`cjk_width = false`) | CJK mode (`true`) |
|---|---|---|
| `⑩` U+2469 (A) | `[1]` | `[2]` |
| `①` U+2460 (A) | `[1]` | `[2]` |
| `a` | `[1]` | `[1]` |
| `中` | `[2]` | `[2]` |

It matches `wcwidth` under a CJK locale and Ghostty's `cjk_width`; the default
keeps Ambiguous at 1 so the grid stays in sync with a non-CJK `wcwidth`, and a
renderer's Enclosed-Alphanumerics scale-down keeps the ink inside the 1-cell
box.

Locked by `tests::cjk_width_makes_ambiguous_wide`, which serialises against the
table-cross-check tests that read the same global.

### Hangul jamo and syllable widths

L jamo (U+1100) is 2 cells; V/T jamo (U+1160..=U+11FF) are zero-width
continuations that compose onto a preceding L/LV. A lone V/T jamo occupies 1
cell (the lone-mark fix). Precomposed syllables (U+AC00..=U+D7A3) are 2 cells.

| text | `cell_widths` |
|---|---|
| `U+1100` (L) | `[2]` |
| `U+1161` (V) | `[1]` (lone) |
| `U+1100 U+1161` (L+V) | `[2, 0]` |
| `U+1100 U+1161 U+11A8` (L+V+T) | `[2, 0, 0]` |
| `가` (LV) / `각` (LVT) | `[2]` |

Locked by `tests::hangul_jamo_and_syllable_widths`.

### Cell↔char walkers

Three char-index walkers over the precomputed `cell_widths` vector — a
renderer's run char-range boundaries (floor/ceil/end):

- `cell_col_char_from_widths(w, col)` — the glyph STARTING at (or containing)
  `col`, skipping 0-width continuations (floor).
- `cell_col_char_ceil_from_widths(w, col)` — the first glyph at or past `col`
  (ceil; a 0-width char at the boundary is included).
- `cell_col_char_end_from_widths(w, col)` — one past the glyph ENDING at
  `col`, including its trailing 0-width continuations. The boundary is
  inclusive (`cell + w >= col`, not `>`), so a glyph whose cell range ends
  exactly at `col` is included (the 2-cell ZWJ-family run-end case).

These own every cell↔char boundary in one place, so grid width (not font
advance) propagates through a renderer's run layout. The byte-level
counterparts (`cell_col_byte_widths`, `cell_col_byte_from_widths`,
`cell_col_byte_ceil_from_widths`) and the grapheme tails
(`grapheme_end_from_widths`, `grapheme_tail_end_from_widths`) are locked by
`tests::cell_col_char_boundaries_floor_ceil_end`,
`tests::cell_col_byte_widths_steps_over_merged_emoji` and
`tests::cell_col_boundaries_past_end`.

### Write order (Layer 3)

Width is not a function of text alone (§Theorem 1 of the formal model). The
crate exposes the pieces a streaming consumer needs:

| item | purpose |
|---|---|
| `EmojiTrace` + `SplitBoundaries` | record the boundary trace `B` and flush set `F` as bytes are written |
| `split_widths(text, &boundaries, start)` | the pure `W`: `correct_F(seg(B, text))` |
| `classify_cell(..)` | the cursor advance of one print, from the live state |
| `wrap_width(cell, dfa_w)` | the autowrap width (≥ the cursor advance) |
| `splice_offset(t, pos, end, diff)` | the edit splice that maintains `fb` |

`fb` is sparse — a sorted `(byte_offset, flags)` list — and carries three bits:
0 = fresh base (a control closed the prior scope), 1 = re-open boundary
(print-time only), 2 = fresh base an ICH shifted right. Bit 2 exists so a
standalone mark that an insert pushed past a non-space predecessor still keeps
its own cell; `clear_fresh_range` drops fresh/ICH bits at a mark that ends up
attaching.

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

The data files download into `assets/emoji/` (§4), which is git-ignored and
is the examples' argument default. From the repository root:

```bash
cargo run --release -p vtty_width --example conformance
```

or with explicit paths:

```bash
cargo run --release -p vtty_width --example conformance -- \
  assets/emoji/emoji-test.txt \
  assets/emoji/emoji-zwj-sequences.txt \
  assets/emoji/emoji-variation-sequences.txt \
  assets/emoji/emoji-sequences.txt
```

Expected output:

```text
emoji-test.txt
  fully-qualified:     3944/3944
  minimally-qualified: 1029/1029
  unqualified:         243/243
  component:           9/9
emoji-zwj-sequences.txt: 1614/1614
emoji-variation-sequences.txt: text 371/371, emoji 359/359, keycap 12/12
emoji-sequences.txt
  basic: 502/502, keycap: 12/12, flag: 259/259, modifier: 665/665, tag: 3/3
```

The per-file examples (`emoji_conformance`, `zwj_conformance`,
`variation_conformance`, `sequences_conformance`) remain the focused
single-file checks.

## 4. Data sources

The four files are ~1.2 MB and versioned by upstream, so they are downloaded
rather than committed — `assets/emoji/` is git-ignored. From the repository
root:

```bash
U=https://www.unicode.org/Public/17.0.0
mkdir -p assets/emoji
curl -sL $U/emoji/emoji-test.txt -o assets/emoji/emoji-test.txt
curl -sL $U/emoji/emoji-zwj-sequences.txt -o assets/emoji/emoji-zwj-sequences.txt
curl -sL $U/ucd/emoji/emoji-variation-sequences.txt \
  -o assets/emoji/emoji-variation-sequences.txt
curl -sL $U/emoji/emoji-sequences.txt -o assets/emoji/emoji-sequences.txt
```

Bump `17.0.0` in both the URL and the header of this document when the
authoritative tables move; the counts in §1 change with them.
