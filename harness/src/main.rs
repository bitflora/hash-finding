use mixlab::bias::{estimate_exact, estimate_sampled, Estimate};
use mixlab::extra::{bic_sampled, counter_inputs, lowweight_inputs, rrc_inputs, structured_noise_units};
use mixlab::mixers::Xmx;
use mixlab::refs::{self, Kind};
use mixlab::rng::Xoshiro256;
use mixlab::search::{search, space_size, sweep, SearchCfg};
use mixlab::with_mixer;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

const USAGE: &str = "\
mixlab — avalanche-bias evaluation and search for xorshift-multiply mixers

USAGE:
  mixlab refs   [--samples N] [--exact32] [--seed S] [--extra]
  mixlab eval   (--name REF | --bits B --spec \"[s m s m s]\") [--exact] [--samples N] [--seed S] [--extra]
  mixlab sweep  --bits B [--rounds R] (--exhaustive | --random N) [--seed S] --out FILE.csv
  mixlab search --bits B [--rounds R] [--start SPEC] [--iters N] [--neighbors K]
                [--levels 16,18,20] [--final-samples LOG2] [--seed S] --out FILE.csv
  mixlab surrogate --bits B --spec SPEC [--compare]           # analytic (sampling-free) bias prediction
  mixlab surrogate --bits 32 --list FILE.csv [--baseline p20] # rank-correlate vs published exact biases
  mixlab surrogate --bits 16 --random N [--top 0.1] [--baseline p12] [--seed S] [--out F.csv]
  mixlab scale  (--list FILE.csv | --bits 24 --random N) [--widths 12,16,20,24] [--maps offset:low,prop:high] [--truth]
                [--baseline] [--seed S] --out F.csv   # exact small-width projections vs 32-bit exact
  mixlab race   --bits B --start SPEC [--iters N] [--neighbors K] [--n0 p14] [--cap p28] [--z 3]
                [--verify-samples p31] [--candidates FILE] [--patience P] [--seed S] --out F.csv  # sequential racing search
  mixlab paired --bits B --base SPEC [--others FILE | --neighbours K | --random K] [--samples p20]
                [--reps 32] [--truth] [--seed S] [--out F.csv]  # variance of A-B with/without common inputs

Sample counts may be given as 2^k via 'p<k>', e.g. --samples p24.
Search with bits<=32 uses exact final evaluation unless --final-samples is set.";

struct Args {
    cmd: String,
    kv: HashMap<String, String>,
}

impl Args {
    fn parse() -> Args {
        let mut it = std::env::args().skip(1);
        let cmd = it.next().unwrap_or_default();
        let mut kv = HashMap::new();
        let rest: Vec<String> = it.collect();
        let mut i = 0;
        while i < rest.len() {
            let k = rest[i].trim_start_matches("--").to_string();
            if i + 1 < rest.len() && !rest[i + 1].starts_with("--") {
                kv.insert(k, rest[i + 1].clone());
                i += 2;
            } else {
                kv.insert(k, String::new());
                i += 1;
            }
        }
        Args { cmd, kv }
    }
    fn has(&self, k: &str) -> bool {
        self.kv.contains_key(k)
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.kv.get(k).map(|s| s.as_str())
    }
    fn num(&self, k: &str, default: u64) -> u64 {
        match self.get(k) {
            None => default,
            Some(v) => parse_num(v).unwrap_or_else(|| die(&format!("bad number for --{k}: {v}"))),
        }
    }
}

fn parse_num(v: &str) -> Option<u64> {
    if let Some(p) = v.strip_prefix('p') {
        p.parse::<u32>().ok().map(|k| 1u64 << k)
    } else if let Some(h) = v.strip_prefix("0x") {
        u64::from_str_radix(h, 16).ok()
    } else {
        v.parse().ok()
    }
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}\n\n{USAGE}");
    std::process::exit(2)
}

fn fmt_est(e: &Estimate) -> String {
    if e.exact {
        format!("exact  bias={:.17}  max|diff|={:.3e}", e.score, e.max_abs)
    } else {
        format!(
            "n=2^{:.1}  bias={:.5} ± {:.5} (debiased; raw {:.5})  max|diff|={:.3e}",
            (e.n as f64).log2(),
            e.debiased,
            e.se,
            e.score,
            e.max_abs
        )
    }
}

