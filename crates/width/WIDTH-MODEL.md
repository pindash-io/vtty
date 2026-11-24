# Width model — formal specification

Terminal cell-width assignment in `vtty_width`, stated as pure mathematics. The
prose is confined to the **binding** that maps each symbol to its code
identifier, so every formula below can be checked against one function.

Three layers, each a refinement of the previous:

| layer | function | question |
|---|---|---|
| 1 | $w_1 : \Sigma \to \{0,1,2,\bot\}$ | how wide is one code point alone? |
| 2 | $w_2 : \Sigma^{*} \to \mathbb{N}^{*}$ | how wide is one string, read cold? |
| 3 | $W$ | how wide is a string, given the order it was *written*? |

Layer 3 exists because width is **not** a function of text alone (§Theorem 1).

Extracted from the design specification, trimmed to what this crate
implements: mark *resolution* (which base a mark attaches to) and the
screen-level differential sweeps live in the consumer, not here.

## Binding

| symbol | code |
|---|---|
| $w_1$ | `vtty_width::cell_width` |
| $w_1^{0}$ | `cell_width(c).unwrap_or(0)` |
| $w_2$ | `vtty_width::cell_widths` |
| $\sum w_2$ | `vtty_width::cell_width_sum` |
| $wc$ | the baked ICU4X width (`Properties::width`), derived from `GeneralCategory` + `EastAsianWidth` |
| $EP$ | `Properties::is_emoji_presentation` |
| $EAW$ | `Properties::is_ambiguous` (EAW "A"), bit 8 of the packed `u16` |
| $\mathsf{cjk}$ | `vtty_width::cjk_width` / `set_cjk_width` — an `AtomicBool` |
| $\mathsf{cat}$ | `vtty_width::properties::emoji_category` (crate-internal) |
| $\mathsf{starter}$ | `vtty_width::emoji::is_emoji_starter` (crate-internal) |
| $\mathsf{cont}$ | `vtty_width::emoji::is_continuation` (crate-internal) |
| $\mathsf{target}$ | `vtty_width::is_zwj_target` |
| $\mathsf{vs\_base}$ | `vtty_width::is_vs_base` (attach) |
| $\mathsf{vs\_target}$ | `vtty_width::is_vs_target` (widen) |
| $\rho$ | `vtty_width::emoji::run_width` (crate-internal) |
| $\sigma$ | `vtty_width::EmojiDFA::presentation_style` |
| the lone-mark fix | `vtty_width::emoji::finalize_lead` (crate-internal) |
| $\mathsf{cont\_t}$ | `vtty_width::emoji::run_continues` (crate-internal) |
| the grapheme-breaker decision | `vtty_width::composes_precomposed` |
| cell↔char floor/ceil/end | `vtty_width::{cell_col_char_from_widths, cell_col_char_ceil_from_widths, cell_col_char_end_from_widths}` |
| cell↔byte floor/ceil + grapheme tails | `vtty_width::{cell_col_byte_widths, cell_col_byte_from_widths, cell_col_byte_ceil_from_widths, grapheme_end_from_widths, grapheme_tail_end_from_widths}` |
| $B$ | `SplitBoundaries.fb` — sparse $(byte, flags)$ trace |
| $F$ | `SplitBoundaries.zwj_flushes` |
| $W$ | `vtty_width::split_widths(text, &SplitBoundaries, a)` |
| $\mathsf{split}$ | `vtty_width::write_order::split_offsets` (crate-internal) |
| $\mathsf{seg}$ | `vtty_width::write_order::segmented_widths` (crate-internal) |
| $\mathsf{correct\_F}$ | `vtty_width::write_order::override_flush_width` (crate-internal) |
| edit splice | `vtty_width::splice_offset` |
| live flags | `vtty_width::EmojiTrace.{zwj, prev_emoji, drop_next_zwj, grapheme_open}` |
| per-print classification | `vtty_width::classify_cell` |
| wrap decision | `vtty_width::wrap_width` |

## Notation

