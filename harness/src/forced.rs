//! Shift-forced exact output pairs of 2-round xmx mixers (FINDINGS F37).
//!
//! For `[s0 m0 s1 m1 s2]` at width n, flipping one input bit can make two
//! output bits flip together (or exactly oppositely) for every input and
//! every odd multiplier pair. The cause is that odd multiplication keeps the
//! lowest bit of an XOR difference: if `a` and `b` first differ at bit t,
//! `m*a` and `m*b` agree below t and differ at t.
//!
//! Walk a flip of input bit i through the mixer:
//! - `v = x ^ (x >> s0)` differs at bit i and, if i >= s0, at i - s0. Its
//!   lowest differing bit is `a = i - s0` (i >= s0) or `a = i`.
//! - `w = m0 * v` agrees with the unflipped run below `a` and differs at `a`.
//! - `z = w ^ (w >> s1)`: bit k of z mixes w_k and w_{k+s1}. If `a >= s1 + j`,
//!   z agrees below j (and at j when `a > s1 + j`); if `a == s1 + j`, the
//!   lowest difference of z is exactly bit j.
//! - `y = m1 * z` then agrees below j and its bit j changes never
//!   (`a > s1 + j`) or always (`a == s1 + j`).
//! - Output bits j + s2 >= n - s2 are `y_{j+s2}` alone, and output bit j is
//!   `y_j ^ y_{j+s2}`, so `Δout_j ^ Δout_{j+s2} = Δy_j` is constant.
//!
//! So the pair (j, j+s2) is forced by input bit i whenever
//! `max(0, n - 2*s2) <= j < n - s2` and `s1 + j <= a(i) < s1 + j + s2`:
//! correlation +1 when `a > s1 + j`, -1 when `a == s1 + j`. The upper bound
//! keeps the pair a correlation: z's lowest difference is bit `a - s1`, so
//! y's is too, and if `a - s1 >= j + s2` both output flips are constant.
//! At s2 = n/2 this is F3/F11's (0, s2) pair. At 32 bits with s2 >= 12 the
//! upper bound never binds, so a template has a forced pair iff
//! `max(s0 - 1, n - 1 - s0) >= s1 + max(0, n - 2*s2)`.

use crate::mixers::{mask, Xmx};

/// An input bit whose flip makes two output bits flip together (`sign` +1)
/// or oppositely (`sign` -1) for every input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExactPair {
    pub input: u32,
    pub out_lo: u32,
    pub out_hi: u32,
    pub sign: i8,
}

/// The exact pairs the shifts alone force on `[s0 m0 s1 m1 s2]` at width
/// `bits`, for any odd multipliers. Sorted.
pub fn forced_pairs(bits: u32, s0: u32, s1: u32, s2: u32) -> Vec<ExactPair> {
    forced_pairs_rounds(bits, &[s0, s1, s2])
}

/// The k-round form of `forced_pairs` (FINDINGS F39), for shifts
/// `[s0, s1, ..., sk]` of a k-round xmx (k >= 1).
///
/// Track c, the lowest bit at which the flipped run differs. The first
/// xorshift of a single flipped input bit i gives c = i - s0 (i >= s0) or
/// c = i, and each odd multiply keeps c. A middle xorshift s maps c to
/// c - s if c >= s (bit c - s mixes the changed bit c into an unchanged
/// one, and every lower bit mixes two unchanged bits). If c < s, bits below
/// c pick up the random changes above c, and nothing stays forced. After the
/// last multiply y first differs at c, and the final xorshift s forces
/// (j, j+s) for max(0, n - 2s) <= j < n - s with j <= c < j + s.
pub fn forced_pairs_rounds(bits: u32, shifts: &[u32]) -> Vec<ExactPair> {
    assert!(shifts.len() >= 2, "need at least one round");
    let n = bits as i64;
    let s: Vec<i64> = shifts.iter().map(|&v| v as i64).collect();
    let last = *s.last().unwrap();
    let mut out = Vec::new();
    'input: for i in 0..n {
        let mut c = if i >= s[0] { i - s[0] } else { i };
        for &m in &s[1..s.len() - 1] {
            if c < m {
                continue 'input;
            }
            c -= m;
        }
        for j in (n - 2 * last).max(0)..(n - last) {
            if j <= c && c < j + last {
                out.push(ExactPair {
                    input: i as u32,
                    out_lo: j as u32,
                    out_hi: (j + last) as u32,
                    sign: if j == c { -1 } else { 1 },
                });
            }
        }
    }
    out.sort();
    out
}

/// True when no output pair is forced by the shifts alone.
pub fn shifts_clean(bits: u32, shifts: &[u32]) -> bool {
    forced_pairs_rounds(bits, shifts).is_empty()
}

/// Every exact pair of `f` at width `bits` (<= 20), by exhaustion over all
/// inputs: input bit i and output bits j < k such that neither output bit's
/// flip is constant but their XOR is.
pub fn exact_pairs_exhaustive(f: &Xmx) -> Vec<ExactPair> {
    let bits = f.bits;
    assert!(bits <= 20, "exhaustive exact-pair search needs bits <= 20");
    let msk = mask(bits);
    let ys: Vec<u64> = (0..=msk).map(|x| f.apply(x)).collect();
    let mut out = Vec::new();
    for i in 0..bits {
        // For output bit j: bit k of ever1[j] is set if Δout_j ^ Δout_k was
        // ever 1, ever0[j] likewise for 0.
        let mut ever1 = vec![0u64; bits as usize];
        let mut ever0 = vec![0u64; bits as usize];
        // Output bits whose own flip varies (a constant bit has no correlation).
        let mut seen1 = 0u64;
        let mut seen0 = 0u64;
        // Corresponding indices in the two halves differ only in input bit i.
        let half = 1usize << i;
        for block in ys.chunks_exact(2 * half) {
            let (low, high) = block.split_at(half);
            for (&a, &b) in low.iter().zip(high) {
                let d = a ^ b;
                seen1 |= d;
                seen0 |= !d & msk;
                for j in 0..bits as usize {
                    let dj = if d >> j & 1 == 1 { d ^ msk } else { d };
                    ever1[j] |= dj;
                    ever0[j] |= !dj & msk;
                }
            }
        }
        let varies = seen1 & seen0;
        for j in 0..bits {
            if varies >> j & 1 == 0 {
                continue;
            }
            for k in j + 1..bits {
                if varies >> k & 1 == 0 {
                    continue;
                }
                let (e1, e0) = (ever1[j as usize] >> k & 1, ever0[j as usize] >> k & 1);
                if e1 != e0 {
                    out.push(ExactPair { input: i, out_lo: j, out_hi: k, sign: if e0 == 1 { 1 } else { -1 } });
                }
            }
        }
    }
    out
}