fn evaluate<F: Fn(u64) -> u64 + Sync>(f: &F, bits: u32, a: &Args, force_exact: bool) -> String {
    let seed = a.num("seed", 1);
    let t = Instant::now();
    let exact = (force_exact || a.has("exact")) && bits <= 32;
    let e = if exact {
        estimate_exact(f, bits)
    } else {
        let default = if bits <= 16 { 1 << 20 } else { 1 << 24 };
        estimate_sampled(f, bits, a.num("samples", default), seed, 16)
    };
    let mut s = format!("{}  [{:.1}s]", fmt_est(&e), t.elapsed().as_secs_f64());
    if a.has("extra") {
        // keep structured sets far smaller than the domain
        let cn = 1u64 << (bits / 2).min(16);
        let ctr = structured_noise_units(f, bits, &counter_inputs(bits, cn));
        let rrc = structured_noise_units(f, bits, &rrc_inputs(bits, cn));
        let lw = structured_noise_units(f, bits, &lowweight_inputs(bits));
        let bic = bic_sampled(f, bits, 1 << 16, seed ^ 0xb1c);
        let (wi, wj, wk, wc) = bic.worst;
        s += &format!(
            "\n      structured avalanche (noise units, ideal≈1): counter={ctr:.2} rrc={rrc:.2} lowweight={lw:.2}\
             \n      BIC (n=2^16): rms z={:.3} (ideal≈1)  max|z|={:.2} (ideal≈{:.2}) worst: in {wi} -> out {wj},{wk} corr {wc:+.4}",
            bic.rms_z, bic.max_z, bic.expected_max_z
        );
    }
    s
}

fn eval_ref(r: &refs::Reference, a: &Args, force_exact: bool) -> String {
    match &r.kind {
        Kind::Xmx(x) => eval_xmx(x, a, force_exact),
        Kind::Custom(g) => evaluate(g, r.bits, a, force_exact),
    }
}

