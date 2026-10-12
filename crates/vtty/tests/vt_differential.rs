// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential test: run a corpus of VT byte sequences through both `vtty`
//! and the reference `vte` crate, normalize the action streams, and assert
//! they match. This is how we find sequences vtty does not yet handle.
//!
//! vtty's enum API (`vtty::Event` + `Parser::parse_events`) is compared
//! against vte's `Perform` callbacks, both normalized to `vtty::Event`.
//!
//! Run with `--nocapture` to see the full difference report:
//!   cargo test --test `vt_differential` -- --nocapture

mod common;

use common::{corpus, diff_report, run_vte, run_vtty};

#[test]
fn differential_against_vte() {
    let mut diffs = 0_usize;
    for bytes in corpus() {
        let l = run_vtty(&bytes);
        let r = run_vte(&bytes);
        if let Some(report) = diff_report(&bytes, &l, &r, "vtty", "vte") {
            println!("DIFF {}\n{report}", diffs + 1);
            diffs += 1;
        }
    }
    assert_eq!(diffs, 0, "{diffs} sequences differ from vte");
}
