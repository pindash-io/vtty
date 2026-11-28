// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Emoji segmentation, the UTS #51 rules for which chars merge into one
//! terminal glyph.

use parley_emoji::{EmojiDFA, EmojiPresentationStyle, EmojiSegmentationCategory};

use crate::properties::{Gcb, Properties, cjk_fast_width, emoji_category};

/// A zero-width mark that modifies a base (UAX #29 GB9 + UTS #51): a variation
/// selector or ZWJ. Combining marks are not selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    /// VS16, emoji presentation.
    Vs16,
    /// VS15, text presentation.
    Vs15,
    /// ZWJ, joins the next element.
    Zwj,
    /// Not a selector: a combining mark (or any other char).
    None,
}

impl Selector {
    /// Classify a zero-width mark; [`Self::None`] for combining marks.
    #[inline]
    pub const fn from_char(c: char) -> Self {
        match c {
            '\u{FE0F}' => Self::Vs16,
            '\u{FE0E}' => Self::Vs15,
            '\u{200D}' => Self::Zwj,
            _ => Self::None,
        }
    }

    /// Whether this is [`Self::None`] (a combining mark, not a selector).
    #[inline]
    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    /// Whether this is a variation selector (VS15 or VS16).
    #[inline]
    pub const fn is_vs(self) -> bool {
        matches!(self, Self::Vs15 | Self::Vs16)
    }

    /// Whether this is a zero-width joiner (ZWJ).
    #[inline]
    pub const fn is_zwj(self) -> bool {
        matches!(self, Self::Zwj)
    }

    /// Whether the selector attaches to `base`; a stray selector is dropped.
    /// VS15/VS16 use [`is_vs_base`], ZWJ uses [`is_zwj_target`].
    #[inline]
    pub fn attaches_to(self, base: char) -> bool {
        if self.is_zwj() {
            return is_zwj_target(base);
        }
        if self.is_vs() {
            return is_vs_base(base);
        }
        true
    }

    /// The base's target width when this selector applies; `None` for ZWJ and
    /// for a VS on a non-variation base.
    #[inline]
    pub fn target_width(self, base: char) -> Option<usize> {
        if !self.is_vs() || !is_vs_target(base) {
            return None;
        }
        Some(match self {
            Self::Vs16 => 2,
            _ => 1,
        })
    }
}

/// An open emoji run: the lead plus its continuations, whose width is known
/// only once the run closes (a later VS16/ZWJ can change it).
pub(crate) struct Run {
    dfa: EmojiDFA,
    lead: char,
    lead_w1: usize,
    lead_ep: bool,
    lead_cat: EmojiSegmentationCategory,
    lead_gcb: Gcb,
    len: usize,
    prev_zwj: bool,
    ri_count: u8,
    keycap_end: bool,
}

impl Run {
    /// Start a new run with lead `c` (already classified as `cat`).
    pub(crate) fn start(
        c: char,
        cat: EmojiSegmentationCategory,
        lead_w1: usize,
        lead_ep: bool,
        lead_gcb: Gcb,
    ) -> Self {
        let mut dfa = EmojiDFA::new();
        dfa.step_record(cat);
        Self {
            dfa,
            lead: c,
            lead_w1,
            // Used only for the basic case (lead + combining marks); VS16/ZWJ
            // sequences are decided by the DFA.
            lead_ep,
            lead_cat: cat,
            lead_gcb,
            len: 1,
            prev_zwj: cat == EmojiSegmentationCategory::Zwj,
            ri_count: u8::from(cat == EmojiSegmentationCategory::RegionalIndicator),
            keycap_end: false,
        }
    }

    /// Record `c` (already classified as `cat`) as a continuation.
    pub(crate) fn push(&mut self, cat: EmojiSegmentationCategory) {
        self.dfa.step_record(cat);
        self.len += 1;
        self.prev_zwj = cat == EmojiSegmentationCategory::Zwj;
        self.ri_count = if cat == EmojiSegmentationCategory::RegionalIndicator {
            self.ri_count + 1
        } else {
            0
        };
        self.keycap_end |= cat == EmojiSegmentationCategory::KeycapEnd;
    }

    /// The number of chars in this run (lead + continuations).
    pub(crate) const fn len(&self) -> usize {
        self.len
    }
}

/// Whether a zero-width mark may attach to `base`: a ZWJ needs an emoji ZWJ
/// target, VS15/VS16 an emoji base, a combining mark any non-space base.
#[inline]
pub fn mark_attaches(mark: char, base: char) -> bool {
    Selector::from_char(mark).attaches_to(base)
}

/// Whether a char is a valid VS15/VS16 base: `Extended_Pictographic`, which
/// excludes keycap bases.
#[inline]
pub fn is_vs_base(c: char) -> bool {
    Properties::get(c).is_extended_pictographic()
}

