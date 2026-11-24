use mixlab::bias::{estimate_sampled, exact, exact_serial};
use mixlab::mixers::Xmx;
use mixlab::refs::{self, Kind};
use mixlab::rng::Xoshiro256;
use mixlab::search::{config_from_index, random_config, space_size};
use mixlab::with_mixer;

fn reference(name: &str) -> (Xmx, f64) {
    let r = refs::by_name(name).unwrap();
    match r.kind {
        Kind::Xmx(x) => (x, r.published.unwrap_or(f64::NAN)),
        _ => panic!(),
    }
}

#[test]
fn random_16bit_mixers_are_bijections() {
    let mut rng = Xoshiro256::new(7);
    for rounds in 1..=4 {
        for _ in 0..5 {
            let x = random_config(16, rounds, &mut rng);
            let mut seen = vec![false; 1 << 16];
            with_mixer!(x, f => {
                for v in 0..(1u64 << 16) {
                    let h = f(v) as usize;
                    assert!(!seen[h], "{x} not a bijection");
                    seen[h] = true;
                    assert_eq!(x.apply(v), h as u64);
                }
            });
        }
    }
}

#[test]
fn inverses_roundtrip() {
    let mut rng = Xoshiro256::new(9);
    for bits in [8u32, 13, 32, 47, 64] {
        for rounds in 1..=3 {
            let x = random_config(bits, rounds, &mut rng);
            for _ in 0..1000 {
                let v = rng.next_u64() & mixlab::mixers::mask(bits);
                assert_eq!(x.invert(x.apply(v)), v, "{x}");
            }
        }
    }
}

#[test]
fn parse_display_roundtrip() {
    let x = Xmx::parse(32, "[16 21f0aaad 15 735a2d97 15]").unwrap();
    assert_eq!(x.to_string(), "[16 21f0aaad 15 735a2d97 15]");
    assert!(Xmx::parse(32, "[16 21f0aaac 15 735a2d97 15]").is_err()); // even
}

#[test]
fn index_decoding_covers_space() {
    let total = space_size(4, 1);
    let mut set = std::collections::HashSet::new();
    for i in 0..total {
        set.insert(config_from_index(4, 1, i));
    }
    assert_eq!(set.len() as u128, total);
}

#[test]
fn published_16bit_biases_match() {
    for name in ["hash16_xm2", "ironborn16"] {
        let (x, published) = reference(name);
        // hash-prospector's 16-bit figures (hp16.c) omit the x1000 factor used
        // by the 32-bit tool; we always report the x1000 scale.
        let got = with_mixer!(x, f => exact_serial(&f, 16).score()) / 1000.0;
        let rel = (got - published).abs() / published;
        assert!(rel < 1e-9, "{name}: got {got}, published {published} (ratio {})", got / published);
    }
}

/// Slow (~tens of seconds each): run with `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn published_32bit_biases_match() {
    for name in ["lowbias32", "ironborn_1076", "ironborn_10704", "prospector32", "triple32"] {
        let (x, published) = reference(name);
        let got = with_mixer!(x, f => exact(&f, 32).score());
        let rel = (got - published).abs() / published;
        eprintln!("{name}: got {got:.17}, published {published:.17}");
        assert!(rel < 1e-9, "{name}: got {got}, published {published}");
    }
}

#[test]
#[ignore]
fn sampled_estimator_converges_to_exact() {
    let (x, published) = reference("lowbias32");
    let e = with_mixer!(x, f => estimate_sampled(&f, 32, 1 << 26, 3, 16));
    eprintln!("lowbias32 sampled: {e:?}");
    assert!((e.ms - (published / 1000.0).powi(2)).abs() < 4.0 * e.se_ms, "{e:?}");
}

#[test]
fn fast32_matches_generic_exact() {
    let mut rng = Xoshiro256::new(11);
    for bits in [3u32, 8, 13, 16, 20] {
        for rounds in 1..=4 {
            let x = random_config(bits, rounds, &mut rng);
            let a = with_mixer!(x, f => exact_serial(&f, bits));
            let b = mixlab::fast32::exact_xmx(&x, true);
            assert_eq!(a.n, b.n, "{x}");
            assert_eq!(a.bins, b.bins, "{x}");
        }
    }
}

