# Width model: formal specification and conformance

Terminal cell-width assignment in `vtty_width`, stated as pure mathematics. The
prose is confined to the **binding** that maps each symbol to its code
identifier, so every formula below can be checked against one function.

Three layers, each a refinement of the previous:

| layer | function | question |
|---|---|---|
| 1 | $`w_1 : \Sigma \to \{0,1,2,\bot\}`$ | how wide is one code point alone? |
| 2 | $`w_2 : \Sigma^{*} \to \mathbb{N}^{*}`$ | how wide is one string, read cold? |
| 3 | $`W`$ | how wide is a string, given the order it was *written*? |

Layer 3 exists because width is **not** a function of text alone (§Theorem 1).

Extracted from the design specification, trimmed to what this crate
implements: mark *resolution* (which base a mark attaches to) and the
screen-level differential sweeps live in the consumer, not here.

## Binding

| symbol | code |
|---|---|
| $`w_1`$ | `vtty_width::cell_width` |
| $`w_1^{0}`$ | `cell_width(c).unwrap_or(0)` |
| $`w_2`$ | `vtty_width::cell_widths` |
| $`\sum w_2`$ | `vtty_width::cell_width_sum` |
| $`wc`$ | the baked ICU4X width (`Properties::width`), derived from `GeneralCategory` + `EastAsianWidth` |
| $`EP`$ | `Properties::is_emoji_presentation` |
| $`EAW`$ | `Properties::is_ambiguous` (EAW "A"), bit 8 of the packed `u16` |
| $`\mathsf{cjk}`$ | `vtty_width::cjk_width` / `set_cjk_width`, an `AtomicBool` |
| $`\mathsf{cat}`$ | `vtty_width::properties::emoji_category` (crate-internal) |
| $`\mathsf{starter}`$ | `vtty_width::emoji::is_emoji_starter` (crate-internal) |
| $`\mathsf{cont}`$ | `vtty_width::emoji::is_continuation` (crate-internal) |
| $`\mathsf{target}`$ | `vtty_width::is_zwj_target` |
| $`\mathsf{vs\_base}`$ | `vtty_width::is_vs_base` (attach) |
| $`\mathsf{vs\_target}`$ | `vtty_width::is_vs_target` (widen) |
| $`\rho`$ | `vtty_width::emoji::run_width` (crate-internal) |
| $`\sigma`$ | `vtty_width::EmojiDFA::presentation_style` |
| the lone-mark fix | `vtty_width::emoji::finalize_lead` (crate-internal) |
| $`\mathsf{cont\_t}`$ | `vtty_width::emoji::run_continues` (crate-internal) |
| the grapheme-breaker decision | `vtty_width::composes_precomposed` |
| cell↔char floor/ceil/end | `vtty_width::{cell_col_char_from_widths, cell_col_char_ceil_from_widths, cell_col_char_end_from_widths}` |
| cell↔byte floor/ceil + grapheme tails | `vtty_width::{cell_col_byte_widths, cell_col_byte_from_widths, cell_col_byte_ceil_from_widths, grapheme_end_from_widths, grapheme_tail_end_from_widths}` |
| $`B`$ | `SplitBoundaries.fb`, a sparse $`(byte, flags)`$ trace |
| $`F`$ | `SplitBoundaries.zwj_flushes` |
| $`W`$ | `vtty_width::split_widths(text, &SplitBoundaries, a)` |
| $`\mathsf{split}`$ | `vtty_width::write_order::split_offsets` (crate-internal) |
| $`\mathsf{seg}`$ | `vtty_width::write_order::segmented_widths` (crate-internal) |
| $`\mathsf{correct\_F}`$ | `vtty_width::write_order::override_flush_width` (crate-internal) |
| edit splice | `vtty_width::splice_offset` |
| live flags | `vtty_width::EmojiTrace.{zwj, prev_emoji, drop_next_zwj, grapheme_open}` |
| per-print classification | `vtty_width::classify_cell` |
| wrap decision | `vtty_width::wrap_width` |

## Notation

