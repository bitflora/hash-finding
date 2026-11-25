//! GPU exact and sampled counts must equal the CPU's exactly. Needs the
//! `gpu` feature (default), an NVIDIA GPU and the CUDA Toolkit (NVRTC); run
//! with `cargo test --release --test gpu`.
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

#[test]
fn gpu_sampling_matches_cpu_inputs_and_counts() {
    let g = gpu();
    let mut rng = Xoshiro256::new(33);
    // sizes below, at and across chunk (2^14) and per-thread (256) boundaries,
    // and across several launches for the batch of 24 at 2^22
    for n in [1u64, 255, 256, 1000, 1 << 14, (1 << 14) + 1, 3 * (1 << 14) + 77, 1 << 18, (1 << 22) + 300] {
        for bits in [8u32, 21, 32] {
            for rounds in 1..=4 {
                let k = if n >= 1 << 22 { 24 } else { 3 };
                let xs: Vec<Xmx> = (0..k).map(|_| random_config(bits, rounds, &mut rng)).collect();
                let seeds: Vec<u64> = (0..k).map(|_| rng.next_u64()).collect();
                let got = g.sampled_batch(&xs, n, &seeds).unwrap();
                for ((x, s), c) in xs.iter().zip(&seeds).zip(&got) {
                    let want = mixlab::racing::sample_counts_cpu(x, n, *s);
                    assert_eq!(want.n, c.n);
                    assert_eq!(want.bins, c.bins, "{x} n={n} seed={s}");
                }
            }
        }
    }
}

#[test]
fn backend_batch_mixes_gpu_and_cpu_widths() {
    let mut rng = Xoshiro256::new(34);
    // 5 and 40 bits stay on the CPU; the 3-round mixer forms its own GPU group
    let mut xs: Vec<Xmx> = [5u32, 16, 16, 40].iter().map(|&b| random_config(b, 2, &mut rng)).collect();
    xs.push(random_config(16, 3, &mut rng));
    let seeds: Vec<u64> = (0..xs.len() as u64).collect();
    mixlab::exact::set_backend(mixlab::exact::Backend::Gpu).unwrap();
    let got = mixlab::exact::sampled_batch(&xs, 5000, &seeds);
    for ((x, s), c) in xs.iter().zip(&seeds).zip(&got) {
        assert_eq!(mixlab::racing::sample_counts_cpu(x, 5000, *s).bins, c.bins, "{x}");
    }
}
