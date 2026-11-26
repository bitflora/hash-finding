//! The shift-forced exact-pair rule (FINDINGS F37) against exhaustive truth.
use mixlab::forced::{
    exact_pairs_exhaustive, forced_pairs, forced_pairs_rounds, shifts_clean,
    shortest_forced_parities, ExactPair, ForcedParity,
};
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

/// Every shift list of a `rounds`-round mixer at width `bits`, with each
/// shift in 1..bits and the last one <= bits/2.
fn templates(bits: u32, rounds: usize) -> Vec<Vec<u32>> {
    let mut all = vec![vec![]];
    for k in 0..=rounds {
        let top = if k == rounds { bits / 2 } else { bits - 1 };
        all = all
            .into_iter()
            .flat_map(|t: Vec<u32>| (1..=top).map(move |s| [t.clone(), vec![s]].concat()))
            .collect();
    }
    all
}

/// For every template and `draws` random multiplier lists:
/// - every predicted relation holds for every input of every draw;
/// - every predicted pair is a genuine correlation (neither bit constant) in
///   at least one draw (a special multiplier such as 1 + 2^k can make a bit
///   constant);
/// - every pair exact in all draws is predicted.
fn check_width(bits: u32, rounds: usize, draws: usize) -> (usize, usize) {
    let mut rng = Xoshiro256::new(0xf0_7ced + bits as u64 + 100 * rounds as u64);
    let (mut count, mut forced) = (0, 0);
    for shifts in templates(bits, rounds) {
        let want: BTreeSet<ExactPair> = forced_pairs_rounds(bits, &shifts).into_iter().collect();
        let mut common: Option<BTreeSet<ExactPair>> = None;
        let mut union = BTreeSet::new();
        for _ in 0..draws {
            let mults = (0..rounds).map(|_| odd(&mut rng, bits)).collect();
            let f = Xmx::new(bits, shifts.clone(), mults).unwrap();
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
        let label = format!("{bits}-bit {shifts:?}");
        let never: Vec<_> = want.difference(&union).collect();
        assert!(never.is_empty(), "{label}: predicted but never a correlation: {never:?}");
        let extra: Vec<_> = common.unwrap().difference(&want).copied().collect();
        assert!(extra.is_empty(), "{label}: exact in every draw but not predicted: {extra:?}");
        count += 1;
        forced += !want.is_empty() as usize;
    }
    (count, forced)
}

#[test]
fn rule_matches_exhaustive_truth_at_8_bits() {
    let (t, f) = check_width(8, 2, 12);
    eprintln!("8-bit: {f} of {t} templates forced");
}

#[test]
fn rule_matches_exhaustive_truth_at_10_bits() {
    let (t, f) = check_width(10, 2, 12);
    eprintln!("10-bit: {f} of {t} templates forced");
}

#[test]
fn rule_matches_exhaustive_truth_at_12_bits() {
    let (t, f) = check_width(12, 2, 12);
    eprintln!("12-bit: {f} of {t} templates forced");
}

#[test]
fn one_round_rule_matches_exhaustive_truth() {
    for bits in [8, 10, 12, 14] {
        let (t, f) = check_width(bits, 1, 12);
        eprintln!("{bits}-bit 1-round: {f} of {t} templates forced");
    }
}

#[test]
fn three_round_rule_matches_exhaustive_truth() {
    for bits in [8, 10] {
        let (t, f) = check_width(bits, 3, 12);
        eprintln!("{bits}-bit 3-round: {f} of {t} templates forced");
    }
}

#[test]
fn four_round_rule_matches_exhaustive_truth_at_8_bits() {
    let (t, f) = check_width(8, 4, 8);
    eprintln!("8-bit 4-round: {f} of {t} templates forced");
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
    let spec = "[16 4b0332d3 15 549a3597 15]";
    assert_eq!(
        run(&["forced", "--spec", spec, "--parity"]),
        format!("{spec}: shortest forced parity weight 3\n  input 15 -> out 0,15,30 parity flip 1\n  input 31 -> out 0,15,30 parity flip 1\n")
    );
    assert_eq!(
        run(&["forced", "--shifts", "17/11/15/14", "--parity"]),
        "17/11/15/14: no forced parity from the F39 prefix rule\n"
    );
    let grid = run(&["forced", "--bits", "8", "--cpu"]);
    assert_eq!(grid.lines().count(), 1 + 7 * 7 * 4);
    assert!(grid.contains("\n2/1/4,"));
}

/// triple32 and mx3 are clean; 2-round splitmix64 / Stafford Mix13 is not.
#[test]
fn published_three_and_two_round_shifts() {
    assert!(shifts_clean(32, &[17, 11, 15, 14]), "triple32");
    assert!(shifts_clean(64, &[32, 29, 32, 29]), "mx3");
    let p = forced_pairs_rounds(64, &[30, 27, 31]);
    assert!(p.contains(&ExactPair { input: 29, out_lo: 2, out_hi: 33, sign: -1 }), "{p:?}");
}

#[test]
fn shortest_parities_match_brute_force_at_8_bits() {
    let bits = 8;
    let mut rng = Xoshiro256::new(39);
    for rounds in 1..=3 {
        for mut shifts in templates(bits, rounds).into_iter().filter(|s| s[rounds] == 1) {
            for last in 1..bits {
                shifts[rounds] = last;
                let middle: u32 = shifts[1..rounds].iter().sum();
                let mut expected = Vec::new();
                for input in 0..bits {
                    let first = if input < shifts[0] { input } else { input - shifts[0] };
                    let Some(c) = first.checked_sub(middle) else {
                        continue;
                    };
                    for output_mask in 1..=mask(bits) {
                        let before = (output_mask ^ (output_mask << last)) & mask(bits);
                        if before >> c <= 1 {
                            expected.push((
                                output_mask.count_ones(),
                                input,
                                output_mask,
                                before >> c == 1,
                            ));
                        }
                    }
                }
                expected.sort_unstable();
                if let Some(&(weight, ..)) = expected.first() {
                    expected.retain(|p| p.0 == weight);
                }
                let got = shortest_forced_parities(bits, &shifts);
                let mut actual: Vec<_> = got
                    .iter()
                    .map(|p| (p.output_mask.count_ones(), p.input, p.output_mask, p.flips))
                    .collect();
                actual.sort_unstable();
                assert_eq!(actual, expected, "{shifts:?}");

                let mults = (0..rounds).map(|_| odd(&mut rng, bits)).collect();
                let f = Xmx::new(bits, shifts.clone(), mults).unwrap();
                for p in got {
                    assert!((0..=mask(bits)).all(|x| {
                        let delta = f.apply(x) ^ f.apply(x ^ (1 << p.input));
                        ((delta & p.output_mask).count_ones() % 2 != 0) == p.flips
                    }), "{f}: {p:?}");
                }
            }
        }
    }
}

#[test]
fn shortest_parity_can_span_a_full_64_bit_word() {
    assert_eq!(
        shortest_forced_parities(64, &[32, 31, 1]),
        [31, 63].map(|input| ForcedParity { input, output_mask: u64::MAX, flips: true })
    );
}
