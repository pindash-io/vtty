# vtty_types

Terminal model types and the `Perform` callback trait for `vtty`.

This is the data plane: the `Perform` trait, the owned `Event`/`Recording`
API, and the terminal model types (`ColorId`, `SgrFlags`, `Charset`,
`ProgramStatus`, …). The `vtty` parser crate depends on it and re-exports its
API; `vtty_codec` depends on it so serialization does not have to pull in the
parser.

## License

Licensed under either of [Apache-2.0](../LICENSE-APACHE) or
[MIT](../LICENSE-MIT) at your option.
