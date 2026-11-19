//! Analytic (sampling-free) surrogate for the avalanche matrix of an xmx mixer.
//!
//! For each input bit j we propagate the XOR difference e_j through the mixer,
//! representing it by per-bit flip *correlations* c_i = 1 - 2·P(bit i flips),
//! under a Markov / bit-independence assumption (as in differential
//! cryptanalysis of ARX ciphers):
//!
//! * xorshift `x ^= x >> s` is linear: c'_i = c_i · c_{i+s}  (piling-up lemma).
//! * multiply by odd m, input v uniform with XOR difference Δ:
//!   1. additive difference a = (v ^ Δ) - v = Σ_i d_i·s_i·2^i, where d_i ~ Bern(p_i)
//!      (difference present) and s_i = ±1 fair (the bit of v) — exact given Δ;
//!   2. D = m·a (mod 2^n) = Σ_i d_i·s_i·(m << i);
//!   3. output XOR difference (z + D) ^ z for z = m·v, treated as uniform and
//!      independent of D (the Markov assumption); bit k flips iff D_k ^ carry_k,
//!      and the carry chain is a 2-state Markov chain — exact for known D.
//!
//!   When Δ is deterministic and sparse (round 1: weight 1 or 2) all sign
//!   patterns of D are enumerated and the carry chain is solved exactly for each.
//!   Otherwise D's bits are tracked as independent marginals, built by adding the
//!   signed shifted copies of m one at a time (carry-chain DP per copy).
//!
//! The approximation error comes from (a) z vs D dependence and (b) discarding
//! correlations between difference bits after each multiply.

use crate::mixers::{mask, Xmx};

/// Max number of deterministic difference bits whose sign patterns are enumerated.
const MAX_ENUM: usize = 12;

#[inline]
fn xorshift_c(c: &mut [f64], s: usize) {
    let n = c.len();
    // ascending: c[i+s] is still the old value when c[i] is updated
    for i in 0..n.saturating_sub(s) {
        c[i] *= c[i + s];
    }
}

// All quantities below are in the *correlation domain*: for a bit with
// P(bit = 1) = p we store c = 1 - 2p. This avoids catastrophic cancellation
// when p ≈ 1/2 (the interesting regime). Carry probability g is likewise
// stored as cg = 1 - 2g.

/// Correlations of bits of (z + D) ^ z for uniform z and a *known* D,
/// added (with `weight`) into `out`.
fn carry_flip_exact(d: u64, n: usize, out: &mut [f64], weight: f64) {
    let mut cg = 1.0f64; // no carry into bit 0
    for (k, o) in out.iter_mut().enumerate().take(n) {
        if (d >> k) & 1 == 1 {
            *o -= weight * cg;
            cg = (cg - 1.0) / 2.0;
        } else {
            *o += weight * cg;
            cg = (cg + 1.0) / 2.0;
        }
    }
}

/// Same, with D's bits independent with correlations cq_k.
/// flip_k = D_k ^ carry_k  =>  c = cq·cg ;  g' = (q+g)/2  =>  cg' = (cq+cg)/2.
fn carry_flip_indep(cq: &[f64]) -> Vec<f64> {
    let mut cg = 1.0f64;
    cq.iter()
        .map(|&q| {
            let c = q * cg;
            cg = (q + cg) / 2.0;
            c
        })
        .collect()
}

/// acc bits independent (correlations cr_k) plus a known constant t:
/// add `weight` × new bit correlations into `out`.
fn add_const_indep(cr: &[f64], t: u64, out: &mut [f64], weight: f64) {
    let mut cg = 1.0f64;
    for (k, (&r, o)) in cr.iter().zip(out.iter_mut()).enumerate() {
        let cx = r * cg; // acc ^ carry
        if (t >> k) & 1 == 1 {
            *o -= weight * cx;
            // carry' = acc | carry  =>  P(no carry') = P(!acc)P(!carry)
            cg = (1.0 + r) * (1.0 + cg) / 2.0 - 1.0;
        } else {
            *o += weight * cx;
            // carry' = acc & carry
            cg = 1.0 - (1.0 - r) * (1.0 - cg) / 2.0;
        }
    }
}