/// Whether a VS15/VS16 changes the width of `c`: the `Emoji` property minus
/// keycap bases.
#[inline]
pub fn is_vs_target(c: char) -> bool {
    Properties::get(c).is_emoji() && emoji_category(c) != EmojiSegmentationCategory::KeycapBase
}

/// Whether a zero-width `mark` is a stray for `base`: the inverse of
/// [`mark_attaches`] for the ZWJ/VS family.
#[inline]
pub fn mark_is_stray(mark: char, base: char) -> bool {
    !mark_attaches(mark, base)
}

/// Whether a char is a keycap base (`#`, `*`, `0-9`): a buffered VS16/VS15
/// after it attaches, but a lone one is dropped.
#[inline]
pub fn is_keycap_base(c: char) -> bool {
    emoji_category(c) == EmojiSegmentationCategory::KeycapBase
}

/// Whether a char can be the target of a preceding ZWJ (a ZWJ-sequence
/// continuation).
#[inline]
pub fn is_zwj_target(c: char) -> bool {
    // ASCII and the CJK/kana/fullwidth blocks are never emoji targets: skip
    // the property lookup on the append-path hot loop.
    if c.is_ascii() || cjk_fast_width(c).is_some() {
        return false;
    }
    matches!(
        emoji_category(c),
        EmojiSegmentationCategory::Emoji
            | EmojiSegmentationCategory::EmojiPresentation
            | EmojiSegmentationCategory::EmojiModifierBase
    )
}

/// U+FF9E/U+FF9F: `Grapheme_Extend`, but terminals render them as 1 halfwidth
/// cell, so the DFA must not treat them as continuations.
#[inline]
pub const fn is_halfwidth_sound_mark(c: char) -> bool {
    matches!(c, '\u{FF9E}' | '\u{FF9F}')
}

/// A category that continues the current emoji run (zero cursor cells).
#[inline]
const fn is_continuation(cat: EmojiSegmentationCategory) -> bool {
    use EmojiSegmentationCategory as C;
    matches!(
        cat,
        C::Vs15 | C::Vs16 | C::Zwj | C::KeycapEnd | C::TagSpec | C::TagEnd | C::EmojiModifier
    )
}

/// Whether a category can start an emoji sequence (a real base glyph).
#[inline]
pub(crate) const fn is_emoji_starter(cat: EmojiSegmentationCategory) -> bool {
    use EmojiSegmentationCategory as C;
    matches!(
        cat,
        C::Emoji
            | C::EmojiPresentation
            | C::EmojiModifierBase
            | C::KeycapBase
            | C::TagBase
            | C::RegionalIndicator
    )
}

/// Whether `c` can start an emoji sequence, from its packed properties: the
/// hot form of `is_emoji_starter(category_from(p, c))`.
#[inline]
pub(crate) const fn is_emoji_starter_from(p: Properties, c: char) -> bool {
    p.is_regional_indicator()
        || p.is_modifier_base()
        || p.is_emoji_presentation()
        || (p.is_emoji() && !p.is_emoji_modifier())
        || matches!(c as u32, 0x30..=0x39 | 0x23 | 0x2A | 0x1F3F4)
}

/// Whether `c` (with packed `p`) is an emoji continuation: the direct form
/// of `is_continuation(category_from(p, c))`.
#[inline]
pub(crate) const fn is_continuation_from(p: Properties, c: char) -> bool {
    p.is_emoji_modifier()
        || matches!(
            c as u32,
            0x200D | 0x20E3 | 0xFE0E | 0xFE0F
                | 0xE0030..=0xE0039
                | 0xE0061..=0xE007A
                | 0xE007F
        )
}

/// The standalone-emoji lookahead, from packed properties (no category
/// classification): does `c2` continue a fresh emoji run led by `c`?
#[inline]
pub(crate) const fn fresh_emoji_continues_from(
    p: Properties,
    c: char,
    p2: Properties,
    c2: char,
    gcb2: Gcb,
) -> bool {
    if matches!(c as u32, 0x30..=0x39 | 0x23 | 0x2A) {
        // KeycapBase lead: only VS15/VS16/U+20E3 continue.
        matches!(c2 as u32, 0xFE0E | 0xFE0F | 0x20E3)
    } else if p.is_regional_indicator() {
        p2.is_regional_indicator()
    } else {
        is_continuation_from(p2, c2)
            || (matches!(gcb2, Gcb::Extend | Gcb::SpacingMark) && !is_halfwidth_sound_mark(c2))
    }
}

