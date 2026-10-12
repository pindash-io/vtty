// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! ICU4X runtime lookups (feature `icu`) — the width derivation used as the
//! runtime alternative to the baked table. The emoji property source for the
//! segmentation DFA lives in [`crate::Properties::from_icu`].

use icu_properties::CodePointMapData;
use icu_properties::CodePointMapDataBorrowed;
use icu_properties::CodePointSetData;
use icu_properties::CodePointSetDataBorrowed;
use icu_properties::props::{
    DefaultIgnorableCodePoint, EastAsianWidth, GeneralCategory, GraphemeClusterBreak,
};

/// ICU4X-backed display width — an alternative derivation to `unicode-width`.
///
/// The cell width is derived from two Unicode properties:
/// - `GeneralCategory`: control (`Cc`)/surrogate (`Cs`)/unassigned (`Cn`) have
///   no width (`None`); combining (`Mn`/`Me`), format (`Cf`), and line/paragraph
///   separators are zero-width (`Some(0)`).
/// - `EastAsianWidth`: `Wide`/`Fullwidth` → `Some(2)`, everything else
///   (including `Ambiguous`) → `Some(1)` — matching `unicode-width`'s
///   ambiguous = 1 default.
///
/// [`crate::cell_width`] uses the baked table; this exists as the ICU4X
/// runtime alternative (and the source for baking the width into the
/// `PackTab` table).
pub fn width_icu(cp: char) -> Option<usize> {
    thread_local! {
        static GC: CodePointMapDataBorrowed<'static, GeneralCategory> =
            const { CodePointMapData::new() };
        static EAW: CodePointMapDataBorrowed<'static, EastAsianWidth> =
            const { CodePointMapData::new() };
        static GCB: CodePointMapDataBorrowed<'static, GraphemeClusterBreak> =
            const { CodePointMapData::new() };
        static DI: CodePointSetDataBorrowed<'static> =
            const { CodePointSetData::new::<DefaultIgnorableCodePoint>() };
    }
    GC.with(|gc| match cp as u32 {
        // unicode-width's documented special cases (see the generator).
        0x115F | 0x17A4 => Some(2),
        0x2D7F => Some(1),
        0x1160..=0x11FF | 0xD7B0..=0xD7C6 | 0xD7CB..=0xD7FB => Some(0),
        _ => match gc.get(cp) {
            // Control and surrogate have no width; default-ignorable and
            // grapheme-extend chars are zero-width (matching unicode-width's
            // zero-width rule). Everything else falls through to East Asian
            // Width.
            GeneralCategory::Control | GeneralCategory::Surrogate => None,
            _ if DI.with(|di| di.contains(cp))
                || GCB.with(|g| g.get(cp) == GraphemeClusterBreak::Extend)
                || is_curated_zero_width(cp as u32) =>
            {
                Some(0)
            }
            _ => EAW.with(|eaw| match eaw.get(cp) {
                EastAsianWidth::Wide | EastAsianWidth::Fullwidth => Some(2),
                _ => Some(1),
            }),
        },
    })
}

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