- $`\Sigma`$: the Unicode scalar values; $`\Sigma^{*}`$, finite strings over $`\Sigma`$.
- $`\varepsilon`$: empty string; $`s \cdot t`$, concatenation; $`|s|`$, code-point count.
- $`s[k]`$: the $`k`$-th code point; $`s[i..j)`$, code points $`i \dots j-1`$ (code-point indices, not bytes).
- $`\mathsf{byte}(c)`$: the UTF-8 length of $`c`$; $`\mathsf{bytes}(s,k) = \sum_{t<k}\mathsf{byte}(s[t])`$.
- $`\mathbb{N} = \{0,1,2,\dots\}`$; $`\mathbb{B} = \{\mathsf{false},\mathsf{true}\}`$; $`\bot`$, "no width".
- $`[u,v]`$: the closed scalar range.

---

## Layer 1: $`w_1`$ (per-char base width)

```math
\begin{aligned}
\mathsf{CTRL}    &= \{ c : c < \texttt{0x20} \lor c = \texttt{0x7F} \} \\
\mathsf{PRINT}   &= \{ c : \texttt{0x20} \le c \le \texttt{0x7E} \} \\
\mathsf{SKIN}    &= [\texttt{U+1F3FB}, \texttt{U+1F3FF}] \\
\mathsf{TAG}     &= [\texttt{U+E0020}, \texttt{U+E007F}] \\
\mathsf{DAKUTEN} &= \{ \texttt{U+FF9E}, \texttt{U+FF9F} \}
\end{aligned}
```

```math
w_1(c) =
\begin{cases}
\bot & c \in \mathsf{CTRL} \\
1    & c \in \mathsf{PRINT} \\
0    & c \in \mathsf{SKIN} \cup \mathsf{TAG} \\
1    & c \in \mathsf{DAKUTEN} \\
2    & wc(c) \le 1 \land EP(c) \\
2    & wc(c) = 1 \land EAW(c) = \mathsf{Ambiguous} \land \mathsf{cjk} \\
wc(c) & \text{otherwise}
\end{cases}
```

$`wc`$ is wcwidth. $`EP`$ is `Emoji_Presentation`: a text-default emoji such as
❤ U+2764 (`Emoji=Yes`, `Emoji_Presentation=No`) is **one** cell alone; only a
VS16 or a ZWJ-family membership widens it (Layer 2).

$`\mathsf{cjk}`$ is the runtime CJK mode. When on, every width-1 Ambiguous char
(Enclosed Alphanumerics, Greek, Cyrillic, box drawing, …) becomes 2 cells,
matching `wcwidth` under a CJK locale and Ghostty's `cjk_width`. It is off by
default, so Ambiguous stays 1 cell and the renderer's Enclosed-Alphanumerics
scale-down keeps the ink inside the 1-cell box.

$`w_1^{0}(c) = w_1(c)`$ when $`w_1(c) \neq \bot`$, else $`0`$.

**Invariant 1 (halfwidth sound marks are never continuations).**
$`\mathsf{DAKUTEN}`$ chars carry `Grapheme_Cluster_Break=Extend`, but they are
independent halfwidth cells ($`w_1 = 1`$). Layer 2 must exclude them from its
`Extend`/`SpacingMark` continuation branch, or `❤ﾞ` reads `[1,0]` instead of
`[1,1]`. Locked by
`vtty_width::tests::halfwidth_sound_mark_does_not_attach_to_emoji_base`.

---

## Layer 2: $`w_2`$ (per-run emoji DFA)

### Categories

```math
C = \{ \mathsf{Emoji}, \mathsf{EmojiPresentation}, \mathsf{EmojiModifierBase},
\mathsf{EmojiModifier}, \mathsf{KeycapBase}, \mathsf{KeycapEnd},
\mathsf{RegionalIndicator}, \mathsf{TagBase}, \mathsf{TagSpec},
\mathsf{TagEnd}, \mathsf{Vs15}, \mathsf{Vs16}, \mathsf{Zwj}, \mathsf{Other} \}
```

$`\mathsf{cat} : \Sigma \to C`$ is `parley_emoji`'s segmentation category.

### Predicates

