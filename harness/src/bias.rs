//! Avalanche (SAC) counting and hash-prospector-compatible bias scores.

use crate::mixers::mask;
use crate::rng::{stream_seed, Xoshiro256};
use rayon::prelude::*;

/// Bit-sliced ("vertical") counter: adds up to 2^P - 1 words, counting each
/// bit position independently, at ~2 word-ops per add on average.
#[derive(Clone, Copy)]
pub struct VCounter<const P: usize> {
    planes: [u64; P],
}

impl<const P: usize> VCounter<P> {
    pub const CAPACITY: u32 = ((1u64 << P) - 1) as u32;

    pub fn new() -> Self {
        VCounter { planes: [0; P] }
    }

    #[inline(always)]
    pub fn add(&mut self, w: u64) {
        let mut c = w;
        for p in self.planes.iter_mut() {
            let t = *p & c;
            *p ^= c;
            c = t;
            if c == 0 {
                break;
            }
        }
    }

    /// Add per-bit totals into `acc[0..bits]` and reset.
    pub fn flush(&mut self, acc: &mut [u64], bits: u32) {
        if self.planes.iter().all(|&p| p == 0) {
            return;
        }
        for k in 0..bits as usize {
            let mut v = 0u64;
            for (i, p) in self.planes.iter().enumerate() {
                v |= ((p >> k) & 1) << i;
            }
            acc[k] += v;
        }
        self.planes = [0; P];
    }
}

impl<const P: usize> Default for VCounter<P> {
    fn default() -> Self {
        Self::new()
    }
}

/// SAC count matrix: bins[j * bits + k] = #inputs where flipping input bit j
/// flipped output bit k; `n` = number of inputs.
#[derive(Clone, Debug)]
pub struct Counts {
    pub bits: u32,
    pub n: u64,
    pub bins: Vec<u64>,
}

impl Counts {
    pub fn new(bits: u32) -> Self {
        Counts { bits, n: 0, bins: vec![0; (bits * bits) as usize] }
    }

    pub fn merge(mut self, o: Counts) -> Counts {
        self.n += o.n;
        for (a, b) in self.bins.iter_mut().zip(o.bins) {
            *a += b;
        }
        self
    }

    /// diff = (count - n/2) / (n/2), as in prospector.c.
    pub fn diffs(&self) -> impl Iterator<Item = f64> + '_ {
        assert!(self.n > 0, "counts require at least one input");
        let half = self.n as f64 / 2.0;
        self.bins.iter().map(move |&c| (c as f64 - half) / half)
    }

    /// Mean squared diff.
    pub fn ms(&self) -> f64 {
        let m = self.bins.len() as f64;
        self.diffs().map(|d| d * d).sum::<f64>() / m
    }

    /// hash-prospector score: 1000 * sqrt(mean diff^2).
    pub fn score(&self) -> f64 {
        1000.0 * self.ms().sqrt()
    }

    /// Unbiased mean-square estimate for i.i.d. sampled inputs (may be negative).
    /// E[raw_ms] = true_ms + (1 - true_ms)/n.
    pub fn ms_debiased(&self) -> f64 {
        assert!(self.n > 1, "debiased ms requires at least two inputs");
        let n = self.n as f64;
        (n * self.ms() - 1.0) / (n - 1.0)
    }

    pub fn max_abs(&self) -> f64 {
        self.diffs().fold(0.0, |a, d| a.max(d.abs()))
    }
}

/// SPREAD[b] has byte i = bit i of b (so adding it counts 8 bits at once in
/// 8 byte lanes; lanes overflow after 255 adds).
const SPREAD: [u64; 256] = {
    let mut t = [0u64; 256];
    let mut b = 0;
    while b < 256 {
        let mut v = 0u64;
        let mut i = 0;
        while i < 8 {
            v |= (((b >> i) & 1) as u64) << (8 * i);
            i += 1;
        }
        t[b] = v;
        b += 1;
    }
    t
};

/// Serial accumulator for one thread (branch-free byte-lane counting).
pub struct Acc {
    bits: u32,
    nbytes: usize,
    /// lanes[j * 8 + byte]: 8 byte-lane counters for output bits 8*byte..8*byte+7
    lanes: Vec<u64>,
    pending: u32,
    counts: Counts,
}

impl Acc {
    pub fn new(bits: u32) -> Self {
        Acc {
            bits,
            nbytes: bits.div_ceil(8) as usize,
            lanes: vec![0; bits as usize * 8],
            pending: 0,
            counts: Counts::new(bits),
        }
    }

