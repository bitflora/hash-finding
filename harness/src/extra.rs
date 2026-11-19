//! Secondary yardsticks beyond plain avalanche bias:
//! - avalanche on *structured* inputs (counters, rotated/reversed/complemented
//!   counters in the spirit of Evensen's RRC, low-Hamming-weight words);
//! - the Bit Independence Criterion (pairwise correlation of output flips).
//!
//! Structured-input results are reported as `ms * n`: the mean squared diff in
//! units of the noise expected from an ideal random function (~1.0 = looks
//! ideal; >> 1 = structure leaking through).

use crate::bias::{Acc, Counts, VCounter};
use crate::mixers::mask;
use crate::rng::{stream_seed, Xoshiro256};
use rayon::prelude::*;

pub fn counts_on_inputs<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32, xs: &[u64]) -> Counts {
    xs.par_chunks(4096)
        .map(|ch| {
            let mut a = Acc::new(bits);
            for &x in ch {
                a.add_input(f, x);
            }
            a.finish()
        })
        .reduce_with(Counts::merge)
        .unwrap_or_else(|| Counts::new(bits))
}

/// ms * n : ~1 for an ideal mixer on a set of *independent* input pairs.
pub fn noise_units(c: &Counts) -> f64 {
    c.ms() * c.n as f64
}

/// Avalanche on a structured input set, in noise units (ideal ≈ 1).
/// For each input bit j the set is canonicalised to unique pairs
/// {x & !e_j, x | e_j} so no pair is counted twice (duplicates would inflate
/// the baseline: e.g. small counters contain both x and x^e_j for low j).
pub fn structured_noise_units<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32, xs: &[u64]) -> f64 {
    let b = bits as usize;
    let total: f64 = (0..b)
        .into_par_iter()
        .map(|j| {
            let e = 1u64 << j;
            let mut v: Vec<u64> = xs.iter().map(|&x| x & !e).collect();
            v.sort_unstable();
            v.dedup();
            let mut cnt = vec![0u64; b];
            for &x in &v {
                let s = f(x) ^ f(x | e);
                for (k, c) in cnt.iter_mut().enumerate() {
                    *c += (s >> k) & 1;
                }
            }
            let n = v.len() as f64;
            cnt.iter()
                .map(|&c| {
                    let d = (c as f64 - n / 2.0) / (n / 2.0);
                    d * d * n
                })
                .sum::<f64>()
        })
        .sum();
    total / (b * b) as f64
}

pub fn counter_inputs(bits: u32, n: u64) -> Vec<u64> {
    let m = mask(bits);
    (0..n).map(|i| i & m).collect()
}

fn reverse_bits(x: u64, bits: u32) -> u64 {
    x.reverse_bits() >> (64 - bits)
}

fn rotl(x: u64, r: u32, bits: u32) -> u64 {
    let m = mask(bits);
    let r = r % bits;
    if r == 0 {
        x & m
    } else {
        ((x << r) | (x >> (bits - r))) & m
    }
}

/// Counter 0..n under {identity, bit-reverse, rotate by bits/4, bits/2, 3bits/4}
/// and the complement of each: 10n inputs (duplicates kept).
pub fn rrc_inputs(bits: u32, n: u64) -> Vec<u64> {
    let m = mask(bits);
    let mut v = Vec::with_capacity((n * 10) as usize);
    for i in 0..n {
        let base = i & m;
        let forms = [
            base,
            reverse_bits(base, bits),
            rotl(base, bits / 4, bits),
            rotl(base, bits / 2, bits),
            rotl(base, 3 * bits / 4, bits),
        ];
        for f in forms {
            v.push(f);
            v.push(!f & m);
        }
    }
    v
}

/// All words of Hamming weight 0, 1, 2 and their complements.
pub fn lowweight_inputs(bits: u32) -> Vec<u64> {
    let m = mask(bits);
    let mut v = vec![0u64];
    for a in 0..bits {
        v.push(1u64 << a);
        for b in (a + 1)..bits {
            v.push((1u64 << a) | (1u64 << b));
        }
    }
    let comp: Vec<u64> = v.iter().map(|x| !x & m).collect();
    v.extend(comp);
    v
}

#[derive(Clone, Debug)]
pub struct Bic {
    pub n: u64,
    /// RMS of z = corr * sqrt(n) over all (input bit, output pair); ~1 is ideal.
    pub rms_z: f64,
    /// Largest |z|; compare to `expected_max_z`.
    pub max_z: f64,
    /// Approx. expected max |z| for an ideal mixer (Gaussian extreme value).
    pub expected_max_z: f64,
    /// (input bit i, output bit j, output bit k, corr) at the max |z|.
    pub worst: (usize, usize, usize, f64),
}

/// Bit Independence Criterion on `n` random inputs.
pub fn bic_sampled<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32, n: u64, seed: u64) -> Bic {
    let b = bits as usize;
    let msk = mask(bits);
    const CHUNK: u64 = 1 << 13;
    let nchunks = n.div_ceil(CHUNK);
    // acc[(i*b + j)*b + k] = #samples where input bit i flips both output j and k
    let acc: Vec<u64> = (0..nchunks)
        .into_par_iter()
        .map(|c| {
            let len = CHUNK.min(n - c * CHUNK);
            let mut rng = Xoshiro256::new(stream_seed(seed, c));
            let mut ctr = vec![VCounter::<16>::new(); b * b];
            let mut acc = vec![0u64; b * b * b];
            for _ in 0..len {
                let x = rng.next_u64() & msk;
                let h0 = f(x);
                for i in 0..b {
                    let s = h0 ^ f(x ^ (1u64 << i));
                    let mut t = s;
                    while t != 0 {
                        let j = t.trailing_zeros() as usize;
                        ctr[i * b + j].add(s);
                        t &= t - 1;
                    }
                }
            }
            for (idx, cnt) in ctr.iter_mut().enumerate() {
                cnt.flush(&mut acc[idx * b..(idx + 1) * b], bits);
            }
            acc
        })
        .reduce_with(|mut a, o| {
            for (x, y) in a.iter_mut().zip(o) {
                *x += y;
            }
            a
        })
        .unwrap();
    let nf = n as f64;
    let mut sum2: f64 = 0.0;
    let mut cnt: f64 = 0.0;
    let mut maxz: f64 = 0.0;
    let mut worst = (0, 0, 0, 0.0);
    for i in 0..b {
        for j in 0..b {
            let pj = acc[(i * b + j) * b + j] as f64 / nf;
            for k in (j + 1)..b {
                let pk = acc[(i * b + k) * b + k] as f64 / nf;
                let pjk = acc[(i * b + j) * b + k] as f64 / nf;
                let den = (pj * (1.0 - pj) * pk * (1.0 - pk)).sqrt();
                if den <= 0.0 {
                    continue;
                }
                let corr = (pjk - pj * pk) / den;
                let z = corr * nf.sqrt();
                sum2 += z * z;
                cnt += 1.0;
                if z.abs() > maxz {
                    maxz = z.abs();
                    worst = (i, j, k, corr);
                }
            }
        }
    }
    let m = cnt.max(1.0);
    let expected = (2.0 * (2.0 * m).ln()).sqrt();
    Bic { n, rms_z: (sum2 / m).sqrt(), max_z: maxz, expected_max_z: expected, worst }
}
