# vtty_core

Zero-allocation, panic-free DEC VT500 terminal parser engine with UTF-8
decoding and `no_std` support.

This is the engine crate; prefer the [`vtty`](https://docs.rs/vtty) facade,
which re-exports this crate's public API. `vtty_core` exists so
[`vtty_codec`](https://docs.rs/vtty_codec) can implement `Encode`/`Decode`
against the engine types without depending on the facade.

## License

Licensed under either of [Apache-2.0](../LICENSE-APACHE) or
[MIT](../LICENSE-MIT) at your option.
