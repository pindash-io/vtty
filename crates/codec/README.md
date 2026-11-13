# vtty_codec

<!-- cargo-rdme start -->

**Compact**, **`no_std`**, **panic-free** binary serialization.

[`Encode`](https://docs.rs/vtty_codec/latest/vtty_codec/traits/trait.Encode.html) turns a value into a **deterministic** byte stream and
[`Decode`](https://docs.rs/vtty_codec/latest/vtty_codec/traits/trait.Decode.html) turns that stream back into a value.

## Wire format

- Integers — fixed-width little-endian.
- Floats — IEEE-754 bit patterns, little-endian (`f32`↔`u32`, `f64`↔`u64`).
- `bool` — one byte, `0` or `1`.
- `char` — its `u32` code point, little-endian.
- `()` — zero bytes.
- Slices, strings and collections — an LEB128 item/byte count followed by
  the items.
- Enums — one discriminant byte followed by the variant payload.

## Safety

Encoding never allocates or panics. Decoding allocates only in proportion
to the bytes actually read, so a bogus length prefix cannot force a large
allocation — it fails with [`DecodeError::UnexpectedEof`](https://docs.rs/vtty_codec/latest/vtty_codec/error/enum.DecodeError.html#variant.UnexpectedEof) instead.

<!-- cargo-rdme end -->
