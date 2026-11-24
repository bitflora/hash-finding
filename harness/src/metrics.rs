//! Candidate 64-bit scoring metrics (FINDINGS F27/F28), usable at any width so
//! they can be validated at 32 bits against exact bias and PractRand.
//!
//! Every avalanche-style metric is in noise units: the mean over the table of
//! d^2 * n_row, with d = (flips - n/2)/(n/2). An ideal random function gives
//! ~1 (sd ~ sqrt(2/cells)); a real defect of mean square ms adds ms * n, so
//! defects grow linearly with n.
//!
//!   ctr   counter 0..2^k, single-bit input flips (pairs canonicalised)
//!   rev   bit-reversed counter (PractRand stdin mode 2 analogue)
//!   hi    counter in the high half (rotate by bits/2)
//!   adj2  random x, adjacent two-bit flips 3<<i
//!   add   random x, additive differences x vs x + 2^i
//!   sac   plain avalanche on random x, for reference
//!   two   every two-bit flip {i, j} with j - i != s0 (those collapse to one
//!         high bit after the first xorshift for every constant); mean, worst
//!   BIC   rms z and worst |corr| (random x)
//!   sparse  exact screen: min NAF density of k*m0 mod 2^j (k in {1,3},
//!         j >= 12); low values predict a large adj2 defect (F27)

use crate::extra::{bic_sampled, Bic};
use crate::mixers::{mask, Xmx};
use crate::rng::{stream_seed, Xoshiro256};
use rayon::prelude::*;

/// Noise units for a table of `rows` rows; `pairs(row, emit)` yields input pairs.
pub fn units<F, P>(f: &F, bits: u32, rows: usize, pairs: P) -> f64
where
    F: Fn(u64) -> u64 + Sync,
    P: Fn(usize, &mut dyn FnMut(u64, u64)) + Sync,
{
    let b = bits as usize;
    let tot: f64 = (0..rows)
        .into_par_iter()
        .map(|r| {
            let mut cnt = vec![0u64; b];
            let mut n = 0u64;
            pairs(r, &mut |x, y| {
                let mut s = f(x) ^ f(y);
                n += 1;
                while s != 0 {
                    cnt[s.trailing_zeros() as usize] += 1;
                    s &= s - 1;
                }
            });
            let nf = n as f64;
            cnt.iter()
                .map(|&c| {
                    let d = (c as f64 - nf / 2.0) / (nf / 2.0);
                    d * d * nf
                })
                .sum::<f64>()
        })
        .sum();
    tot / (rows * b) as f64
}

/// Counter 0..2^k mapped through a bit permutation `t`; `tinv_bit(j)` is the
/// counter bit that becomes word bit j. Flips word bit j; a pair whose partner
/// is also in the set is counted once.
pub fn counter_units<F: Fn(u64) -> u64 + Sync>(
    f: &F,
    bits: u32,
    k: u32,
    t: &(dyn Fn(u64) -> u64 + Sync),
    tinv_bit: &(dyn Fn(u32) -> u32 + Sync),
) -> f64 {
    units(f, bits, bits as usize, |j, emit| {
        let e = 1u64 << j;
        let p = tinv_bit(j as u32);
        for c in 0..(1u64 << k) {
            if p < k && (c >> p) & 1 == 1 {
                continue;
            }
            let x = t(c) & !e;
            emit(x, x | e);
        }
    })
}

/// `rows` rows of n random inputs x, paired with `diff(row, x)`.
pub fn random_units<F: Fn(u64) -> u64 + Sync>(
    f: &F,
    bits: u32,
    n: u64,
    seed: u64,
    rows: usize,
    diff: &(dyn Fn(usize, u64) -> u64 + Sync),
) -> f64 {
    let m = mask(bits);
    units(f, bits, rows, |r, emit| {
        let mut rng = Xoshiro256::new(stream_seed(seed, r as u64));
        for _ in 0..n {
            let x = rng.next_u64() & m;
            emit(x, diff(r, x) & m);
        }
    })
}

/// Non-adjacent-form weight (number of nonzero signed digits).
pub fn naf_weight(mut x: u128) -> u32 {
    let mut w = 0;
    while x != 0 {
        if x & 1 == 1 {
            if x & 3 == 1 {
                x -= 1;
            } else {
                x += 1;
            }
            w += 1;
        }
        x >>= 1;
    }
    w
}

/// Exact screen: min over k in `ks` and j in jmin..=bits of
/// naf_weight(min(v, 2^j - v)) / j with v = k*m mod 2^j. Returns (density, k, j).
pub fn sparse_multiple(m: u64, bits: u32, ks: &[u64], jmin: u32) -> (f64, u64, u32) {
    let mut best = (f64::INFINITY, 0, 0);
    for &k in ks {
        for j in jmin..=bits {
            let modj = 1u128 << j;
            let v = (k as u128 * m as u128) % modj;
            let v = v.min(modj - v);
            let d = naf_weight(v) as f64 / j as f64;
            if d < best.0 {
                best = (d, k, j);
            }
        }
    }
    best
}

