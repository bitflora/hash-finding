use std::{fs, process::Command};

#[test]
fn export_screen_verify_and_reject_invalid_inputs() {
    let path = std::env::temp_dir().join(format!("mixlab-seeds-{}.csv", std::process::id()));
    let verified = format!("{}.verified.csv", path.display());
    assert!(!path.exists() && !std::path::Path::new(&verified).exists());
    let args = [
        "--bits",
        "64",
        "--count",
        "3",
        "--samples",
        "32",
        "--verify",
        "1",
        "--verify-samples",
        "64",
        "--out",
        path.to_str().unwrap(),
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_seed-search"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let csv = fs::read_to_string(&path).unwrap();
    let rows: Vec<_> = csv
        .lines()
        .skip(1)
        .map(|s| s.split(',').collect::<Vec<_>>())
        .collect();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_eq!(row.len(), 10);
        mixlab::mixers::Xmx::parse(64, row[0]).unwrap();
        assert_eq!(row[4], "screen");
        assert_eq!(row[5], "32");
        assert!(row[7].parse::<f64>().unwrap().is_finite());
    }
    let v = fs::read_to_string(&verified).unwrap();
    let fields: Vec<_> = v.lines().nth(1).unwrap().split(',').collect();
    assert_eq!(fields[4], "fresh-sampled");
    assert_eq!(fields[5], "64");
    let selected = rows.iter().find(|r| r[0] == fields[0]).unwrap();
    assert_ne!(selected[6], fields[6]);
    let best = rows
        .iter()
        .map(|r| r[7].parse::<f64>().unwrap())
        .min_by(f64::total_cmp)
        .unwrap();
    assert_eq!(selected[7].parse::<f64>().unwrap(), best);
    assert!(!Command::new(env!("CARGO_BIN_EXE_seed-search"))
        .args(args)
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(fs::read_to_string(&path).unwrap(), csv);
    fs::remove_file(&path).unwrap();
    fs::remove_file(&verified).unwrap();
    for invalid in [
        vec!["--bits", "33"],
        vec!["--shifts", "0"],
        vec!["--count", "109"],
        vec!["--samples", "1"],
        vec!["--source", "typo"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_seed-search"))
            .args(invalid)
            .args(["--out", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!path.exists());
    }
    assert!(Command::new(env!("CARGO_BIN_EXE_seed-search"))
        .args([
            "--generate-only",
            "--count",
            "3",
            "--out",
            path.to_str().unwrap()
        ])
        .output()
        .unwrap()
        .status
        .success());
    let generated = fs::read_to_string(&path).unwrap();
    assert!(generated
        .lines()
        .skip(1)
        .all(|line| line.split(',').nth(4) == Some("generated")));
    fs::remove_file(path).unwrap();
}
