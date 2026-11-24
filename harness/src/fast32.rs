//! Fast exact avalanche counting for xmx mixers of width 3..=32.
//! Evaluates 8 consecutive inputs at once in u32 lanes (auto-vectorises to
//! AVX2 with target-cpu=native) and counts with branch-free bit-sliced
//! counters. The default path compresses 16 consecutive 8-input groups per
//! input bit with a Harley-Seal carry-save adder tree (~5 ops per word) before
//! rippling the weight-16 carries into 8 planes; the original ripple counter
//! (`Acc32`, ~25 ops per word) is kept as a bit-identical reference.

use crate::bias::Counts;
use crate::mixers::{mask, Xmx};
use rayon::prelude::*;

const L: usize = 8;
const P: usize = 8;

#[derive(Clone, Copy)]
struct K32 {
    mask: u32,
    s: [u32; 5],
    m: [u32; 4],
}

impl K32 {
    fn from(x: &Xmx) -> K32 {
        let k = x.kernel();
        let mut m = [0u32; 4];
        for i in 0..4 {
            m[i] = k.m[i] as u32;
        }
        K32 { mask: mask(x.bits) as u32, s: k.s, m }
    }
}

/// Rounds after the first xorshift: y is x ^ (x >> s0). FULL skips the
/// width mask (bits == 32, mask all ones).
#[inline(always)]
fn mix8_tail<const R: usize, const FULL: bool>(k: &K32, mut x: [u32; L]) -> [u32; L] {
    for i in 0..R {
        let m = k.m[i];
        if i > 0 {
            let s = k.s[i];
            for v in x.iter_mut() {
                *v ^= *v >> s;
            }
        }
        for v in x.iter_mut() {
            *v = if FULL { v.wrapping_mul(m) } else { v.wrapping_mul(m) & k.mask };
        }
    }
    let s = k.s[R];
    for v in x.iter_mut() {
        *v ^= *v >> s;
    }
    x
}

#[inline(always)]
fn mix8<const R: usize>(k: &K32, mut x: [u32; L]) -> [u32; L] {
    for i in 0..R {
        let s = k.s[i];
        let m = k.m[i];
        for v in x.iter_mut() {
            *v ^= *v >> s;
            *v = v.wrapping_mul(m) & k.mask;
        }
    }
    let s = k.s[R];
    for v in x.iter_mut() {
        *v ^= *v >> s;
    }
    x
}

struct Acc32 {
    bits: usize,
    planes: Vec<[[u32; L]; P]>,
    pending: u32,
    counts: Counts,
}

impl Acc32 {
    fn new(bits: u32) -> Self {
        Acc32 {
            bits: bits as usize,
            planes: vec![[[0; L]; P]; bits as usize],
            pending: 0,
            counts: Counts::new(bits),
        }
    }

