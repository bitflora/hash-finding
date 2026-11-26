//! Read batches of three-round 32-bit specs from stdin and use the harness's
//! sampled counter. Each batch starts with "log2_samples seed candidates".
use mixlab::{exact, fast32, mixers::Xmx};
use rayon::prelude::*;
use std::io::{self, BufRead, Write};

fn main() {
    let cpu = match std::env::args().nth(1).as_deref() {
        Some("--cpu") => true,
        Some("--gpu") => false,
        _ => panic!("expected --cpu or --gpu"),
    };
    if !cpu {
        let device = exact::set_backend(exact::Backend::Gpu).expect("CUDA backend");
        eprintln!("Sampling on {}", device.unwrap());
    }

    let mut lines = io::stdin().lock().lines().map(Result::unwrap);
    let mut out = io::BufWriter::new(io::stdout().lock());
    while let Some(header) = lines.next() {
        let fields: Vec<u64> = header
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let [lg, seed, len] = fields[..] else {
            panic!("expected log2_samples seed candidates");
        };
        assert!((1..=32).contains(&lg));
        let n = 1 << lg;
        let candidates: Vec<_> = (&mut lines)
            .take(len as usize)
            .map(|s| {
                let x = Xmx::parse(32, &s).expect("valid 32-bit spec");
                assert_eq!(x.rounds(), 3);
                x
            })
            .collect();
        assert_eq!(candidates.len(), len as usize);
        let counts = if cpu {
            candidates
                .par_iter()
                .map(|x| fast32::sampled_xmx(x, n, seed))
                .collect()
        } else {
            exact::sampled_batch(&candidates, n, &vec![seed; candidates.len()])
        };
        for c in counts {
            writeln!(out, "{:.17e}", c.ms_debiased()).unwrap();
        }
        out.flush().unwrap();
    }
}
