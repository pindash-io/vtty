// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Build-time baked emoji+width property table generator.
//!
//! Reads the Unicode properties from ICU4X, packs the per-codepoint values
//! with the `packtab` crate, and writes the generated Rust to `OUT_DIR`
//! (included by `src/baked.rs`). `unicode-width` is NOT used here — it is the
//! benchmark/reference table only.
//!
//! Env knobs:
//! - `VTTY_EMOJI_COMPRESSION` (default 5.0): 0 = flat direct index,
//!   >= 10 = smallest table.
//! - `VTTY_EMOJI_SAFE` (default unset): set to emit bounds-checked array
//!   access instead of `get_unchecked`.
#![expect(
    clippy::cast_possible_truncation,
    reason = "Terminal cell/pixel math truncates widths and coordinates to fixed-size types where the range is known-safe"
)]

use std::env;
use std::fs;
use std::path::PathBuf;

use icu_properties::props::{
    DefaultIgnorableCodePoint, EastAsianWidth, Emoji, EmojiModifier, EmojiModifierBase,
    EmojiPresentation, ExtendedPictographic, GeneralCategory, GraphemeClusterBreak,
    RegionalIndicator,
};
use icu_properties::{CodePointMapData, CodePointSetData};

const N: usize = 0x110000;

/// unicode-width's curated zero-width format/spacing marks that the raw
/// Unicode properties (GC / `Default_Ignorable` / `Grapheme_Extend`) don't
/// express.
fn is_curated_zero_width(cp: u32) -> bool {
    matches!(
        cp,
        0x0605
            | 0x070F
            | 0x0890
            | 0x0891
            | 0x08E2
            | 0x0D4E
            | 0xA8FA
            | 0x111C2
            | 0x111C3
            | 0x113D1
            | 0x1193F
            | 0x11941
            | 0x11A84
            | 0x11A85
            | 0x11A86
            | 0x11A87
            | 0x11A88
            | 0x11A89
            | 0x11D46
            | 0x11F02
    )
}

