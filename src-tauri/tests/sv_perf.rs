//! Parser throughput and memory on a generated ~50 MB Auctionator-shaped
//! file. Ignored by default; run with
//! `cargo test --release --test sv_perf -- --ignored --nocapture`.
//!
//! Targets (spec §3): at least 100 MB/s, peak memory about 3× file size.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;
use wow_forever_buddy_lib::sv;

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let now = CURRENT.fetch_add(layout.size(), Relaxed) + layout.size();
        PEAK.fetch_max(now, Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Price history per item, the shape that makes real Auctionator/TSM files
/// reach 50–100 MB, plus some string-heavy posting history.
fn generate(target_bytes: usize) -> String {
    let mut s = String::with_capacity(target_bytes + 4096);
    s.push_str("\nAUCTIONATOR_PRICE_DATABASE = {\n\t[\"__dbversion\"] = 6,\n\t[\"Forever_Alliance\"] = {\n");
    let mut item = 1000u32;
    while s.len() < target_bytes {
        item += 1;
        let _ = writeln!(s, "\t\t[\"{item}\"] = {{");
        for series in ["l", "a", "h"] {
            let _ = writeln!(s, "\t\t\t[\"{series}\"] = {{");
            for day in 0..12u32 {
                let _ = writeln!(
                    s,
                    "\t\t\t\t[{}] = {},",
                    19990 + day,
                    (item * 37 + day * 101) % 250_000
                );
            }
            s.push_str("\t\t\t},\n");
        }
        let _ = writeln!(s, "\t\t\t[\"m\"] = {},", item * 13 % 99_999);
        if item.is_multiple_of(10) {
            let _ = write!(
                s,
                "\t\t\t[\"link\"] = \"|cff1eff00|Hitem:{item}::::::::60:::::|h[Generated Item {item}]|h|r\",\n\t\t\t[\"note\"] = \"posted by Thrandor\\nundercut by 1c\",\n\t\t\t[\"ratio\"] = {}.{},\n",
                item % 7,
                item % 1000
            );
        }
        s.push_str("\t\t},\n");
    }
    s.push_str("\t},\n}\n");
    s
}

#[test]
#[ignore]
fn parse_50mb() {
    let src = generate(50 * 1024 * 1024);
    let mb = src.len() as f64 / (1024.0 * 1024.0);

    // Warm up once so page faults on the allocator's first growth don't skew
    // the timing, then measure.
    drop(sv::parse(src.as_bytes()).unwrap());

    let baseline = CURRENT.load(Relaxed);
    PEAK.store(baseline, Relaxed);
    let start = Instant::now();
    let parsed = sv::parse(src.as_bytes()).unwrap();
    let elapsed = start.elapsed().as_secs_f64();
    let peak = PEAK.load(Relaxed) - baseline;
    let retained = CURRENT.load(Relaxed) - baseline;

    let throughput = mb / elapsed;
    let peak_ratio = peak as f64 / src.len() as f64;
    let retained_ratio = retained as f64 / src.len() as f64;
    println!(
        "{mb:.1} MB in {:.0} ms = {throughput:.0} MB/s; peak {:.1} MB ({peak_ratio:.2}x), retained {:.1} MB ({retained_ratio:.2}x)",
        elapsed * 1000.0,
        peak as f64 / 1048576.0,
        retained as f64 / 1048576.0,
    );
    assert_eq!(parsed.len(), 1);

    assert!(peak_ratio <= 3.0, "peak memory {peak_ratio:.2}x file size");
    if !cfg!(debug_assertions) {
        assert!(throughput >= 100.0, "{throughput:.0} MB/s");
    }
}
