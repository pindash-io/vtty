// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Chunking-invariance for the streaming codec: a value must decode to the
//! same result no matter how its encoding is split across `Decoder::advance`
//! calls. This is the observable contract of `Decoder`'s buffered, re-decode
//! state machine (varint lengths + length-prefixed payloads arrive
//! incrementally).

use vtty_codec::{Decode, Decoder, Encode, decode_from_slice, encode_to_vec};
use vtty_core::{Event, Recording};

fn assert_chunking<T>(value: &T)
where
    T: Decode + Encode + PartialEq + core::fmt::Debug,
{
    let bytes = encode_to_vec(value).expect("encode");

    // Whole-slice reference decode.
    assert_eq!(
        &decode_from_slice::<T>(&bytes).expect("decode"),
        value,
        "whole-slice decode",
    );

    // Byte-at-a-time feeding.
    {
        let mut dec = Decoder::<T>::new();
        let mut got = None;
        for &b in &bytes {
            match dec.advance(core::slice::from_ref(&b)) {
                Ok(Some(v)) => {
                    got = Some(v);
                    break;
                }
                Ok(None) => {}
                Err(e) => panic!("bytewise error for {value:?}: {e:?}"),
            }
        }
        assert_eq!(got.as_ref(), Some(value), "bytewise {value:?}");
    }

    // Every two-way split: exactly one side completes the value.
    for split in 0..=bytes.len() {
        let mut dec = Decoder::<T>::new();
        let first = dec.advance(&bytes[..split]).expect("first chunk");
        let second = dec.advance(&bytes[split..]).expect("second chunk");
        let got = match (first, second) {
            (Some(v), None) | (None, Some(v)) => v,
            (a, b) => panic!("split {split} of {value:?}: {a:?} / {b:?}"),
        };
        assert_eq!(&got, value, "split {split} of {value:?}");
    }
}

#[test]
fn primitives() {
    assert_chunking(&true);
    assert_chunking(&false);
    assert_chunking(&0_u8);
    assert_chunking(&255_u8);
    assert_chunking(&300_u16);
    assert_chunking(&65535_u16);
    assert_chunking(&1_000_000_u32);
    assert_chunking(&u64::MAX);
}

#[test]
fn length_prefixed() {
    assert_chunking(&String::new());
    assert_chunking(&"hello".to_string());
    assert_chunking(&"中🚀".to_string());
    assert_chunking(&Vec::<u8>::new());
    assert_chunking(&vec![1_u8, 2, 3]);
    // Long enough to exercise the "length prefix promises more bytes" path
    // (the decoder must wait rather than error).
    assert_chunking(&(0_u8..200_u8).collect::<Vec<u8>>());
    assert_chunking(&vec!["a".to_string(), "bb".to_string(), "中".to_string()]);
}

#[test]
fn events() {
    assert_chunking(&Event::Print('中'));
    assert_chunking(&Event::Execute(10));
    assert_chunking(&Event::DcsUnhook);
    assert_chunking(&Event::PrintBytes(b"abc".to_vec()));

    let mut rec = Recording::new();
    rec.csi(&[1, 31], 0, b"", b'm');
    rec.osc(&[b"0", b"title"], true);
    assert_chunking(&rec);
}