    #[inline(always)]
    fn add8<const R: usize>(&mut self, k: &K32, x: [u32; L]) {
        let h0 = mix8::<R>(k, x);
        for j in 0..self.bits {
            let mut y = x;
            for v in y.iter_mut() {
                *v ^= 1 << j;
            }
            let h1 = mix8::<R>(k, y);
            let mut c = [0u32; L];
            for l in 0..L {
                c[l] = h0[l] ^ h1[l];
            }
            let p = &mut self.planes[j];
            for plane in p.iter_mut() {
                for l in 0..L {
                    let t = plane[l] & c[l];
                    plane[l] ^= c[l];
                    c[l] = t;
                }
            }
        }
        self.counts.n += L as u64;
        self.pending += 1;
        if self.pending == 255 {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let b = self.bits;
        for j in 0..b {
            let row = &mut self.counts.bins[j * b..(j + 1) * b];
            for (q, plane) in self.planes[j].iter_mut().enumerate() {
                for w in plane.iter_mut() {
                    let mut v = *w;
                    while v != 0 {
                        row[v.trailing_zeros() as usize] += 1u64 << q;
                        v &= v - 1;
                    }
                    *w = 0;
                }
            }
        }
        self.pending = 0;
    }

    fn finish(mut self) -> Counts {
        self.flush();
        self.counts
    }
}

type W = [u32; L];

#[inline(always)]
fn csa(a: W, b: W, c: W) -> (W, W) {
    let mut h = [0u32; L];
    let mut l = [0u32; L];
    for i in 0..L {
        let u = a[i] ^ b[i];
        h[i] = (a[i] & b[i]) | (u & c[i]);
        l[i] = u ^ c[i];
    }
    (h, l)
}

/// Groups of 8 inputs per Harley-Seal block.
const G: usize = 16;

/// Harley-Seal accumulator: per input bit j, persistent ones/twos/fours/eights
/// words plus P ripple planes counting weight-16 carries.
struct AccHs {
    bits: usize,
    /// [ones, twos, fours, eights] per input bit
    low: Vec<[W; 4]>,
    planes: Vec<[W; P]>,
    pending: u32,
    counts: Counts,
}

impl AccHs {
    fn new(bits: u32) -> Self {
        AccHs {
            bits: bits as usize,
            low: vec![[[0; L]; 4]; bits as usize],
            planes: vec![[[0; L]; P]; bits as usize],
            pending: 0,
            counts: Counts::new(bits),
        }
    }

    /// 16 groups of 8 inputs.
    #[inline(always)]
    fn add_block<const R: usize, const FULL: bool>(&mut self, k: &K32, xs: &[W; G]) {
        // The first xorshift is linear: xs(x ^ e_j) = xs(x) ^ xs(e_j).
        let s0 = k.s[0];
        let mut ys = [[0u32; L]; G];
        let mut h0 = [[0u32; L]; G];
        for g in 0..G {
            for l in 0..L {
                let x = xs[g][l];
                ys[g][l] = x ^ (x >> s0);
            }
            h0[g] = mix8_tail::<R, FULL>(k, ys[g]);
        }
        for j in 0..self.bits {
            let e = (1u32 << j) ^ ((1u32 << j) >> s0);
            let mut d = [[0u32; L]; G];
            for g in 0..G {
                let mut y = ys[g];
                for v in y.iter_mut() {
                    *v ^= e;
                }
                let h1 = mix8_tail::<R, FULL>(k, y);
                for l in 0..L {
                    d[g][l] = h0[g][l] ^ h1[l];
                }
            }
            let [mut ones, mut twos, mut fours, mut eights] = self.low[j];
            let (ta, o) = csa(ones, d[0], d[1]); ones = o;
            let (tb, o) = csa(ones, d[2], d[3]); ones = o;
            let (fa, t) = csa(twos, ta, tb); twos = t;
            let (ta, o) = csa(ones, d[4], d[5]); ones = o;
            let (tb, o) = csa(ones, d[6], d[7]); ones = o;
            let (fb, t) = csa(twos, ta, tb); twos = t;
            let (ea, f) = csa(fours, fa, fb); fours = f;
            let (ta, o) = csa(ones, d[8], d[9]); ones = o;
            let (tb, o) = csa(ones, d[10], d[11]); ones = o;
            let (fa, t) = csa(twos, ta, tb); twos = t;
            let (ta, o) = csa(ones, d[12], d[13]); ones = o;
            let (tb, o) = csa(ones, d[14], d[15]); ones = o;
            let (fb, t) = csa(twos, ta, tb); twos = t;
            let (eb, f) = csa(fours, fa, fb); fours = f;
            let (mut c, e) = csa(eights, ea, eb); eights = e;
            self.low[j] = [ones, twos, fours, eights];
            for plane in self.planes[j].iter_mut() {
                for l in 0..L {
                    let t = plane[l] & c[l];
                    plane[l] ^= c[l];
                    c[l] = t;
                }
            }
        }
        self.counts.n += (G * L) as u64;
        self.pending += 1;
        if self.pending == 255 {
            self.flush_planes();
        }
    }

