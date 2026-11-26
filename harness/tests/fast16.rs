use mixlab::mixers::Xmx;
use mixlab::rng::Xoshiro256;
use mixlab::search::random_config;

/// fast16 must give bit-identical counts to fast32 at every width it takes,
/// for 1-4 rounds, with extreme shifts (1 and bits-1) as well as typical ones.
#[test]
fn fast16_matches_fast32() {
    let mut rng = Xoshiro256::new(16);
    for bits in 9u32..=16 {
        for rounds in 1..=4 {
            for _ in 0..3 {
                let x = random_config(bits, rounds, &mut rng);
                assert!(mixlab::fast16::supports(&x));
                let a = mixlab::fast32::exact_xmx(&x, false);
                let b = mixlab::fast16::exact_xmx(&x);
                assert_eq!((a.n, &a.bins), (b.n, &b.bins), "{x}");
            }
            let mut x = random_config(bits, rounds, &mut rng);
            for (i, s) in x.shifts.iter_mut().enumerate() {
                *s = if i % 2 == 0 { 1 } else { bits - 1 };
            }
            let a = mixlab::fast32::exact_xmx(&x, false);
            let b = mixlab::fast16::exact_xmx(&x);
            assert_eq!((a.n, &a.bins), (b.n, &b.bins), "{x}");
        }
    }
}

/// hash-prospector's hash16_xm2 / xm3 (published 0.0085905051336723701 and
/// 0.0045976709018820602 without the x1000 factor).
#[test]
fn fast16_reproduces_prospector_16bit() {
    for (spec, published) in [("[8 88b5 7 db2d 9]", 8.5905051336723701), ("[7 2993 5 e877 9 0235 10]", 4.5976709018820602)] {
        let x = Xmx::parse(16, spec).unwrap();
        let s = mixlab::fast16::exact_xmx(&x).score();
        assert!((s - published).abs() < 1e-12, "{spec}: {s}");
    }
}

/// The bounded score returns the exact ms when it is within the bound and
/// None when it is not, on both sides of the true value.
#[test]
fn fast16_bounded_agrees_with_exact() {
    let mut rng = Xoshiro256::new(61);
    for _ in 0..40 {
        let x = random_config(16, 2, &mut rng);
        let ms = mixlab::fast16::exact_xmx(&x).ms();
        assert_eq!(mixlab::fast16::exact_ms_bounded(&x, ms * 1.000001), Some(ms), "{x}");
        assert_eq!(mixlab::fast16::exact_ms_bounded(&x, ms * 0.999999), None, "{x}");
        assert_eq!(mixlab::fast16::exact_ms_bounded(&x, f64::INFINITY), Some(ms), "{x}");
    }
}

/// exhaust2 at 10 bits returns exactly the pairs whose full exact score is
/// within the bound, with the same ms, compared against scoring every pair.
#[test]
fn exhaust2_matches_brute_force() {
    let bits = 10;
    let shifts = [5u32, 4, 5];
    let mut all = Vec::new();
    for m0 in (1..1u64 << bits).step_by(2) {
        for m1 in (1..1u64 << bits).step_by(2) {
            let x = Xmx::new(bits, shifts.to_vec(), vec![m0, m1]).unwrap();
            all.push((m0, m1, mixlab::fast32::exact_xmx(&x, false).ms()));
        }
    }
    let mut v: Vec<f64> = all.iter().map(|t| t.2).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let limit = v[v.len() / 100]; // keep about the best 1%
    let want: Vec<_> = all.iter().copied().filter(|t| t.2 <= limit).collect();
    let got = mixlab::fast16::exhaust2(bits, shifts, 0..1 << bits, limit, |_| {});
    assert!(want.len() > 100);
    assert_eq!(got, want);
}

/// The default exact path (`exact::exact_xmx`) routes 9..=16 bits to fast16.
#[test]
fn exact_dispatch_uses_fast16_counts() {
    let x = Xmx::parse(16, "[8 88b5 7 db2d 9]").unwrap();
    let a = mixlab::exact::exact_xmx(&x, true);
    let b = mixlab::fast16::exact_xmx(&x);
    assert_eq!((a.n, &a.bins), (b.n, &b.bins));
}
