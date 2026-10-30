// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! DEC VT500 escape-sequence state machine: parser states, the `Action`
//! table, and the `transition` lookup.
//!
//! Reference: Paul Flo Williams' VT500 parser state diagram
//! <https://vt100.net/emu/dec_ansi_parser> — the "State Definitions" and
//! "Action Definitions" sections map 1:1 to [`ParseState`] and [`Action`].
//! The grammar those states encode is ECMA-48 §5.4/§8.3
//! (<https://ecma-international.org/wp-content/uploads/ECMA-48_5th_edition_june_1991.pdf>).

/// VT500 parser states (mutually exclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub(crate) enum ParseState {
    /// Ground
    #[default]
    Ground = 0,
    /// Escape
    Escape = 1,
    /// `EscapeIntermediate`
    EscapeIntermediate = 2,
    /// `CsiEntry`
    CsiEntry = 3,
    /// `CsiParam`
    CsiParam = 4,
    /// `CsiIntermediate`
    CsiIntermediate = 5,
    /// `CsiIgnore`
    CsiIgnore = 6,
    /// `OscString`
    OscString = 7,
    /// `DcsEntry`
    DcsEntry = 8,
    /// `DcsParam`
    DcsParam = 9,
    /// `DcsIntermediate`
    DcsIntermediate = 10,
    /// `DcsPassthrough`
    DcsPassthrough = 11,
    /// `DcsIgnore`
    DcsIgnore = 12,
    /// `SosPmApcString`
    SosPmApcString = 13,
}

// ── Actions (bitmask — a byte can trigger several) ─────────────────────

bitflags::bitflags! {
    /// Parser actions a byte can trigger (a byte can trigger several).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct Action: u16 {
        /// No action.
        const NONE = 0;
        /// print
        const PRINT = 1 << 0;
        /// execute
        const EXECUTE = 1 << 1;
        /// csi_dispatch
        const CSI_DISPATCH = 1 << 2;
        /// esc_dispatch
        const ESC_DISPATCH = 1 << 3;
        /// hook
        const HOOK = 1 << 4;
        /// put
        const PUT = 1 << 5;
        /// unhook
        const UNHOOK = 1 << 6;
        /// osc_start
        const OSC_START = 1 << 7;
        /// osc_put
        const OSC_PUT = 1 << 8;
        /// osc_end
        const OSC_END = 1 << 9;
    }
}