    fn flush_planes(&mut self) {
        let b = self.bits;
        for j in 0..b {
            let row = &mut self.counts.bins[j * b..(j + 1) * b];
            for (q, plane) in self.planes[j].iter_mut().enumerate() {
                for w in plane.iter_mut() {
                    let mut v = *w;
                    while v != 0 {
                        row[v.trailing_zeros() as usize] += 16u64 << q;
                        v &= v - 1;
                    }
                    *w = 0;
                }
            }
        }
        self.pending = 0;
    }

    fn finish(mut self) -> Counts {
        self.flush_planes();
        let b = self.bits;
        for j in 0..b {
            let row = &mut self.counts.bins[j * b..(j + 1) * b];
            for (q, word) in self.low[j].iter().enumerate() {
                for &w in word.iter() {
                    let mut v = w;
                    while v != 0 {
                        row[v.trailing_zeros() as usize] += 1u64 << q;
                        v &= v - 1;
                    }
                }
            }
        }
        self.counts
    }
}

/// Counts over inputs [lo, hi); full 128-input blocks use Harley-Seal, the
/// remainder (and widths below 7 bits) the ripple counter.
fn run_range_hs<const R: usize>(k: &K32, bits: u32, lo: u64, hi: u64) -> Counts {
    const BLK: u64 = (G * L) as u64;
    let mut a = AccHs::new(bits);
    let mut x0 = lo;
    let mut xs = [[0u32; L]; G];
    while x0 + BLK <= hi {
        for (i, v) in xs.iter_mut().flatten().enumerate() {
            *v = (x0 as u32).wrapping_add(i as u32);
        }
        if bits == 32 { a.add_block::<R, true>(k, &xs) } else { a.add_block::<R, false>(k, &xs) }
        x0 += BLK;
    }
    let c = a.finish();
    if x0 < hi { c.merge(run_range::<R>(k, bits, x0, hi)) } else { c }
}

fn run_range<const R: usize>(k: &K32, bits: u32, lo: u64, hi: u64) -> Counts {
    let mut a = Acc32::new(bits);
    let mut x0 = lo;
    while x0 + L as u64 <= hi {
        let b = x0 as u32;
        let x = [b, b + 1, b + 2, b + 3, b + 4, b + 5, b + 6, b + 7];
        a.add8::<R>(k, x);
        x0 += L as u64;
    }
    let c = a.finish();
    if x0 == hi {
        return c;
    }
    let mut tail = [0u32; L];
    let len = (hi - x0) as usize;
    for (i, v) in tail[..len].iter_mut().enumerate() {
        *v = (x0 + i as u64) as u32;
    }
    c.merge(run_inputs::<R>(k, bits, &tail[..len]))
}

fn exact_r<const R: usize>(k: &K32, bits: u32, parallel: bool, hs: bool) -> Counts {
    let total = 1u64 << bits;
    let run = |lo: u64, hi: u64| if hs { run_range_hs::<R>(k, bits, lo, hi) } else { run_range::<R>(k, bits, lo, hi) };
    if !parallel || total <= 1 << 16 {
        return run(0, total);
    }
    let chunk = (total / 2048).max(1 << 14);
    (0..total / chunk)
        .into_par_iter()
        .map(|c| run(c * chunk, (c + 1) * chunk))
        .reduce_with(Counts::merge)
        .unwrap()
}

fn dispatch(x: &Xmx, f: impl Fn(&K32, usize) -> Counts) -> Counts {
    assert!((3..=32).contains(&x.bits), "fast32 needs 3..=32 bits");
    let k = K32::from(x);
    match x.rounds() {
        r @ 1..=4 => f(&k, r),
        r => panic!("unsupported rounds {r}"),
    }
}

macro_rules! by_rounds {
    ($r:expr, $f:ident, $($a:expr),*) => {
        match $r { 1 => $f::<1>($($a),*), 2 => $f::<2>($($a),*), 3 => $f::<3>($($a),*), _ => $f::<4>($($a),*) }
    };
}

/// Exact counts for an xmx mixer with 3 <= bits <= 32 (Harley-Seal path).
pub fn exact_xmx(x: &Xmx, parallel: bool) -> Counts {
    dispatch(x, |k, r| by_rounds!(r, exact_r, k, x.bits, parallel, true))
}

/// Same counts via the original ripple counter (reference for tests/benchmarks).
pub fn exact_xmx_ripple(x: &Xmx, parallel: bool) -> Counts {
    dispatch(x, |k, r| by_rounds!(r, exact_r, k, x.bits, parallel, false))
}

/// Single-threaded counts over inputs [lo, hi); `hs` selects the counter.
pub fn counts_range(x: &Xmx, lo: u64, hi: u64, hs: bool) -> Counts {
    dispatch(x, |k, r| {
        if hs { by_rounds!(r, run_range_hs, k, x.bits, lo, hi) } else { by_rounds!(r, run_range, k, x.bits, lo, hi) }
    })
}

/// Counts over `xs` (any inputs < 2^bits), single-threaded; Harley-Seal for
/// full 128-input blocks, ripple counter for the rest.
fn run_inputs<const R: usize>(k: &K32, bits: u32, xs_all: &[u32]) -> Counts {
    const BLK: usize = G * L;
    let mut a = AccHs::new(bits);
    let mut blocks = xs_all.chunks_exact(BLK);
    let mut xs = [[0u32; L]; G];
    for b in &mut blocks {
        for (d, s) in xs.iter_mut().flatten().zip(b) {
            *d = *s;
        }
        if bits == 32 { a.add_block::<R, true>(k, &xs) } else { a.add_block::<R, false>(k, &xs) }
    }
    let mut c = a.finish();
    let rest = blocks.remainder();
    let full = rest.len() / L * L;
    if full > 0 {
        let mut r = Acc32::new(bits);
        for g in rest[..full].chunks_exact(L) {
            r.add8::<R>(k, g.try_into().unwrap());
        }
        c = c.merge(r.finish());
    }
    let b = bits as usize;
    for &x in &rest[full..] {
        // at most 7 leftover inputs: count lane 0 directly
        let h0 = mix8::<R>(k, [x; L])[0];
        for j in 0..b {
            let mut d = h0 ^ mix8::<R>(k, [x ^ (1 << j); L])[0];
            while d != 0 {
                c.bins[j * b + d.trailing_zeros() as usize] += 1;
                d &= d - 1;
            }
        }
        c.n += 1;
    }
    c
}

/// Same inputs and counts as `bias::sampled(f, bits, n, seed)` for this mixer
/// (identical RNG streams and chunking), but SIMD with the Harley-Seal counter.
pub fn sampled_xmx(x: &Xmx, n: u64, seed: u64) -> Counts {
    use crate::bias::SAMPLE_CHUNK;
    use crate::rng::{stream_seed, Xoshiro256};
    let bits = x.bits;
    let msk = mask(bits);
    let nchunks = n.div_ceil(SAMPLE_CHUNK);
    dispatch(x, |k, r| {
        (0..nchunks)
            .into_par_iter()
            .map(|c| {
                let len = SAMPLE_CHUNK.min(n - c * SAMPLE_CHUNK);
                let mut rng = Xoshiro256::new(stream_seed(seed, c));
                let xs: Vec<u32> = (0..len).map(|_| (rng.next_u64() & msk) as u32).collect();
                by_rounds!(r, run_inputs, k, bits, &xs)
            })
            .reduce_with(Counts::merge)
            .unwrap_or_else(|| Counts::new(bits))
    })
}
