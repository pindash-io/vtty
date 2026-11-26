// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential test: the same corpus through `vtty` and the `vte` crate, with
//! both action streams normalized to `vtty::Event` and compared.
//!
//! `cargo test --test vt_differential -- --nocapture` prints the divergences.

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
