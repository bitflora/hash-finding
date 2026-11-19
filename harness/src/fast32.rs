//! Fast exact avalanche counting for xmx mixers of width 3..=32.
//! Evaluates 8 consecutive inputs at once in u32 lanes (auto-vectorises to
//! AVX2 with target-cpu=native) and counts with branch-free bit-sliced
//! counters (8 planes, flushed every 255 steps).

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

fn run_range<const R: usize>(k: &K32, bits: u32, lo: u64, hi: u64) -> Counts {
    let mut a = Acc32::new(bits);
    let mut x0 = lo;
    while x0 < hi {
        let b = x0 as u32;
        let x = [b, b + 1, b + 2, b + 3, b + 4, b + 5, b + 6, b + 7];
        a.add8::<R>(k, x);
        x0 += L as u64;
    }
    a.finish()
}

fn exact_r<const R: usize>(k: &K32, bits: u32, parallel: bool) -> Counts {
    let total = 1u64 << bits;
    if !parallel || total <= 1 << 16 {
        return run_range::<R>(k, bits, 0, total);
    }
    let chunk = (total / 2048).max(1 << 14);
    (0..total / chunk)
        .into_par_iter()
        .map(|c| run_range::<R>(k, bits, c * chunk, (c + 1) * chunk))
        .reduce_with(Counts::merge)
        .unwrap()
}

/// Exact counts for an xmx mixer with 3 <= bits <= 32.
pub fn exact_xmx(x: &Xmx, parallel: bool) -> Counts {
    assert!((3..=32).contains(&x.bits), "fast32 needs 3..=32 bits");
    let k = K32::from(x);
    match x.rounds() {
        1 => exact_r::<1>(&k, x.bits, parallel),
        2 => exact_r::<2>(&k, x.bits, parallel),
        3 => exact_r::<3>(&k, x.bits, parallel),
        4 => exact_r::<4>(&k, x.bits, parallel),
        r => panic!("unsupported rounds {r}"),
    }
}
