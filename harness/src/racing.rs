//! Noise-control experiments for comparing two mixers by sampling:
//! how much does feeding both the *same* random inputs (common random
//! numbers, CRN) reduce the variance of the estimated difference?

use crate::bias::sampled_serial;
use crate::mixers::Xmx;
use crate::rng::stream_seed;
use crate::with_mixer;
use rayon::prelude::*;

/// Debiased mean-square diff of `x` over `n` random inputs from `seed`.
pub fn ms_sampled(x: &Xmx, n: u64, seed: u64) -> f64 {
    with_mixer!(x, f => sampled_serial(&f, x.bits, n, seed).ms_debiased())
}

/// Replicated paired comparison of `a` against each of `others`.
pub struct PairStats {
    /// mean of ms(a) - ms(b) over CRN replicates (unbiased either way)
    pub mean: f64,
    /// variance of the difference with independent inputs per mixer
    pub var_ind: f64,
    /// variance of the difference with common inputs
    pub var_crn: f64,
    /// variance of a single ms estimate of b (for reference)
    pub var_b: f64,
}

fn var(v: &[f64]) -> f64 {
    let m = v.iter().sum::<f64>() / v.len() as f64;
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64
}

/// `reps` replicates of n samples. Replicate r uses seed stream (seed, r) for
/// `a` and for CRN-`b`, and stream (seed ^ C, r) for independent-`b`.
pub fn paired(a: &Xmx, others: &[Xmx], n: u64, reps: usize, seed: u64) -> Vec<PairStats> {
    let s_crn = |r: usize| stream_seed(seed, r as u64);
    let s_ind = |r: usize| stream_seed(seed ^ 0x5eed_1d1d_0000_0001, r as u64);
    let ma: Vec<f64> = (0..reps).into_par_iter().map(|r| ms_sampled(a, n, s_crn(r))).collect();
    // tasks: (other index, rep) -> (crn ms, independent ms)
    let tasks: Vec<(usize, usize)> = (0..others.len()).flat_map(|i| (0..reps).map(move |r| (i, r))).collect();
    let res: Vec<(f64, f64)> = tasks
        .par_iter()
        .map(|&(i, r)| (ms_sampled(&others[i], n, s_crn(r)), ms_sampled(&others[i], n, s_ind(r))))
        .collect();
    (0..others.len())
        .map(|i| {
            let row = &res[i * reps..(i + 1) * reps];
            let dc: Vec<f64> = (0..reps).map(|r| ma[r] - row[r].0).collect();
            let di: Vec<f64> = (0..reps).map(|r| ma[r] - row[r].1).collect();
            let b: Vec<f64> = row.iter().map(|p| p.1).collect();
            PairStats {
                mean: dc.iter().sum::<f64>() / reps as f64,
                var_ind: var(&di),
                var_crn: var(&dc),
                var_b: var(&b),
            }
        })
        .collect()
}

/// Human-readable description of how `b` differs from `a`.
pub fn describe_move(a: &Xmx, b: &Xmx) -> String {
    let mut parts = Vec::new();
    for (i, (x, y)) in a.shifts.iter().zip(&b.shifts).enumerate() {
        if x != y {
            parts.push(format!("s{i}{:+}", *y as i64 - *x as i64));
        }
    }
    for (i, (x, y)) in a.mults.iter().zip(&b.mults).enumerate() {
        let d = x ^ y;
        if d != 0 {
            let bits: Vec<String> = (0..64).filter(|k| d >> k & 1 == 1).map(|k| k.to_string()).collect();
            parts.push(format!("m{i}^b{}", bits.join("+b")));
        }
    }
    if parts.is_empty() { "same".into() } else { parts.join(" ") }
}

/// Input designs for variance-reduction tests.
#[derive(Clone, Copy, Debug)]
pub enum Design {
    /// i.i.d. uniform inputs
    Plain,
    /// blocks of 2^k inputs: random high part, low k bits run over all values
    LowBits(u32),
    /// blocks of 2^k inputs: a random affine subspace of dimension k
    Affine(u32),
}