fn main() {
    let compression = env::var("VTTY_EMOJI_COMPRESSION")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(5.0);
    let unsafe_access = env::var_os("VTTY_EMOJI_SAFE").is_none();

    // ── Compute the per-codepoint value ───────────────────────────────
    let emoji = CodePointSetData::new::<Emoji>();
    let presentation = CodePointSetData::new::<EmojiPresentation>();
    let modifier = CodePointSetData::new::<EmojiModifier>();
    let modifier_base = CodePointSetData::new::<EmojiModifierBase>();
    let regional_indicator = CodePointSetData::new::<RegionalIndicator>();
    let extended_pictographic = CodePointSetData::new::<ExtendedPictographic>();
    let default_ignorable = CodePointSetData::new::<DefaultIgnorableCodePoint>();
    let gc = CodePointMapData::<GeneralCategory>::new();
    let eaw = CodePointMapData::<EastAsianWidth>::new();
    let gcb = CodePointMapData::<GraphemeClusterBreak>::new();

    // Bit 0 = Emoji, bit 1 = Emoji_Presentation, bit 2 = Emoji_Modifier,
    // bit 3 = Emoji_Modifier_Base, bit 4 = Regional_Indicator,
    // bit 7 = Extended_Pictographic, bits 5-6 = display width (0/1/2,
    // derived from ICU4X). Mirrors parley_data's
    // `Properties` emoji flags.
    let mut data = vec![0_i64; N];
    for (cp, slot) in data.iter_mut().enumerate() {
        if (0xD800..=0xDFFF).contains(&(cp as u32)) {
            continue; // surrogates are not scalar values
        }
        let c = char::from_u32(cp as u32).unwrap();
        let mut b = 0_i64;
        if emoji.contains(c) {
            b |= 1 << 0;
        }
        if presentation.contains(c) {
            b |= 1 << 1;
        }
        if modifier.contains(c) {
            b |= 1 << 2;
        }
        if modifier_base.contains(c) {
            b |= 1 << 3;
        }
        if regional_indicator.contains(c) {
            b |= 1 << 4;
        }
        if extended_pictographic.contains(c) {
            b |= 1 << 7;
        }
        let w = match cp as u32 {
            // unicode-width's documented special cases (reproduced here so the
            // baked table matches without linking unicode-width).
            0x115F | 0x17A4 => 2,
            0x2D7F => 1,
            0x1160..=0x11FF | 0xD7B0..=0xD7C6 | 0xD7CB..=0xD7FB => 0,
            _ => match gc.get(c) {
                GeneralCategory::Control | GeneralCategory::Surrogate => 0,
                _ if default_ignorable.contains(c)
                    || gcb.get(c) == GraphemeClusterBreak::Extend
                    || is_curated_zero_width(cp as u32) =>
                {
                    0
                }
                _ => match eaw.get(c) {
                    EastAsianWidth::Wide | EastAsianWidth::Fullwidth => 2,
                    _ => 1,
                },
            },
        };
        b |= (w as i64) << 5;
        *slot = b;
    }

    // ── Grapheme cluster break property (UAX #29 GB rules) ─────────────
    // A compact 4-bit symbol per codepoint, baked as a second table for
    // vtty_grapheme. The emoji-specific GCB values (EBase, EBaseGAZ,
    // EModifier, GlueAfterZwj) fold into `Other` — GB11 uses
    // `Extended_Pictographic` directly, and emoji sequences are handled by
    // the parley DFA.
    let mut gcb_data = vec![0_i64; N];
    for (cp, slot) in gcb_data.iter_mut().enumerate() {
        if (0xD800..=0xDFFF).contains(&(cp as u32)) {
            continue; // surrogates are not scalar values
        }
        let c = char::from_u32(cp as u32).unwrap();
        // Fold `Extended_Pictographic` into the table as its own symbol (GB11
        // uses it directly; it is otherwise `Other`). Regional indicators are
        // NOT ExtPict, so the RI pairing rules are unaffected.
        *slot = if extended_pictographic.contains(c) {
            14
        } else {
            match gcb.get(c) {
                GraphemeClusterBreak::Control => 1,
                GraphemeClusterBreak::CR => 2,
                GraphemeClusterBreak::LF => 3,
                GraphemeClusterBreak::Extend => 4,
                GraphemeClusterBreak::ZWJ => 5,
                GraphemeClusterBreak::RegionalIndicator => 6,
                GraphemeClusterBreak::Prepend => 7,
                GraphemeClusterBreak::SpacingMark => 8,
                GraphemeClusterBreak::L => 9,
                GraphemeClusterBreak::V => 10,
                GraphemeClusterBreak::T => 11,
                GraphemeClusterBreak::LV => 12,
                GraphemeClusterBreak::LVT => 13,
                _ => 0,
            }
        };
    }

    // ── Pack + generate (parley_data's pipeline) ──────────────────────
    let (info, best) = packtab::pack_table(&data, Some(0), compression);
    let code = packtab::generate(
        &info,
        best,
        "emoji_packtab",
        packtab::codegen::Language::Rust { unsafe_access },
    );

    let (gcb_info, gcb_best) = packtab::pack_table(&gcb_data, Some(0), compression);
    let gcb_code = packtab::generate(
        &gcb_info,
        gcb_best,
        "emoji_gcb",
        packtab::codegen::Language::Rust { unsafe_access },
    );

    let wrappers = format!(
        "// Baked composite per-codepoint properties (GENERATED by build.rs).\n\
         // Bits 0-4: 1 = Emoji, 2 = Emoji_Presentation, 4 = Emoji_Modifier,\n\
         // 8 = Emoji_Modifier_Base, 16 = Regional_Indicator;\n\
         // bits 5-6: display width (0/1/2, from ICU4X).\n\
         #[rustfmt::skip]\n\
         {code}\n\n\
         #[allow(missing_docs, reason = \"packtab generated code\")]\n\
         #[inline]\n\
         pub(crate) fn composite_get(cp: u32) -> u8 {{\n\
         \x20   emoji_packtab_get(cp as usize)\n\
         }}\n"
    );

    let gcb_wrappers = format!(
        "// Baked grapheme cluster break property (GENERATED by build.rs).\n\
         // 0=Other 1=Control 2=CR 3=LF 4=Extend 5=ZWJ 6=RI 7=Prepend\n\
         // 8=SpacingMark 9=L 10=V 11=T 12=LV 13=LVT.\n\
         #[rustfmt::skip]\n\
         {gcb_code}\n\n\
         #[allow(missing_docs, reason = \"packtab generated code\")]\n\
         #[inline]\n\
         pub(crate) fn gcb_get(cp: u32) -> u8 {{\n\
         \x20   emoji_gcb_get(cp as usize)\n\
         }}\n"
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("emoji_packtab.rs"), wrappers).unwrap();
    fs::write(out_dir.join("emoji_gcb.rs"), gcb_wrappers).unwrap();

    println!("cargo:rerun-if-env-changed=VTTY_EMOJI_COMPRESSION");
    println!("cargo:rerun-if-env-changed=VTTY_EMOJI_SAFE");
}