```math
\begin{aligned}
\mathsf{starter}(c) &\iff \mathsf{cat}(c) \in \{ \mathsf{Emoji}, \mathsf{EmojiPresentation}, \mathsf{EmojiModifierBase}, \mathsf{KeycapBase}, \mathsf{TagBase}, \mathsf{RegionalIndicator} \} \\
\mathsf{cont}(c) &\iff \mathsf{cat}(c) \in \{ \mathsf{Vs15}, \mathsf{Vs16}, \mathsf{Zwj}, \mathsf{KeycapEnd}, \mathsf{TagSpec}, \mathsf{TagEnd}, \mathsf{EmojiModifier} \} \\
\mathsf{target}(c) &\iff \mathsf{cat}(c) \in \{ \mathsf{Emoji}, \mathsf{EmojiPresentation}, \mathsf{EmojiModifierBase} \} \\
\mathsf{vs\_base}(c) &\iff ExtPict(c) \\
\mathsf{vs\_target}(c) &\iff Emoji(c) \land \mathsf{cat}(c) \neq \mathsf{KeycapBase}
\end{aligned}
```

```math
\mathsf{ZWJ} = \texttt{U+200D}, \quad \mathsf{VS15} = \texttt{U+FE0E}, \quad \mathsf{VS16} = \texttt{U+FE0F}
```

A selector has **two** predicates, and they are not the same question:

- $`\mathsf{vs\_base}`$: `Extended_Pictographic` (including the reserved emoji
  ranges), decides whether the selector **attaches** (is stored as a
  continuation) or is a stray. Keycap bases are $`\mathsf{Emoji}`$ but not
  $`ExtPict`$, so a selector there is a stray in the direct path; the full
  keycap sequence goes through the DFA.
- $`\mathsf{vs\_target}`$: $`\mathsf{Emoji}`$ minus keycap bases, decides whether
  the selector **changes the base's width**. An ExtPict-but-not-Emoji base
  attaches the selector with no width change.

### Runs

A **run** is a maximal interval $`[i,j)`$ with $`\mathsf{starter}(s[i])`$ whose
scan never rejects. The scan carries $`(\mathsf{prev\_zwj}, \mathsf{ri})`$:

```text
scan(s, i):
  t ← i;  prev_zwj ← false;  ri ← 0
  loop:
    if t = i:  accept
    else:
      cont_t ⇐ starter(s[i]) ∧
        (   (cat(s[i]) = KeycapBase ∧ cat(s[t]) ∈ {Vs15, Vs16, KeycapEnd})
          ∨ (cat(s[i]) = RegionalIndicator ∧ cat(s[t]) = RegionalIndicator ∧ ri = 1)
          ∨ (cat(s[i]) ∉ {KeycapBase, RegionalIndicator} ∧ cont(s[t]))
          ∨ (prev_zwj ∧ target(s[t]))
          ∨ (cat(s[i]) ∉ {KeycapBase, RegionalIndicator}
             ∧ GCB(s[t]) ∈ {Extend, SpacingMark} ∧ s[t] ∉ DAKUTEN) )
      if ¬cont_t:  stop
    ri      ← ri + 1  if cat(s[t]) = RegionalIndicator  else 0
    prev_zwj ← (cat(s[t]) = Zwj)
    t ← t + 1
```

Keycap and RI leads have no $`\to \mathsf{Zwj}`$ transition, so a ZWJ after them
is a **stray**, not a joiner. The last clause is the "a mark may attach to any
emoji base" exception; it is keyed on the GCB class rather than the simpler
$`w_1 = 0`$ test, so format controls ($`w_1 = 0`$, `GCB=Control`) do **not**
continue a run, and Invariant 1 excludes $`\mathsf{DAKUTEN}`$.

### Presentation and lead width

```math
\mathsf{vs}(r) = \text{the first selector in } r \; (\bot,\ \mathsf{VS15},\ \text{or } \mathsf{VS16})
\qquad
\mathsf{keycap\_end}(r) \iff \exists t \in [i,j): \mathsf{cat}(s[t]) = \mathsf{KeycapEnd}
```

```math
\sigma(r) =
\begin{cases}
\mathsf{Text}  & \mathsf{vs}(r) = \mathsf{VS15} \\
\mathsf{Emoji} & \mathsf{vs}(r) = \mathsf{VS16} \\
\mathsf{Text}  & \mathsf{type}(r) = \mathsf{Basic} \land \lnot EP(s[i]) \\
\mathsf{Emoji} & \text{otherwise}
\end{cases}
```

```math
\rho(r) =
\begin{cases}
2 & \mathsf{cat}(s[i]) = \mathsf{KeycapBase} \land \mathsf{vs}(r) \neq \bot \land \mathsf{keycap\_end}(r) \\
1 & \mathsf{cat}(s[i]) = \mathsf{KeycapBase} \\
2 & \sigma(r) = \mathsf{Emoji} \\
1 & \text{otherwise}
\end{cases}
```