- $\Sigma$ — the Unicode scalar values; $\Sigma^{*}$ — finite strings over $\Sigma$.
- $\varepsilon$ — empty string; $s \cdot t$ — concatenation; $|s|$ — code-point count.
- $s\[k\]$ — the $k$-th code point; $s\[i..j)$ — code points $i \dots j-1$ (code-point indices, not bytes).
- $\mathsf{byte}(c)$ — the UTF-8 length of $c$; $\mathsf{bytes}(s,k) = \sum_{t<k}\mathsf{byte}(s\[t\])$.
- $\mathbb{N} = \{0,1,2,\dots\}$; $\mathbb{B} = \{\mathsf{false},\mathsf{true}\}$; $\bot$ — "no width".
- $\[u,v\]$ — the closed scalar range.

---

## Layer 1 — $w_1$ (per-char base width)

$$
\begin{aligned}
\mathsf{CTRL}    &= \{ c : c < \texttt{0x20} \lor c = \texttt{0x7F} \} \\
\mathsf{PRINT}   &= \{ c : \texttt{0x20} \le c \le \texttt{0x7E} \} \\
\mathsf{SKIN}    &= \[\texttt{U+1F3FB}, \texttt{U+1F3FF}\] \\
\mathsf{TAG}     &= \[\texttt{U+E0020}, \texttt{U+E007F}\] \\
\mathsf{DAKUTEN} &= \{ \texttt{U+FF9E}, \texttt{U+FF9F} \}
\end{aligned}
$$

$$
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
$$

$wc$ is wcwidth. $EP$ is `Emoji_Presentation`: a text-default emoji such as
❤ U+2764 (`Emoji=Yes`, `Emoji_Presentation=No`) is **one** cell alone; only a
VS16 or a ZWJ-family membership widens it (Layer 2).

$\mathsf{cjk}$ is the runtime CJK mode. When on, every width-1 Ambiguous char
(Enclosed Alphanumerics, Greek, Cyrillic, box drawing, …) becomes 2 cells,
matching `wcwidth` under a CJK locale and Ghostty's `cjk_width`. It is off by
default, so Ambiguous stays 1 cell and the renderer's Enclosed-Alphanumerics
scale-down keeps the ink inside the 1-cell box.

$w_1^{0}(c) = w_1(c)$ when $w_1(c) \neq \bot$, else $0$.

**Invariant 1 (halfwidth sound marks are never continuations).**
$\mathsf{DAKUTEN}$ chars carry `Grapheme_Cluster_Break=Extend`, but they are
independent halfwidth cells ($w_1 = 1$). Layer 2 must exclude them from its
`Extend`/`SpacingMark` continuation branch, or `❤ﾞ` reads `[1,0]` instead of
`[1,1]`. Locked by
`vtty_width::tests::halfwidth_sound_mark_does_not_attach_to_emoji_base`.

---

## Layer 2 — $w_2$ (per-run emoji DFA)

### Categories

$$
C = \{ \mathsf{Emoji}, \mathsf{EmojiPresentation}, \mathsf{EmojiModifierBase},
\mathsf{EmojiModifier}, \mathsf{KeycapBase}, \mathsf{KeycapEnd},
\mathsf{RegionalIndicator}, \mathsf{TagBase}, \mathsf{TagSpec},
\mathsf{TagEnd}, \mathsf{Vs15}, \mathsf{Vs16}, \mathsf{Zwj}, \mathsf{Other} \}
$$

$\mathsf{cat} : \Sigma \to C$ is `parley_emoji`'s segmentation category.

### Predicates

$$
\begin{aligned}
\mathsf{starter}(c) &\iff \mathsf{cat}(c) \in \{ \mathsf{Emoji}, \mathsf{EmojiPresentation}, \mathsf{EmojiModifierBase}, \mathsf{KeycapBase}, \mathsf{TagBase}, \mathsf{RegionalIndicator} \} \\
\mathsf{cont}(c) &\iff \mathsf{cat}(c) \in \{ \mathsf{Vs15}, \mathsf{Vs16}, \mathsf{Zwj}, \mathsf{KeycapEnd}, \mathsf{TagSpec}, \mathsf{TagEnd}, \mathsf{EmojiModifier} \} \\
\mathsf{target}(c) &\iff \mathsf{cat}(c) \in \{ \mathsf{Emoji}, \mathsf{EmojiPresentation}, \mathsf{EmojiModifierBase} \} \\
\mathsf{vs\_base}(c) &\iff ExtPict(c) \\
\mathsf{vs\_target}(c) &\iff Emoji(c) \land \mathsf{cat}(c) \neq \mathsf{KeycapBase}
\end{aligned}
$$

