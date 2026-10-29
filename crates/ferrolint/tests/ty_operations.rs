//! The operation rules ty answers (`ty` feature). The fixture's findings are
//! Pylint's, on a file where astroid infers the operands from their values,
//! except the two `E1131` (lines 32 and 33), which only ty reports.

#![cfg(feature = "ty")]
#![allow(clippy::unwrap_used)]

mod support;

use support::Project;

const CODES: &str = "\"E0633\", \"E1102\", \"E1130\", \"E1131\", \"E1133\", \"E1135\", \"E1136\", \"E1137\", \"E1138\"";

#[test]
fn the_fixture_matches_pylint() {
    let project = Project::new();
    project
        .write("ferrolint.toml", format!("select = [{CODES}]\n"))
        .write("operations.py", include_str!("data/ty/operations.py"));
    let run = project.run(&["--no-cache"]);
    let found: Vec<String> = run
        .finding_lines()
        .into_iter()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let line_number = parts.next().unwrap().split(':').nth(1).unwrap();
            format!("{line_number}:{}", parts.nth(1).unwrap())
        })
        .collect();
    let expected: Vec<&str> = include_str!("data/ty/operations.expected")
        .lines()
        .collect();
    assert_eq!(found, expected, "{}", run.summary());
}

#[test]
fn a_missing_virtual_env_is_warned() {
    let project = Project::new();
    project
        .write("ferrolint.toml", "select = [\"E1136\"]\n")
        .write("a.py", "x = 1\n");
    let without = project.run(&["--no-cache"]);
    assert!(
        without.stderr.contains("VIRTUAL_ENV is not set"),
        "{}",
        without.stderr
    );
    let with = project.run_in(&["--no-cache"], Some(project.path()));
    assert!(!with.stderr.contains("VIRTUAL_ENV"), "{}", with.stderr);
}