/// Uses the SIMD exact path for xmx mixers of width <= 32, then the generic
/// path for the --extra metrics.
fn eval_xmx(x: &Xmx, a: &Args, force_exact: bool) -> String {
    if (force_exact || a.has("exact")) && (3..=32).contains(&x.bits) {
        let t = Instant::now();
        let c = mixlab::fast32::exact_xmx(x, true);
        let e = Estimate { exact: true, n: c.n, score: c.score(), debiased: c.score(), se: 0.0, max_abs: c.max_abs() };
        let mut s = format!("{}  [{:.1}s]", fmt_est(&e), t.elapsed().as_secs_f64());
        if a.has("extra") {
            let mut b = Args { cmd: a.cmd.clone(), kv: a.kv.clone() };
            b.kv.remove("exact");
            b.kv.insert("samples".into(), "p16".into());
            let more = with_mixer!(x, f => evaluate(&f, x.bits, &b, false));
            s += &more[more.find('\n').unwrap_or(more.len())..];
        }
        s
    } else {
        with_mixer!(x, f => evaluate(&f, x.bits, a, force_exact))
    }
}

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "refs" => {
            for r in refs::all() {
                let exact = r.bits <= 16 || (r.bits == 32 && a.has("exact32"));
                let pubd = r.published.map(|p| format!("  (published {p:.17})")).unwrap_or_default();
                println!("{:<16} {:>2}-bit  {}{}", r.name, r.bits, eval_ref(&r, &a, exact), pubd);
            }
        }
        "eval" => {
            if let Some(name) = a.get("name") {
                let r = refs::by_name(name).unwrap_or_else(|| die(&format!("unknown reference {name}")));
                println!("{}  {}", r.name, eval_ref(&r, &a, false));
            } else {
                let bits = a.num("bits", 32) as u32;
                let spec = a.get("spec").unwrap_or_else(|| die("need --spec or --name"));
                let x = Xmx::parse(bits, spec).unwrap_or_else(|e| die(&e));
                println!("{x}  {}", eval_xmx(&x, &a, false));
            }
        }
        "sweep" => {
            let bits = a.num("bits", 8) as u32;
            let rounds = a.num("rounds", 2) as usize;
            let out = a.get("out").unwrap_or_else(|| die("need --out"));
            let mut w = BufWriter::new(File::create(out).expect("create output"));
            let total = space_size(bits, rounds);
            let t = Instant::now();
            let report = |d: u64| eprint!("\r{d} configs  [{:.0}s]   ", t.elapsed().as_secs_f64());
            let n = if a.has("exhaustive") {
                if total > 1u128 << 34 {
                    die(&format!("space has {total} configs; too large for --exhaustive"));
                }
                sweep(bits, rounds, 0..total, &mut w, report)
            } else {
                let n = a.num("random", 100_000);
                let mut rng = Xoshiro256::new(a.num("seed", 1));
                let idx = (0..n).map(move |_| {
                    let hi = rng.next_u64() as u128;
                    let lo = rng.next_u64() as u128;
                    ((hi << 64) | lo) % total
                });
                sweep(bits, rounds, idx, &mut w, report)
            }
            .expect("write");
            eprintln!("\nwrote {n} rows to {out} (space size {total})");
        }
        "search" => {
            let bits = a.num("bits", 32) as u32;
            let rounds = a.num("rounds", 2) as usize;
            let levels: Vec<u64> = a
                .get("levels")
                .unwrap_or(if bits <= 32 { "16,18,20,22" } else { "16,18,20,22,24" })
                .split(',')
                .map(|k| 1u64 << k.trim().parse::<u32>().unwrap_or_else(|_| die("bad --levels")))
                .collect();
            let final_samples = match a.get("final-samples") {
                Some(k) => Some(1u64 << k.parse::<u32>().unwrap_or_else(|_| die("bad --final-samples"))),
                None if bits <= 32 => None,
                None => Some(1u64 << 26),
            };
            let cfg = SearchCfg {
                bits,
                rounds,
                iters: a.num("iters", 50) as usize,
                neighbors: a.num("neighbors", 96) as usize,
                levels,
                keep_frac: 0.25,
                finalists: a.num("finalists", 2) as usize,
                final_samples,
                seed: a.num("seed", 1),
            };
            let start = a.get("start").map(|s| Xmx::parse(bits, s).unwrap_or_else(|e| die(&e)));
            let out = a.get("out").unwrap_or_else(|| die("need --out"));
            let mut w = BufWriter::new(File::create(out).expect("create output"));
            writeln!(w, "# {cfg:?}").unwrap();
            writeln!(w, "iter,event,config,score").unwrap();
            let t = Instant::now();
            let (best, score) = search(&cfg, start, |line| {
                writeln!(w, "{line}").unwrap();
                w.flush().unwrap();
                eprintln!("[{:>6.0}s] {line}", t.elapsed().as_secs_f64());
            });
            println!("best {best}  score {score:.10}");
        }
        "surrogate" => surrogate_cmd(&a),
        "scale" => scale_cmd(&a),
        "paired" => paired_cmd(&a),
        "design" => design_cmd(&a),
        "race" => race_cmd(&a),
        _ => {
            println!("{USAGE}");
        }
    }
}

fn sampled_debiased(x: &Xmx, n: u64, seed: u64) -> f64 {
    with_mixer!(x, f => {
        let c = mixlab::bias::sampled_serial(&f, x.bits, n, seed);
        1000.0 * c.ms_debiased().max(0.0).sqrt()
    })
}

fn report_rank(label: &str, truth: &[f64], pred: &[f64]) {
    println!(
        "  {label:<28} spearman={:+.4}  pearson={:+.4}  (n={})",
        mixlab::surrogate::spearman(truth, pred),
        mixlab::surrogate::pearson(truth, pred),
        truth.len()
    );
}