$$
\mathsf{ZWJ} = \texttt{U+200D}, \quad \mathsf{VS15} = \texttt{U+FE0E}, \quad \mathsf{VS16} = \texttt{U+FE0F}
$$

A selector has **two** predicates, and they are not the same question:

- $\mathsf{vs\_base}$ — `Extended_Pictographic` (including the reserved emoji
  ranges) — decides whether the selector **attaches** (is stored as a
  continuation) or is a stray. Keycap bases are $\mathsf{Emoji}$ but not
  $ExtPict$, so a selector there is a stray in the direct path; the full
  keycap sequence goes through the DFA.
- $\mathsf{vs\_target}$ — $\mathsf{Emoji}$ minus keycap bases — decides whether
  the selector **changes the base's width**. An ExtPict-but-not-Emoji base
  attaches the selector with no width change.

### Runs

A **run** is a maximal interval $\[i,j)$ with $\mathsf{starter}(s\[i\])$ whose
scan never rejects. The scan carries $(\mathsf{prev\_zwj}, \mathsf{ri})$:

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

Keycap and RI leads have no $\to \mathsf{Zwj}$ transition, so a ZWJ after them
is a **stray**, not a joiner. The last clause is the "a mark may attach to any
emoji base" exception; it is keyed on the GCB class rather than the simpler
$w_1 = 0$ test, so format controls ($w_1 = 0$, `GCB=Control`) do **not**
continue a run, and Invariant 1 excludes $\mathsf{DAKUTEN}$.

### Presentation and lead width

$$
\mathsf{vs}(r) = \text{the first selector in } r \; (\bot,\ \mathsf{VS15},\ \text{or } \mathsf{VS16})
\qquad
\mathsf{keycap\_end}(r) \iff \exists t \in \[i,j): \mathsf{cat}(s\[t\]) = \mathsf{KeycapEnd}
$$

$$
\sigma(r) =
\begin{cases}
\mathsf{Text}  & \mathsf{vs}(r) = \mathsf{VS15} \\
\mathsf{Emoji} & \mathsf{vs}(r) = \mathsf{VS16} \\
\mathsf{Text}  & \mathsf{type}(r) = \mathsf{Basic} \land \lnot EP(s\[i\]) \\
\mathsf{Emoji} & \text{otherwise}
\end{cases}
$$

$$
\rho(r) =
\begin{cases}
2 & \mathsf{cat}(s\[i\]) = \mathsf{KeycapBase} \land \mathsf{vs}(r) \neq \bot \land \mathsf{keycap\_end}(r) \\
1 & \mathsf{cat}(s\[i\]) = \mathsf{KeycapBase} \\
2 & \sigma(r) = \mathsf{Emoji} \\
1 & \text{otherwise}
\end{cases}
$$

$\rho$ is not $\sigma$ alone: $\sigma(\texttt{1 VS16}) = \mathsf{Emoji}$, yet the
incomplete keycap stays 1 cell. Only the complete
$\mathsf{base}\ \mathsf{VS16}\ \mathsf{KeycapEnd}$ widens.

### Width vector

$$
\mathsf{raw}_2\[k\] =
\begin{cases}
\rho(r)        & s\[k\] \text{ leads a run } r \text{ with } |r| > 1 \\
w_1^{0}(s\[k\])  & s\[k\] \text{ leads a lone run } (|r| = 1) \\
0              & s\[k\] \text{ is a continuation}
\end{cases}
$$

Every code point belongs to exactly one run, and a non-starter lead is always a
lone run.

### Lone-mark fix

$w_2 = \mathsf{fix}(\mathsf{raw}_2, s)$. A width-0 lead is one cell when it has
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

| text | $w_2$ |
|---|---|
| `\u{301}` | `[1]` |
| `\u{301}\u{301}` | `[1, 0]` |
| `" \u{301}"` (space + mark) | `[1, 0]` |
| `" \u{301}\u{301}"` (space + two marks) | `[1, 1, 0]` |
| `1\u{323}\u{323}0` (visible base + two marks) | `[1, 0, 0, 1]` |

### Combining vs spacing look-alike characters

A *mark* is a char whose base width is 0. The spacing look-alike characters are `Sk`
modifiers and keep their 1-cell width; they are not marks:

