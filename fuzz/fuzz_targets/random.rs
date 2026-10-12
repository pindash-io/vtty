// Structured random fuzzing: generate VT-ish byte sequences from the fuzzer
// seed and run the vtty-vs-vte differential check. Complements the byte-
// mutation targets by biasing toward escape sequences and multi-byte UTF-8,
// which pure mutation takes longer to discover.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vtty_fuzz::{has_known_divergence, run_vte, run_vtty};

/// xorshift64 PRNG, deterministic per fuzzer input.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fuzz_target!(|data: &[u8]| {
    // Seed from the input bytes so libFuzzer's mutations explore the generator.
    let seed = data
        .iter()
        .fold(0x9e37_79b9_7f4a_7c15_u64, |h, &b| {
            h.wrapping_mul(31).wrapping_add(u64::from(b))
        })
        .max(1);
    let mut rng = Rng(seed);
    let len = 64 + (rng.next() % 4096) as usize;
    let mut buf = Vec::with_capacity(len);
    for _ in 0..len {
        buf.push(match rng.next() % 12 {
            0 => 0x1b,                              // ESC
            1 => b'[',                              // CSI
            2 => b']',                              // OSC
            3 => b'P',                              // DCS
            4 => b';',                              // param separator
            5 => b':',                              // subparam separator
            6 => b'0' + (rng.next() % 10) as u8,    // digit
            7 => 0x20 + (rng.next() % 0x5f) as u8,  // printable ASCII
            8 => 0xc2 + (rng.next() % 30) as u8,    // 2-byte UTF-8 lead
            9 => 0xe0 + (rng.next() % 16) as u8,    // 3-byte lead
            10 => 0x80 + (rng.next() % 0x40) as u8, // continuation
            _ => rng.next() as u8,                  // anything
        });
    }

    if has_known_divergence(&buf) {
        return;
    }
    let l = run_vtty(&buf);
    let r = run_vte(&buf);
    if l != r {
        panic!(
            "divergence:\n  input: {:?}\n  vtty:  {:?}\n  vte:   {:?}",
            buf, l, r
        );
    }
});
