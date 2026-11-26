//! The shift-forced exact-pair rule (FINDINGS F37) against exhaustive truth.
use mixlab::forced::{exact_pairs_exhaustive, forced_pairs, ExactPair};
use mixlab::mixers::{mask, Xmx};
use mixlab::rng::Xoshiro256;
use std::collections::BTreeSet;

fn odd(rng: &mut Xoshiro256, bits: u32) -> u64 {
    (rng.next_u64() & mask(bits)) | 1
}

/// Does `p`'s relation (Δout_lo == Δout_hi for sign +1, != for -1) hold for
/// input `x`?
fn holds(f: &Xmx, p: &ExactPair, x: u64) -> bool {
    let d = f.apply(x) ^ f.apply(x ^ (1 << p.input));
    ((d >> p.out_lo & 1) == (d >> p.out_hi & 1)) == (p.sign > 0)
}

/// For every shift triple with s2 <= n/2 and `draws` random multiplier pairs:
/// - every predicted relation holds for every input of every draw;
/// - every predicted pair is a genuine correlation (neither bit constant) in
///   at least one draw (a special multiplier such as 1 + 2^k can make a bit
///   constant);
/// - every pair exact in all draws is predicted.
fn check_width(bits: u32, draws: usize) -> (usize, usize) {
    let mut rng = Xoshiro256::new(0xf0_7ced + bits as u64);
    let (mut templates, mut forced) = (0, 0);
    for s0 in 1..bits {
        for s1 in 1..bits {
            for s2 in 1..=bits / 2 {
                let want: BTreeSet<ExactPair> = forced_pairs(bits, s0, s1, s2).into_iter().collect();
                let mut common: Option<BTreeSet<ExactPair>> = None;
                let mut union = BTreeSet::new();
                for _ in 0..draws {
                    let f = Xmx::new(bits, vec![s0, s1, s2], vec![odd(&mut rng, bits), odd(&mut rng, bits)]).unwrap();
                    for p in &want {
                        assert!((0..=mask(bits)).all(|x| holds(&f, p, x)), "{f}: {p:?} does not always hold");
                    }
                    let got: BTreeSet<ExactPair> = exact_pairs_exhaustive(&f).into_iter().collect();
                    union.extend(got.iter().copied());
                    common = Some(match common {
                        None => got,
                        Some(c) => c.intersection(&got).copied().collect(),
                    });
                }
                let label = format!("{bits}-bit {s0}/{s1}/{s2}");
                let never: Vec<_> = want.difference(&union).collect();
                assert!(never.is_empty(), "{label}: predicted but never a correlation: {never:?}");
                let extra: Vec<_> = common.unwrap().difference(&want).copied().collect();
                assert!(extra.is_empty(), "{label}: exact in every draw but not predicted: {extra:?}");
                templates += 1;
                forced += !want.is_empty() as usize;
            }
        }
    }
    (templates, forced)
}

#[test]
fn rule_matches_exhaustive_truth_at_8_bits() {
    let (t, f) = check_width(8, 12);
    eprintln!("8-bit: {f} of {t} templates forced");
}

#[test]
fn rule_matches_exhaustive_truth_at_10_bits() {
    let (t, f) = check_width(10, 12);
    eprintln!("10-bit: {f} of {t} templates forced");
}

#[test]
fn rule_matches_exhaustive_truth_at_12_bits() {
    let (t, f) = check_width(12, 12);
    eprintln!("12-bit: {f} of {t} templates forced");
}

/// At 32 bits, each predicted relation holds on random inputs.
#[test]
fn predicted_pairs_hold_at_32_bits() {
    let mut rng = Xoshiro256::new(37);
    for (s0, s1, s2) in [(14, 15, 15), (18, 15, 15), (16, 13, 15), (13, 12, 13), (16, 15, 16), (15, 12, 15)] {
        let pairs = forced_pairs(32, s0, s1, s2);
        assert!(!pairs.is_empty(), "{s0}/{s1}/{s2}");
        for _ in 0..4 {
            let f = Xmx::new(32, vec![s0, s1, s2], vec![odd(&mut rng, 32), odd(&mut rng, 32)]).unwrap();
            for p in &pairs {
                for _ in 0..20_000 {
                    let x = rng.next_u64() & mask(32);
                    assert!(holds(&f, p, x), "{f}: {p:?} fails at x={x:#x}");
                }
            }
        }
    }
}

/// F11's pair: at s2 = 16 the trigger is input s1 (s1 < s0) and outputs (0, 16).
#[test]
fn s2_16_gives_the_f11_pair() {
    let p = forced_pairs(32, 16, 15, 16);
    assert!(p.iter().any(|p| (p.input, p.out_lo, p.out_hi) == (15, 0, 16)), "{p:?}");
}

/// The clean templates of FINDINGS F35, including the current best's.
#[test]
fn clean_32_bit_templates() {
    for (s0, s1, s2) in [(16, 15, 15), (15, 15, 15), (17, 15, 15), (16, 14, 15), (15, 16, 15), (16, 16, 15)] {
        assert!(forced_pairs(32, s0, s1, s2).is_empty(), "{s0}/{s1}/{s2}");
    }
}

#[test]
fn forced_cli() {
    let run = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_mixlab")).args(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(run(&["forced", "--bits", "32", "--shifts", "14/15/15", "--cpu"]), "14/15/15: input 31 -> out 2,17 corr -1\n");
    assert_eq!(run(&["forced", "--bits", "32", "--shifts", "16/15/15", "--cpu"]), "16/15/15: no forced pair\n");
    let grid = run(&["forced", "--bits", "8", "--cpu"]);
    assert_eq!(grid.lines().count(), 1 + 7 * 7 * 4);
    assert!(grid.contains("\n2/1/4,"));
}
