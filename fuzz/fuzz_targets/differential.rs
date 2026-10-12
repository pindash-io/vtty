// Differential fuzz target: vtty vs the reference `vte` parser.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vtty_fuzz::{has_known_divergence, run_vte, run_vtty};

fuzz_target!(|data: &[u8]| {
    if has_known_divergence(data) {
        return;
    }
    let l = run_vtty(data);
    let r = run_vte(data);
    if l != r {
        panic!(
            "divergence:\n  input: {:?}\n  vtty:  {:?}\n  vte:   {:?}",
            data, l, r
        );
    }
});
