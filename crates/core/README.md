# vtty_core

<!-- cargo-rdme start -->

The irreducible core of `vtty`: the [`Perform`](https://docs.rs/vtty_core/latest/vtty_core/perform/trait.Perform.html) callback contract, the
owned [`Event`](https://docs.rs/vtty_core/latest/vtty_core/event/enum.Event.html)/[`Recording`](https://docs.rs/vtty_core/latest/vtty_core/event/struct.Recording.html) API, the terminal model types (`ColorId`,
`SgrFlags`, `Charset`, `TermFlags`, …), and the reference byte-cursor
[`State`](https://docs.rs/vtty_core/latest/vtty_core/state/struct.State.html) sink.

This crate is split out so the `vtty` parser facade and `vtty_codec` can
share it without the codec depending on the parser. `vtty` re-exports its
API.

## Features

- `reply` — DECRQSS/DECRQM/XTGETTCAP answer formatting (`Reply`).
- `termcap` — XTGETTCAP terminfo capability table (`Cap`/`CapValue`);
  implies `reply`.

## Stability

`#[non_exhaustive]` marks enums that mirror a still-growing external
vocabulary, so a future variant cannot break a downstream `match` in
silence. Closed sets such as [`ColorSpec`](https://docs.rs/vtty_core/latest/vtty_core/style/enum.ColorSpec.html) and [`SgrEffect`](https://docs.rs/vtty_core/latest/vtty_core/style/enum.SgrEffect.html) stay
exhaustive. Types with public fields stay constructible by struct
literal, so they are never `#[non_exhaustive]`.

<!-- cargo-rdme end -->
