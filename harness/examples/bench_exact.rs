//! A/B timing of the exact 32-bit counters, single-threaded over 2^26 inputs.
//! `cargo run --release --example bench_exact [log2_inputs]`
use mixlab::mixers::Xmx;
use std::time::Instant;

fn main() {
    let lg: u32 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(26);
    let x = Xmx::parse(32, "[16 21f0aaad 15 735a2d97 15]").unwrap();
    let (lo, hi) = (0x1234_0000u64, 0x1234_0000u64 + (1u64 << lg));
    let mut best = [f64::MAX; 2];
    for rep in 0..5 {
        for (i, hs) in [false, true].into_iter().enumerate() {
            let t = Instant::now();
            let c = mixlab::fast32::counts_range(&x, lo, hi, hs);
            let s = t.elapsed().as_secs_f64();
            best[i] = best[i].min(s);
            println!("rep {rep} {:<12} {s:7.3} s  (checksum {})", if hs { "harley-seal" } else { "ripple" }, c.bins.iter().sum::<u64>());
        }
    }
    println!(
        "best: ripple {:.3} s, harley-seal {:.3} s, speedup x{:.2}; full 2^32 on 1 thread ≈ {:.0} s vs {:.0} s",
        best[0],
        best[1],
        best[0] / best[1],
        best[0] * (1u64 << (32 - lg)) as f64,
        best[1] * (1u64 << (32 - lg)) as f64
    );
}
