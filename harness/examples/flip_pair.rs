//! Count two output flips under a fixed XOR input difference.
//! Usage: flip_pair SPEC HEX_INPUT_MASK OUT_J OUT_K LOG2_SAMPLES|exact
use mixlab::{mixers::Xmx, rng::Xoshiro256, with_mixer};
use rayon::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [spec, mask, j, k, lg] = &args[..] else {
        panic!("expected SPEC HEX_INPUT_MASK OUT_J OUT_K LOG2_SAMPLES|exact");
    };
    let x = Xmx::parse(32, spec).expect("32-bit mixer");
    let delta = u32::from_str_radix(mask, 16).expect("hex input difference") as u64;
    let j: u32 = j.parse().expect("first output bit");
    let k: u32 = k.parse().expect("second output bit");
    assert!(delta != 0 && j < k && k < 32);
    let (n, bins) = with_mixer!(x, f => {
        let cell = |input| {
            let diff = f(input) ^ f(input ^ delta);
            let a = (diff >> j) & 1;
            let b = (diff >> k) & 1;
            (2 * a + b) as usize
        };
        if lg == "exact" {
            const CHUNK: u64 = 1 << 20;
            let n = 1u64 << 31;
            let low = (1u64 << delta.trailing_zeros()) - 1;
            let bins = (0..n / CHUNK)
                .into_par_iter()
                .map(|c| {
                    let mut bins = [0u64; 4];
                    // Insert a zero at the lowest set bit of delta to visit
                    // each unordered pair of inputs exactly once.
                    for i in (c * CHUNK)..((c + 1) * CHUNK) {
                        let input = (i & low) | ((i & !low) << 1);
                        bins[cell(input)] += 1;
                    }
                    bins
                })
                .reduce(|| [0; 4], |a, b| std::array::from_fn(|i| a[i] + b[i]));
            (n, bins)
        } else {
            let lg: u32 = lg.parse().expect("sample exponent");
            assert!((1..=32).contains(&lg));
            let n = 1u64 << lg;
            let mut rng = Xoshiro256::new(39);
            let mut bins = [0u64; 4];
            for _ in 0..n {
                bins[cell(rng.next_u64() & u32::MAX as u64)] += 1;
            }
            (n, bins)
        }
    });
    let [n00, n01, n10, n11] = bins;
    println!("{x},{delta:08x},{j},{k},{n},{n00},{n01},{n10},{n11}");
}