    #[inline(always)]
    pub fn add_input<F: Fn(u64) -> u64>(&mut self, f: &F, x: u64) {
        let h0 = f(x);
        let nb = self.nbytes;
        for j in 0..self.bits as usize {
            let s = h0 ^ f(x ^ (1u64 << j));
            let l = &mut self.lanes[j * 8..j * 8 + 8];
            for (b, lane) in l.iter_mut().enumerate().take(nb) {
                *lane += SPREAD[((s >> (8 * b)) & 0xff) as usize];
            }
        }
        self.counts.n += 1;
        self.pending += 1;
        if self.pending == 255 {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let b = self.bits as usize;
        for j in 0..b {
            for k in 0..b {
                let lane = self.lanes[j * 8 + k / 8];
                self.counts.bins[j * b + k] += (lane >> (8 * (k % 8))) & 0xff;
            }
            for l in &mut self.lanes[j * 8..j * 8 + 8] {
                *l = 0;
            }
        }
        self.pending = 0;
    }

    pub fn finish(mut self) -> Counts {
        self.flush();
        self.counts
    }
}

/// Exact counts over all 2^bits inputs, single-threaded (for small widths,
/// where you parallelise over configurations instead).
pub fn exact_serial<F: Fn(u64) -> u64>(f: &F, bits: u32) -> Counts {
    assert!(bits <= 32);
    let mut a = Acc::new(bits);
    for x in 0..(1u64 << bits) {
        a.add_input(f, x);
    }
    a.finish()
}

/// Exact counts over all 2^bits inputs, multi-threaded.
pub fn exact<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32) -> Counts {
    assert!(bits <= 32);
    let total = 1u64 << bits;
    let chunk = (total / 4096).max(1 << 12).min(total);
    let nchunks = total / chunk;
    (0..nchunks)
        .into_par_iter()
        .map(|c| {
            let mut a = Acc::new(bits);
            for x in c * chunk..(c + 1) * chunk {
                a.add_input(f, x);
            }
            a.finish()
        })
        .reduce_with(Counts::merge)
        .unwrap()
}

pub const SAMPLE_CHUNK: u64 = 1 << 14;

/// Counts over `n` uniformly random inputs, serial. Deterministic in `seed`.
pub fn sampled_serial<F: Fn(u64) -> u64>(f: &F, bits: u32, n: u64, seed: u64) -> Counts {
    let msk = mask(bits);
    let mut rng = Xoshiro256::new(seed);
    let mut a = Acc::new(bits);
    for _ in 0..n {
        a.add_input(f, rng.next_u64() & msk);
    }
    a.finish()
}

/// Counts over `n` uniformly random inputs, multi-threaded. Deterministic in
/// `seed` (independent of thread count).
pub fn sampled<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32, n: u64, seed: u64) -> Counts {
    let nchunks = n.div_ceil(SAMPLE_CHUNK);
    (0..nchunks)
        .into_par_iter()
        .map(|c| {
            let len = SAMPLE_CHUNK.min(n - c * SAMPLE_CHUNK);
            sampled_serial(f, bits, len, stream_seed(seed, c))
        })
        .reduce_with(Counts::merge)
        .unwrap_or_else(|| Counts::new(bits))
}

/// A bias estimate with uncertainty.
#[derive(Clone, Debug)]
pub struct Estimate {
    pub exact: bool,
    pub n: u64,
    /// Raw hash-prospector score (includes sampling noise if sampled).
    pub score: f64,
    /// Display score: 1000*sqrt(max(0, ms)); equals `score` if exact.
    pub debiased: f64,
    /// Signed unbiased mean-square estimate, or exact mean square.
    pub ms: f64,
    /// Approximate standard error of the pooled mean-square estimate.
    /// Uses a Gaussian batch approximation with covariance between entries.
    pub se_ms: f64,
    /// Largest |diff| over the matrix.
    pub max_abs: f64,
}

pub fn estimate_exact<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32) -> Estimate {
    let c = exact(f, bits);
    let s = c.score();
    Estimate { exact: true, n: c.n, score: s, debiased: s, ms: c.ms(), se_ms: 0.0, max_abs: c.max_abs() }
}

