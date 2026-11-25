//! Independent candidate populations, with fixed-budget screening and fresh verification.
use mixlab::{
    bias::estimate_sampled,
    mixers::{mask, Xmx},
    refs::{self, Kind},
    rng::{stream_seed, Xoshiro256},
    with_mixer,
};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{BufWriter, Write},
    time::Instant,
};

const HELP: &str = "seed-search --source spectral|random|matched|mixer --bits 32|64 --out FILE.csv
  [--count 64] [--seed 1] [--shifts 15,16,17] [--min-mult-bits B]
  [--samples p18] [--verify 0] [--verify-samples p26] [--generate-only] [--gpu]
Default minimum multiplier length is B (full width); lower it to include short table constants.
CSV is compatible with mixlab race --candidates (32 bits). Verification is exact at 32 bits,
fresh sampled at 64 bits; --gpu runs the exact verification on the GPU. Fixed-budget screening can miss good candidates; no record claims.";

fn number(s: &str) -> Result<u64, String> {
    if let Some(p) = s.strip_prefix('p') {
        p.parse::<u32>()
            .ok()
            .and_then(|k| 1u64.checked_shl(k))
            .ok_or_else(|| format!("bad count {s}"))
    } else {
        s.parse().map_err(|_| format!("bad number {s}"))
    }
}

fn spectral(bits: u32, minimum: u32) -> Vec<u64> {
    include_str!("../../data/spectral.csv")
        .lines()
        .skip(1)
        .filter_map(|line| {
            let c: Vec<_> = line.split(',').collect();
            let b: u32 = c[0].parse().unwrap();
            let len: u32 = c[2].parse().unwrap();
            (b == bits && len >= minimum).then(|| u64::from_str_radix(c[3], 16).unwrap())
        })
        .collect()
}