/// Raw (not debiased) mean-square diff over `n` inputs drawn with `design`.
pub fn ms_design(x: &Xmx, n: u64, seed: u64, design: Design) -> f64 {
    use crate::bias::Acc;
    use crate::mixers::mask;
    use crate::rng::Xoshiro256;
    let bits = x.bits;
    let msk = mask(bits);
    with_mixer!(x, f => {
        let mut rng = Xoshiro256::new(seed);
        let mut acc = Acc::new(bits);
        match design {
            Design::Plain => {
                for _ in 0..n {
                    acc.add_input(&f, rng.next_u64() & msk);
                }
            }
            Design::LowBits(k) => {
                let lm = (1u64 << k) - 1;
                for _ in 0..(n >> k) {
                    let h = rng.next_u64() & msk & !lm;
                    let r = rng.next_u64() & lm;
                    for l in 0..=lm {
                        acc.add_input(&f, h | (l ^ r));
                    }
                }
            }
            Design::Affine(k) => {
                for _ in 0..(n >> k) {
                    let basis: Vec<u64> = (0..k).map(|_| rng.next_u64() & msk).collect();
                    let mut v = rng.next_u64() & msk;
                    acc.add_input(&f, v);
                    for i in 1u64..(1 << k) {
                        v ^= basis[i.trailing_zeros() as usize]; // Gray-code walk
                        acc.add_input(&f, v);
                    }
                }
            }
        }
        acc.finish().ms()
    })
}

// ---------------------------------------------------------------------------
// Sequential racing against an incumbent.
// ---------------------------------------------------------------------------

use crate::bias::{sampled, Counts};

/// Standard deviation of a debiased ms estimate from n i.i.d. inputs
/// (approximate noise law: Var ~= (4 ms/n + 2/n^2) / N, N = bits^2).
pub fn sd_ms(ms: f64, n: u64, bits: u32) -> f64 {
    let n = n as f64;
    ((4.0 * ms.max(0.0) / n + 2.0 / (n * n)) / (bits * bits) as f64).sqrt()
}

/// Gaussian-tail multiple-look adjustment within one race. For z >= 0,
/// Q(sqrt(z^2 + 2 ln M)) <= Q(z)/M. The union bound needs no independence
/// between looks, but Gaussian tails and the variance model remain approximate.
/// This does not provide a search-wide or distribution-free error guarantee.
fn race_threshold(cfg: &RaceCfg, candidates: usize) -> f64 {
    let mut looks = 1usize;
    let mut n = cfg.n0;
    while n < cfg.cap {
        n = n.saturating_mul(2).min(cfg.cap);
        looks += 1;
    }
    let tests = candidates.max(1) as f64 * (looks + 1) as f64;
    (cfg.z * cfg.z + 2.0 * tests.ln()).sqrt()
}

#[derive(Clone, Debug)]
pub struct RaceCfg {
    pub bits: u32,
    /// first sample count; cumulative n doubles each round
    pub n0: u64,
    /// max samples per candidate before the fallback decision
    pub cap: u64,
    /// nominal Gaussian z before within-race multiple-look adjustment
    pub z: f64,
    /// None => exact verification (bits <= 32); Some(n) => fresh n-sample verification
    pub verify_samples: Option<u64>,
    pub seed: u64,
    /// race_search only: skip neighbours whose final shift is >= this.
    /// `Some(bits/2)` avoids the exact output-pair correlation of FINDINGS F11.
    pub max_last_shift: Option<u32>,
    /// race_search only: keep the low K bits of every multiplier fixed
    /// (FINDINGS F10 "low-bit core" test). Shift moves are still allowed.
    pub fix_low_bits: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Worse,
    /// verified better (accepted)
    Better,
    /// looked better or undecided, but verification rejected it
    Rejected,
    /// never reached a decision (race ended early by an acceptance)
    Unraced,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub cand: Xmx,
    pub verdict: Verdict,
    /// samples spent on this candidate in the race
    pub n: u64,
    pub ms_hat: f64,
    /// ms from verification, if verified
    pub verified_ms: Option<f64>,
}

pub struct RaceResult {
    pub accepted: Option<(Xmx, f64)>,
    pub decisions: Vec<Decision>,
    /// total sampled inputs (race + sampled verifications)
    pub inputs: u64,
    pub exact_evals: usize,
}

