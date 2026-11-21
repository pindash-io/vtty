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
#![expect(
    clippy::cast_possible_truncation,
    reason = "Terminal cell/pixel math truncates widths and coordinates to fixed-size types where the range is known-safe"
)]

use std::env;
use std::fs;
use std::path::PathBuf;

use icu_properties::props::{
    DefaultIgnorableCodePoint, EastAsianWidth, Emoji, EmojiModifier,
    EmojiModifierBase, EmojiPresentation, ExtendedPictographic,
    GeneralCategory, GraphemeClusterBreak, RegionalIndicator,
};
use icu_properties::{CodePointMapData, CodePointSetData};

const N: usize = 0x110000;

/// unicode-width's curated zero-width format/spacing marks that the raw
/// Unicode properties (GC / `Default_Ignorable` / `Grapheme_Extend`) don't
/// express.
const fn is_curated_zero_width(cp: u32) -> bool {
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

    // Compute the per-codepoint value
    let emoji = CodePointSetData::new::<Emoji>();
    let presentation = CodePointSetData::new::<EmojiPresentation>();
    let modifier = CodePointSetData::new::<EmojiModifier>();
    let modifier_base = CodePointSetData::new::<EmojiModifierBase>();
    let regional_indicator = CodePointSetData::new::<RegionalIndicator>();
    let extended_pictographic =
        CodePointSetData::new::<ExtendedPictographic>();
    let default_ignorable =
        CodePointSetData::new::<DefaultIgnorableCodePoint>();
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
            0xFF9E | 0xFF9F => 1,
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
        if eaw.get(c) == EastAsianWidth::Ambiguous {
            b |= 1 << 8;
        }
        // GCB symbol (0-14), packed into bits 9-12 — one table instead of
        // two. The emoji-specific GCB values (EBase, EBaseGAZ, EModifier,
        // GlueAfterZwj) fold into `Other`; GB11 uses `Extended_Pictographic`
        // directly, and emoji sequences are handled by the parley DFA.
        let gcb_sym = if extended_pictographic.contains(c) {
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
        b |= (gcb_sym as i64) << 9;
        *slot = b;
    }

    // Pack + generate (parley_data's pipeline)
    let (info, best) = packtab::pack_table(&data, Some(0), compression);
    let code = packtab::generate(
        &info,
        best,
        "emoji_packtab",
        packtab::codegen::Language::Rust { unsafe_access: false },
    );

    let wrappers = format!(
        "// Baked composite per-codepoint properties (GENERATED by build.rs).\n\
         // Bits 0-4: 1 = Emoji, 2 = Emoji_Presentation, 4 = Emoji_Modifier,\n\
         // 8 = Emoji_Modifier_Base, 16 = Regional_Indicator;\n\
         // bits 5-6: display width (0/1/2, from ICU4X); bit 7: ExtPict;\n\
         // bit 8: EAW Ambiguous; bits 9-12: grapheme cluster break symbol.\n\
         #[rustfmt::skip]\n\
         {code}\n\n\
         #[allow(missing_docs, reason = \"packtab generated code\")]\n\
         #[inline]\n\
         pub(crate) fn composite_get(cp: u32) -> u16 {{\n\
         \x20   emoji_packtab_get(cp as usize)\n\
         }}\n"
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("emoji_packtab.rs"), wrappers).unwrap();

    // A second, 1-bit table for the emoji-presentation property alone. The
    // mixed-width print fast path consults it (instead of the 5-level
    // composite table) for the ~1400 EP codepoints; the rest of Unicode is
    // 0, so it packs much smaller and looks up faster.
    let mut data_ep = vec![0_i64; N];
    for (cp, slot) in data_ep.iter_mut().enumerate() {
        if (0xD800..=0xDFFF).contains(&(cp as u32)) {
            continue;
        }
        if presentation.contains(char::from_u32(cp as u32).unwrap()) {
            *slot = 1;
        }
    }
    let (ep_info, ep_best) =
        packtab::pack_table(&data_ep, Some(0), compression);
    let ep_code = packtab::generate(
        &ep_info,
        ep_best,
        "ep_packtab",
        packtab::codegen::Language::Rust { unsafe_access: false },
    );
    let ep_wrappers = format!(
        "// Baked emoji-presentation bitset (GENERATED by build.rs).\n\
         #[rustfmt::skip]\n\
         {ep_code}\n\n\
         #[allow(missing_docs, reason = \"packtab generated code\")]\n\
         #[inline]\n\
         pub(crate) fn ep_get(cp: u32) -> u8 {{\n\
         \x20   ep_packtab_get(cp as usize)\n\
         }}\n"
    );
    fs::write(out_dir.join("ep_packtab.rs"), ep_wrappers).unwrap();

    // A CodePointTrie-style two-level table (ICU4X's container shape) as an
    // alternative to the packtab level-walk: `index[cp >> 6] + (cp & 0x3f)`
    // addresses the deduplicated 64-codepoint block array in two reads. Split
    // BMP/supplementary so each index stays within `u16`. Gated on the `trie`
    // feature (experimental; the packtab path stays the default).
    if env::var("CARGO_FEATURE_TRIE").is_ok() {
        let trie = gen_trie_tables("composite", &data, "ep", &data_ep);
        fs::write(out_dir.join("trie.rs"), trie).unwrap();
    }

    println!("cargo:rerun-if-env-changed=VTTY_EMOJI_COMPRESSION");
}