/// Propagate flip correlations through `x *= m` (mod 2^n).
fn mul_c(c: &[f64], m: u64, n: usize) -> Vec<f64> {
    let msk = mask(n as u32);
    // p_i = P(difference bit i set) = (1 - c_i)/2
    let active: Vec<usize> = (0..n).filter(|&i| c[i] < 1.0 - 2e-15).collect();
    if active.is_empty() {
        return vec![1.0; n];
    }
    let deterministic = active.iter().all(|&i| c[i] < -1.0 + 2e-12);
    if deterministic && active.len() <= MAX_ENUM {
        let mut out = vec![0.0f64; n];
        let t = active.len();
        let w = 1.0 / (1u64 << t) as f64;
        for signs in 0..(1u64 << t) {
            let mut a: u64 = 0;
            for (b, &i) in active.iter().enumerate() {
                let term = 1u64 << i;
                a = if (signs >> b) & 1 == 0 { a.wrapping_add(term) } else { a.wrapping_sub(term) };
            }
            let d = a.wrapping_mul(m) & msk;
            carry_flip_exact(d, n, &mut out, w);
        }
        out
    } else {
        // correlations of D = Σ d_i s_i (m << i), starting from D = 0 (c = 1)
        let mut r = vec![1.0f64; n];
        let mut tmp = vec![0.0f64; n];
        for &i in &active {
            let pi = (1.0 - c[i]) / 2.0;
            let tp = (m << i) & msk;
            let tm = tp.wrapping_neg() & msk;
            for (o, &rk) in tmp.iter_mut().zip(&r) {
                *o = (1.0 - pi) * rk;
            }
            add_const_indep(&r, tp, &mut tmp, pi / 2.0);
            add_const_indep(&r, tm, &mut tmp, pi / 2.0);
            std::mem::swap(&mut r, &mut tmp);
        }
        carry_flip_indep(&r)
    }
}

/// Predicted avalanche matrix: entry [j*bits + k] = predicted diff
/// (= 2·P(flip) − 1, up to sign) for input bit j, output bit k.
pub fn matrix(x: &Xmx) -> Vec<f64> {
    let n = x.bits as usize;
    let mut out = Vec::with_capacity(n * n);
    for j in 0..n {
        let mut c = vec![1.0f64; n];
        c[j] = -1.0;
        for r in 0..x.rounds() {
            xorshift_c(&mut c, x.shifts[r] as usize);
            c = mul_c(&c, x.mults[r], n);
        }
        xorshift_c(&mut c, x.shifts[x.rounds()] as usize);
        out.extend(c.iter().map(|v| -v));
    }
    out
}

/// Predicted hash-prospector score: 1000·sqrt(mean diff²).
pub fn score(x: &Xmx) -> f64 {
    let m = matrix(x);
    1000.0 * (m.iter().map(|d| d * d).sum::<f64>() / m.len() as f64).sqrt()
}

/// Spearman rank correlation.
pub fn spearman(a: &[f64], b: &[f64]) -> f64 {
    fn ranks(v: &[f64]) -> Vec<f64> {
        let mut idx: Vec<usize> = (0..v.len()).collect();
        idx.sort_by(|&i, &j| v[i].partial_cmp(&v[j]).unwrap());
        let mut r = vec![0.0; v.len()];
        let mut i = 0;
        while i < idx.len() {
            let mut j = i;
            while j + 1 < idx.len() && v[idx[j + 1]] == v[idx[i]] {
                j += 1;
            }
            let avg = (i + j) as f64 / 2.0;
            for t in i..=j {
                r[idx[t]] = avg;
            }
            i = j + 1;
        }
        r
    }
    pearson(&ranks(a), &ranks(b))
}

pub fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    sab / (saa * sbb).sqrt()
}

// ---------------------------------------------------------------------------
// Hybrid (ablation) estimators: the difference up to the LAST multiply is
// taken exactly from sampled input pairs (keeping all correlations created by
// earlier rounds); only the last multiply / final xorshift is modelled.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hybrid {
    /// exact XOR difference before last multiply; independent-bit model after
    A,
    /// exact additive difference D at last multiply, w ~ uniform ⟂ D (exact
    /// carry chain); final xorshift treated as independent bits (piling-up)
    B1,
    /// as B1 but final xorshift parity taken jointly from the carry chain
    B,
}

impl std::str::FromStr for Hybrid {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "A" | "a" => Ok(Hybrid::A),
            "B1" | "b1" => Ok(Hybrid::B1),
            "B" | "b" => Ok(Hybrid::B),
            _ => Err(format!("unknown hybrid mode {s} (A, B1, B)")),
        }
    }
}

/// Carry-chain transition for (z + D) with z uniform: P(c' = 1 | c) for bit d.
#[inline]
fn carry_step(d: u64, c: usize) -> f64 {
    match (d, c) {
        (1, 1) => 1.0,
        (1, 0) | (0, 1) => 0.5,
        _ => 0.0,
    }
}