fn surrogate_cmd(a: &Args) {
    use mixlab::surrogate;
    use rayon::prelude::*;
    let bits = a.num("bits", 32) as u32;
    let seed = a.num("seed", 1);
    if let Some(spec) = a.get("spec") {
        let x = Xmx::parse(bits, spec).unwrap_or_else(|e| die(&e));
        let t = Instant::now();
        let pm = if let Some(mode) = a.get("hybrid") {
            let mode: surrogate::Hybrid = mode.parse().unwrap_or_else(|e: String| die(&e));
            let n = a.num("samples", 1 << 16);
            let (m, se) = surrogate::hybrid_matrix(&x, mode, n, seed);
            println!(
                "{x}  hybrid {mode:?} n=2^{:.0}/row  debiased bias={:.6}  raw={:.6}  [{:.2} s]",
                (n as f64).log2(),
                surrogate::debiased_score(&m, &se),
                1000.0 * (m.iter().map(|d| d * d).sum::<f64>() / m.len() as f64).sqrt(),
                t.elapsed().as_secs_f64()
            );
            m
        } else {
            let m = surrogate::matrix(&x);
            let ps = 1000.0 * (m.iter().map(|d| d * d).sum::<f64>() / m.len() as f64).sqrt();
            println!("{x}  surrogate bias={ps:.6}  [{:.2} ms]", t.elapsed().as_secs_f64() * 1e3);
            m
        };
        if a.has("compare") && (3..=32).contains(&bits) {
            let c = mixlab::fast32::exact_xmx(&x, true);
            let ex: Vec<f64> = c.diffs().collect();
            let pe: Vec<f64> = pm.iter().map(|v| v.abs()).collect();
            let ee: Vec<f64> = ex.iter().map(|v| v.abs()).collect();
            println!("  exact bias={:.6}", c.score());
            report_rank("per-entry signed diff", &ex, &pm);
            report_rank("per-entry |diff|", &ee, &pe);
            // which entries dominate each
            let top = |v: &[f64]| {
                let mut i: Vec<usize> = (0..v.len()).collect();
                i.sort_by(|&p, &q| v[q].abs().partial_cmp(&v[p].abs()).unwrap());
                i.into_iter().take(5).map(|t| format!("({},{}) {:+.2e}", t / bits as usize, t % bits as usize, v[t])).collect::<Vec<_>>().join("  ")
            };
            println!("  top exact entries (in,out):     {}", top(&ex));
            println!("  top surrogate entries (in,out): {}", top(&pm));
        }
        return;
    }
    let baseline = a.get("baseline").map(|v| parse_num(v).unwrap_or_else(|| die("bad --baseline")));
    let (configs, truth): (Vec<Xmx>, Vec<f64>) = if let Some(path) = a.get("list") {
        let text = std::fs::read_to_string(path).expect("read list");
        text.lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut it = l.split(',');
                let spec = it.next().unwrap();
                let v: f64 = it.next().unwrap().parse().expect("bias");
                (Xmx::parse(bits, spec).unwrap_or_else(|e| die(&e)), v)
            })
            .unzip()
    } else {
        let n = a.num("random", 2000);
        let rounds = a.num("rounds", 2) as usize;
        let mut rng = Xoshiro256::new(seed);
        let total = space_size(bits, rounds);
        let cfgs: Vec<Xmx> = (0..n)
            .map(|_| {
                let r = ((rng.next_u64() as u128) << 64 | rng.next_u64() as u128) % total;
                mixlab::search::config_from_index(bits, rounds, r)
            })
            .collect();
        let tr: Vec<f64> = cfgs.par_iter().map(|x| mixlab::search::exact_score_serial(x).0).collect();
        (cfgs, tr)
    };
    let t = Instant::now();
    let pred: Vec<f64> = configs.par_iter().map(surrogate::score).collect();
    let ts = t.elapsed().as_secs_f64() / configs.len() as f64;
    println!("surrogate: {:.3} ms/config (12 threads amortised)", ts * 1e3);
    report_rank("surrogate vs exact (all)", &truth, &pred);
    let base: Option<Vec<f64>> = baseline.map(|n| {
        let t = Instant::now();
        let v: Vec<f64> = configs.par_iter().enumerate().map(|(i, x)| sampled_debiased(x, n, seed ^ i as u64)).collect();
        println!(
            "sampling baseline n=2^{:.0}: {:.3} ms/config",
            (n as f64).log2(),
            t.elapsed().as_secs_f64() / configs.len() as f64 * 1e3
        );
        report_rank("sampled vs exact (all)", &truth, &v);
        v
    });
    if let Some(frac) = a.get("top").and_then(|v| v.parse::<f64>().ok()) {
        let mut idx: Vec<usize> = (0..truth.len()).collect();
        idx.sort_by(|&i, &j| truth[i].partial_cmp(&truth[j]).unwrap());
        idx.truncate(((truth.len() as f64 * frac).ceil() as usize).max(3));
        let tt: Vec<f64> = idx.iter().map(|&i| truth[i]).collect();
        let pp: Vec<f64> = idx.iter().map(|&i| pred[i]).collect();
        report_rank(&format!("surrogate, best {:.0}% only", frac * 100.0), &tt, &pp);
        if let Some(b) = &base {
            let bb: Vec<f64> = idx.iter().map(|&i| b[i]).collect();
            report_rank(&format!("sampled, best {:.0}% only", frac * 100.0), &tt, &bb);
        }
    }
    if let Some(out) = a.get("out") {
        let mut w = BufWriter::new(File::create(out).expect("create"));
        writeln!(w, "config,exact,surrogate{}", if base.is_some() { ",sampled" } else { "" }).unwrap();
        for i in 0..configs.len() {
            let b = base.as_ref().map(|b| format!(",{:.8}", b[i])).unwrap_or_default();
            writeln!(w, "\"{}\",{:.10},{:.6e}{}", configs[i], truth[i], pred[i], b).unwrap();
        }
    }
}

