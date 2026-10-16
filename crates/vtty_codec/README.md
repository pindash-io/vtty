# vtty_codec

Compact, `no_std`, panic-free binary serialization.

A generic `Encode`/`Decode` codec (fixed-width little-endian primitives,
LEB128 length prefixes, tag + payload enums), plus streaming (`Decoder`) and
iterator (`decode_iter`) decoders. `vtty_codec` also provides codec
implementations for the `vtty` data types (events, replies, sequence
primitives).

## License

Licensed under either of [Apache-2.0](../LICENSE-APACHE) or
[MIT](../LICENSE-MIT) at your option.