fn population(
    source: &str,
    bits: u32,
    minimum: u32,
    shifts: &[u32],
    count: usize,
    seed: u64,
) -> Result<Vec<Xmx>, String> {
    let mut pool = spectral(bits, minimum);
    if source == "mixer" {
        pool = refs::all()
            .into_iter()
            .filter(|r| r.bits == bits)
            .filter_map(|r| {
                if let Kind::Xmx(x) = r.kind {
                    (x.rounds() == 2).then_some(x.mults)
                } else {
                    None
                }
            })
            .flatten()
            .filter(|m| 64 - m.leading_zeros() >= minimum)
            .collect();
    }
    pool.sort_unstable();
    pool.dedup();
    if pool.is_empty() {
        return Err("no multiplier seeds at this width/minimum".into());
    }
    let mut rng = Xoshiro256::new(seed);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    // Enumerate the small finite populations before shuffling: no rejection bias or hangs.
    if source == "spectral" || source == "mixer" {
        let capacity = pool.len().pow(2) * shifts.len().pow(3);
        if count > capacity {
            return Err(format!(
                "requested {count}, but population has only {capacity} unique configurations"
            ));
        }
        for &m0 in &pool {
            for &m1 in &pool {
                for &s0 in shifts {
                    for &s1 in shifts {
                        for &s2 in shifts {
                            out.push(Xmx::new(bits, vec![s0, s1, s2], vec![m0, m1])?);
                        }
                    }
                }
            }
        }
        for i in (1..out.len()).rev() {
            let j = rng.below((i + 1) as u64) as usize;
            out.swap(i, j);
        }
        out.truncate(count);
    } else {
        while out.len() < count {
            let ss = (0..3)
                .map(|_| shifts[rng.below(shifts.len() as u64) as usize])
                .collect();
            let mm = (0..2)
                .map(|_| {
                    if source == "matched" {
                        let len = 64 - pool[rng.below(pool.len() as u64) as usize].leading_zeros();
                        (rng.next_u64() & mask(len)) | (1u64 << (len - 1)) | 1
                    } else {
                        (rng.next_u64() & mask(bits)) | 1
                    }
                })
                .collect();
            let x = Xmx::new(bits, ss, mm)?;
            if seen.insert(x.clone()) {
                out.push(x);
            }
        }
    }
    Ok(out)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut kv = HashMap::new();
    while let Some(k) = args.next() {
        if k == "--help" {
            println!("{HELP}");
            return Ok(());
        }
        if k == "--generate-only" || k == "--gpu" {
            kv.insert(k, String::new());
            continue;
        }
        if ![
            "--source",
            "--bits",
            "--out",
            "--count",
            "--seed",
            "--shifts",
            "--min-mult-bits",
            "--samples",
            "--verify",
            "--verify-samples",
        ]
        .contains(&k.as_str())
        {
            return Err(format!("unknown option {k}").into());
        }
        let v = args.next().ok_or("missing option value")?;
        if kv.insert(k.clone(), v).is_some() {
            return Err(format!("duplicate option {k}").into());
        }
    }
    let num = |k: &str, default: u64| -> Result<u64, String> {
        kv.get(k).map_or(Ok(default), |s| number(s))
    };
    let bits_raw = num("--bits", 32)?;
    if bits_raw != 32 && bits_raw != 64 {
        return Err("--bits must be 32 or 64".into());
    }
    let bits = bits_raw as u32;
    let minimum = num("--min-mult-bits", bits_raw)?;
    if minimum == 0 || minimum > bits_raw {
        return Err("invalid --min-mult-bits".into());
    }
    let source = kv.get("--source").map(String::as_str).unwrap_or("spectral");
    if !["spectral", "random", "matched", "mixer"].contains(&source) {
        return Err("invalid --source".into());
    }
    let count = num("--count", 64)?;
    if count == 0 || count > 1_000_000 {
        return Err("--count must be 1..=1000000".into());
    }
    let seed = num("--seed", 1)?;
    let samples = num("--samples", 1 << 18)?;
    let verify_samples = num("--verify-samples", 1 << 26)?;
    if samples < 32 || verify_samples < 32 {
        return Err("sample counts must be >=32".into());
    }
    let verify = num("--verify", 0)?;
    if verify > count {
        return Err("--verify exceeds --count".into());
    }
    let generate = kv.contains_key("--generate-only");
    if kv.contains_key("--gpu") {
        eprintln!("exact verification on GPU: {}", mixlab::exact::use_gpu()?);
    }
    if generate && verify != 0 {
        return Err("--generate-only cannot verify".into());
    }
    let mut shifts = if let Some(s) = kv.get("--shifts") {
        s.split(',')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![bits / 2 - 1, bits / 2, bits / 2 + 1]
    };
    shifts.sort_unstable();
    shifts.dedup();
    if shifts.is_empty() || shifts.iter().any(|&s| s == 0 || s >= bits) {
        return Err("shifts must be in 1..bits".into());
    }
    // Bound Cartesian expansion for user-supplied shift sets.
    if shifts.len() > 9 {
        return Err("at most 9 shift values per run".into());
    }
    let candidates = population(source, bits, minimum as u32, &shifts, count as usize, seed)?;
    let path = kv.get("--out").ok_or("need --out")?;
    let mut writer = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    writeln!(
        writer,
        "spec,source,bits,seed,stage,n,sample_seed,ms,se_ms,seconds"
    )?;
    let t = Instant::now();
    let mut ranked = Vec::new();
    for (i, x) in candidates.iter().enumerate() {
        if generate {
            writeln!(writer, "{x},{source},{bits},{seed},generated,0,,,,")?;
        } else {
            let sample_seed = stream_seed(seed ^ 0x53435245454e, i as u64);
            let e = with_mixer!(x, f => estimate_sampled(&f,bits,samples,sample_seed,16));
            writeln!(
                writer,
                "{x},{source},{bits},{seed},screen,{samples},{sample_seed},{:.17e},{:.17e},{:.6}",
                e.ms,
                e.se_ms,
                t.elapsed().as_secs_f64()
            )?;
            ranked.push((e.ms, i));
        }
        writer.flush()?;
    }
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    // A separate file keeps --candidates input unique and screening output usable after interruption.
    if verify > 0 {
        let mut verified = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(format!("{path}.verified.csv"))?,
        );
        writeln!(
            verified,
            "spec,source,bits,seed,stage,n,sample_seed,ms,se_ms,seconds"
        )?;
        for &(_, i) in ranked.iter().take(verify as usize) {
            let x = &candidates[i];
            let (stage, n, s, ms, se) = if bits == 32 {
                let c = mixlab::exact::exact_xmx(x, true);
                ("exact", c.n, 0, c.ms(), 0.0)
            } else {
                let s = stream_seed(seed ^ 0x564552494659, i as u64);
                let e = with_mixer!(x, f => estimate_sampled(&f,bits,verify_samples,s,16));
                ("fresh-sampled", e.n, s, e.ms, e.se_ms)
            };
            writeln!(
                verified,
                "{x},{source},{bits},{seed},{stage},{n},{s},{ms:.17e},{se:.17e},{:.6}",
                t.elapsed().as_secs_f64()
            )?;
            verified.flush()?;
            eprintln!("{stage}: {x} ms={ms:.8e} se={se:.3e}");
        }
    }
    eprintln!(
        "{source}: {count} candidates in {:.3}s; screen inputs={}",
        t.elapsed().as_secs_f64(),
        if generate {
            0
        } else {
            count as u128 * samples as u128
        }
    );
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}\n{HELP}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_and_population() {
        assert_eq!(spectral(32, 1).len(), 11);
        assert_eq!(spectral(64, 1).len(), 10);
        assert_eq!(spectral(32, 32), vec![0x915f77f5, 0x93d765dd]);
        for bits in [32, 64] {
            for m in spectral(bits, 1) {
                assert!(m & 1 == 1 && m <= mask(bits));
            }
            let shifts = [bits / 2 - 1, bits / 2, bits / 2 + 1];
            for source in ["spectral", "random", "matched", "mixer"] {
                let a = population(source, bits, bits, &shifts, 64, 7).unwrap();
                assert_eq!(a, population(source, bits, bits, &shifts, 64, 7).unwrap());
                assert_eq!(a.iter().collect::<HashSet<_>>().len(), 64);
                for x in &a {
                    assert!(x.mults.iter().all(|m| m & 1 == 1 && *m <= mask(bits)));
                    if source == "spectral" {
                        assert!(x.mults.iter().all(|m| spectral(bits, bits).contains(m)));
                    }
                    if source == "matched" {
                        assert!(x.mults.iter().all(|m| 64 - m.leading_zeros() == bits));
                    }
                }
            }
        }
        assert!(population("spectral", 32, 32, &[16], 5, 1).is_err());
    }
}