/// Verified ms of a candidate: exact, or a fresh sample.
fn verify(x: &Xmx, cfg: &RaceCfg, salt: u64) -> (f64, u64) {
    match cfg.verify_samples {
        None if x.bits >= 3 => (crate::fast32::exact_xmx(x, true).ms(), 0),
        None => (with_mixer!(x, f => crate::bias::exact(&f, x.bits).ms()), 0),
        Some(n) => {
            let s = stream_seed(cfg.seed ^ 0x7e51f1, salt);
            (with_mixer!(x, f => sampled(&f, x.bits, n, s).ms_debiased()), n)
        }
    }
}

/// Race `cands` against an incumbent whose ms is `inc_ms` with variance
/// `inc_var` (0 if exact). First verified improvement wins.
pub fn race(cfg: &RaceCfg, inc_ms: f64, inc_var: f64, cands: Vec<Xmx>, salt: u64) -> RaceResult {
    assert!(cfg.n0 >= 2 && cfg.cap >= cfg.n0, "need 2 <= n0 <= cap");
    assert!(cfg.z.is_finite() && cfg.z > 0.0, "z must be positive and finite");
    assert!(cfg.verify_samples.map_or(cfg.bits <= 32, |n| n >= 2));
    assert!(cands.iter().all(|c| c.bits == cfg.bits));
    let bits = cfg.bits;
    let z = race_threshold(cfg, cands.len());
    let mut alive: Vec<(usize, Counts)> = (0..cands.len()).map(|i| (i, Counts::new(bits))).collect();
    let mut dec: Vec<Option<Decision>> = vec![None; cands.len()];
    let mut inputs = 0u64;
    let mut exact_evals = 0usize;
    let mut n = 0u64;
    let mut inc = cfg.n0;
    let mut round = 0u64;
    // verification of a list of (idx, ms_hat), best first; returns accepted idx
    let try_verify = |list: Vec<(usize, f64)>,
                          dec: &mut Vec<Option<Decision>>,
                          inputs: &mut u64,
                          exact_evals: &mut usize,
                          n: u64|
     -> Option<(usize, f64)> {
        for (i, mh) in list {
            let (vm, used) = verify(&cands[i], cfg, (salt << 20) ^ i as u64);
            *inputs += used;
            if used == 0 {
                *exact_evals += 1;
            }
            // Sampled verification uses the same adjusted threshold.
            let better = match cfg.verify_samples {
                None => vm < inc_ms,
                Some(vn) => inc_ms - vm > z * (sd_ms(vm, vn, bits).powi(2) + inc_var).sqrt(),
            };
            dec[i] = Some(Decision {
                cand: cands[i].clone(),
                verdict: if better { Verdict::Better } else { Verdict::Rejected },
                n,
                ms_hat: mh,
                verified_ms: Some(vm),
            });
            if better {
                return Some((i, vm));
            }
        }
        None
    };
    let accepted = loop {
        let cseed = |i: usize| stream_seed(stream_seed(cfg.seed, (salt << 24) ^ i as u64), round);
        alive = alive
            .into_par_iter()
            .map(|(i, c)| {
                let more = with_mixer!(&cands[i], f => sampled(&f, bits, inc, cseed(i)));
                (i, c.merge(more))
            })
            .collect();
        inputs += inc * alive.len() as u64;
        n += inc;
        round += 1;
        let mut still = Vec::new();
        let mut promising = Vec::new();
        for (i, c) in alive {
            let mh = c.ms_debiased();
            let sd = (sd_ms(mh, n, bits).powi(2) + inc_var).sqrt();
            let d = mh - inc_ms;
            if d > z * sd {
                dec[i] = Some(Decision { cand: cands[i].clone(), verdict: Verdict::Worse, n, ms_hat: mh, verified_ms: None });
            } else if d < -z * sd {
                promising.push((i, mh));
            } else {
                still.push((i, c));
            }
        }
        alive = still;
        promising.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        if let Some(acc) = try_verify(promising, &mut dec, &mut inputs, &mut exact_evals, n) {
            break Some(acc);
        }
        if alive.is_empty() {
            break None;
        }
        if n >= cfg.cap {
            let mut rest: Vec<(usize, f64)> = alive.iter().map(|(i, c)| (*i, c.ms_debiased())).collect();
            rest.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let r = try_verify(rest, &mut dec, &mut inputs, &mut exact_evals, n);
            break r;
        }
        inc = n.min(cfg.cap - n); // cumulative doubling, respecting non-power-of-two caps
    };
    let decisions = dec
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            d.unwrap_or(Decision { cand: cands[i].clone(), verdict: Verdict::Unraced, n, ms_hat: f64::NAN, verified_ms: None })
        })
        .collect();
    RaceResult { accepted: accepted.map(|(i, m)| (cands[i].clone(), m)), decisions, inputs, exact_evals }
}

