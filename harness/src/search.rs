//! Configuration sweeps (small widths) and noise-aware local search.

use crate::bias::{exact, exact_serial, sampled, sampled_serial};
use crate::mixers::{mask, Xmx};
use crate::rng::{stream_seed, Xoshiro256};
use crate::with_mixer;
use rayon::prelude::*;
use std::collections::HashSet;
use std::io::Write;

/// Number of distinct R-round configurations at a width:
/// (bits-1)^(R+1) shift choices x (2^(bits-1))^R odd multipliers.
pub fn space_size(bits: u32, rounds: usize) -> u128 {
    let s = (bits as u128 - 1).pow(rounds as u32 + 1);
    let m = (1u128 << (bits - 1)).pow(rounds as u32);
    s * m
}

/// Decode a mixed-radix index into a configuration.
pub fn config_from_index(bits: u32, rounds: usize, mut idx: u128) -> Xmx {
    let sr = bits as u128 - 1;
    let mr = 1u128 << (bits - 1);
    let mut shifts = Vec::with_capacity(rounds + 1);
    let mut mults = Vec::with_capacity(rounds);
    for _ in 0..=rounds {
        shifts.push((idx % sr) as u32 + 1);
        idx /= sr;
    }
    for _ in 0..rounds {
        mults.push((((idx % mr) as u64) << 1) | 1);
        idx /= mr;
    }
    Xmx { bits, shifts, mults }
}

pub fn exact_score_serial(x: &Xmx) -> (f64, f64) {
    let c = if x.bits >= 3 && x.bits <= 32 {
        crate::fast32::exact_xmx(x, false)
    } else {
        with_mixer!(x, f => exact_serial(&f, x.bits))
    };
    (c.score(), c.max_abs())
}

fn csv_header(rounds: usize) -> String {
    let mut h = String::from("bits");
    for i in 0..=rounds {
        h += &format!(",s{i}");
    }
    for i in 0..rounds {
        h += &format!(",m{i}");
    }
    h + ",score,max_abs"
}

fn csv_row(x: &Xmx, score: f64, max_abs: f64) -> String {
    let mut r = format!("{}", x.bits);
    for s in &x.shifts {
        r += &format!(",{s}");
    }
    for m in &x.mults {
        r += &format!(",{m:x}");
    }
    r + &format!(",{score:.10},{max_abs:.6}")
}

/// Exactly score configurations given by `indices` (parallel over configs),
/// writing CSV rows in order. Widths <= 24 recommended.
pub fn sweep<W: Write>(
    bits: u32,
    rounds: usize,
    indices: impl Iterator<Item = u128>,
    out: &mut W,
    mut progress: impl FnMut(u64),
) -> std::io::Result<u64> {
    writeln!(out, "{}", csv_header(rounds))?;
    let mut done = 0u64;
    let mut buf: Vec<u128> = Vec::with_capacity(1 << 14);
    let flush = |buf: &mut Vec<u128>, out: &mut W, done: &mut u64| -> std::io::Result<()> {
        let rows: Vec<String> = buf
            .par_iter()
            .map(|&i| {
                let x = config_from_index(bits, rounds, i);
                let (s, m) = exact_score_serial(&x);
                csv_row(&x, s, m)
            })
            .collect();
        for r in rows {
            writeln!(out, "{r}")?;
        }
        *done += buf.len() as u64;
        buf.clear();
        Ok(())
    };
    for i in indices {
        buf.push(i);
        if buf.len() == buf.capacity() {
            flush(&mut buf, out, &mut done)?;
            progress(done);
        }
    }
    flush(&mut buf, out, &mut done)?;
    progress(done);
    Ok(done)
}

#[derive(Clone, Debug)]
pub struct SearchCfg {
    pub bits: u32,
    pub rounds: usize,
    pub iters: usize,
    pub neighbors: usize,
    /// Successive-halving sample sizes (random inputs), ascending.
    pub levels: Vec<u64>,
    /// Fraction of candidates kept after each level.
    pub keep_frac: f64,
    /// How many survivors get the final (expensive) evaluation.
    pub finalists: usize,
    /// None => exact final evaluation (bits <= 32); Some(n) => n samples.
    pub final_samples: Option<u64>,
    pub seed: u64,
}

