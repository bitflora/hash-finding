//! Exact avalanche counting for xmx mixers of width 9..=16, in 32 u16 lanes
//! (one 512-bit register; explicit AVX-512BW intrinsics when the build has
//! them, since LLVM leaves the plain array form scalar), so the multiplies
//! are 16-bit `vpmullw`.
//!
//! Layout: `y = x ^ (x >> s0)` and `h(x)` are computed once for all 2^bits
//! inputs; then each input bit j is a separate pass over that table (the
//! first xorshift is linear, so `y(x ^ e_j) = y(x) ^ y(e_j)`). The passes use
//! the same Harley-Seal carry-save counter as `fast32`. Because a pass
//! finishes one row of the count matrix, `exact_ms_bounded` can stop as soon
//! as the rows done so far already exceed a bound.
//!
//! Counts are identical to `fast32::exact_xmx` (tested in `tests/fast16.rs`).

use crate::bias::Counts;
use crate::mixers::{mask, Xmx};
use std::cell::RefCell;

const L: usize = 32;
/// Vectors per Harley-Seal block (16 * 32 = 512 inputs).
const G: usize = 16;
/// Ripple planes for weight-16 carries: at most 2^16 / (32 * 16) = 128 per
/// lane, so 8 planes never overflow and need no mid-pass flush.
const P: usize = 8;

/// 32 u16 lanes: one zmm register with AVX-512BW.
#[cfg(all(target_arch = "x86_64", target_feature = "avx512bw"))]
mod v {
    use std::arch::x86_64::*;
    #[derive(Clone, Copy)]
    pub struct V(__m512i);
    #[allow(unused_unsafe)]
    impl V {
        #[inline(always)]
        pub fn zero() -> V {
            unsafe { V(_mm512_setzero_si512()) }
        }
        #[inline(always)]
        pub fn splat(x: u16) -> V {
            unsafe { V(_mm512_set1_epi16(x as i16)) }
        }
        #[inline(always)]
        pub fn from_array(a: [u16; super::L]) -> V {
            unsafe { V(_mm512_loadu_si512(a.as_ptr() as *const _)) }
        }
        #[inline(always)]
        pub fn to_array(self) -> [u16; super::L] {
            let mut a = [0u16; super::L];
            unsafe { _mm512_storeu_si512(a.as_mut_ptr() as *mut _, self.0) };
            a
        }
        #[inline(always)]
        pub fn xor(self, o: V) -> V {
            unsafe { V(_mm512_xor_si512(self.0, o.0)) }
        }
        #[inline(always)]
        pub fn and(self, o: V) -> V {
            unsafe { V(_mm512_and_si512(self.0, o.0)) }
        }
        #[inline(always)]
        pub fn or(self, o: V) -> V {
            unsafe { V(_mm512_or_si512(self.0, o.0)) }
        }
        #[inline(always)]
        pub fn shr(self, s: u32) -> V {
            unsafe { V(_mm512_srl_epi16(self.0, _mm_cvtsi32_si128(s as i32))) }
        }
        #[inline(always)]
        pub fn mul(self, o: V) -> V {
            unsafe { V(_mm512_mullo_epi16(self.0, o.0)) }
        }
    }
}

/// Portable fallback with the same interface.
#[cfg(not(all(target_arch = "x86_64", target_feature = "avx512bw")))]
mod v {
    const L: usize = super::L;
    #[derive(Clone, Copy)]
    pub struct V([u16; L]);
    #[inline(always)]
    fn zip(a: V, b: V, f: impl Fn(u16, u16) -> u16) -> V {
        let mut r = [0u16; L];
        for i in 0..L {
            r[i] = f(a.0[i], b.0[i]);
        }
        V(r)
    }
    impl V {
        #[inline(always)]
        pub fn zero() -> V {
            V([0; L])
        }
        #[inline(always)]
        pub fn splat(x: u16) -> V {
            V([x; L])
        }
        #[inline(always)]
        pub fn from_array(a: [u16; L]) -> V {
            V(a)
        }
        #[inline(always)]
        pub fn to_array(self) -> [u16; L] {
            self.0
        }
        #[inline(always)]
        pub fn xor(self, o: V) -> V {
            zip(self, o, |a, b| a ^ b)
        }
        #[inline(always)]
        pub fn and(self, o: V) -> V {
            zip(self, o, |a, b| a & b)
        }
        #[inline(always)]
        pub fn or(self, o: V) -> V {
            zip(self, o, |a, b| a | b)
        }
        #[inline(always)]
        pub fn shr(self, s: u32) -> V {
            zip(self, self, |a, _| a >> s)
        }
        #[inline(always)]
        pub fn mul(self, o: V) -> V {
            zip(self, o, |a, b| a.wrapping_mul(b))
        }
    }
}