/// Local search where each iteration races K fresh neighbours against the
/// incumbent. `log` gets CSV lines: iter,event,config,score,detail
pub fn race_search(
    cfg: &RaceCfg,
    start: Xmx,
    iters: usize,
    neighbors: usize,
    patience: usize,
    mut log: impl FnMut(&str),
) -> (Xmx, f64) {
    use crate::rng::Xoshiro256;
    use crate::search::{ms_to_score, neighbor};
    use std::collections::HashSet;
    let mut rng = Xoshiro256::new(cfg.seed ^ 0x5ea2c4);
    let estimate_inc = |x: &Xmx, salt: u64| -> (f64, f64) {
        let (m, used) = verify(x, cfg, salt);
        let var = if used == 0 { 0.0 } else { sd_ms(m, used, cfg.bits).powi(2) };
        (m, var)
    };
    let mut cur = start;
    let (mut cur_ms, mut cur_var) = estimate_inc(&cur, u64::MAX);
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(cur.to_string());
    log(&format!("0,start,{cur},{:.10},sd={:.3e}", ms_to_score(cur_ms), cur_var.sqrt()));
    let mut total_inputs = 0u64;
    let mut total_exact = 0usize;
    let mut stale = 0usize;
    for it in 1..=iters {
        if stale >= patience {
            log(&format!("{it},patience,{cur},{:.10},no improvement in {patience} iterations", ms_to_score(cur_ms)));
            break;
        }
        let mut cands = Vec::new();
        let mut tries = 0;
        while cands.len() < neighbors && tries < neighbors * 50 {
            tries += 1;
            let y = neighbor(&cur, &mut rng);
            if cfg.max_last_shift.is_some_and(|m| y.shifts.last().is_some_and(|&s| s >= m)) {
                continue;
            }
            if let Some(k) = cfg.fix_low_bits {
                let lo = (1u64 << k) - 1;
                if y.mults.iter().zip(&cur.mults).any(|(a, b)| (a ^ b) & lo != 0) {
                    continue;
                }
            }
            if seen.insert(y.to_string()) {
                cands.push(y);
            }
        }
        if cands.is_empty() {
            log(&format!("{it},exhausted,{cur},{:.10},", ms_to_score(cur_ms)));
            break;
        }
        let k = cands.len();
        let r = race(cfg, cur_ms, cur_var, cands, it as u64);
        // Early acceptance is not evidence against unfinished candidates.
        release_unraced(&mut seen, &r.decisions);
        total_inputs += r.inputs;
        total_exact += r.exact_evals;
        let cnt = |v: Verdict| r.decisions.iter().filter(|d| d.verdict == v).count();
        let mut ns: Vec<u64> = r.decisions.iter().filter(|d| d.verdict == Verdict::Worse).map(|d| d.n).collect();
        ns.sort();
        let med = ns.get(ns.len() / 2).map(|&n| (n as f64).log2()).unwrap_or(f64::NAN);
        let detail = format!(
            "k={k} worse={} rejected={} unraced={} median_n_drop=2^{med:.1} inputs=2^{:.1} exact={} cum_inputs=2^{:.1} cum_exact={}",
            cnt(Verdict::Worse),
            cnt(Verdict::Rejected),
            cnt(Verdict::Unraced),
            (r.inputs.max(1) as f64).log2(),
            r.exact_evals,
            (total_inputs.max(1) as f64).log2(),
            total_exact
        );
        if let Some((x, m)) = r.accepted {
            cur = x;
            // re-estimate with a fresh sample (avoids winner's curse) unless exact
            if cfg.verify_samples.is_some() {
                let (m2, v2) = estimate_inc(&cur, (it as u64) << 40);
                cur_ms = m2;
                cur_var = v2;
            } else {
                cur_ms = m;
            }
            log(&format!("{it},improved,{cur},{:.10},{detail}", ms_to_score(cur_ms)));
            stale = 0;
        } else {
            stale += 1;
            log(&format!("{it},iter,{cur},{:.10},{detail}", ms_to_score(cur_ms)));
        }
    }
    (cur, ms_to_score(cur_ms))
}