/// Width-scaling study. Input CSV: spec,exact_bias[,group]. Rows with
/// exact_bias 0 get it computed (32-bit exact) when --truth is given.
fn scale_cmd(a: &Args) {
    use mixlab::scale::{project, MultMap, ShiftMap};
    use mixlab::surrogate::spearman;
    use rayon::prelude::*;
    let path = a.get("list");
    let seed = a.num("seed", 1);
    let bits = a.num("bits", 32) as u32;
    let mut rows: Vec<(Xmx, f64, String)> = if let Some(n) = a.get("random") {
        // random 2-round configs at `bits` with plausible shifts (half-width ± 3)
        let n: usize = n.parse().unwrap_or_else(|_| die("bad --random"));
        let mut rng = Xoshiro256::new(seed);
        (0..n)
            .map(|_| {
                let shifts = (0..3).map(|_| bits / 2 - 3 + rng.below(7) as u32).collect();
                let mults = (0..2).map(|_| (rng.next_u64() & mixlab::mixers::mask(bits)) | 1).collect();
                (Xmx { bits, shifts, mults }, 0.0, "random".to_string())
            })
            .collect()
    } else {
        let path = path.unwrap_or_else(|| die("need --list or --random"));
        let text = std::fs::read_to_string(path).expect("read list");
        text.lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                let x = Xmx::parse(bits, f[0].trim_matches('"')).unwrap_or_else(|e| die(&e));
                let v: f64 = f.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let g = f.get(2).map(|s| s.to_string()).unwrap_or_else(|| "all".into());
                (x, v, g)
            })
            .collect()
    };
    let missing = rows.iter().filter(|r| r.1 == 0.0).count();
    if missing > 0 {
        if !a.has("truth") && !a.has("random") {
            die(&format!("{missing} rows have no exact bias; pass --truth to compute them"));
        }
        let t = Instant::now();
        eprintln!("computing {missing} exact {bits}-bit truths...");
        let todo: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].1 == 0.0).collect();
        let vals: Vec<f64> = todo.par_iter().map(|&i| mixlab::search::exact_score_serial(&rows[i].0).0).collect();
        for (i, v) in todo.into_iter().zip(vals) {
            rows[i].1 = v;
        }
        eprintln!("  done [{:.0}s]", t.elapsed().as_secs_f64());
    }
    let widths: Vec<u32> = a
        .get("widths")
        .unwrap_or("12,16,20,24")
        .split(',')
        .map(|w| w.trim().parse().unwrap_or_else(|_| die("bad --widths")))
        .collect();
    let maps: Vec<(ShiftMap, MultMap)> = a
        .get("maps")
        .unwrap_or("offset:low,offset:high,prop:low,prop:high")
        .split(',')
        .map(|m| {
            let (s, t) = m.split_once(':').unwrap_or_else(|| die("bad --maps"));
            (s.parse().unwrap_or_else(|e: String| die(&e)), t.parse().unwrap_or_else(|e: String| die(&e)))
        })
        .collect();
    let truth: Vec<f64> = rows.iter().map(|r| r.1).collect();
    let mut cols: Vec<(String, Vec<f64>, f64)> = Vec::new(); // name, values, ms/config
    for &w in &widths {
        for &(sm, mm) in &maps {
            let t = Instant::now();
            let v: Vec<f64> = rows
                .par_iter()
                .map(|r| mixlab::search::exact_score_serial(&project(&r.0, w, sm, mm)).0)
                .collect();
            let ms = t.elapsed().as_secs_f64() * 1e3 / rows.len() as f64;
            cols.push((format!("w{w}_{sm:?}_{mm:?}").to_lowercase(), v, ms));
        }
        if a.has("baseline") {
            // equal cost: exact at width w evaluates (w+1)*2^w mixer calls;
            // sampling at 32 bits costs 33 calls per input.
            let n = (((w as u64 + 1) << w) / (bits as u64 + 1)).max(64);
            let t = Instant::now();
            let v: Vec<f64> =
                rows.par_iter().enumerate().map(|(i, r)| sampled_debiased(&r.0, n, seed ^ (i as u64 * 7919))).collect();
            let ms = t.elapsed().as_secs_f64() * 1e3 / rows.len() as f64;
            cols.push((format!("sampled{bits}_eqcost_w{w}"), v, ms));
        }
    }
    // groups: each distinct group label, everything, the best half and best tenth overall
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    let mut labels: Vec<String> = rows.iter().map(|r| r.2.clone()).collect();
    labels.sort();
    labels.dedup();
    let all: Vec<usize> = (0..rows.len()).collect();
    groups.push(("all".into(), all.clone()));
    let mut best = all.clone();
    best.sort_by(|&i, &j| truth[i].partial_cmp(&truth[j]).unwrap());
    let mut tenth = best.clone();
    tenth.truncate(rows.len() / 10);
    best.truncate(rows.len() / 2);
    groups.push(("best half".into(), best));
    if rows.len() >= 100 {
        groups.push(("best 10%".into(), tenth));
    }
    if labels.len() > 1 {
        for l in labels {
            groups.push((l.clone(), all.iter().copied().filter(|&i| rows[i].2 == l).collect()));
        }
    }
    print!("{:<28} {:>9}", "predictor", "ms/cfg");
    for (g, idx) in &groups {
        print!(" {:>14}", format!("{} ({})", &g[..g.len().min(9)], idx.len()));
    }
    println!();
    for (name, v, ms) in &cols {
        print!("{name:<28} {ms:>9.2}");
        for (_, idx) in &groups {
            let tt: Vec<f64> = idx.iter().map(|&i| truth[i]).collect();
            let pp: Vec<f64> = idx.iter().map(|&i| v[i]).collect();
            print!(" {:>14}", format!("{:+.3}", spearman(&tt, &pp)));
        }
        println!();
    }
    if let Some(out) = a.get("out") {
        let mut w = BufWriter::new(File::create(out).expect("create"));
        write!(w, "spec,exact_bias,group").unwrap();
        for (n, _, _) in &cols {
            write!(w, ",{n}").unwrap();
        }
        writeln!(w).unwrap();
        for (i, r) in rows.iter().enumerate() {
            write!(w, "{},{:.17},{}", r.0, r.1, r.2).unwrap();
            for (_, v, _) in &cols {
                write!(w, ",{:.10e}", v[i]).unwrap();
            }
            writeln!(w).unwrap();
        }
    }
}

