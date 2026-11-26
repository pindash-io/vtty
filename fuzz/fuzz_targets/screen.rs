// Screen invariant fuzz target: arbitrary bytes through the parser into a
// `Screen`, asserting its structural invariants after every chunk.

#![no_main]
#![allow(missing_docs, reason = "fuzz target is internal test infrastructure")]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut screen = vtty_screen::Screen::new(80, 24);
    let mut parser = vtty::Parser::with_simd();

    // Feed in chunks so chunk-boundary state (partial UTF-8, OSC deferral,
    // wrap-pending across feeds) is exercised, not just whole-input parsing.
    for chunk in data.chunks(64) {
        parser.advance(&mut screen, chunk);
        screen.check_invariants();
    }
});
