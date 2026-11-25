//! Harness support for the shift-template comparison (plan.md, Plan A).
use mixlab::mixers::Xmx;
use mixlab::racing::{race_search, RaceCfg};
use std::process::Command;

fn cfg(fix_shifts: bool) -> RaceCfg {
    RaceCfg {
        bits: 8,
        n0: 1 << 8,
        cap: 1 << 12,
        z: 3.0,
        verify_samples: None,
        seed: 7,
        max_last_shift: None,
        fix_low_bits: None,
        max_bic_corr: None,
        fix_shifts,
    }
}

/// Every incumbent the race logs (start and each accepted improvement).
fn incumbents(fix_shifts: bool, start: &Xmx) -> (Vec<Xmx>, Xmx) {
    let mut seen = Vec::new();
    let (best, _) = race_search(&cfg(fix_shifts), start.clone(), 40, 24, 1000, |line| {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() >= 3 && (f[1] == "start" || f[1] == "improved") {
            seen.push(Xmx::parse(8, f[2]).unwrap());
        }
    });
    (seen, best)
}

#[test]
fn fix_shifts_keeps_the_start_template() {
    // Shifts of 1 are a poor 8-bit template, so a free race leaves it quickly.
    let start = Xmx::parse(8, "[1 ad 1 97 1]").unwrap();
    let (fixed, best) = incumbents(true, &start);
    assert!(fixed.len() >= 2, "race with fixed shifts never improved: {fixed:?}");
    for x in fixed.iter().chain(std::iter::once(&best)) {
        assert_eq!(x.shifts, start.shifts, "{x} left the start template");
    }
    assert_ne!(best.mults, start.mults);
    // Control: without the flag the same race does move a shift.
    let (_, free_best) = incumbents(false, &start);
    assert_ne!(free_best.shifts, start.shifts, "control race never moved a shift");
}

#[test]
fn surrogate_score_list_matches_library_scores() {
    let specs = ["[16 cb0332d3 15 549a3597 15]", "[15 21f0aaad 14 d35a2d97 13]", "[17 7feb352d 16 846ca68b 15]"];
    let path = std::env::temp_dir().join(format!("mixlab-surrogate-list-{}.csv", std::process::id()));
    std::fs::write(&path, format!("spec,group\n{}\n", specs.map(|s| format!("{s},x")).join("\n"))).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mixlab"))
        .args(["surrogate", "--bits", "32", "--score-list", path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(&path).ok();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "spec,surrogate_bias");
    assert_eq!(lines.len(), specs.len() + 1);
    for (line, spec) in lines[1..].iter().zip(specs) {
        let x = Xmx::parse(32, spec).unwrap();
        let (s, v) = line.split_once(',').unwrap();
        assert_eq!(s, x.to_string());
        assert_eq!(v, format!("{:.10}", mixlab::surrogate::score(&x)));
    }
}
