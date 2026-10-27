//! `E1101` on `self` and `cls` (`ty` feature). The fixture's findings are Pylint's, on a
//! file with no metaclass, which Pylint checks and this rule leaves silent.

#![cfg(feature = "ty")]
#![allow(clippy::unwrap_used)]

mod support;

use support::Project;

const CODES: &str = "\"E1101\"";

#[test]
fn the_fixture_matches_pylint() {
    let project = Project::new();
    project
        .write("ferrolint.toml", format!("select = [{CODES}]\n"))
        .write("members.py", include_str!("data/ty/members.py"));
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
    let expected: Vec<&str> = include_str!("data/ty/members.expected").lines().collect();
    assert_eq!(found, expected, "{}", run.summary());
}