pub fn random_config(bits: u32, rounds: usize, rng: &mut Xoshiro256) -> Xmx {
    let shifts = (0..=rounds)
        .map(|_| {
            let s = bits as i64 / 2 + rng.below(7) as i64 - 3;
            s.clamp(1, bits as i64 - 1) as u32
        })
        .collect();
    let mults = (0..rounds).map(|_| (rng.next_u64() & mask(bits)) | 1).collect();
    Xmx { bits, shifts, mults }
}

/// Small random move: tweak one shift by +-1, or flip 1-3 bits of one multiplier.
pub fn neighbor(x: &Xmx, rng: &mut Xoshiro256) -> Xmx {
    let mut y = x.clone();
    if rng.below(5) == 0 {
        let i = rng.below(y.shifts.len() as u64) as usize;
        let s = y.shifts[i] as i64 + if rng.below(2) == 0 { -1 } else { 1 };
        y.shifts[i] = s.clamp(1, x.bits as i64 - 1) as u32;
    } else {
        let i = rng.below(y.mults.len() as u64) as usize;
        let r = rng.unit();
        let flips = if r < 0.6 { 1 } else if r < 0.9 { 2 } else { 3 };
        for _ in 0..flips {
            let b = 1 + rng.below(x.bits as u64 - 1);
            y.mults[i] ^= 1u64 << b;
        }
    }
    y
}

fn ms_sampled_serial(x: &Xmx, n: u64, seed: u64) -> f64 {
    with_mixer!(x, f => sampled_serial(&f, x.bits, n, seed).ms_debiased())
}

fn final_ms(x: &Xmx, cfg: &SearchCfg) -> f64 {
    match cfg.final_samples {
        None if x.bits >= 3 && x.bits <= 32 => crate::fast32::exact_xmx(x, true).ms(),
        None => with_mixer!(x, f => exact(&f, x.bits).ms()),
        // fixed seed: common random numbers across all final comparisons
        Some(n) => with_mixer!(x, f => sampled(&f, x.bits, n, stream_seed(cfg.seed, 0xf1a1)).ms_debiased()),
    }
}

pub fn ms_to_score(ms: f64) -> f64 {
    1000.0 * ms.max(0.0).sqrt()
}

/// Local search with successive halving. Returns the best configuration and
/// its final-level score. `log` receives CSV lines:
/// iter,event,config,score
pub fn search(cfg: &SearchCfg, start: Option<Xmx>, mut log: impl FnMut(&str)) -> (Xmx, f64) {
    let mut rng = Xoshiro256::new(cfg.seed);
    let mut cur = start.unwrap_or_else(|| random_config(cfg.bits, cfg.rounds, &mut rng));
    let mut cur_ms = final_ms(&cur, cfg);
    let mut seen: HashSet<Xmx> = HashSet::new();
    seen.insert(cur.clone());
    log(&format!("0,start,{},{:.10}", cur, ms_to_score(cur_ms)));
    for it in 1..=cfg.iters {
        let mut cands: Vec<Xmx> = Vec::new();
        let mut tries = 0;
        while cands.len() < cfg.neighbors && tries < cfg.neighbors * 20 {
            tries += 1;
            let y = neighbor(&cur, &mut rng);
            if seen.insert(y.clone()) {
                cands.push(y);
            }
        }
        if cands.is_empty() {
            log(&format!("{it},exhausted,{},{:.10}", cur, ms_to_score(cur_ms)));
            break;
        }
        for (li, &n) in cfg.levels.iter().enumerate() {
            let lseed = stream_seed(cfg.seed, ((it as u64) << 8) | li as u64);
            let mut scored: Vec<(f64, Xmx)> = cands
                .into_par_iter()
                .map(|c| (ms_sampled_serial(&c, n, lseed), c))
                .collect();
            scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let keep = ((scored.len() as f64 * cfg.keep_frac).ceil() as usize)
                .max(cfg.finalists)
                .min(scored.len());
            scored.truncate(keep);
            cands = scored.into_iter().map(|(_, c)| c).collect();
        }
        cands.truncate(cfg.finalists);
        let mut improved = false;
        for c in cands {
            let m = final_ms(&c, cfg);
            if m < cur_ms {
                cur = c;
                cur_ms = m;
                improved = true;
            }
        }
        let ev = if improved { "improved" } else { "iter" };
        log(&format!("{it},{ev},{},{:.10}", cur, ms_to_score(cur_ms)));
    }
    (cur, ms_to_score(cur_ms))
}
