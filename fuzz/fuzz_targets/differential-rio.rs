// Differential fuzz target: vtty vs rio-vt's standalone parser. Same shape as
// `differential.rs` (vte); rio-vt is a fork of Alacritty's VTE, so the known
// divergences are the same.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vtty_fuzz::{
    Event, has_intermediate_overflow, has_known_divergence, has_rio_utf8_simd_divergence, run_vtty,
};

struct RioRec {
    acts: Vec<Event>,
}

impl rio_vt::performer::parser::Perform for RioRec {
    fn print(&mut self, c: char) {
        self.acts.push(Event::Print(c));
    }
    fn print_str(&mut self, s: &str) {
        self.acts.extend(s.chars().map(Event::Print));
    }
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        self.acts.extend(
            codepoints
                .iter()
                .map(|&cp| Event::Print(char::from_u32(cp).unwrap_or('\u{FFFD}'))),
        );
    }
    fn execute(&mut self, b: u8) {
        self.acts.push(Event::Execute(b));
    }
    fn csi_dispatch(
        &mut self,
        params: &rio_vt::performer::parser::Params,
        int: &[u8],
        _ignore: bool,
        f: char,
    ) {
        let flat: smallvec::SmallVec<i32, 6> = params.iter().flatten().map(|&v| v as i32).collect();
        self.acts.push(Event::CsiDispatch {
            params: flat,
            intermediates: int.iter().copied().collect(),
            final_byte: f as u8,
        });
    }
    fn esc_dispatch(&mut self, int: &[u8], _ignore: bool, b: u8) {
        self.acts.push(Event::EscDispatch {
            intermediates: int.iter().copied().collect(),
            byte: b,
        });
    }
    fn hook(
        &mut self,
        params: &rio_vt::performer::parser::Params,
        int: &[u8],
        _ignore: bool,
        f: char,
    ) {
        let flat: smallvec::SmallVec<i32, 6> = params.iter().flatten().map(|&v| v as i32).collect();
        self.acts.push(Event::DcsHook {
            params: flat,
            intermediates: int.iter().copied().collect(),
            final_byte: f as u8,
        });
    }
    fn put(&mut self, b: u8) {
        self.acts.push(Event::DcsPut(b));
    }
    fn unhook(&mut self) {
        self.acts.push(Event::DcsUnhook);
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
        self.acts.push(Event::OscDispatch {
            params: params.iter().map(|p| p.to_vec()).collect(),
            bell_terminated: bell,
        });
    }
}

fn run_rio(bytes: &[u8]) -> Vec<Event> {
    let mut rec = RioRec { acts: Vec::new() };
    let mut p = rio_vt::performer::parser::Parser::new();
    p.advance(&mut rec, bytes);
    rec.acts
}

fuzz_target!(|data: &[u8]| {
    if has_known_divergence(data) {
        return;
    }
    // rio-vt allows up to four intermediates; vtty (and vte) cap at two.
    if has_intermediate_overflow(data, 2) {
        return;
    }
    // rio-vt's simdutf backend decodes malformed ground-state UTF-8 wrongly on
    // x86_64 (upstream bug); vtty/vte are correct, so skip those inputs.
    if has_rio_utf8_simd_divergence(data) {
        return;
    }
    let l = run_vtty(data);
    let r = run_rio(data);
    if l != r {
        panic!(
            "divergence:\n  input: {:?}\n  vtty:  {:?}\n  rio:   {:?}",
            data, l, r
        );
    }
});
