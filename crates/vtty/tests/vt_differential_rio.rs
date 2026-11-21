// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential test: run a corpus of VT byte sequences through both `vtty`
//! and `rio-vt`'s standalone parser, normalize the action streams, and assert
//! they match. rio-vt is a fork of Alacritty's VTE, so the baseline is the
//! same as `vt_differential.rs` (vte) with two documented divergences:
//! rio-vt allows up to four intermediates (vtty/vte cap at two), and it keeps
//! the payload of some OSC commands verbatim instead of splitting every `;`.
//!
//! Run with `--nocapture` to see the full difference report:
//!   cargo test --test `vt_differential_rio` -- --nocapture

mod common;

use common::{
    corpus, diff_report, has_intermediate_overflow,
    has_osc_verbatim_divergence, run_rio, run_vtty,
};

#[test]
fn differential_against_rio_vt() {
    let mut diffs = 0_usize;
    let mut skipped = 0_usize;
    for bytes in corpus() {
        // Documented divergence: rio-vt allows up to four intermediates, while
        // vtty (and vte) cap at two per the DEC spec. Skip those sequences.
        if has_intermediate_overflow(&bytes, 2) {
            skipped += 1;
            continue;
        }
        // Documented divergence: rio-vt keeps the payload of some OSC commands
        // verbatim (semicolons stay in the final param) while vtty/vte split
        // every `;`. Skip those sequences.
        if has_osc_verbatim_divergence(&bytes) {
            skipped += 1;
            continue;
        }
        let l = run_vtty(&bytes);
        let r = run_rio(&bytes);
        if let Some(report) = diff_report(&bytes, &l, &r, "vtty", "rio-vt") {
            println!("DIFF {}\n{report}", diffs + 1);
            diffs += 1;
        }
    }
    assert_eq!(
        diffs, 0,
        "{diffs} sequences differ from rio-vt ({skipped} skipped)"
    );
}
