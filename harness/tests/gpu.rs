//! GPU exact counts must equal fast32's exactly. Needs `--features gpu`, an
//! NVIDIA GPU and the CUDA Toolkit (NVRTC); run with
//! `cargo test --release --features gpu --test gpu`.
#![cfg(feature = "gpu")]

use mixlab::mixers::Xmx;
use mixlab::rng::Xoshiro256;
use mixlab::search::random_config;

fn gpu() -> std::sync::MutexGuard<'static, mixlab::gpu::Gpu> {
    mixlab::gpu::global().expect("GPU unavailable").lock().unwrap()
}

#[test]
fn gpu_matches_fast32_on_small_widths() {
    let g = gpu();
    let mut rng = Xoshiro256::new(31);
    for bits in [8u32, 9, 13, 16, 17, 20, 23, 24] {
        for rounds in 1..=4 {
            let x = random_config(bits, rounds, &mut rng);
            let a = mixlab::fast32::exact_xmx(&x, false);
            let b = g.exact_xmx(&x).unwrap();
            assert_eq!(a.n, b.n, "{x}");
            assert_eq!(a.bins, b.bins, "{x} at {bits} bits");
        }
    }
}

#[test]
fn gpu_matches_fast32_on_32bit_ranges() {
    let g = gpu();
    let mut rng = Xoshiro256::new(32);
    for rounds in 1..=4 {
        let x = random_config(32, rounds, &mut rng);
        // a window spanning several launches, plus one near the top of the range
        for (lo, hi) in [(0u64, 3 << 26), ((1 << 32) - (1 << 20), 1 << 32), (12345 * 256, 12345 * 256 + (1 << 18))] {
            let a = mixlab::fast32::counts_range(&x, lo, hi, true);
            let b = g.counts_range(&x, lo, hi).unwrap();
            assert_eq!(a.bins, b.bins, "{x} on [{lo}, {hi})");
        }
    }
}

#[test]
fn gpu_scores_current_best_exactly() {
    let x = Xmx::parse(32, "16 4b0332d3 15 549a3597 15").unwrap();
    let c = gpu().exact_xmx(&x).unwrap();
    assert_eq!(c.n, 1u64 << 32);
    assert_eq!(c.score(), 0.09873011186879989);
}
