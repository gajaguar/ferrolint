//! `E1101` on `self` and `cls` (`ty` feature). The fixtures' findings are Pylint's, on files with
//! no metaclass, which Pylint checks and this rule leaves silent.

#![cfg(feature = "ty")]
#![allow(clippy::unwrap_used)]

mod support;

use support::Project;

/// A virtualenv with an empty `site-packages`: ty needs one to start.
fn venv() -> Project {
    let venv = Project::new();
    venv.write("pyvenv.cfg", "home = /usr/bin\nversion = 3.14.0\n")
        .write("lib/python3.14/site-packages/.keep", "");
    venv
}

const CODES: &str = "\"E1101\"";

#[test]
fn the_fixture_matches_pylint() {
    let project = Project::new();
    project
        .write("ferrolint.toml", format!("select = [{CODES}]\n"))
        .write("members.py", include_str!("data/ty/members.py"));
    let run = project.run_in(&["--no-cache"], Some(venv().path()));
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

/// Each case is an `[options]` table and the lines Pylint reports for it on `members_options.py`.
const OPTIONS: &[(&str, &[&str])] = &[
    (
        "",
        &[
            "6 Instance of 'Skipped' has no 'absent' member",
            "16 Instance of 'FieldHelper' has no 'absent' member",
            "24 Instance of 'Plain' has no 'ghost_one' member",
            "24 Instance of 'Plain' has no 'nme' member; maybe 'name'?",
            "24 Instance of 'Plain' has no 'nam' member; maybe 'name'?",
        ],
    ),
    (
        "generated-members = \"ghost.*\"",
        &[
            "6 Instance of 'Skipped' has no 'absent' member",
            "16 Instance of 'FieldHelper' has no 'absent' member",
            "24 Instance of 'Plain' has no 'nme' member; maybe 'name'?",
            "24 Instance of 'Plain' has no 'nam' member; maybe 'name'?",
        ],
    ),
    (
        "ignored-classes = \"Skipped\"",
        &[
            "16 Instance of 'FieldHelper' has no 'absent' member",
            "24 Instance of 'Plain' has no 'ghost_one' member",
            "24 Instance of 'Plain' has no 'nme' member; maybe 'name'?",
            "24 Instance of 'Plain' has no 'nam' member; maybe 'name'?",
        ],
    ),
    (
        "mixin-class-rgx = \".*Helper\"",
        &[
            "6 Instance of 'Skipped' has no 'absent' member",
            "11 Instance of 'FieldMixin' has no 'absent' member",
            "24 Instance of 'Plain' has no 'ghost_one' member",
            "24 Instance of 'Plain' has no 'nme' member; maybe 'name'?",
            "24 Instance of 'Plain' has no 'nam' member; maybe 'name'?",
        ],
    ),
    (
        "ignored-checks-for-mixins = \"\"",
        &[
            "6 Instance of 'Skipped' has no 'absent' member",
            "11 Instance of 'FieldMixin' has no 'absent' member",
            "16 Instance of 'FieldHelper' has no 'absent' member",
            "24 Instance of 'Plain' has no 'ghost_one' member",
            "24 Instance of 'Plain' has no 'nme' member; maybe 'name'?",
            "24 Instance of 'Plain' has no 'nam' member; maybe 'name'?",
        ],
    ),
    (
        "missing-member-hint = \"no\"",
        &[
            "6 Instance of 'Skipped' has no 'absent' member",
            "16 Instance of 'FieldHelper' has no 'absent' member",
            "24 Instance of 'Plain' has no 'ghost_one' member",
            "24 Instance of 'Plain' has no 'nme' member",
            "24 Instance of 'Plain' has no 'nam' member",
        ],
    ),
    (
        "missing-member-hint-distance = \"2\"\nmissing-member-max-choices = \"2\"",
        &[
            "6 Instance of 'Skipped' has no 'absent' member",
            "16 Instance of 'FieldHelper' has no 'absent' member",
            "24 Instance of 'Plain' has no 'ghost_one' member",
            "24 Instance of 'Plain' has no 'nme' member; maybe one of 'name' or 'names'?",
            "24 Instance of 'Plain' has no 'nam' member; maybe one of 'name' or 'names'?",
        ],
    ),
];

#[test]
fn the_options_match_pylint() {
    let project = Project::new();
    project.write("members.py", include_str!("data/ty/members_options.py"));
    for (options, expected) in OPTIONS {
        project.write(
            "ferrolint.toml",
            format!("select = [{CODES}]\n\n[options]\n{options}\n"),
        );
        let run = project.run_in(&["--no-cache"], Some(venv().path()));
        let found: Vec<String> = run
            .finding_lines()
            .into_iter()
            .map(|line| {
                let (position, rest) = line.split_once(' ').unwrap();
                let (_, message) = rest.split_once(") ").unwrap();
                format!("{} {message}", position.split(':').nth(1).unwrap())
            })
            .collect();
        assert_eq!(&found, expected, "{options}: {}", run.summary());
    }
}
