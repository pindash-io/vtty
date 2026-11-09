// Screen invariant fuzz target: feed arbitrary bytes through the parser into
// a `Screen`, asserting the screen's structural invariants after every chunk.
// This exercises the implicit state machines — autowrap, origin mode,
// insert/replace, scroll margins, alt-screen swap, and the ring buffer — the
// same way `check_invariants` does, but with libFuzzer's coverage guidance.

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