$`\rho`$ is not $`\sigma`$ alone: $`\sigma(\texttt{1 VS16}) = \mathsf{Emoji}`$, yet the
incomplete keycap stays 1 cell. Only the complete
$`\mathsf{base}\ \mathsf{VS16}\ \mathsf{KeycapEnd}`$ widens.

### Width vector

```math
\mathsf{raw}_2[k] =
\begin{cases}
\rho(r)        & s[k] \text{ leads a run } r \text{ with } |r| > 1 \\
w_1^{0}(s[k])  & s[k] \text{ leads a lone run } (|r| = 1) \\
0              & s[k] \text{ is a continuation}
\end{cases}
```

Every code point belongs to exactly one run, and a non-starter lead is always a
lone run.

### Lone-mark fix

$`w_2 = \mathsf{fix}(\mathsf{raw}_2, s)`$. A width-0 lead is one cell when it has
no visible base to attach to, **or** when its only base is a space and another
mark follows (the space is invisible, so the *first* mark must be visible for
the second to have something to attach to).

```text
fix(w, s):
  has_base ← false;  space_base ← false;  hangul ← 0
  for k = 0 … |s|−1:
    followed ← s[k+1] is a width-0 non-selector mark
    if w[k] = 0:
      attaches ← match GCB(s[k]):
                   V => hangul & 1 ≠ 0
                   T => hangul & 2 ≠ 0
                   _ => has_base
      w[k] ← 1  iff  s[k] is not a selector
                 ∧ GCB(s[k]) ∈ {Extend, SpacingMark, V, T}
                 ∧ (¬attaches ∨ (followed ∧ space_base))
    if w[k] > 0:
      has_base ← true;  space_base ← (s[k] = ' ');  hangul ← hangul_after(GCB(s[k]))
    else if GCB(s[k]) ∈ {Control, CR, LF}:
      has_base ← false;  space_base ← false;  hangul ← 0
    else if GCB(s[k]) ∈ {V, T}:
      hangul ← hangul_after(GCB(s[k]))
  return w
```

`hangul` is the two-bit GB6/GB7/GB8 state: bit 0 = a V may follow, bit 1 = a T
may follow; `L → 1`, `LV → 3`, `V → 3`, `LVT → 2`, `T → 2`, else `0`.

| text | $`w_2`$ |
|---|---|
| `\u{301}` | `[1]` |
| `\u{301}\u{301}` | `[1, 0]` |
| `" \u{301}"` (space + mark) | `[1, 0]` |
| `" \u{301}\u{301}"` (space + two marks) | `[1, 1, 0]` |
| `1\u{323}\u{323}0` (visible base + two marks) | `[1, 0, 0, 1]` |

### Combining vs spacing look-alike characters

A *mark* is a char whose base width is 0. The spacing look-alike characters are `Sk`
modifiers and keep their 1-cell width; they are not marks:

| char | $`w_1`$ | with base `e` / `" "` |
|---|---|---|
| U+0301 COMBINING ACUTE (Mn) | 0 | `[1,0]` / `[1,0]` |
| U+00B4 ACUTE ACCENT (Sk) | 1 | `[1,1]` / `[1,1]` |
| U+0060 GRAVE ACCENT (Sk) | 1 | `[1,1]` / `[1,1]` |

Locked by `vtty_width::tests::combining_vs_spacing_acute`.

---

## Layer 3: write order

Width depends on the order chars were written, not just on the resulting
string: a control byte closes the DFA scope. Layer 3 makes that order explicit
as an automaton over an **event stream**, and reduces it to two pieces of
data (the per-byte boundary trace $`B`$ and the flushed-base set $`F`$) that the
pure function $`W`$ consumes.

### Event stream

```math
E = e_1 \dots e_N, \qquad e \in \{ P(c) : c \in \Sigma \} \cup \{ K \}
\qquad \text{(print, or a grapheme-breaking control)}
```

$`s = \mathsf{str}(E)`$ is the accumulated text.

### Per-print classification

```text
classify(c, w):                      # called before printing c
  c = ZWJ:   zwj ← prev_emoji ∧ ¬drop_next_zwj;  prev_emoji ← false
  w = 0:     zwj ← false              # prev_emoji unchanged
  w > 0:     zwj ← false;  prev_emoji ← target(c)
```

