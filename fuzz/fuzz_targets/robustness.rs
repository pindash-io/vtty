// Robustness fuzz target: feed arbitrary bytes to every parser and ensure
// none of them panic. This is the only meaningful check for `vt100`, which is
// a full-screen emulator (no event stream to compare).

#![no_main]
#![allow(missing_docs, reason = "fuzz target is internal test infrastructure")]

use libfuzzer_sys::fuzz_target;

/// No-op sink for vte.
struct VteNoop;
impl vte::Perform for VteNoop {}

/// No-op sink for rio-vt.
struct RioNoop;
impl rio_vt::performer::parser::Perform for RioNoop {}

/// No-op actor for vtparse.
struct VtparseNoop;
impl vtparse::VTActor for VtparseNoop {
    fn print(&mut self, _b: char) {}
    fn execute_c0_or_c1(&mut self, _control: u8) {}
    fn dcs_hook(
        &mut self,
        _mode: u8,
        _params: &[i64],
        _intermediates: &[u8],
        _ignored_excess_intermediates: bool,
    ) {
    }
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self) {}
    fn esc_dispatch(
        &mut self,
        _params: &[i64],
        _intermediates: &[u8],
        _ignored_excess_intermediates: bool,
        _byte: u8,
    ) {
    }
    fn csi_dispatch(
        &mut self,
        _params: &[vtparse::CsiParam],
        _parameters_truncated: bool,
        _byte: u8,
    ) {
    }
    fn osc_dispatch(&mut self, _params: &[&[u8]]) {}
    fn apc_dispatch(&mut self, _data: Vec<u8>) {}
}

fuzz_target!(|data: &[u8]| {
    // vtty (scalar and SIMD).
    let mut p = vtty::Parser::new();
    p.advance(&mut vtty::Recording::default(), data);
    let mut p = vtty::Parser::with_simd();
    p.advance(&mut vtty::Recording::default(), data);

    // vte.
    let mut p = vte::Parser::new();
    p.advance(&mut VteNoop, data);

    // vt100 (full-screen emulator).
    let mut p = vt100::Parser::new(
        std::num::NonZero::new(24).expect("24 is non-zero"),
        std::num::NonZero::new(80).expect("80 is non-zero"),
        0,
    );
    p.process(data);

    // vtparse.
    let mut p = vtparse::VTParser::new();
    p.parse(data, &mut VtparseNoop);

    // rio-vt standalone parser.
    let mut p = rio_vt::performer::parser::Parser::new();
    p.advance(&mut RioNoop, data);
});