| char | $w_1$ | with base `e` / `" "` |
|---|---|---|
| U+0301 COMBINING ACUTE (Mn) | 0 | `[1,0]` / `[1,0]` |
| U+00B4 ACUTE ACCENT (Sk) | 1 | `[1,1]` / `[1,1]` |
| U+0060 GRAVE ACCENT (Sk) | 1 | `[1,1]` / `[1,1]` |

Locked by `vtty_width::tests::combining_vs_spacing_acute`.

---

## Layer 3 — write order

Width depends on the order chars were written, not just on the resulting
string: a control byte closes the DFA scope. Layer 3 makes that order explicit
as an automaton over an **event stream**, and reduces it to two pieces of
data — the per-byte boundary trace $B$ and the flushed-base set $F$ — that the
pure function $W$ consumes.

### Event stream

$$
E = e_1 \dots e_N, \qquad e \in \{ P(c) : c \in \Sigma \} \cup \{ K \}
\qquad \text{(print, or a grapheme-breaking control)}
$$

$s = \mathsf{str}(E)$ is the accumulated text.

### Per-print classification

```text
classify(c, w):                      # called before printing c
  c = ZWJ:   zwj ← prev_emoji ∧ ¬drop_next_zwj;  prev_emoji ← false
  w = 0:     zwj ← false              # prev_emoji unchanged
  w > 0:     zwj ← false;  prev_emoji ← target(c)
```

The cursor advance (`cell`) of the char is `classify_cell`:

$$
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
$$

"Buffered" means the char is part of the segmented whole-text DFA ($w$ known);
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
recorded as bit 0 of $fb$. A ZWJ that **re-opens** after a control records its
boundary as bit 1 at print time.

### The derived width $W$

$$
B = \{ \mathsf{bytes}(s,k) : fb\[k\] \neq 0 \}, \qquad F = \mathsf{zwj\_flushes}
$$

$$
\begin{aligned}
\mathsf{split}(B,a,b) = \operatorname{sorted}\{\, x : {} & x \in B,\ a < x < b,\ x \text{ is a char boundary},\\
& (\, s\[x-1\] = \mathsf{ZWJ} \land w_1^{0}(s\[x\]) > 0 \,) \\
& {}\lor (\, \mathsf{fresh}(x) \land w_1^{0}(s\[x\]) = 0 \land s\[x\] \notin \{ \mathsf{VS15}, \mathsf{VS16}, \mathsf{ZWJ} \} \,) \,\}
\end{aligned}
$$

The second clause is the standalone-mark rule: a width-0 mark written as a
fresh base (after a control, or shifted by an insert) keeps its own cell, which
the whole-text DFA would otherwise re-attach to a preceding base.

$$
\mathsf{seg}(B, s\[a..b)) = w_2(s\[a..x_1)) \cdot w_2(s\[x_1..x_2)) \cdot \dots \cdot w_2(s\[x_k..b))
$$

$\mathsf{correct\_F}$ un-widens a REP'd or direct-ZWJ flushed base that the
whole-text DFA widened to 2, **unless** a selector in the trailing zero-width
run explicitly re-widens or narrows it (the selector wins):

$$
\begin{aligned}
\mathsf{overridable}(s,f) \iff {} & f \text{ is a char boundary} \land s\[f..\] \neq \varepsilon \land s\[f\] \notin \{ \mathsf{VS15}, \mathsf{VS16} \} \\
& \land \exists\, \mathsf{ZWJ} \in s\[f..\] \text{ before the first char with } w_1^{0} > 0 \\
& \land \nexists\, \mathsf{VS16}/\mathsf{VS15} \in s\[f..\] \text{ before the first char with } w_1^{0} > 0
\end{aligned}
$$

```text
correct_F(w, s, a, F):
  for f ∈ F, f ≥ a, overridable(s, f):
    c ← the code point ending at f
    if w₁⁰(c) > 0 ∧ w[idx(c)] > w₁⁰(c):
      w[idx(c)] ← max(w₁⁰(c), 1)
  return w
```

$$
W(s,a,b,B,F) = \mathsf{correct\_F}(\mathsf{seg}(B,s\[a..b)),\ s,\ a,\ F)
$$

`split_widths` is exactly this: `seg` at the split offsets, then `correct_F`.

### Theorem 1 (non-compositionality)

$W$ is a function of $(s,B,F)$, not of $s$ alone:

$$
\begin{aligned}
E_1 = P(👍)\,P(\mathrm{ZWJ})\,P(👍)     &\implies s = \text{👍 ZWJ 👍},\ B = \varnothing               &\implies W &= \[2,0,0\] \\
E_2 = P(👍)\,P(\mathrm{ZWJ})\,K\,P(👍) &\implies s = \text{👍 ZWJ 👍},\ B = \{\mathsf{bytes}(s,2)\}     &\implies W &= \[2,0,2\]
\end{aligned}
$$

$B$ and $F$ cannot be recovered from $s$, which is why a caller that streams
bytes must carry an `EmojiTrace` alongside the text.

### Representation

`fb` is a **sparse per-byte flag map** — `SplitBoundaries.fb`, a sorted
`(byte_offset, flags)` list; bytes absent from it are implicitly zero. A
dense byte-per-character array would double the memory of emoji-bearing
scrollback, and fresh bases are rare.

$$
\begin{aligned}
fb\[k\] \mathbin{\&} 1 \neq 0 &\iff s\[k\] \text{ was written as a fresh base (a control closed the prior scope)} \\
fb\[k\] \mathbin{\&} 2 \neq 0 &\iff s\[k\] \text{ re-opened a ZWJ after a control (print-time only)} \\
fb\[k\] \mathbin{\&} 4 \neq 0 &\iff s\[k\] \text{ is a fresh base an ICH shifted right (a standalone mark)}
\end{aligned}
$$

Bit 2 exists because a standalone mark can be *shifted* past a later overwrite
so that its predecessor is no longer a space, yet it must still stay one cell;
`shift_fresh_over_insert` records it and `split` fires on `fresh ∨ bit 2`. A
width-0 mark that attaches clears the fresh/ICH bits at its byte
(`clear_fresh_range`), so a stale marker cannot misclassify it.

Edits maintain `fb` through `splice_offset(t, pos, end, diff)`: keep $t < pos$,
drop $\[pos, end)$, shift $t \ge end$ by $diff$ (clamped to 0).

---

## Corollaries that bind callers

### Corollary (a keycap is 2 cells only when complete)

$$
\begin{aligned}
w_2(\texttt{1 ZWJ 👍}) &= \[1, 0, 2\] & w_2(\texttt{1 VS16}) &= \[1, 0\] \\
w_2(\texttt{1 U+20E3}) &= \[1, 0\]   & w_2(\texttt{1 VS16 U+20E3}) &= \[2, 0, 0\]
\end{aligned}
$$

A ZWJ after a keycap base or a regional indicator is a stray.

### Corollary (a selector after a family continuation does not re-widen)

The selector's width change is keyed off the base's **current** width, not its
native width: a base already 2 cells has a delta of 0, so the selector appends
with no column advance.

$$
W(\text{❤ ZWJ 👍 VS16}) = \[2,0,0,0\]
$$

### Corollary (wrap width ≠ cursor advance)

$$
\mathsf{wrap\_width}(\mathsf{cell}, \mathsf{dfa\_w}) = \max(\mathsf{cell}, \mathsf{dfa\_w})
$$

A 0-cell ZWJ continuation still occupies its DFA width for the autowrap
decision: at a full row, `ZWJ 👍VS16` wraps the `👍` as a fresh 2-cell base and
the VS16 attaches to that new base — a soft wrap breaks the grapheme.

## Deviations from the design specification

Where the implementation is deliberately narrower than the prose spec:

1. **The "plain mark" clause of $\mathsf{cont\_t}$ is `GCB ∈ {Extend,
   SpacingMark} ∧ ¬DAKUTEN`**, not $w_1 = 0 \land s \notin \{\text{selectors}\}$.
   The prose form would also admit format controls ($w_1 = 0$,
   `GCB=Control`) as run continuations.
2. **The lone-mark fix is gated on the GCB class** (`Extend`, `SpacingMark`,
   `V`, `T`) and on the Hangul pair state, so a lone V or T jamo attaches only
   to a compatible syllable; the prose `fix` has no Hangul arm.
3. **`classify_cell` has a `keycap` arm** (`(followed_by_zwj ∨ keycap) ∧ w >
   raw`) that the prose classification does not state.

## Conformance

The Unicode data-file counts (≈9 000 sequences, 0 mismatches), the three
deliberate terminal overrides, and the analytical engine that checks them are
in [`CONFORMANCE.md`](CONFORMANCE.md).