The cursor advance (`cell`) of the char is `classify_cell`:

```math
\mathsf{cell} =
\begin{cases}
0 & \mathsf{zwj} \land \mathsf{target}(c) \\
w & \mathsf{raw} > 0 \land w = 0 \\
w & (\mathsf{followed\_by\_zwj} \lor \mathsf{keycap}) \land w > \mathsf{raw} \\
w & \mathsf{raw} = 0 \\
\mathsf{raw} & \text{otherwise}
\end{cases}
\quad\text{(buffered: } w = \mathsf{dfa\_w} \neq \bot\text{)}, \qquad
\mathsf{cell} =
\begin{cases}
0 & \mathsf{zwj} \land \mathsf{target}(c) \land \lnot\mathsf{wrap\_pending} \\
\mathsf{raw} & \text{otherwise}
\end{cases}
\quad\text{(direct)}
```

"Buffered" means the char is part of the segmented whole-text DFA ($`w`$ known);
"direct" means it was written straight to a cell (a REP'd base, a direct mark)
with no DFA lookahead. The `keycap` arm lets a buffered in-sequence cell take
the DFA width when it exceeds the native one.

### Controls and boundaries

```text
close_zwj_scope:  if zwj:  fb[max(zwj_tail, cursor_byte)] ← fb[…] ∨ 1
reset_zwj_state:  zwj ← false;  prev_emoji ← false;
                  drop_next_zwj ← false;  grapheme_open ← false
```

Every control closes the joiner; a real control also restarts the split,
recorded as bit 0 of $`fb`$. A ZWJ that **re-opens** after a control records its
boundary as bit 1 at print time.

### The derived width $`W`$

```math
B = \{ \mathsf{bytes}(s,k) : fb[k] \neq 0 \}, \qquad F = \mathsf{zwj\_flushes}
```

```math
\mathsf{split}(B,a,b) = \mathrm{sorted}\,\{\, x \in B : a < x < b,\ x \text{ is a char boundary},\ \mathsf{keep}(x) \,\}
```

```math
\mathsf{keep}(x) = \bigl( s[x-1] = \mathsf{ZWJ} \land w_1^{0}(s[x]) > 0 \bigr) \;\lor\; \bigl( \mathsf{fresh}(x) \land w_1^{0}(s[x]) = 0 \land s[x] \notin \{ \mathsf{VS15}, \mathsf{VS16}, \mathsf{ZWJ} \} \bigr)
```

The first clause of `keep` is the ZWJ boundary; the second is the
standalone-mark rule: a width-0 mark written as a fresh base (after a control,
or shifted by an insert) keeps its own cell, which the whole-text DFA would
otherwise re-attach to a preceding base.

```math
\mathsf{seg}(B, s[a..b)) = w_2(s[a..x_1)) \cdot w_2(s[x_1..x_2)) \cdot \dots \cdot w_2(s[x_k..b))
```

$`\mathsf{correct\_F}`$ un-widens a REP'd or direct-ZWJ flushed base that the
whole-text DFA widened to 2, **unless** a selector in the trailing zero-width
run explicitly re-widens or narrows it (the selector wins):

```math
\begin{aligned}
\mathsf{overridable}(s,f) \iff {} & f \text{ is a char boundary} \land s[f..] \neq \varepsilon \land s[f] \notin \{ \mathsf{VS15}, \mathsf{VS16} \} \\
& \land \exists\, \mathsf{ZWJ} \in s[f..] \text{ before the first char with } w_1^{0} > 0 \\
& \land \nexists\, \mathsf{VS16}/\mathsf{VS15} \in s[f..] \text{ before the first char with } w_1^{0} > 0
\end{aligned}
```

```text
correct_F(w, s, a, F):
  for f ∈ F, f ≥ a, overridable(s, f):
    c ← the code point ending at f
    if w₁⁰(c) > 0 ∧ w[idx(c)] > w₁⁰(c):
      w[idx(c)] ← max(w₁⁰(c), 1)
  return w
```

```math
W(s,a,b,B,F) = \mathsf{correct\_F}(\mathsf{seg}(B,s[a..b)),\ s,\ a,\ F)
```

`split_widths` is exactly this: `seg` at the split offsets, then `correct_F`.

### Theorem 1 (non-compositionality)

$`W`$ is a function of $`(s,B,F)`$, not of $`s`$ alone:

```math
\begin{aligned}
E_1 = P(👍)\,P(\mathrm{ZWJ})\,P(👍)     &\implies s = \text{👍 ZWJ 👍},\ B = \varnothing               &\implies W &= [2,0,0] \\
E_2 = P(👍)\,P(\mathrm{ZWJ})\,K\,P(👍) &\implies s = \text{👍 ZWJ 👍},\ B = \{\mathsf{bytes}(s,2)\}     &\implies W &= [2,0,2]
\end{aligned}
```

$`B`$ and $`F`$ cannot be recovered from $`s`$, which is why a caller that streams
bytes must carry an `EmojiTrace` alongside the text.

### Representation

`fb` is a **sparse per-byte flag map**: `SplitBoundaries.fb`, a sorted
`(byte_offset, flags)` list; bytes absent from it are implicitly zero. A
dense byte-per-character array would double the memory of emoji-bearing
scrollback, and fresh bases are rare.

```math
\begin{aligned}
fb[k] \mathbin{\&} 1 \neq 0 &\iff s[k] \text{ was written as a fresh base (a control closed the prior scope)} \\
fb[k] \mathbin{\&} 2 \neq 0 &\iff s[k] \text{ re-opened a ZWJ after a control (print-time only)} \\
fb[k] \mathbin{\&} 4 \neq 0 &\iff s[k] \text{ is a fresh base an ICH shifted right (a standalone mark)}
\end{aligned}
```

Bit 2 exists because a standalone mark can be *shifted* past a later overwrite
so that its predecessor is no longer a space, yet it must still stay one cell;
`shift_fresh_over_insert` records it and `split` fires on `fresh ∨ bit 2`. A
width-0 mark that attaches clears the fresh/ICH bits at its byte
(`clear_fresh_range`), so a stale marker cannot misclassify it.

Edits maintain `fb` through `splice_offset(t, pos, end, diff)`: keep $`t < pos`$,
drop $`[pos, end)`$, shift $`t \ge end`$ by $`diff`$ (clamped to 0).

---

## Corollaries that bind callers

### Corollary (a keycap is 2 cells only when complete)

```math
\begin{aligned}
w_2(\texttt{1 ZWJ 👍}) &= [1, 0, 2] & w_2(\texttt{1 VS16}) &= [1, 0] \\
w_2(\texttt{1 U+20E3}) &= [1, 0]   & w_2(\texttt{1 VS16 U+20E3}) &= [2, 0, 0]
\end{aligned}
```

A ZWJ after a keycap base or a regional indicator is a stray.

### Corollary (a selector after a family continuation does not re-widen)

The selector's width change is keyed off the base's **current** width, not its
native width: a base already 2 cells has a delta of 0, so the selector appends
with no column advance.

```math
W(\text{❤ ZWJ 👍 VS16}) = [2,0,0,0]
```

### Corollary (wrap width ≠ cursor advance)

```math
\mathsf{wrap\_width}(\mathsf{cell}, \mathsf{dfa\_w}) = \max(\mathsf{cell}, \mathsf{dfa\_w})
```

A 0-cell ZWJ continuation still occupies its DFA width for the autowrap
decision: at a full row, `ZWJ 👍VS16` wraps the `👍` as a fresh 2-cell base and
the VS16 attaches to that new base, so a soft wrap breaks the grapheme.

## Deviations from the design specification

Where the implementation is deliberately narrower than the prose spec:

1. **The "plain mark" clause of $`\mathsf{cont\_t}`$ is `GCB ∈ {Extend,
   SpacingMark} ∧ ¬DAKUTEN`**, not $`w_1 = 0 \land s \notin \{\text{selectors}\}`$.
   The prose form would also admit format controls ($`w_1 = 0`$,
   `GCB=Control`) as run continuations.
2. **The lone-mark fix is gated on the GCB class** (`Extend`, `SpacingMark`,
   `V`, `T`) and on the Hangul pair state, so a lone V or T jamo attaches only
   to a compatible syllable; the prose `fix` has no Hangul arm.
3. **`classify_cell` has a `keycap` arm** (`(followed_by_zwj ∨ keycap) ∧ w >
   raw`) that the prose classification does not state.

## Conformance

What the model has to agree with: the authoritative Unicode data files, the
three deliberate terminal divergences from them, the terminal-only rules the
layers do not cover, and the engine that checks every sequence. The model
follows UTS #51 as of **Unicode 17.0.0**; ICU4X 2.3.0 bakes the per-codepoint
properties, `parley_emoji` 0.13.0 supplies the version-agnostic DFA.

### Classification and summary

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

#### The three divergence classes

| class | predicate | signature | justification |
|---|---|---|---|
| **K₁**: keycap-incomplete | `s = c VS16`, `c ∈ {# * 0-9}` (no `U+20E3`) | `[1,0]` | a keycap base is `Emoji` but not `ExtPict`, so a lone VS16 after it is a stray. The complete `c VS16 U+20E3` widens to `[2,0,0]`. |
| **K₂**: keycap-no-VS16 | `s = c U+20E3`, `c ∈ {# * 0-9}` | `[1,0]` | without the VS16 the keycap is text presentation. |
| **S**: lone skin tone | `s = c`, `c ∈ {1F3FB..1F3FF}` | `[1]` | a skin-tone modifier is a 0-width combining mark; a lone one is one cell. |

These are deliberate, Ghostty-aligned terminal overrides, not defects. They are
the `ρ` keycap arm and the `w₁` skin-tone arm of the formal model.

#### Two more locked behaviours

| rule | signature | locked by |
|---|---|---|
| halfwidth sound marks never continue a run (Invariant 1) | `❤ﾞ` = `[1,1]`, not `[1,0]` | `tests::halfwidth_sound_mark_does_not_attach_to_emoji_base` |
| a selector attaches per `ExtPict`, widens per `Emoji` | `vs_base` ≠ `vs_target` | `tests::emoji_predicates_classify_selectors_and_bases` |

### Terminal-only rules

Behaviours the layers above do not cover, plus the write-order API
surface a streaming consumer drives.

#### Canonical composition (grapheme-breaker decision)

`composes_precomposed(base, mark)`: does `base` + `mark` have a Unicode
canonical precomposed form (`e`+U+0301 → `é`)? A renderer inserts the ZWSP
grapheme break only when this is true; a non-composing base (`◌`, symbols,
CJK) keeps its grapheme intact so the GPOS mark anchor still positions the mark
over the base. Scoped to the combining diacritical marks U+0300..=U+036F (the
`ccmp` block); emoji selectors and Hangul jamo are excluded.

Locked by `tests::composes_precomposed_scopes_to_diacritics`.

#### CJK width mode table

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

#### Hangul jamo and syllable widths

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

#### Cell↔char walkers

Three char-index walkers over the precomputed `cell_widths` vector: a
renderer's run char-range boundaries (floor/ceil/end):

- `cell_col_char_from_widths(w, col)`: the glyph STARTING at (or containing)
  `col`, skipping 0-width continuations (floor).
- `cell_col_char_ceil_from_widths(w, col)`: the first glyph at or past `col`
  (ceil; a 0-width char at the boundary is included).
- `cell_col_char_end_from_widths(w, col)`: one past the glyph ENDING at
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

#### Write-order API surface

Width is not a function of text alone (§Theorem 1 of the formal model). The
crate exposes the pieces a streaming consumer needs:

| item | purpose |
|---|---|
| `EmojiTrace` + `SplitBoundaries` | record the boundary trace `B` and flush set `F` as bytes are written |
| `split_widths(text, &boundaries, start)` | the pure `W`: `correct_F(seg(B, text))` |
| `classify_cell(..)` | the cursor advance of one print, from the live state |
| `wrap_width(cell, dfa_w)` | the autowrap width (≥ the cursor advance) |
| `splice_offset(t, pos, end, diff)` | the edit splice that maintains `fb` |

`fb` is sparse (a sorted `(byte_offset, flags)` list) and carries three bits:
0 = fresh base (a control closed the prior scope), 1 = re-open boundary
(print-time only), 2 = fresh base an ICH shifted right. Bit 2 exists so a
standalone mark that an insert pushed past a non-space predecessor still keeps
its own cell; `clear_fresh_range` drops fresh/ICH bits at a mark that ends up
attaching.

The three terminal overrides (K₁/K₂/S) narrow the catalogue:

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

### Analytical engine

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

### Data sources

The four files are ~1.2 MB and versioned by upstream, so they are downloaded
rather than committed: `assets/emoji/` is git-ignored. From the repository
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