use v::V;

#[derive(Clone, Copy)]
struct K16 {
    mask: V,
    s: [u32; 5],
    m: [V; 4],
}

/// Widths and round counts this kernel takes.
pub fn supports(x: &Xmx) -> bool {
    (9..=16).contains(&x.bits) && (1..=4).contains(&x.rounds())
}

/// Rounds after the first xorshift (the input is y = x ^ (x >> s0)).
#[inline(always)]
fn tail<const R: usize, const FULL: bool>(k: &K16, mut x: V) -> V {
    for i in 0..R {
        if i > 0 {
            x = x.xor(x.shr(k.s[i]));
        }
        x = x.mul(k.m[i]);
        if !FULL {
            x = x.and(k.mask);
        }
    }
    x.xor(x.shr(k.s[R]))
}

#[inline(always)]
fn csa(a: V, b: V, c: V) -> (V, V) {
    let u = a.xor(b);
    (a.and(b).or(u.and(c)), u.xor(c))
}

#[inline(always)]
fn add_word(row: &mut [u64], w: V, weight: u64) {
    for v in w.to_array() {
        let mut v = v;
        while v != 0 {
            row[v.trailing_zeros() as usize] += weight;
            v &= v - 1;
        }
    }
}

thread_local! {
    /// (ys, h0) tables, reused across calls on a thread.
    static BUF: RefCell<(Vec<V>, Vec<V>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

/// One row (input bit j) of the count matrix, added into `row`.
#[inline(always)]
fn row_counts<const R: usize, const FULL: bool>(k: &K16, ys: &[V], h0: &[V], j: u32, row: &mut [u64]) {
    let e = V::splat(((1u32 << j) ^ ((1u32 << j) >> k.s[0])) as u16);
    let z = V::zero();
    let (mut ones, mut twos, mut fours, mut eights) = (z, z, z, z);
    let mut planes = [z; P];
    for (yb, hb) in ys.chunks_exact(G).zip(h0.chunks_exact(G)) {
        let mut d = [z; G];
        for g in 0..G {
            d[g] = hb[g].xor(tail::<R, FULL>(k, yb[g].xor(e)));
        }
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
        let (mut c, e8) = csa(eights, ea, eb); eights = e8;
        for plane in planes.iter_mut() {
            let t = plane.and(c);
            *plane = plane.xor(c);
            c = t;
        }
    }
    for (q, w) in [ones, twos, fours, eights].into_iter().enumerate() {
        add_word(row, w, 1 << q);
    }
    for (q, w) in planes.into_iter().enumerate() {
        add_word(row, w, 16 << q);
    }
}

/// Rows of the count matrix from the top input bit down (high input bits
/// reach few output bits through the multiplies, so their rows tend to be the
/// worst and a bound fails soonest); `keep_going(j, counts)` is asked after
/// row j and may stop the pass early.
fn run<const R: usize, const FULL: bool>(k: &K16, bits: u32, mut keep_going: impl FnMut(usize, &Counts) -> bool) -> Counts {
    let n = 1usize << bits;
    let b = bits as usize;
    let mut c = Counts::new(bits);
    c.n = n as u64;
    BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        let (ys, h0) = &mut *buf;
        ys.clear();
        h0.clear();
        for v in 0..n / L {
            let mut y = [0u16; L];
            for (l, t) in y.iter_mut().enumerate() {
                let x = (v * L + l) as u16;
                *t = x ^ (x >> k.s[0]);
            }
            let y = V::from_array(y);
            ys.push(y);
            h0.push(tail::<R, FULL>(k, y));
        }
        for j in (0..b).rev() {
            row_counts::<R, FULL>(k, ys, h0, j as u32, &mut c.bins[j * b..(j + 1) * b]);
            if !keep_going(j, &c) {
                break;
            }
        }
    });
    c
}