/// Approximate variance of the pooled squared mean, retaining correlations
/// between SAC entries. Batch means are approximately Gaussian. Weighted
/// residuals estimate per-input covariance S with df = batches - 1.
/// Var(ms) ~= (4 mu' Sigma mu / n + 2 tr(Sigma^2)/n^2) / entries^2.
/// Wishart corrections remove finite-batch noise from tr(S^2) and mu'Smu.
/// This is not an always-valid confidence bound, especially for tiny batches.
fn pooled_se_ms(parts: &[Counts], total: &Counts) -> f64 {
    let n = total.n as f64;
    let df = (parts.len() - 1) as f64;
    let mean: Vec<f64> = total.diffs().collect();
    let residuals: Vec<Vec<f64>> = parts.iter().map(|p| {
        let scale = (p.n as f64).sqrt();
        p.diffs().zip(&mean).map(|(v, m)| scale * (v - m)).collect()
    }).collect();
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let trace = residuals.iter().map(|r| dot(r, r)).sum::<f64>() / df;
    let trace_square = residuals.iter().flat_map(|a| residuals.iter().map(move |b| dot(a, b).powi(2))).sum::<f64>() / df.powi(2);
    let sigma_square = (df * (df * trace_square - trace * trace) / ((df - 1.0) * (df + 2.0))).max(0.0);
    let mean_s_mean = residuals.iter().map(|r| dot(r, &mean).powi(2)).sum::<f64>() / df;
    let signal = (mean_s_mean - sigma_square / n).max(0.0);
    let variance = (4.0 * signal / n + 2.0 * sigma_square / n.powi(2)) / (mean.len() as f64).powi(2);
    variance.sqrt() * n / (n - 1.0)
}

/// Pooled point estimate and covariance-aware approximate uncertainty.
/// Uses exactly n inputs, including any remainder when splitting batches.
pub fn estimate_sampled<F: Fn(u64) -> u64 + Sync>(
    f: &F,
    bits: u32,
    n: u64,
    seed: u64,
    batches: u64,
) -> Estimate {
    assert!(batches >= 4 && n >= 2 * batches, "need >=4 batches and >=2 inputs per batch");
    let parts: Vec<Counts> = (0..batches).map(|b| {
        let len = n / batches + u64::from(b < n % batches);
        sampled(f, bits, len, stream_seed(seed ^ 0xb47c4, b))
    }).collect();
    let total = parts.iter().cloned().reduce(Counts::merge).unwrap();
    let ms = total.ms_debiased();
    let se_ms = pooled_se_ms(&parts, &total);
    Estimate {
        exact: false, n: total.n, score: total.score(),
        debiased: 1000.0 * ms.max(0.0).sqrt(), ms, se_ms,
        max_abs: total.max_abs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_counts_and_exact_unbiasing() {
        let c = Counts { bits: 1, n: 3, bins: vec![1] };
        assert!((c.diffs().next().unwrap() + 1.0 / 3.0).abs() < 1e-14);
        // Enumerate all Bernoulli samples: E[estimate] = (2p-1)^2.
        for p in [0.25f64, 0.5, 0.75] {
            let mut expectation = 0.0;
            for sample in 0u32..8 {
                let ones = sample.count_ones();
                let c = Counts { bits: 1, n: 3, bins: vec![ones as u64] };
                expectation += p.powi(ones as i32) * (1.0-p).powi(3-ones as i32) * c.ms_debiased();
            }
            assert!((expectation - (2.0*p-1.0).powi(2)).abs() < 1e-14);
        }
    }

    #[test]
    fn pooled_uncertainty_tracks_correlated_replicates() {
        // Every table entry shares the same Bernoulli observation. An entry-
        // independence formula would underestimate variance by a factor of 16.
        for p in [0.5, 0.75] {
            let mut rng = Xoshiro256::new(123);
            let mut estimates = Vec::new();
            let mut predicted = 0.0;
            for _ in 0..400 {
                let parts: Vec<Counts> = (0..16).map(|b| {
                    let n = 128 + u64::from(b < 3);
                    let ones = (0..n).filter(|_| rng.unit() < p).count() as u64;
                    Counts { bits: 4, n, bins: vec![ones; 16] }
                }).collect();
                let total = parts.iter().cloned().reduce(Counts::merge).unwrap();
                estimates.push(total.ms_debiased());
                predicted += pooled_se_ms(&parts, &total).powi(2);
            }
            let mean = estimates.iter().sum::<f64>() / estimates.len() as f64;
            let variance = estimates.iter().map(|v| (v-mean).powi(2)).sum::<f64>() / (estimates.len()-1) as f64;
            let ratio = predicted / estimates.len() as f64 / variance;
            // At the null, clamping the signal component is deliberately
            // conservative. Away from the null the variance should calibrate.
            let upper = if p == 0.5 { 3.0 } else { 1.8 };
            assert!((0.65..upper).contains(&ratio), "p={p} variance ratio={ratio}");
        }
    }

    #[test]
    fn sampled_estimate_uses_requested_remainder() {
        let e = estimate_sampled(&|x| x, 4, 103, 1, 16);
        assert_eq!(e.n, 103);
        assert_eq!(e.ms, 1.0);
        assert_eq!(e.se_ms, 0.0);
    }
}
