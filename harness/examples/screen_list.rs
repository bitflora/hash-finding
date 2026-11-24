//! Resumable, fixed-budget 32-bit screening with the existing SIMD sampler.
//! Usage: screen_list INPUT.csv OUTPUT.csv LOG2_SAMPLES SEED
//! INPUT's first column must be spec. Rankings use signed debiased ms.
use mixlab::{fast32::sampled_xmx, mixers::Xmx};
use std::{collections::HashSet, fs::OpenOptions, io::Write, time::Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 5, "INPUT.csv OUTPUT.csv LOG2_SAMPLES SEED");
    let lg: u32 = args[3].parse().expect("sample exponent");
    assert!((1..=32).contains(&lg));
    let n = 1u64 << lg;
    let seed: u64 = args[4].parse().expect("seed");
    let prior = match std::fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => panic!("read output: {e}"),
    };
    let mut done = HashSet::new();
    for line in prior.lines().skip(1) {
        let p: Vec<_> = line.split(',').collect();
        assert_eq!(p.len(), 5, "incomplete output row");
        assert_eq!(p[2].parse::<u64>().unwrap(), n, "sample budget changed");
        assert_eq!(p[3].parse::<u64>().unwrap(), seed, "seed changed");
        p[1].parse::<f64>().expect("sampled ms");
        done.insert(p[0].to_string());
    }
    let mut out = OpenOptions::new().create(true).append(true).open(&args[2]).unwrap();
    if prior.is_empty() {
        writeln!(out, "spec,sampled_ms,samples,seed,seconds").unwrap();
    }
    let input = std::fs::read_to_string(&args[1]).expect("read input");
    let start = Instant::now();
    let mut completed = 0;
    for line in input.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        let spec = line.split(',').next().unwrap().trim_matches('"');
        if done.contains(spec) { continue; }
        let x = Xmx::parse(32, spec).expect("valid 32-bit spec");
        let t = Instant::now();
        let c = sampled_xmx(&x, n, seed);
        assert_eq!(c.n, n);
        writeln!(out, "{x},{:.17e},{n},{seed},{:.6}", c.ms_debiased(), t.elapsed().as_secs_f64()).unwrap();
        out.flush().unwrap();
        done.insert(spec.to_string());
        completed += 1;
        if completed % 100 == 0 {
            eprintln!("screened {completed} new configurations [{:.1}s]", start.elapsed().as_secs_f64());
        }
    }
    eprintln!("screen complete: {completed} new configurations [{:.1}s]", start.elapsed().as_secs_f64());
}
