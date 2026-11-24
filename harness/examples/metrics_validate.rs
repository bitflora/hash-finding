//! Candidate 64-bit scoring metrics, evaluated on any-width xmx mixers so they
//! can be validated at 32 bits against exact bias and PractRand.
//!
//! Every avalanche-style metric is reported in noise units: mean over the
//! table of d^2 * n_row, where d = (flips - n/2)/(n/2). An ideal random
//! function gives ~1 (sd ~ sqrt(2/cells)); a real defect of mean square ms
//! adds ms * n, so defects grow linearly with n.
//!
//!   ctr   counter 0..2^k, single-bit input flips (pairs canonicalised)
//!   rev   bit-reversed counter (PractRand mode 2 analogue)
//!   hi    counter in the high half (rotate by bits/2)
//!   adj2  random x, adjacent two-bit flips 3<<i
//!   add   random x, additive differences x vs x + 2^i
//!   sac   plain avalanche on random x, for reference
//!   BIC   rms z and worst |corr| (random x)
//!   two   mean over all two-bit flips {i, j} (j - i != s0), random x; plus worst
//!
//! Usage: cargo run --release --example metrics_validate -- --bits 32
//!        [--kctr 24] [--nrand p22] [--nbic p20] [--seed 1] SPEC...
//! Output: one CSV line per spec on stdout.
use mixlab::extra::bic_sampled;
use mixlab::mixers::{mask, Xmx};
use mixlab::rng::{stream_seed, Xoshiro256};
use rayon::prelude::*;

/// Units for a table of `rows` rows; `pairs(row)` yields input pairs.
fn units<F, P>(f: &F, bits: u32, rows: usize, pairs: P) -> f64
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

/// Counter 0..2^k mapped through a bit permutation `t` (with `tinv_bit`
/// giving the counter bit that becomes word bit j); flips word bit j.
fn counter_units<F: Fn(u64) -> u64 + Sync>(
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
                continue; // the partner is also in the set: count the pair once
            }
            let x = t(c) & !e;
            emit(x, x | e);
        }
    })
}

fn random_units<F: Fn(u64) -> u64 + Sync>(
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

fn parse_n(s: &str) -> u64 {
    match s.strip_prefix('p') {
        Some(k) => 1u64 << k.parse::<u32>().unwrap(),
        None => s.parse().unwrap(),
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut opt = |name: &str, def: &str| -> String {
        if let Some(i) = args.iter().position(|a| a == name) {
            let v = args[i + 1].clone();
            args.drain(i..i + 2);
            v
        } else {
            def.to_string()
        }
    };
    let bits: u32 = opt("--bits", "32").parse().unwrap();
    let kctr: u32 = opt("--kctr", "24").parse().unwrap();
    let nrand = parse_n(&opt("--nrand", "p22"));
    let nbic = parse_n(&opt("--nbic", "p20"));
    let ntwo = parse_n(&opt("--ntwo", "p18"));
    let seed: u64 = opt("--seed", "1").parse().unwrap();
    let rev = move |c: u64| c.reverse_bits() >> (64 - bits);
    let rot = move |c: u64| ((c << (bits / 2)) | (c >> (bits - bits / 2))) & mask(bits);
    println!("spec,ctr,rev,hi,adj2,add,sac,bic_rmsz,bic_worst_corr,bic_worst,two,two_worst,two_worst_pair");
    for spec in args {
        let x = Xmx::parse(bits, &spec).unwrap();
        let f = |v: u64| x.apply(v);
        let ctr = counter_units(&f, bits, kctr, &|c| c, &|j| j);
        let rv = counter_units(&f, bits, kctr, &rev, &|j| bits - 1 - j);
        let hi = counter_units(&f, bits, kctr, &rot, &|j| (j + bits - bits / 2) % bits);
        let adj2 = random_units(&f, bits, nrand, seed, bits as usize - 1, &|r, v| v ^ (3u64 << r));
        let add = random_units(&f, bits, nrand, seed ^ 0xadd, bits as usize, &|r, v| v.wrapping_add(1u64 << r));
        let sac = random_units(&f, bits, nrand, seed ^ 0x5ac, bits as usize, &|r, v| v ^ (1u64 << r));
        // All two-bit flips {i, j}, except j - i == s0 (structural: the first
        // xorshift collapses them to a single high bit for every constant).
        let s0 = x.shifts[0] as usize;
        let pairs: Vec<(usize, usize)> = (0..bits as usize)
            .flat_map(|i| ((i + 1)..bits as usize).map(move |j| (i, j)))
            .filter(|&(i, j)| j - i != s0)
            .collect();
        let per: Vec<f64> = pairs
            .iter()
            .map(|&(i, j)| {
                let d = (1u64 << i) | (1u64 << j);
                random_units(&f, bits, ntwo, seed ^ 0x2b17 ^ ((i * 64 + j) as u64), 1, &|_, v| v ^ d)
            })
            .collect();
        let two = per.iter().sum::<f64>() / per.len() as f64;
        let (wi, wv) = per.iter().enumerate().fold((0, 0.0), |a, (k, &v)| if v > a.1 { (k, v) } else { a });
        let bic = bic_sampled(&f, bits, nbic, seed ^ 0xb1c);
        let (i, j, k, c) = bic.worst;
        println!(
            "\"{x}\",{ctr:.4},{rv:.4},{hi:.4},{adj2:.4},{add:.4},{sac:.4},{:.4},{:.4},{i}->{j}/{k},{two:.4},{wv:.2},{}-{}",
            bic.rms_z,
            c.abs(),
            pairs[wi].0,
            pairs[wi].1
        );
    }
}