/// Correlations of out_k = Δw_k ^ Δw_{k+s} (or Δw_k when k+s >= n) where
/// Δw = (w + D) ^ w, w uniform, D known. Joint over the carry chain.
fn pair_flip_exact(d: u64, n: usize, s: usize, out: &mut [f64]) {
    // forward marginals P(c_k = 1)
    let mut pc = vec![0.0f64; n + 1];
    for k in 0..n {
        let dk = (d >> k) & 1;
        pc[k + 1] = pc[k] * carry_step(dk, 1) + (1.0 - pc[k]) * carry_step(dk, 0);
    }
    for k in 0..n {
        let dk = ((d >> k) & 1) as usize;
        if k + s >= n {
            // P(Δw_k = 1) = P(c_k != d_k)
            let p1 = if dk == 1 { 1.0 - pc[k] } else { pc[k] };
            out[k] += 1.0 - 2.0 * p1;
            continue;
        }
        let ds = ((d >> (k + s)) & 1) as usize;
        let mut p_par1 = 0.0;
        for c0 in 0..2usize {
            let w0 = if c0 == 1 { pc[k] } else { 1.0 - pc[k] };
            if w0 == 0.0 {
                continue;
            }
            // propagate a point mass at c_k = c0 for s steps
            let mut v = [0.0f64; 2];
            v[c0] = 1.0;
            for t in k..k + s {
                let dt = (d >> t) & 1;
                let p1 = v[1] * carry_step(dt, 1) + v[0] * carry_step(dt, 0);
                v = [1.0 - p1, p1];
            }
            for (c1, &pv) in v.iter().enumerate() {
                let par = (dk ^ c0) ^ (ds ^ c1);
                if par == 1 {
                    p_par1 += w0 * pv;
                }
            }
        }
        out[k] += 1.0 - 2.0 * p_par1;
    }
}

/// Hybrid estimate of the avalanche matrix (predicted diffs, as `matrix`),
/// plus per-entry standard errors. `samples` input pairs per input bit.
pub fn hybrid_matrix(x: &Xmx, mode: Hybrid, samples: u64, seed: u64) -> (Vec<f64>, Vec<f64>) {
    use crate::rng::{stream_seed, Xoshiro256};
    use rayon::prelude::*;
    let n = x.bits as usize;
    let msk = mask(x.bits);
    let r = x.rounds();
    let m_last = x.mults[r - 1];
    let s_last = x.shifts[r] as usize;
    // state just before the last multiply
    let pre = |mut v: u64| -> u64 {
        for i in 0..r - 1 {
            v ^= v >> x.shifts[i];
            v = v.wrapping_mul(x.mults[i]) & msk;
        }
        v ^ (v >> x.shifts[r - 1])
    };
    let rows: Vec<(Vec<f64>, Vec<f64>)> = (0..n)
        .into_par_iter()
        .map(|j| {
            let mut rng = Xoshiro256::new(stream_seed(seed, j as u64));
            let mut sum = vec![0.0f64; n];
            let mut sum2 = vec![0.0f64; n];
            let mut c = vec![0.0f64; n];
            for _ in 0..samples {
                let v = rng.next_u64() & msk;
                let y = pre(v);
                let y2 = pre(v ^ (1u64 << j));
                match mode {
                    Hybrid::A => {
                        let dy = y ^ y2;
                        let cin: Vec<f64> = (0..n).map(|i| if (dy >> i) & 1 == 1 { -1.0 } else { 1.0 }).collect();
                        let mut cw = mul_c(&cin, m_last, n);
                        xorshift_c(&mut cw, s_last);
                        c.copy_from_slice(&cw);
                    }
                    Hybrid::B1 => {
                        let d = y2.wrapping_sub(y).wrapping_mul(m_last) & msk;
                        c.iter_mut().for_each(|v| *v = 0.0);
                        carry_flip_exact(d, n, &mut c, 1.0);
                        xorshift_c(&mut c, s_last);
                    }
                    Hybrid::B => {
                        let d = y2.wrapping_sub(y).wrapping_mul(m_last) & msk;
                        c.iter_mut().for_each(|v| *v = 0.0);
                        pair_flip_exact(d, n, s_last, &mut c);
                    }
                }
                for k in 0..n {
                    sum[k] += c[k];
                    sum2[k] += c[k] * c[k];
                }
            }
            let kf = samples as f64;
            let mean: Vec<f64> = sum.iter().map(|s| -s / kf).collect();
            let se: Vec<f64> = (0..n)
                .map(|k| {
                    let m = sum[k] / kf;
                    ((sum2[k] / kf - m * m).max(0.0) / kf).sqrt()
                })
                .collect();
            (mean, se)
        })
        .collect();
    let mut mean = Vec::with_capacity(n * n);
    let mut se = Vec::with_capacity(n * n);
    for (m, s) in rows {
        mean.extend(m);
        se.extend(s);
    }
    (mean, se)
}

/// Debiased score from a noisy matrix estimate: 1000·sqrt(mean(m² − se²)).
pub fn debiased_score(mean: &[f64], se: &[f64]) -> f64 {
    let ms = mean.iter().zip(se).map(|(m, s)| m * m - s * s).sum::<f64>() / mean.len() as f64;
    1000.0 * ms.max(0.0).sqrt()
}