fn dispatch(x: &Xmx, keep_going: impl FnMut(usize, &Counts) -> bool) -> Counts {
    assert!(supports(x), "fast16 needs 9..=16 bits and 1..=4 rounds, got {x}");
    let kk = x.kernel();
    let m = [0, 1, 2, 3].map(|i| V::splat(kk.m[i] as u16));
    let k = K16 { mask: V::splat(mask(x.bits) as u16), s: kk.s, m };
    let full = x.bits == 16;
    macro_rules! go {
        ($r:literal) => {
            if full { run::<$r, true>(&k, x.bits, keep_going) } else { run::<$r, false>(&k, x.bits, keep_going) }
        };
    }
    match x.rounds() {
        1 => go!(1),
        2 => go!(2),
        3 => go!(3),
        _ => go!(4),
    }
}

/// Exact counts over all 2^bits inputs, single-threaded (batch callers
/// parallelise across mixers).
pub fn exact_xmx(x: &Xmx) -> Counts {
    dispatch(x, |_, _| true)
}

/// Exact mean-square diff (`Counts::ms`) if it is at most `limit_ms`, else
/// `None`. Stops after the first input-bit row whose running sum of squared
/// diffs already exceeds `limit_ms * bits^2` (the sum only grows), so most
/// poor mixers cost one or two of the `bits` passes.
pub fn exact_ms_bounded(x: &Xmx, limit_ms: f64) -> Option<f64> {
    let b = x.bits as usize;
    let half = (1u64 << x.bits) as f64 / 2.0;
    let budget = limit_ms * (b * b) as f64;
    let mut sum = 0.0;
    let mut over = false;
    let c = dispatch(x, |j, c| {
        sum += c.bins[j * b..(j + 1) * b].iter().map(|&v| ((v as f64 - half) / half).powi(2)).sum::<f64>();
        over = sum > budget;
        !over
    });
    if over {
        return None;
    }
    let ms = c.ms();
    (ms <= limit_ms).then_some(ms)
}

/// Every 2-round mixer `[s0 m0 s1 m1 s2]` at width `bits` (9..=16) with the
/// given shifts and odd multipliers `m0` in `m0s`, any odd `m1`, whose exact
/// ms is at most `limit_ms`, as (m0, m1, ms) sorted by (m0, m1). Parallel over
/// m0. `progress(done)` is called with the number of m0 values finished.
pub fn exhaust2(
    bits: u32,
    shifts: [u32; 3],
    m0s: std::ops::Range<u64>,
    limit_ms: f64,
    progress: impl Fn(u64) + Sync,
) -> Vec<(u64, u64, f64)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    let done = AtomicU64::new(0);
    let odd: Vec<u64> = m0s.filter(|m| m & 1 == 1).collect();
    let mut hits: Vec<(u64, u64, f64)> = odd
        .par_iter()
        .flat_map_iter(|&m0| {
            let mut out = Vec::new();
            let mut x = Xmx::new(bits, shifts.to_vec(), vec![m0, 1]).expect("valid exhaust2 shifts and m0");
            for m1 in (1..1u64 << bits).step_by(2) {
                x.mults[1] = m1;
                if let Some(ms) = exact_ms_bounded(&x, limit_ms) {
                    out.push((m0, m1, ms));
                }
            }
            progress(done.fetch_add(1, Ordering::Relaxed) + 1);
            out
        })
        .collect();
    hits.sort_by_key(|&(a, b, _)| (a, b));
    hits
}