/// Keep candidates abandoned by early acceptance eligible for later iterations.
fn release_unraced(seen: &mut std::collections::HashSet<String>, decisions: &[Decision]) {
    for d in decisions {
        if d.verdict == Verdict::Unraced {
            seen.remove(&d.cand.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_looks_raise_threshold_and_cap_is_respected() {
        let x = Xmx::parse(8, "[4 ad 3 97 4]").unwrap();
        let cfg = RaceCfg { bits: 8, n0: 16, cap: 37, z: 100.0, verify_samples: None, seed: 2, max_last_shift: None, fix_low_bits: None };
        assert!(race_threshold(&cfg, 96) > race_threshold(&cfg, 1));
        let ms = crate::fast32::exact_xmx(&x, false).ms();
        let result = race(&cfg, ms, 0.0, vec![x], 1);
        assert_eq!(result.inputs, 37);
        assert_eq!(result.decisions[0].n, 37);
        assert_eq!(result.decisions[0].verdict, Verdict::Rejected);
    }

    #[test]
    fn early_acceptance_does_not_blacklist_unfinished_candidates() {
        let x = Xmx::parse(8, "[4 ad 3 97 4]").unwrap();
        let cfg = RaceCfg { bits: 8, n0: 32, cap: 32, z: 100.0, verify_samples: None, seed: 1, max_last_shift: None, fix_low_bits: None };
        // All candidates beat the deliberately poor incumbent; only one is verified.
        let y = Xmx::parse(8, "[4 b5 3 2d 4]").unwrap();
        let result = race(&cfg, 1.0, 0.0, vec![x.clone(), y.clone()], 1);
        assert!(result.accepted.is_some());
        let unfinished = result.decisions.iter().find(|d| d.verdict == Verdict::Unraced).unwrap();
        let accepted = result.accepted.as_ref().unwrap().0.to_string();
        let mut seen = std::collections::HashSet::from([x.to_string(), y.to_string()]);
        release_unraced(&mut seen, &result.decisions);
        assert!(seen.insert(unfinished.cand.to_string()), "candidate must be eligible again");
        assert!(seen.contains(&accepted));
    }
}

// ---------------------------------------------------------------------------
// Validation traces: record each candidate's estimate after every doubling so
// that drop/promote threshold policies can be scored offline against truth.
// ---------------------------------------------------------------------------

/// One look at one candidate: cumulative samples `n` and debiased estimate.
#[derive(Clone, Debug)]
pub struct TracePoint {
    pub cand: usize,
    pub n: u64,
    pub ms_hat: f64,
}

/// Sample every candidate on the race's doubling schedule (independent
/// streams per candidate, as in `race`), stopping a candidate only once
/// |ms_hat - inc_ms| > zmax * sd (model) or n reaches `cap`. Any policy whose
/// thresholds are <= zmax therefore makes the same decisions on this trace as
/// a real race with the same seeds would.
pub fn trace(bits: u32, inc_ms: f64, cands: &[Xmx], n0: u64, cap: u64, zmax: f64, seed: u64, salt: u64) -> Vec<TracePoint> {
    assert!(n0 >= 2 && cap >= n0);
    let mut alive: Vec<(usize, Counts)> = (0..cands.len()).map(|i| (i, Counts::new(bits))).collect();
    let mut out = Vec::new();
    let (mut n, mut inc, mut round) = (0u64, n0, 0u64);
    while !alive.is_empty() {
        let cseed = |i: usize| stream_seed(stream_seed(seed, (salt << 24) ^ i as u64), round);
        alive = alive
            .into_par_iter()
            .map(|(i, c)| {
                let more = with_mixer!(&cands[i], f => sampled(&f, bits, inc, cseed(i)));
                (i, c.merge(more))
            })
            .collect();
        n += inc;
        round += 1;
        let mut still = Vec::new();
        for (i, c) in alive {
            let mh = c.ms_debiased();
            out.push(TracePoint { cand: i, n, ms_hat: mh });
            if (mh - inc_ms).abs() <= zmax * sd_ms(mh, n, bits) && n < cap {
                still.push((i, c));
            }
        }
        alive = still;
        inc = n.min(cap.saturating_sub(n));
    }
    out
}