/// The DEC VT500 transition table for the non-ground states (each `ParseState`
/// arm maps to a "State Definitions" entry on
/// <https://vt100.net/emu/dec_ansi_parser>). This is the compile-time
/// evaluator, expanded into the flat [`TABLE`]; runtime [`transition`] is one
/// indexed load.
const fn transition_impl(state: ParseState, b: u8) -> (Action, ParseState) {
    match state {
        ParseState::Escape => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::Escape),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::EscapeIntermediate),
            0x30..=0x4f | 0x51..=0x57 | 0x59 | 0x5a | 0x5c | 0x60..=0x7e => {
                (Action::ESC_DISPATCH, ParseState::Ground)
            }
            0x50 => (Action::NONE, ParseState::DcsEntry), // 'P' → DCS
            0x58 | 0x5e | 0x5f => (Action::NONE, ParseState::SosPmApcString), // 'X' '^' '_' → SOS/PM/APC
            0x5b => (Action::NONE, ParseState::CsiEntry),                     // '[' → CSI
            0x5d => (Action::OSC_START, ParseState::OscString),               // ']' → OSC
            _ => (Action::NONE, ParseState::Escape),
        },
        ParseState::EscapeIntermediate => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::EscapeIntermediate),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::EscapeIntermediate),
            0x30..=0x7e => (Action::ESC_DISPATCH, ParseState::Ground),
            _ => (Action::NONE, ParseState::EscapeIntermediate),
        },
        ParseState::CsiEntry => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiEntry),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::CsiIntermediate),
            // digits, `;`, the kitty `:` sub-param separator, and `< = > ?`
            0x30..=0x3f => (Action::NONE, ParseState::CsiParam),
            0x40..=0x7e => (Action::CSI_DISPATCH, ParseState::Ground),
            _ => (Action::NONE, ParseState::CsiEntry),
        },
        ParseState::CsiParam => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiParam),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::CsiIntermediate),
            0x30..=0x3b => (Action::NONE, ParseState::CsiParam), // digits, `;`, kitty `:`
            0x3c..=0x3f => (Action::NONE, ParseState::CsiIgnore),
            0x40..=0x7e => (Action::CSI_DISPATCH, ParseState::Ground),
            _ => (Action::NONE, ParseState::CsiParam),
        },
        ParseState::CsiIntermediate => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiIntermediate),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::CsiIntermediate),
            0x30..=0x3f => (Action::NONE, ParseState::CsiIgnore),
            0x40..=0x7e => (Action::CSI_DISPATCH, ParseState::Ground),
            _ => (Action::NONE, ParseState::CsiIntermediate),
        },
        ParseState::CsiIgnore => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiIgnore),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x3f => (Action::NONE, ParseState::CsiIgnore),
            0x40..=0x7e => (Action::NONE, ParseState::Ground),
            _ => (Action::NONE, ParseState::CsiIgnore),
        },
        ParseState::OscString => match b {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::OscString),
            0x07 => (Action::OSC_END, ParseState::Ground),
            0x18 | 0x1a => (Action::OSC_END.union(Action::EXECUTE), ParseState::Ground),
            0x1b => (Action::OSC_END, ParseState::Escape),
            0x9c => (Action::OSC_END, ParseState::Ground),
            _ => (Action::OSC_PUT, ParseState::OscString),
        },
        ParseState::DcsEntry => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsEntry),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::DcsIntermediate),
            // digits, `;`, the kitty `:` sub-param separator, and `< = > ?`
            0x30..=0x3f => (Action::NONE, ParseState::DcsParam),
            0x40..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
            _ => (Action::NONE, ParseState::DcsEntry),
        },
        ParseState::DcsParam => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsParam),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::DcsIntermediate),
            0x30..=0x3b => (Action::NONE, ParseState::DcsParam), // digits, `;`, kitty `:`
            0x3c..=0x3f => (Action::NONE, ParseState::DcsIgnore),
            0x40..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
            _ => (Action::NONE, ParseState::DcsParam),
        },
        ParseState::DcsIntermediate => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsIntermediate),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x2f => (Action::NONE, ParseState::DcsIntermediate),
            0x30..=0x3f => (Action::NONE, ParseState::DcsIgnore),
            0x40..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
            _ => (Action::NONE, ParseState::DcsIntermediate),
        },
        ParseState::DcsPassthrough => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x7e => (Action::PUT, ParseState::DcsPassthrough),
            0x18 | 0x1a => (Action::UNHOOK.union(Action::EXECUTE), ParseState::Ground),
            0x1b => (Action::UNHOOK, ParseState::Escape),
            0x9c => (Action::UNHOOK, ParseState::Ground),
            _ => (Action::NONE, ParseState::DcsPassthrough),
        },
        ParseState::DcsIgnore => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsIgnore),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            0x20..=0x7f => (Action::NONE, ParseState::DcsIgnore),
            0x9c => (Action::NONE, ParseState::Ground),
            _ => (Action::NONE, ParseState::DcsIgnore),
        },
        ParseState::SosPmApcString => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::SosPmApcString),
            0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
            0x1b => (Action::NONE, ParseState::Escape),
            // SOS/PM/APC: the first content byte is the "action" (e.g. `G`
            // for kitty graphics APC `_G`); dispatch it as a hook and reuse
            // the DCS string state for put/unhook. This is more useful than
            // vte's blanket drop (vte predates kitty graphics).
            0x20..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
            0x9c => (Action::NONE, ParseState::Ground),
            _ => (Action::NONE, ParseState::SosPmApcString),
        },
        ParseState::Ground => (Action::NONE, ParseState::Ground),
    }
}

/// The 14 parser states in `repr(u8)` order, indexed by [`build_table`].
const STATES: [ParseState; 14] = [
    ParseState::Ground,
    ParseState::Escape,
    ParseState::EscapeIntermediate,
    ParseState::CsiEntry,
    ParseState::CsiParam,
    ParseState::CsiIntermediate,
    ParseState::CsiIgnore,
    ParseState::OscString,
    ParseState::DcsEntry,
    ParseState::DcsParam,
    ParseState::DcsIntermediate,
    ParseState::DcsPassthrough,
    ParseState::DcsIgnore,
    ParseState::SosPmApcString,
];

/// The DEC VT500 transition table expanded to a flat `[state][byte]` lookup
/// table at compile time. 14 states × 256 bytes = 3584 × 4 bytes ≈ 14 KB,
/// resident in `.rodata`; the runtime lookup is a single load.
static TABLE: [(Action, ParseState); 14 * 256] = build_table();

