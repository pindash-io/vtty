# Emoji conformance taxonomy & analytical engine

`vtty_emoji` resolves display cell width through a single width function
`cell_widths: Σ* → ℕ*`. Its Unicode authority is UTS #51 as of **Unicode
17.0.0** (ICU4X 2.3.0 bakes the per-codepoint properties; `parley_emoji`
0.12.0 supplies the version-agnostic DFA). This document classifies the
authoritative data files, gives the width model a mathematical form, and
specifies the analytical engine (`examples/conformance.rs`) that validates
every sequence in one pass.

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
| | unqualified | `[1]` / `[2]` (CJK) / `[2,0,…]` (ZWJ) / `[1,0]` (keycap) | 243/243 |
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

These are deliberate, Ghostty-aligned terminal overrides, not defects; the
fixtures `vs16_keycap_base.in` etc. lock them.

## 2. Mathematical model

Let `w(s) = [w₁, …, wₙ] = cell_widths(s)`. The **collapse** predicate says a
sequence is ONE 2-cell grapheme:

```math
\mathsf{collapse}(s) \;\equiv\; w_1 = 2 \;\land\; \forall i > 1:\ w_i = 0
```

The width function factors as a **presentation resolution** followed by a
**cluster split**. A sequence is classified by its structure:

```math
\mathsf{member} = \mathsf{Emoji}\ [\mathsf{SkinTone}], \qquad
\mathsf{ZWJ}(s) = \mathsf{member}\ (\mathsf{ZWJ}\ \mathsf{member})^*
```

and the expected signatures are

| class | signature |
|---|---|
| $\mathsf{Basic},\ \mathsf{Flag},\ \mathsf{Modifier},\ \mathsf{Tag},\ \mathsf{ZWJ}$ | $\mathsf{collapse}(s)$ |
| $\mathsf{Keycap}$ | $w = [2,0,0]$ |
| $\mathsf{TextStyle}$ (`c VS15`) | $w = [1,0]$ |
| $\mathsf{EmojiStyle}$ (`c VS16`) | $w = [2,0]$ |

The three terminal overrides are predicates that INTERSECT the Unicode
catalogue and rewrite the signature:

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

The presentation selector rule ($\sigma$ is the presentation style):

```math
\sigma(\mathsf{VS16}) = \mathsf{Emoji},\qquad
\sigma(\mathsf{VS15}) = \mathsf{Text},\qquad
w_1 =
\begin{cases}
2 & \sigma = \mathsf{Emoji} \lor \mathsf{collapse}(s) \\
1 & \sigma = \mathsf{Text}
\end{cases}
```

and a skin-tone / ZWJ / VS / keycap-end / tag char is always a 0-width
continuation ($w_i = 0,\ i > 1$).

## 3. Analytical engine

`examples/conformance.rs` runs all four files in one pass. For each entry it
(1) parses the codepoint field (expanding `CP..CP` ranges), (2) classifies the
entry by file + sequence type / qualification, (3) applies `cell_widths`, and
(4) checks the class signature against the three overrides. It prints the
unified table and any mismatch.

```bash
cargo run --release -p vtty_emoji --example conformance -- \
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