#[derive(Clone, Copy, Debug)]
pub struct MetricsCfg {
    /// log2 of the counter length for ctr/rev/hi.
    pub kctr: u32,
    pub nrand: u64,
    pub ntwo: u64,
    pub nbic: u64,
    pub seed: u64,
}

impl Default for MetricsCfg {
    fn default() -> Self {
        MetricsCfg { kctr: 24, nrand: 1 << 22, ntwo: 1 << 18, nbic: 1 << 20, seed: 1 }
    }
}

#[derive(Clone, Debug)]
pub struct Scorecard {
    pub ctr: f64,
    pub rev: f64,
    pub hi: f64,
    pub adj2: f64,
    pub add: f64,
    pub sac: f64,
    pub bic: Bic,
    pub two: f64,
    pub two_worst: f64,
    pub two_worst_pair: (usize, usize),
    pub sparse: f64,
}

/// The two-bit flip pairs scored by `two`: all {i, j}, j - i != s0.
pub fn two_bit_pairs(bits: u32, s0: u32) -> Vec<(usize, usize)> {
    (0..bits as usize)
        .flat_map(|i| ((i + 1)..bits as usize).map(move |j| (i, j)))
        .filter(|&(i, j)| j - i != s0 as usize)
        .collect()
}

/// Per-pair two-bit noise units, in `two_bit_pairs` order.
pub fn two_bit_units<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32, s0: u32, n: u64, seed: u64) -> Vec<f64> {
    two_bit_pairs(bits, s0)
        .iter()
        .map(|&(i, j)| {
            let d = (1u64 << i) | (1u64 << j);
            random_units(f, bits, n, seed ^ 0x2b17 ^ ((i * 64 + j) as u64), 1, &|_, v| v ^ d)
        })
        .collect()
}

pub fn scorecard_fn<F: Fn(u64) -> u64 + Sync>(f: &F, x: &Xmx, c: &MetricsCfg) -> Scorecard {
    let bits = x.bits;
    let rev = move |v: u64| v.reverse_bits() >> (64 - bits);
    let rot = move |v: u64| ((v << (bits / 2)) | (v >> (bits - bits / 2))) & mask(bits);
    let ctr = counter_units(f, bits, c.kctr, &|v| v, &|j| j);
    let rv = counter_units(f, bits, c.kctr, &rev, &|j| bits - 1 - j);
    let hi = counter_units(f, bits, c.kctr, &rot, &|j| (j + bits - bits / 2) % bits);
    let adj2 = random_units(f, bits, c.nrand, c.seed, bits as usize - 1, &|r, v| v ^ (3u64 << r));
    let add = random_units(f, bits, c.nrand, c.seed ^ 0xadd, bits as usize, &|r, v| v.wrapping_add(1u64 << r));
    let sac = random_units(f, bits, c.nrand, c.seed ^ 0x5ac, bits as usize, &|r, v| v ^ (1u64 << r));
    let s0 = x.shifts[0];
    let per = two_bit_units(f, bits, s0, c.ntwo, c.seed);
    let pairs = two_bit_pairs(bits, s0);
    let two = per.iter().sum::<f64>() / per.len() as f64;
    let (wi, wv) = per.iter().enumerate().fold((0, 0.0), |a, (k, &v)| if v > a.1 { (k, v) } else { a });
    let bic = bic_sampled(f, bits, c.nbic, c.seed ^ 0xb1c);
    let sparse = sparse_multiple(x.mults[0], bits, &[1, 3], 12.min(bits)).0;
    Scorecard { ctr, rev: rv, hi, adj2, add, sac, bic, two, two_worst: wv, two_worst_pair: pairs[wi], sparse }
}

pub fn scorecard(x: &Xmx, c: &MetricsCfg) -> Scorecard {
    crate::with_mixer!(x, f => scorecard_fn(&f, x, c))
}

pub const CSV_HEADER: &str =
    "spec,ctr,rev,hi,adj2,add,sac,bic_rmsz,bic_worst_corr,bic_worst,two,two_worst,two_worst_pair,sparse";

pub fn csv_row(x: &Xmx, s: &Scorecard) -> String {
    let (i, j, k, c) = s.bic.worst;
    format!(
        "\"{x}\",{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{i}->{j}/{k},{:.4},{:.2},{}-{},{:.4}",
        s.ctr,
        s.rev,
        s.hi,
        s.adj2,
        s.add,
        s.sac,
        s.bic.rms_z,
        c.abs(),
        s.two,
        s.two_worst,
        s.two_worst_pair.0,
        s.two_worst_pair.1,
        s.sparse
    )
}
