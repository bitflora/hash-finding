//! Batch SAC scoring for short 32-bit mixers. See analysis/short_core_race.py.
use mixlab::bias::{sampled_serial, Counts};
use rayon::prelude::*;
use std::io::{self, Read};

// Raw CRC32C instruction semantics: zero accumulator, least significant bit first.
#[inline]
fn crc32c(x: u32) -> u32 {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse4.2"))]
    // SAFETY: this branch is compiled only for SSE4.2 targets.
    unsafe {
        core::arch::x86_64::_mm_crc32_u32(0, x)
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "sse4.2")))]
    {
        let mut r = x;
        for _ in 0..32 {
            r = (r >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(r & 1));
        }
        r
    }
}

// Representatives modulo the commuting symmetries x -> x ^ D and x -> x ^ 2^31.
// Both leave the output XOR difference unchanged for every odd multiplier.
fn exact_top_row(b: u32, shift: u32) -> Counts {
    use mixlab::bias::VCounter;
    assert_eq!(b & 1, 1);
    let bins = (0..1u32 << 10)
        .into_par_iter()
        .map(|chunk| {
            let mut totals = [0u64; 32];
            let mut counter = VCounter::<16>::new();
            for i in (chunk << 20)..((chunk + 1) << 20) {
                let x = ((i & !7) << 1) | (i & 7); // fix bits 31 and 3 to zero
                let delta = x.wrapping_mul(b) ^ (x ^ 0x82f63b78).wrapping_mul(b);
                counter.add((delta ^ (delta >> shift)) as u64);
                if i & 32767 == 32767 {
                    counter.flush(&mut totals, 32);
                }
            }
            totals
        })
        .reduce(
            || [0; 32],
            |mut a, b| {
                for (i, count) in b.iter().enumerate() {
                    a[i] += count;
                }
                a
            },
        );
    Counts {
        bits: 32,
        n: 1u64 << 32,
        bins: bins.iter().map(|count| count * 4).collect(),
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "short-race SAMPLE_COUNT SEED < candidates");
    let n: u64 = args[1].parse().unwrap();
    let seed = args[2].parse().unwrap();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let lines: Vec<_> = input.lines().collect();
    let results: Vec<_> = lines
        .par_iter()
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            let a = u32::from_str_radix(fields[1], 16).unwrap();
            let b = u32::from_str_radix(fields[2], 16).unwrap();
            let s: u32 = fields[3].parse().unwrap();
            let t: u32 = fields[4].parse().unwrap();
            if fields[0] == "crc5-exact-row31" {
                let row = exact_top_row(b, t);
                return format!("{},{}", row.ms(), row.max_abs());
            }
            let counts: Counts = match fields[0] {
                "crc3" => sampled_serial(
                    &|x| crc32c((x as u32).wrapping_mul(a)).wrapping_mul(b) as u64,
                    32,
                    n,
                    seed,
                ),
                "crc5" | "crc5-detail" => sampled_serial(
                    &|x| {
                        let y = crc32c((x as u32).wrapping_mul(a)).wrapping_mul(b);
                        (y ^ (y >> t)) as u64
                    },
                    32,
                    n,
                    seed,
                ),
                "xs6" => sampled_serial(
                    &|x| {
                        let y = (x as u32).wrapping_mul(a);
                        let z = (y ^ (y >> s)).wrapping_mul(b);
                        (z ^ (z >> t)) as u64
                    },
                    32,
                    n,
                    seed,
                ),
                _ => panic!("unknown family"),
            };
            if fields[0] == "crc5-detail" {
                let row = Counts {
                    bits: 32,
                    n,
                    bins: counts.bins[31 * 32..].to_vec(),
                };
                format!(
                    "{},{},{}",
                    counts.ms_debiased(),
                    counts.max_abs(),
                    row.ms_debiased()
                )
            } else {
                format!("{},{}", counts.ms_debiased(), counts.max_abs())
            }
        })
        .collect();
    for line in results {
        println!("{line}");
    }
}