/// Paired-comparison noise experiment.
fn paired_cmd(a: &Args) {
    use mixlab::racing::{describe_move, paired};
    use rayon::prelude::*;
    let bits = a.num("bits", 32) as u32;
    let seed = a.num("seed", 1);
    let base = Xmx::parse(bits, a.get("base").unwrap_or_else(|| die("need --base"))).unwrap_or_else(|e| die(&e));
    let mut rng = Xoshiro256::new(seed ^ 0xabc);
    let others: Vec<Xmx> = if let Some(path) = a.get("others") {
        std::fs::read_to_string(path)
            .expect("read")
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| Xmx::parse(bits, l.split(',').next().unwrap().trim_matches('"')).unwrap_or_else(|e| die(&e)))
            .collect()
    } else if let Some(k) = a.get("random") {
        let k: usize = k.parse().unwrap_or_else(|_| die("bad --random"));
        (0..k).map(|_| mixlab::search::random_config(bits, base.rounds(), &mut rng)).collect()
    } else {
        let k = a.num("neighbours", 24) as usize;
        let mut v = Vec::new();
        while v.len() < k {
            let y = mixlab::search::neighbor(&base, &mut rng);
            if y.to_string() != base.to_string() && !v.iter().any(|z: &Xmx| z.to_string() == y.to_string()) {
                v.push(y);
            }
        }
        v
    };
    let n = a.num("samples", 1 << 20);
    let reps = a.num("reps", 32) as usize;
    let t = Instant::now();
    let st = paired(&base, &others, n, reps, seed);
    eprintln!("sampling done [{:.0}s]", t.elapsed().as_secs_f64());
    let truth: Option<(f64, Vec<f64>)> = (a.has("truth") && bits <= 32).then(|| {
        let t = Instant::now();
        let tb = mixlab::fast32::exact_xmx(&base, true).ms();
        let to: Vec<f64> = others.par_iter().map(|x| mixlab::fast32::exact_xmx(x, false).ms()).collect();
        eprintln!("exact truth done [{:.0}s]", t.elapsed().as_secs_f64());
        (tb, to)
    });
    println!(
        "base {base}  n=2^{:.0}  reps={reps}   (ms values x1e9; ratio = var_ind/var_crn = sample-count saving)",
        (n as f64).log2()
    );
    println!("{:<34} {:>10} {:>10} {:>10} {:>10} {:>8}", "move", "mean dms", "true dms", "sd_ind", "sd_crn", "ratio");
    let mut lines = Vec::new();
    for (i, s) in st.iter().enumerate() {
        let tru = truth.as_ref().map(|(tb, to)| tb - to[i]);
        let mv = if a.has("random") || a.has("others") { others[i].to_string() } else { describe_move(&base, &others[i]) };
        println!(
            "{:<34} {:>10.4} {:>10} {:>10.4} {:>10.4} {:>8.2}",
            &mv[..mv.len().min(34)],
            s.mean * 1e9,
            tru.map(|v| format!("{:.4}", v * 1e9)).unwrap_or("-".into()),
            s.var_ind.sqrt() * 1e9,
            s.var_crn.sqrt() * 1e9,
            s.var_ind / s.var_crn
        );
        lines.push(format!(
            "\"{}\",\"{}\",{:.6e},{},{:.6e},{:.6e},{:.6e}",
            others[i],
            mv,
            s.mean,
            tru.map(|v| format!("{v:.6e}")).unwrap_or_default(),
            s.var_ind,
            s.var_crn,
            s.var_b
        ));
    }
    let mut r: Vec<f64> = st.iter().map(|s| s.var_ind / s.var_crn).collect();
    r.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let gm = (r.iter().map(|x| x.ln()).sum::<f64>() / r.len() as f64).exp();
    println!("variance ratio: min {:.2}  median {:.2}  max {:.2}  geometric mean {:.2}", r[0], r[r.len() / 2], r[r.len() - 1], gm);
    if let Some(out) = a.get("out") {
        let mut w = BufWriter::new(File::create(out).expect("create"));
        writeln!(w, "# base {base} n={n} reps={reps} seed={seed}").unwrap();
        writeln!(w, "other,move,mean_dms,true_dms,var_ind,var_crn,var_b").unwrap();
        for l in lines {
            writeln!(w, "{l}").unwrap();
        }
    }
}

