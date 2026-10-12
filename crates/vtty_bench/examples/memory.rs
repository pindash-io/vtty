//! Memory probe: parser struct size + heap allocations during `advance`,
//! vtty vs rio-vt.

use std::alloc::{GlobalAlloc, Layout, System};
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};

#[global_allocator]
static ALLOC: Counting = Counting;

struct Counting;

static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        let now = ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK_BYTES.fetch_max(now, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ALLOC_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn snapshot() -> (usize, usize, usize) {
    (
        ALLOC_COUNT.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
        PEAK_BYTES.load(Ordering::Relaxed),
    )
}

fn reset_peak() {
    PEAK_BYTES.store(ALLOC_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn corpus() -> Vec<u8> {
    let mut line = String::new();
    for i in 0..40 {
        line += &format!("ascii{i} ");
        line += "日本語テキスト ";
        line += "🚀✨🎯 ";
        line += "e\u{301} ";
    }
    line += "\r\n";
    line.repeat(2 * 1024 * 1024 / line.len() + 1).into_bytes()
}

fn main() {
    println!("size_of vtty::Parser      = {} B", size_of::<vtty::Parser>());
    println!("size_of rio_vt::Parser    = {} B", size_of::<rio_vt::performer::parser::Parser>());
    println!("size_of vte::Parser       = {} B", size_of::<vte::Parser>());
    let bytes = corpus();

    // vtty
    let mut p = vtty::Parser::new();
    let mut sink = VttySink;
    p.advance(&mut sink, &bytes); // warm up (std lazy init)
    let (c0, b0, _) = snapshot();
    reset_peak();
    p.advance(&mut sink, &bytes);
    let (c1, b1, peak1) = snapshot();
    println!(
        "vtty   advance: {} allocs, {} bytes net, {} peak",
        c1 - c0,
        b1 - b0,
        peak1 - b1
    );

    // rio-vt
    let mut p = rio_vt::performer::parser::Parser::new();
    let mut sink = RioSink;
    p.advance(&mut sink, &bytes); // warm up
    let (c0, b0, _) = snapshot();
    reset_peak();
    p.advance(&mut sink, &bytes);
    let (c1, b1, peak1) = snapshot();
    println!(
        "rio-vt advance: {} allocs, {} bytes net, {} peak",
        c1 - c0,
        b1 - b0,
        peak1 - b1
    );

    // vte
    let mut p = vte::Parser::new();
    let mut sink = VteSink;
    p.advance(&mut sink, &bytes); // warm up
    let (c0, b0, _) = snapshot();
    reset_peak();
    p.advance(&mut sink, &bytes);
    let (c1, b1, peak1) = snapshot();
    println!(
        "vte    advance: {} allocs, {} bytes net, {} peak",
        c1 - c0,
        b1 - b0,
        peak1 - b1
    );
}

#[derive(Default)]
struct VttySink;

impl vtty::Perform for VttySink {}

#[derive(Default)]
struct RioSink;

impl rio_vt::performer::parser::Perform for RioSink {}

#[derive(Default)]
struct VteSink;

impl vte::Perform for VteSink {}
