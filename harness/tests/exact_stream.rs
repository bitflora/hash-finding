//! `mixlab exact --stream` (a long-running worker) answers each spec line as it
//! arrives and gives the same CSV lines as `mixlab exact --list -`.
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const SPECS: [&str; 3] = ["[8 a3d3 7 4b2d 9]", "[7 6ab3 8 2c4d 8]", "[6 1235 7 7ab3 5 4c2d 8]"];

fn list_output() -> Vec<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mixlab"))
        .args(["exact", "--cpu", "--bits", "16", "--list", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all((SPECS.join("\n") + "\n").as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().lines().map(String::from).collect()
}

#[test]
fn stream_matches_list_and_answers_each_line() {
    let expected = list_output();
    assert_eq!(expected.len(), SPECS.len() + 1);
    let mut child = Command::new(env!("CARGO_BIN_EXE_mixlab"))
        .args(["exact", "--cpu", "--bits", "16", "--stream"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), expected[0]);
    // One spec at a time with stdin left open: each answer must arrive
    // before the next spec is sent.
    for (spec, want) in SPECS.iter().zip(&expected[1..]) {
        writeln!(stdin, "{spec}").unwrap();
        stdin.flush().unwrap();
        line.clear();
        stdout.read_line(&mut line).unwrap();
        assert_eq!(line.trim_end(), want);
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}