/// Variance of the raw ms estimate under different input designs.
fn design_cmd(a: &Args) {
    use mixlab::racing::{ms_design, Design};
    use mixlab::rng::stream_seed;
    use rayon::prelude::*;
    let bits = a.num("bits", 32) as u32;
    let x = Xmx::parse(bits, a.get("spec").unwrap_or_else(|| die("need --spec"))).unwrap_or_else(|e| die(&e));
    let n = a.num("samples", 1 << 20);
    let reps = a.num("reps", 32) as usize;
    let seed = a.num("seed", 1);
    let k = a.num("k", 8) as u32;
    let truth = (a.has("truth") && bits <= 32).then(|| mixlab::fast32::exact_xmx(&x, true).ms());
    println!("{x}  n=2^{:.0} reps={reps}  (ms x1e9; plain-sampling noise floor 1/n = {:.4})", (n as f64).log2(), 1e9 / n as f64);
    if let Some(t) = truth {
        println!("  exact ms = {:.4}", t * 1e9);
    }
    for d in [Design::Plain, Design::LowBits(k), Design::Affine(k)] {
        let v: Vec<f64> = (0..reps).into_par_iter().map(|r| ms_design(&x, n, stream_seed(seed, r as u64), d)).collect();
        let m = v.iter().sum::<f64>() / reps as f64;
        let sd = (v.iter().map(|y| (y - m).powi(2)).sum::<f64>() / (reps - 1) as f64).sqrt();
        println!("  {:<12} mean raw ms {:>10.4}  sd {:>8.4}   theory sd for plain sqrt(2/N)/n = {:.4}", format!("{d:?}"), m * 1e9, sd * 1e9,
            (2.0 / (bits * bits) as f64).sqrt() / n as f64 * 1e9);
    }
}