/// Deduplicate 64-codepoint blocks into a `(index, data)` pair in the
/// `CodePointTrie` shape: `data[index[cp >> 6] + (cp & 0x3f)]`.
fn dedup_trie(data: &[i64]) -> (Vec<u16>, Vec<u16>) {
    const BLOCK: usize = 64;
    let block_count = data.len() / BLOCK;
    let mut map = std::collections::HashMap::<[u16; BLOCK], u16>::new();
    let mut trie_data: Vec<u16> = Vec::new();
    let mut trie_index: Vec<u16> = Vec::with_capacity(block_count);
    for b in 0..block_count {
        let mut block = [0_u16; BLOCK];
        for (i, slot) in block.iter_mut().enumerate() {
            *slot = data[b * BLOCK + i] as u16;
        }
        let off = *map.entry(block).or_insert_with(|| {
            let off = trie_data.len();
            assert!(off < u16::MAX as usize, "trie data exceeds u16 offsets");
            trie_data.extend_from_slice(&block);
            off as u16
        });
        trie_index.push(off);
    }
    (trie_index, trie_data)
}

/// Generate the two-level lookups: `composite_get_trie` (u16 values) and
/// `ep_get_trie` (u8 values), each BMP/supplementary split so the `u16` index
/// stays within range and the BMP hot path keeps a small, L1-resident index.
fn gen_trie_tables(
    composite_name: &str,
    composite: &[i64],
    ep_name: &str,
    ep: &[i64],
) -> String {
    const BMP_BLOCKS: usize = 0x10000 / 64;
    let (cidx, cdata) = dedup_trie(composite);
    let (eidx, edata) = dedup_trie(ep);
    eprintln!(
        "trie: composite index={} data={} ({} KiB), ep index={} data={} ({} KiB)",
        cidx.len(),
        cdata.len(),
        cdata.len() * 2 / 1024,
        eidx.len(),
        edata.len(),
        edata.len() / 1024,
    );
    let bmp_cidx: Vec<u16> = cidx[..BMP_BLOCKS].to_vec();
    let sup_cidx: Vec<u16> = cidx[BMP_BLOCKS..].to_vec();
    let bmp_eidx: Vec<u16> = eidx[..BMP_BLOCKS].to_vec();
    let sup_eidx: Vec<u16> = eidx[BMP_BLOCKS..].to_vec();
    let ep_data_u8: Vec<u8> = edata.iter().map(|&v| v as u8).collect();
    format!(
        "// CodePointTrie-style two-level tables (GENERATED by build.rs).\n\
         #[rustfmt::skip]\n\
         pub(crate) static {cn}_BMP_INDEX: [u16; {bmp}] = {bmp_idx:?};\n\
         #[rustfmt::skip]\n\
         pub(crate) static {cn}_SUP_INDEX: [u16; {sup}] = {sup_idx:?};\n\
         #[rustfmt::skip]\n\
         pub(crate) static {cn}_DATA: [u16; {cdl}] = {cd:?};\n\
         #[rustfmt::skip]\n\
         pub(crate) static {en}_BMP_INDEX: [u16; {bmp}] = {ebmp_idx:?};\n\
         #[rustfmt::skip]\n\
         pub(crate) static {en}_SUP_INDEX: [u16; {sup}] = {esup_idx:?};\n\
         #[rustfmt::skip]\n\
         pub(crate) static {en}_DATA: [u8; {edl}] = {ed:?};\n\
         #[inline]\n\
         pub(crate) fn {cn}_get_trie(cp: u32) -> u16 {{\n\
         \x20   let block = (cp >> 6) as usize;\n\
         \x20   let idx = if block < {bmp} {{\n\
         \x20       {cn}_BMP_INDEX[block] as usize\n\
         \x20   }} else {{\n\
         \x20       {cn}_SUP_INDEX[block - {bmp}] as usize\n\
         \x20   }};\n\
         \x20   {cn}_DATA[idx + ((cp & 63) as usize)]\n\
         }}\n\
         #[inline]\n\
         pub(crate) fn {en}_get_trie(cp: u32) -> u8 {{\n\
         \x20   let block = (cp >> 6) as usize;\n\
         \x20   let idx = if block < {bmp} {{\n\
         \x20       {en}_BMP_INDEX[block] as usize\n\
         \x20   }} else {{\n\
         \x20       {en}_SUP_INDEX[block - {bmp}] as usize\n\
         \x20   }};\n\
         \x20   {en}_DATA[idx + ((cp & 63) as usize)]\n\
         }}\n",
        cn = composite_name,
        en = ep_name,
        bmp = BMP_BLOCKS,
        sup = cidx.len() - BMP_BLOCKS,
        bmp_idx = bmp_cidx,
        sup_idx = sup_cidx,
        cdl = cdata.len(),
        cd = cdata,
        ebmp_idx = bmp_eidx,
        esup_idx = sup_eidx,
        edl = ep_data_u8.len(),
        ed = ep_data_u8,
    )
}