/// The terminal cell width of a run's leading char (1 or 2), from the
/// `parley_emoji` presentation, one column narrower for a bare keycap: a
/// keycap base is 2 cells only in the complete `base + VS16 + U+20E3`
/// sequence (the no-VS16 `1 U+20E3` and the incomplete `1 VS16` both stay
/// text-default 1: a VS16 after a keycap base is dropped because it is not
/// `Extended_Pictographic`).
#[inline]
fn run_width(dfa: &EmojiDFA, lead: char, lead_ep: bool, keycap_end: bool) -> usize {
    if emoji_category(lead) == EmojiSegmentationCategory::KeycapBase {
        return if dfa.has_vs() && keycap_end { 2 } else { 1 };
    }
    match dfa.presentation_style(lead_ep) {
        EmojiPresentationStyle::Emoji => 2,
        _ => 1,
    }
}

/// Whether `c` continues the open run `r`: the spec's `cont_t` predicate: the
/// parley DFA's transition from the run's accumulated state (keycap/RI leads
/// have no `→ ZWJ`, so a ZWJ after them is a stray) plus the deliberate
/// combining-mark exception (`w₁ = 0` non-selector attaches to any emoji
/// base). `next_is_vs15` is the one-char lookahead that implements UTS #51
/// §2.7.1: a VS15 on the next char breaks the ZWJ sequence at `c`.
#[inline]
pub(crate) fn run_continues(
    r: &Run,
    c: char,
    cat: EmojiSegmentationCategory,
    gcb: Gcb,
    next_is_vs15: bool,
) -> bool {
    if !is_emoji_starter(r.lead_cat) {
        return false;
    }
    match r.lead_cat {
        EmojiSegmentationCategory::KeycapBase => matches!(
            cat,
            EmojiSegmentationCategory::Vs15
                | EmojiSegmentationCategory::Vs16
                | EmojiSegmentationCategory::KeycapEnd
        ),
        EmojiSegmentationCategory::RegionalIndicator => {
            cat == EmojiSegmentationCategory::RegionalIndicator && r.ri_count == 1
        }
        _ => {
            // A VS15 on the next char breaks the sequence: `c` stays the last
            // family element and starts its own `c VS15` text presentation.
            if next_is_vs15 && r.prev_zwj && is_zwj_target(c) {
                return false;
            }
            is_continuation(cat)
                || (r.prev_zwj && is_zwj_target(c))
                || (cat == EmojiSegmentationCategory::RegionalIndicator && r.ri_count == 1)
                || (matches!(gcb, Gcb::Extend | Gcb::SpacingMark) && !is_halfwidth_sound_mark(c))
        }
    }
}

/// The run lead's final width, with the lone-mark fix: a lone 0-width mark
/// takes its own cell unless it is a selector.
#[inline]
pub(crate) fn finalize_lead(
    r: &Run,
    has_base: &mut bool,
    hangul: &mut u8,
    space_base: &mut bool,
    followed_by_mark: bool,
) -> usize {
    let lead_w = if r.len > 1 && is_emoji_starter(r.lead_cat) {
        run_width(&r.dfa, r.lead, r.lead_ep, r.keycap_end)
    } else {
        r.lead_w1
    };
    let gcb = r.lead_gcb;
    let w = if lead_w == 0 {
        let sel = Selector::from_char(r.lead);
        // Only a lone combining mark takes its own cell; selectors, controls and
        // format chars stay invisible.
        let attaches = match gcb {
            Gcb::V => *hangul & 1 != 0, // after L/LV/V
            Gcb::T => *hangul & 2 != 0, // after LV/V/LVT/T
            _ => *has_base,             // combining marks: any base
        };
        if !sel.is_none()
            || !matches!(gcb, Gcb::Extend | Gcb::SpacingMark | Gcb::V | Gcb::T)
            || (attaches && !(followed_by_mark && *space_base))
        {
            0
        } else {
            // A lone mark, or a mark after a space that another mark follows, needs
            // its own cell to be a visible base.
            1
        }
    } else {
        lead_w
    };
    if w > 0 {
        *has_base = true;
        *hangul = hangul_after(gcb);
        *space_base = r.lead == ' ';
    } else if matches!(gcb, Gcb::Control | Gcb::CR | Gcb::LF) {
        // A control/format char is invisible and breaks the grapheme chain:
        // a following mark has no visible base to attach to.
        *has_base = false;
        *hangul = 0;
        *space_base = false;
    } else if matches!(gcb, Gcb::V | Gcb::T) {
        // An attached V/T jamo extends the syllable (GB7/GB8).
        *hangul = hangul_after(gcb);
    }
    w
}

/// The Hangul continuation bits after a syllable start: bit 0 = V may follow,
/// bit 1 = T may follow (UAX #29 GB6/GB7/GB8).
#[inline]
const fn hangul_after(gcb: Gcb) -> u8 {
    match gcb {
        Gcb::L => 1,            // V may follow
        Gcb::LV | Gcb::V => 3,  // V or T may follow
        Gcb::LVT | Gcb::T => 2, // T may follow
        _ => 0,
    }
}