const fn build_table() -> [(Action, ParseState); 14 * 256] {
    let mut t = [(Action::NONE, ParseState::Ground); 14 * 256];
    let mut s = 0_usize;
    while s < 14 {
        let mut b = 0_usize;
        while b < 256 {
            let (action, next) = transition_impl(STATES[s], b as u8);
            t[s * 256 + b] = (action, next);
            b += 1;
        }
        s += 1;
    }
    t
}

/// Runtime transition: one indexed load into [`TABLE`], no per-byte match or
/// unpacking.
#[inline(always)]
pub(crate) fn transition(state: ParseState, b: u8) -> (Action, ParseState) {
    TABLE[(state as usize) << 8 | b as usize]
}

#[cfg(test)]
mod tests {
    use super::{Action, ParseState, transition};

    /// Independent transcription of Paul Flo Williams' VT500 parser state
    /// diagram (<https://vt100.net/emu/dec_ansi_parser>), restricted to
    /// bytes `0x00..=0x7f` (C0 + GL + `DEL`). C1 controls (`0x80..=0x9f`) are
    /// dispatched by `Parser::c1_transition`, not this table, and GR bytes
    /// (`0xa0..=0xff`) are UTF-8 lead bytes in a UTF-8 terminal (the DEC
    /// diagram's GR==GL rule applies only to 8-bit character-set mode).
    ///
    /// This is *not* a copy of `transition_impl`: it is re-derived from the
    /// state diagram's `event`/transition legend so a transcription error in
    /// either this reference or the production table fails the comparison.
    fn dec_step(state: ParseState, b: u8) -> (Action, ParseState) {
        match state {
            // escape
            ParseState::Escape => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::Escape),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::EscapeIntermediate),
                0x30..=0x4f | 0x51..=0x57 | 0x59 | 0x5a | 0x5c | 0x60..=0x7e => {
                    (Action::ESC_DISPATCH, ParseState::Ground)
                }
                0x50 => (Action::NONE, ParseState::DcsEntry),
                0x58 | 0x5e | 0x5f => (Action::NONE, ParseState::SosPmApcString),
                0x5b => (Action::NONE, ParseState::CsiEntry),
                0x5d => (Action::OSC_START, ParseState::OscString),
                _ => (Action::NONE, ParseState::Escape),
            },
            // escape intermediate
            ParseState::EscapeIntermediate => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                    (Action::EXECUTE, ParseState::EscapeIntermediate)
                }
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::EscapeIntermediate),
                0x30..=0x7e => (Action::ESC_DISPATCH, ParseState::Ground),
                _ => (Action::NONE, ParseState::EscapeIntermediate),
            },
            // csi entry
            ParseState::CsiEntry => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiEntry),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::CsiIntermediate),
                0x30..=0x39 | 0x3b => (Action::NONE, ParseState::CsiParam),
                0x3c..=0x3f => (Action::NONE, ParseState::CsiParam),
                0x40..=0x7e => (Action::CSI_DISPATCH, ParseState::Ground),
                _ => (Action::NONE, ParseState::CsiEntry),
            },
            // csi param
            ParseState::CsiParam => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiParam),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::CsiIntermediate),
                0x30..=0x39 | 0x3b => (Action::NONE, ParseState::CsiParam),
                0x3c..=0x3f => (Action::NONE, ParseState::CsiIgnore),
                0x40..=0x7e => (Action::CSI_DISPATCH, ParseState::Ground),
                _ => (Action::NONE, ParseState::CsiParam),
            },
            // csi intermediate
            ParseState::CsiIntermediate => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiIntermediate),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::CsiIntermediate),
                0x30..=0x3f => (Action::NONE, ParseState::CsiIgnore),
                0x40..=0x7e => (Action::CSI_DISPATCH, ParseState::Ground),
                _ => (Action::NONE, ParseState::CsiIntermediate),
            },
            // csi ignore
            ParseState::CsiIgnore => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::EXECUTE, ParseState::CsiIgnore),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x3f => (Action::NONE, ParseState::CsiIgnore),
                0x40..=0x7e => (Action::NONE, ParseState::Ground),
                _ => (Action::NONE, ParseState::CsiIgnore),
            },
            // osc string
            ParseState::OscString => match b {
                0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1c..=0x1f => {
                    (Action::NONE, ParseState::OscString)
                }
                0x07 => (Action::OSC_END, ParseState::Ground),
                0x18 | 0x1a => (Action::OSC_END.union(Action::EXECUTE), ParseState::Ground),
                0x1b => (Action::OSC_END, ParseState::Escape),
                _ => (Action::OSC_PUT, ParseState::OscString),
            },
            // dcs entry
            ParseState::DcsEntry => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsEntry),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::DcsIntermediate),
                0x30..=0x39 | 0x3b => (Action::NONE, ParseState::DcsParam),
                0x3c..=0x3f => (Action::NONE, ParseState::DcsParam),
                0x40..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
                _ => (Action::NONE, ParseState::DcsEntry),
            },
            // dcs param
            ParseState::DcsParam => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsParam),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::DcsIntermediate),
                0x30..=0x39 | 0x3b => (Action::NONE, ParseState::DcsParam),
                0x3c..=0x3f => (Action::NONE, ParseState::DcsIgnore),
                0x40..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
                _ => (Action::NONE, ParseState::DcsParam),
            },
            // dcs intermediate
            ParseState::DcsIntermediate => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsIntermediate),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x2f => (Action::NONE, ParseState::DcsIntermediate),
                0x30..=0x3f => (Action::NONE, ParseState::DcsIgnore),
                0x40..=0x7e => (Action::HOOK, ParseState::DcsPassthrough),
                _ => (Action::NONE, ParseState::DcsIntermediate),
            },
            // dcs passthrough
            ParseState::DcsPassthrough => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x7e => (Action::PUT, ParseState::DcsPassthrough),
                0x18 | 0x1a => (Action::UNHOOK.union(Action::EXECUTE), ParseState::Ground),
                0x1b => (Action::UNHOOK, ParseState::Escape),
                _ => (Action::NONE, ParseState::DcsPassthrough),
            },
            // dcs ignore
            ParseState::DcsIgnore => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::DcsIgnore),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                0x20..=0x7f => (Action::NONE, ParseState::DcsIgnore),
                _ => (Action::NONE, ParseState::DcsIgnore),
            },
            // sos/pm/apc string
            ParseState::SosPmApcString => match b {
                0x00..=0x17 | 0x19 | 0x1c..=0x1f => (Action::NONE, ParseState::SosPmApcString),
                0x18 | 0x1a => (Action::EXECUTE, ParseState::Ground),
                0x1b => (Action::NONE, ParseState::Escape),
                _ => (Action::NONE, ParseState::SosPmApcString),
            },
            ParseState::Ground => (Action::NONE, ParseState::Ground),
        }
    }

    /// Deliberate deviations from the DEC diagram (modern protocol
    /// extensions). `(state, byte)` pairs that intentionally behave
    /// differently from DEC; everything else must match `dec_step` exactly.
    fn vtty_deviation(state: ParseState, b: u8) -> Option<(Action, ParseState)> {
        match (state, b) {
            // kitty colon sub-parameters: `:` is a param separator (SGR
            // `38:2::r:g:b`, `4:3` curly underline), not "ignore the whole
            // sequence" as DEC specified.
            (ParseState::CsiEntry, 0x3a) => Some((Action::NONE, ParseState::CsiParam)),
            (ParseState::CsiParam, 0x3a) => Some((Action::NONE, ParseState::CsiParam)),
            (ParseState::DcsEntry, 0x3a) => Some((Action::NONE, ParseState::DcsParam)),
            (ParseState::DcsParam, 0x3a) => Some((Action::NONE, ParseState::DcsParam)),
            // kitty APC graphics `_G`: SOS/PM/APC content bytes dispatch
            // `hook` (and reuse the DCS passthrough state for put/unhook)
            // instead of being ignored as DEC specified.
            (ParseState::SosPmApcString, 0x20..=0x7e) => {
                Some((Action::HOOK, ParseState::DcsPassthrough))
            }
            _ => None,
        }
    }

    /// Exhaustive differential: every non-ground state x every byte in the
    /// table's scope (`0x00..=0x7f`) must match the DEC diagram, except the
    /// documented deviations. 13 x 128 = 1664 assertions.
    #[test]
    fn table_matches_dec_ansi_parser_diagram() {
        const STATES: [ParseState; 13] = [
            ParseState::Escape,
            ParseState::EscapeIntermediate,
            ParseState::CsiEntry,
            ParseState::CsiParam,
            ParseState::CsiIntermediate,
            ParseState::CsiIgnore,
            ParseState::OscString,
            ParseState::DcsEntry,
            ParseState::DcsParam,
            ParseState::DcsIntermediate,
            ParseState::DcsPassthrough,
            ParseState::DcsIgnore,
            ParseState::SosPmApcString,
        ];

        for state in STATES {
            for b in 0x00..=0x7f {
                let expected = vtty_deviation(state, b).unwrap_or_else(|| dec_step(state, b));
                let actual = transition(state, b);
                assert_eq!(
                    actual, expected,
                    "state={state:?} byte=0x{b:02x}: table={actual:?} expected={expected:?}"
                );
            }
        }
    }
}
