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
        let half = (self.n / 2) as f64;
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

    /// Mean squared diff minus the expected sampling-noise contribution (1/n)
    /// for *randomly sampled* inputs: an unbiased estimate of the exact ms.
    pub fn ms_debiased(&self) -> f64 {
        self.ms() - 1.0 / self.n as f64
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

const SAMPLE_CHUNK: u64 = 1 << 14;

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
    /// Noise-corrected score: 1000*sqrt(max(0, ms - 1/n)); equals `score` if exact.
    pub debiased: f64,
    /// Conservative standard error of `debiased` (0 if exact).
    pub se: f64,
    /// Largest |diff| over the matrix.
    pub max_abs: f64,
}

pub fn estimate_exact<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32) -> Estimate {
    let c = exact(f, bits);
    let s = c.score();
    Estimate { exact: true, n: c.n, score: s, debiased: s, se: 0.0, max_abs: c.max_abs() }
}

/// Sampled estimate; `batches` independent sub-samples give the standard error.
pub fn estimate_sampled<F: Fn(u64) -> u64 + Sync>(
    f: &F,
    bits: u32,
    n: u64,
    seed: u64,
    batches: u64,
) -> Estimate {
    let per = (n / batches).max(1);
    let parts: Vec<Counts> =
        (0..batches).map(|b| sampled(f, bits, per, stream_seed(seed ^ 0xb47c4, b))).collect();
    let batch_ms: Vec<f64> = parts.iter().map(|c| c.ms_debiased()).collect();
    let total = parts.into_iter().reduce(Counts::merge).unwrap();
    let d = total.ms_debiased();
    let k = batch_ms.len() as f64;
    let mean = batch_ms.iter().sum::<f64>() / k;
    let var = batch_ms.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (k - 1.0).max(1.0);
    let se_ms = (var / k).sqrt();
    let deb = 1000.0 * d.max(0.0).sqrt();
    // delta method; if d is ~0, report the ms-scale bound instead
    let se = if d > 0.0 {
        1000.0 * se_ms / (2.0 * d.sqrt())
    } else {
        1000.0 * se_ms.sqrt()
    };
    Estimate { exact: false, n: total.n, score: total.score(), debiased: deb, se, max_abs: total.max_abs() }
}
