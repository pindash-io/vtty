// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Emoji segmentation — the UTS #51 rules for which chars merge into one
//! terminal glyph, plus the `Run` state machine the width model drives.
//!
//! The public predicates ([`mark_attaches`], [`is_vs_base`], [`is_zwj_target`],
//! …) answer single questions about a mark and a base; a private `Run` state
//! machine drives the [`crate::width`] model.

use parley_emoji::{EmojiDFA, EmojiPresentationStyle, EmojiSegmentationCategory};

use crate::properties::{Properties, cell_width, emoji_category};

/// Whether a zero-width mark may attach to `base` (UAX #29 GB9 + UTS #51):
/// A variation selector or ZWJ — the zero-width marks that MODIFY a base's
/// emoji presentation (VS15/VS16) or join a ZWJ sequence. Combining marks
/// are NOT selectors: they accept any non-space base instead of rejecting
/// non-emoji bases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    /// VS16 (U+FE0F): forces emoji presentation (width 2).
    Vs16,
    /// VS15 (U+FE0E): forces text presentation (width 1).
    Vs15,
    /// ZWJ (U+200D): joins the next emoji element (no width change).
    Zwj,
    /// Not a selector: a combining mark (or any other char).
    None,
}

impl Selector {
    /// Classify a zero-width mark; [`Self::None`] for combining marks.
    #[inline]
    pub fn from_char(c: char) -> Self {
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

    /// Whether the selector attaches to `base`: the base's category must be
    /// a valid target, otherwise the selector is a STRAY and is dropped.
    /// VS15/VS16 use [`is_vs_base`] (`ExtPict` — the ATTACH predicate); ZWJ
    /// uses [`is_zwj_target`].
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

    /// The base's TARGET cell width when this selector applies, or `None`
    /// for ZWJ (a joiner never changes the width) and for a VS whose base
    /// is NOT a variation base.
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

/// Whether a zero-width mark may attach to `base` (UAX #29 GB9 + UTS #51):
/// a ZWJ needs an emoji ZWJ target, VS15/VS16 need an emoji base, and a
/// combining mark accepts any non-space base.
#[inline]
pub fn mark_attaches(mark: char, base: char) -> bool {
    let s = Selector::from_char(mark);
    if s.is_none() {
        return true;
    }
    s.attaches_to(base)
}

/// Whether a char is a valid VS15/VS16 ATTACH base — the
/// `Extended_Pictographic` binary property (includes the reserved emoji
/// ranges). Keycap bases (`0-9`, `#`, `*`) carry `Emoji` but are NOT `ExtPict`.
#[inline]
pub fn is_vs_base(c: char) -> bool {
    Properties::get(c).is_extended_pictographic()
}

/// Whether a VS15/VS16 CHANGES the width of `c` — the `Emoji` property minus
/// keycap bases.
#[inline]
pub fn is_vs_target(c: char) -> bool {
    Properties::get(c).is_emoji() && emoji_category(c) != EmojiSegmentationCategory::KeycapBase
}

/// Whether a zero-width `mark` is a STRAY for `base` — the inverse of
/// [`mark_attaches`] for the ZWJ/VS family.
#[inline]
pub fn mark_is_stray(mark: char, base: char) -> bool {
    let s = Selector::from_char(mark);
    if s.is_none() {
        return false;
    }
    !s.attaches_to(base)
}

/// Whether a char is a KEYCAP BASE (`#`, `*`, `0-9`). A buffered VS16/VS15
/// after it ATTACHES (the DFA groups it into the keycap sequence — the base
/// widens to 2 cells only in the COMPLETE `base VS16 U+20E3`); a DIRECT lone
/// selector is still dropped (Ghostty's `ExtPict` rule).
#[inline]
pub fn is_keycap_base(c: char) -> bool {
    emoji_category(c) == EmojiSegmentationCategory::KeycapBase
}

/// Whether a char can be the TARGET of a preceding ZWJ (a ZWJ-sequence
/// continuation).
#[inline]
pub fn is_zwj_target(c: char) -> bool {
    matches!(
        emoji_category(c),
        EmojiSegmentationCategory::Emoji
            | EmojiSegmentationCategory::EmojiPresentation
            | EmojiSegmentationCategory::EmojiModifierBase
    )
}

/// A category that continues the current emoji run (zero cursor cells).
#[inline]
fn is_continuation(cat: EmojiSegmentationCategory) -> bool {
    use EmojiSegmentationCategory as C;
    matches!(
        cat,
        C::Vs15 | C::Vs16 | C::Zwj | C::KeycapEnd | C::TagSpec | C::TagEnd | C::EmojiModifier
    )
}

/// Whether a category can START an emoji sequence (a real base glyph).
#[inline]
fn is_emoji_starter(cat: EmojiSegmentationCategory) -> bool {
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

/// The terminal cell width of a run's leading char (1 or 2), from the
/// `parley_emoji` presentation — one column narrower for a bare keycap: a
/// keycap base is 2 cells only in the COMPLETE `base + VS16 + U+20E3`
/// sequence (the no-VS16 `1 U+20E3` and the incomplete `1 VS16` both stay
/// text-default 1 — Ghostty drops a VS16 after a keycap base because it is
/// not `Extended_Pictographic`).
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

/// An open emoji run: the lead char plus its continuations, whose final cell
/// width is only known once the run closes (a VS16/ZWJ that arrives later can
/// widen or extend the lead).
pub(crate) struct Run {
    dfa: EmojiDFA,
    lead: char,
    lead_w1: usize,
    lead_ep: bool,
    lead_cat: EmojiSegmentationCategory,
    len: usize,
    prev_zwj: bool,
    ri_count: u8,
    keycap_end: bool,
}

impl Run {
    /// Start a new run with lead `c` (already classified as `cat`).
    pub(crate) fn start(c: char, cat: EmojiSegmentationCategory) -> Self {
        let mut dfa = EmojiDFA::new();
        dfa.step_record(cat);
        Self {
            dfa,
            lead: c,
            lead_w1: cell_width(c).unwrap_or(0),
            // `presentation_style` uses this only for the basic case (lead +
            // combining marks); a text-default ❤ must stay 1 cell. VS16/ZWJ
            // sequences are decided by the DFA, independent of this flag.
            lead_ep: Properties::get(c).is_emoji_presentation(),
            lead_cat: cat,
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
    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

/// Whether `c` continues the open run `r` — the spec's `cont_t` predicate: the
/// parley DFA's transition from the run's accumulated state (keycap/RI leads
/// have no `→ ZWJ`, so a ZWJ after them is a stray) plus the deliberate
/// combining-mark exception (`w₁ = 0` non-selector attaches to any emoji
/// base).
#[inline]
pub(crate) fn run_continues(r: &Run, c: char, cat: EmojiSegmentationCategory) -> bool {
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
            is_continuation(cat)
                || (r.prev_zwj && is_zwj_target(c))
                || (cat == EmojiSegmentationCategory::RegionalIndicator && r.ri_count == 1)
                || cell_width(c) == Some(0)
        }
    }
}

/// The finalized width of the run's LEAD, with the lone-mark fix folded in: a
/// lone 0-width mark occupies its own cell (1) unless it is a selector
/// (invisible, 0) or a base precedes it (attaches, 0).
#[inline]
pub(crate) fn finalize_lead(
    r: &Run,
    has_base: &mut bool,
    space_base: &mut bool,
    followed_by_mark: bool,
) -> usize {
    let lead_w = if r.len > 1 && is_emoji_starter(r.lead_cat) {
        run_width(&r.dfa, r.lead, r.lead_ep, r.keycap_end)
    } else {
        r.lead_w1
    };
    let w = if lead_w == 0 {
        let sel = Selector::from_char(r.lead);
        if sel.is_vs() || sel.is_zwj() || (*has_base && !(followed_by_mark && *space_base)) {
            0
        } else {
            // A lone mark, OR a mark followed by another combining mark with
            // only a SPACE base: it must occupy its own cell so the following
            // mark has a VISIBLE base (Ghostty renders it left of the next
            // glyph). A visible base already gives the next mark a base.
            1
        }
    } else {
        lead_w
    };
    if w > 0 {
        *has_base = true;
        *space_base = r.lead == ' ';
    }
    w
}