fn race_cmd(a: &Args) {
    use mixlab::racing::{race, race_search, RaceCfg};
    let bits = a.num("bits", 32) as u32;
    let start = Xmx::parse(bits, a.get("start").unwrap_or_else(|| die("need --start"))).unwrap_or_else(|e| die(&e));
    let cfg = RaceCfg {
        bits,
        n0: a.num("n0", 1 << 14),
        cap: a.num("cap", if bits <= 32 { 1 << 28 } else { 1 << 31 }),
        z: a.get("z").map(|z| z.parse().unwrap_or_else(|_| die("bad --z"))).unwrap_or(3.0),
        verify_samples: match a.get("verify-samples") {
            Some(v) => Some(parse_num(v).unwrap_or_else(|| die("bad --verify-samples"))),
            None if bits <= 32 => None,
            None => Some(1 << 32),
        },
        seed: a.num("seed", 1),
    };
    let t = Instant::now();
    if let Some(path) = a.get("candidates") {
        // one race of a fixed candidate list against --start (validation)
        let cands: Vec<Xmx> = std::fs::read_to_string(path)
            .expect("read")
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| Xmx::parse(bits, l.split(',').next().unwrap().trim_matches('"')).unwrap_or_else(|e| die(&e)))
            .collect();
        let inc = if cfg.verify_samples.is_none() { mixlab::fast32::exact_xmx(&start, true).ms() } else { die("--candidates needs bits<=32") };
        let r = race(&cfg, inc, 0.0, cands, 1);
        println!("{cfg:?}
incumbent {start} ms={:.6e}", inc);
        println!("{:<34} {:>9} {:>8} {:>12} {:>12}", "candidate", "verdict", "log2 n", "ms_hat", "verified");
        for d in &r.decisions {
            println!(
                "{:<34} {:>9} {:>8.1} {:>12.4e} {:>12}",
                d.cand.to_string(),
                format!("{:?}", d.verdict),
                (d.n as f64).log2(),
                d.ms_hat,
                d.verified_ms.map(|v| format!("{v:.4e}")).unwrap_or("-".into())
            );
        }
        println!(
            "accepted: {}  inputs=2^{:.2}  exact evals={}  [{:.0}s]",
            r.accepted.map(|(x, m)| format!("{x} ms={m:.6e}")).unwrap_or("none".into()),
            (r.inputs as f64).log2(),
            r.exact_evals,
            t.elapsed().as_secs_f64()
        );
        return;
    }
    let out = a.get("out").unwrap_or_else(|| die("need --out"));
    let mut w = BufWriter::new(File::create(out).expect("create output"));
    writeln!(w, "# {cfg:?}").unwrap();
    writeln!(w, "iter,event,config,score,detail").unwrap();
    let (best, score) = race_search(&cfg, start, a.num("iters", 20) as usize, a.num("neighbors", 96) as usize, a.num("patience", 1000) as usize, |line| {
        writeln!(w, "{line}").unwrap();
        w.flush().unwrap();
        eprintln!("[{:>6.0}s] {line}", t.elapsed().as_secs_f64());
    });
    println!("best {best}  score {score:.10}");
}
