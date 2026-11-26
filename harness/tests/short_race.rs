use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn crc_top_row_matches_full_counts_and_identity_case() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_short-race"))
        .args(["4096", "151"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"crc5-detail 40bb4a67 795cb33b 0 16\ncrc5-exact-row31 1 1 0 16\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    let reference = mixlab::bias::sampled_serial(
        &|x| {
            let mut y = (x as u32).wrapping_mul(0x40bb4a67);
            for _ in 0..32 {
                y = (y >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(y & 1));
            }
            y = y.wrapping_mul(0x795cb33b);
            (y ^ (y >> 16)) as u64
        },
        32,
        4096,
        151,
    );
    let row = mixlab::bias::Counts {
        bits: 32,
        n: 4096,
        bins: reference.bins[992..].to_vec(),
    };
    let detail: Vec<f64> = lines
        .next()
        .unwrap()
        .split(',')
        .map(|x| x.parse().unwrap())
        .collect();
    assert_eq!(
        detail,
        [
            reference.ms_debiased(),
            reference.max_abs(),
            row.ms_debiased()
        ]
    );
    // With the identity second multiplier the whole pipeline is linear for a=1.
    assert_eq!(lines.next(), Some("1,1"));
}

#[test]
fn scores_match_reference_forms() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_short-race"))
        .args(["256", "301"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"crc3 4b0332d3 549a3597 0 0\ncrc5 4b0332d3 549a3597 0 15\nxs6 4b0332d3 549a3597 20 15\n"
    ).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 3);
    for (i, line) in stdout.lines().enumerate() {
        let counts = mixlab::bias::sampled_serial(
            &|x| {
                let mut y = (x as u32).wrapping_mul(0x4b0332d3);
                if i == 2 {
                    y ^= y >> 20;
                } else {
                    for _ in 0..32 {
                        y = if y & 1 == 1 {
                            (y >> 1) ^ 0x82f63b78
                        } else {
                            y >> 1
                        };
                    }
                }
                y = y.wrapping_mul(0x549a3597);
                if i != 0 {
                    y ^= y >> 15;
                }
                y as u64
            },
            32,
            256,
            301,
        );
        let (ms, worst) = line.split_once(',').unwrap();
        assert_eq!(ms.parse::<f64>().unwrap(), counts.ms_debiased());
        assert_eq!(worst.parse::<f64>().unwrap(), counts.max_abs());
    }
}