/// The Harley-Seal counter must give bit-identical counts to the ripple
/// counter, including ragged tails and the top of the 32-bit input range.
#[test]
fn fast32_harley_seal_matches_ripple() {
    let mut rng = Xoshiro256::new(29);
    for bits in [7u32, 9, 12, 17] {
        for rounds in 1..=3 {
            let x = random_config(bits, rounds, &mut rng);
            let a = mixlab::fast32::exact_xmx_ripple(&x, true);
            let b = mixlab::fast32::exact_xmx(&x, true);
            assert_eq!((a.n, &a.bins), (b.n, &b.bins), "{x}");
        }
    }
    let top = 1u64 << 32;
    for (lo, hi) in [(0u64, 1 << 18), (12_345, 12_345 + 3 * 128 + 40), (top - (1 << 16) - 8, top)] {
        for rounds in 1..=3 {
            let x = random_config(32, rounds, &mut rng);
            let a = mixlab::fast32::counts_range(&x, lo, hi, false);
            let b = mixlab::fast32::counts_range(&x, lo, hi, true);
            assert_eq!((a.n, &a.bins), (b.n, &b.bins), "{x} [{lo}, {hi})");
        }
    }
}

/// The SIMD sampler must consume the same RNG stream and give identical
/// counts to the generic sampler, including ragged chunk and group tails.
#[test]
fn fast32_sampled_matches_generic() {
    let mut rng = Xoshiro256::new(31);
    for bits in [3u32, 8, 20, 32] {
        for rounds in 1..=3 {
            let x = random_config(bits, rounds, &mut rng);
            for n in [37u64, 1000, (1 << 14) + 77, 3 << 14] {
                let a = with_mixer!(x, f => mixlab::bias::sampled(&f, bits, n, 99));
                let b = mixlab::fast32::sampled_xmx(&x, n, 99);
                assert_eq!((a.n, &a.bins), (b.n, &b.bins), "{x} n={n}");
            }
        }
    }
}

/// The surrogate's single-multiply carry-chain model should be near-exact for
/// 1-round mixers (its only approximation there is z ⟂ sign pattern).
#[test]
fn surrogate_one_round_matches_exact() {
    let mut rng = Xoshiro256::new(5);
    for _ in 0..10 {
        let x = random_config(16, 1, &mut rng);
        let ex: Vec<f64> = mixlab::fast32::exact_xmx(&x, false).diffs().collect();
        let pr = mixlab::surrogate::matrix(&x);
        let r = mixlab::surrogate::pearson(&ex, &pr);
        assert!(r > 0.97, "{x}: per-entry pearson {r}");
    }
}

#[test]
fn parity_claim_has_correct_shift_conditions() {
    use mixlab::mixers::unxorshift;
    // Exhaust all shifts and inputs at a small width, across several constants.
    let bits = 5;
    for s0 in 1..bits {
        for s1 in 1..bits {
            for s2 in 1..bits {
                for (m0, m1) in [(3, 5), (13, 23), (31, 1)] {
                    let f = Xmx::new(bits, vec![s0,s1,s2], vec![m0,m1]).unwrap();
                    let out_mask: u64 = (0..bits).step_by(s2 as usize).map(|k| 1u64 << k).sum();
                    let mut differences = vec![unxorshift(1 << s1, s0, bits)];
                    if s1 < s0 { differences.push(1 << s1); }
                    if s0+s1 < bits { differences.push(1 << (s0+s1)); }
                    for d in differences {
                        for x in 0..1u64 << bits {
                            assert_eq!(((f.apply(x)^f.apply(x^d)) & out_mask).count_ones()%2, 1, "{f} x={x} d={d}");
                        }
                    }
                }
            }
        }
    }
    let f = Xmx::parse(8, "[2 03 3 05 4]").unwrap();
    let failures = (0..256).filter(|&x| ((f.apply(x)^f.apply(x^8)) & 0x11).count_ones()%2 == 0).count();
    assert_eq!(failures, 192);
}

#[test]
fn fixed_low_multiplier_bits_do_not_fix_low_output_bits() {
    let a = Xmx::parse(32, "[16 21f0aaad 15 735a2d97 15]").unwrap();
    let b = Xmx::parse(32, "[16 21f1aaad 15 735a2d97 15]").unwrap();
    assert_ne!(a.apply(1) & 1, b.apply(1) & 1);
}

/// Compare arbitrary ranges against scalar input-by-input counting, rather
/// than comparing two SIMD paths that could share the same tail bug.
#[test]
fn fast32_ranges_match_scalar_for_every_tail_length() {
    let mut rng = Xoshiro256::new(123);
    for bits in [3, 8, 32] {
        let top = 1u64 << bits;
        for rounds in 1..=4 {
            let x = random_config(bits, rounds, &mut rng);
            for len in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 127, 128, 129, 135, 255, 263] {
                if len > top { continue; }
                for lo in [0, top - len] {
                    let hi = lo + len;
                    let mut acc = mixlab::bias::Acc::new(bits);
                    for v in lo..hi { acc.add_input(&|v| x.apply(v), v); }
                    let expected = acc.finish();
                    for hs in [false, true] {
                        let got = mixlab::fast32::counts_range(&x, lo, hi, hs);
                        assert_eq!((got.n, &got.bins), (expected.n, &expected.bins),
                                   "{x} [{lo}, {hi}) hs={hs}");
                    }
                }
            }
        }
    }
}
