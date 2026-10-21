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

<!-- cargo-rdme end -->
