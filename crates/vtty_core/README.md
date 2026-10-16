# vtty_core

The irreducible core of `vtty`: the `Perform` callback contract, the owned
`Event`/`Recording` API, the terminal model types (`ColorId`, `SgrFlags`,
`Charset`, `TermFlags`, …), and the reference byte-cursor `State` sink.

This crate is split out so `vtty` (the parser facade) and `vtty_codec`
(serialization) can share it without the codec depending on the parser.
`vtty` re-exports its API.

## Features

- `reply` — DECRQSS/DECRQM/XTGETTCAP answer formatting (`Reply`).
- `termcap` — XTGETTCAP terminfo capability table (`Cap`/`CapValue`); implies `reply`.
- `status` — OSC 7501 Program Status Protocol (`ProgramStatus`).

## License

Licensed under either of [Apache-2.0](../LICENSE-APACHE) or
[MIT](../LICENSE-MIT) at your option.
