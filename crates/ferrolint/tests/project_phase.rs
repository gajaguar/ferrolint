//! The project rules (E0401, E0402, E0611, R0401) on trees whose expected
//! findings are Pylint 4.1.3's own, and the one cache entry of their result.

// A helper that panics is how a test fails, as `clippy.toml` allows
// inside `#[test]` functions themselves.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::Path;

use support::Project;

const SELECT: &str = "select = [\"E0401\", \"E0402\", \"E0611\", \"R0401\", \"R0801\"]\n";

/// A project holding the tree `name` of `tests/data/project`, with the
/// rules above selected.
fn project(name: &str) -> Project {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/project");
    let project = Project::new();
    project
        .copy_dir(&from.join(name), ".")
        .write("ferrolint.toml", SELECT);
    project
}

/// The findings of a run as `path:line:code:message`, sorted. A duplicate
/// is one entry: its first line and its `==module:[a:b]` lines, joined by
/// `|`, without the copied lines, which Pylint takes from either file.
fn findings(project: &Project, args: &[&str]) -> Vec<String> {
    let run = project.run(args);
    assert!(run.code <= 1, "{run:?}");
    let mut found: Vec<String> = Vec::new();
    for line in run.finding_lines() {
        if line.starts_with("==") {
            let last = found.last_mut().unwrap();
            last.push('|');
            last.push_str(line);
        } else if line.contains(" error ") || line.contains(" info ") {
            let mut parts = line.splitn(4, ':');
            let (path, row) = (parts.next().unwrap(), parts.next().unwrap());
            let rest = parts.last().unwrap();
            let (code, message) = rest.split_once(") ").unwrap();
            let code = code.split_whitespace().nth(1).unwrap();
            found.push(format!("{path}:{row}:{code}:{message}"));
        }
    }
    found.sort();
    found
}

fn expected(name: &str) -> Vec<String> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/data/project/{name}.expected"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn imports_match_pylint() {
    assert_eq!(
        findings(&project("imports"), &["--no-cache"]),
        expected("imports")
    );
}

#[test]
fn cycles_match_pylint() {
    assert_eq!(
        findings(&project("cycles"), &["--no-cache"]),
        expected("cycles")
    );
}

/// Pylint puts every cycle on the file it checked last, which depends on the
/// file system, so only the cycles are compared. Each strongly connected part
/// of the tree is a single cycle: with more, Pylint's own result depends on
/// the hash order of its sets.
#[test]
fn cycle_edges_match_pylint() {
    let found: Vec<String> = findings(&project("cycles_edges"), &["--no-cache"])
        .iter()
        .map(|line| line.splitn(3, ':').nth(2).unwrap())
        .filter(|finding| finding.starts_with("R0401:"))
        .map(str::to_owned)
        .collect();
    assert_eq!(found, expected("cycles_edges"));
}

#[test]
fn duplicates_match_pylint() {
    let project = project("duplicates");
    assert_eq!(findings(&project, &["--no-cache"]), expected("duplicates"));
}

#[test]
fn the_duplicate_options_reach_the_rule() {
    let project = project("duplicates");
    let config = |options: &str| format!("{SELECT}[options]\n{options}\n");
    project.write("ferrolint.toml", config("min-similarity-lines = \"3\""));
    assert_eq!(
        findings(&project, &["--no-cache"]),
        expected("duplicates.min3")
    );
    project.write(
        "ferrolint.toml",
        config("ignore-imports = \"n\"\nignore-signatures = \"n\""),
    );
    assert_eq!(
        findings(&project, &["--no-cache"]),
        expected("duplicates.keep")
    );
    project.write("ferrolint.toml", config("min-similarity-lines = \"0\""));
    assert_eq!(findings(&project, &["--no-cache"]), Vec::<String>::new());
}

#[test]
fn a_cached_run_replays_until_the_search_path_changes() {
    let project = project("imports");
    let first = findings(&project, &[]);
    assert_eq!(first, expected("imports"));
    assert_eq!(findings(&project, &[]), first);
    // A module that appears beside the importer is a name no file's bytes record.
    project.write("pkg/nope.py", "");
    let after = findings(&project, &[]);
    assert_eq!(after.len(), first.len() - 2);
    assert!(
        after
            .iter()
            .all(|line| !line.contains("No name 'nope' in module 'pkg'"))
    );
}

#[test]
fn an_edit_to_a_cycle_member_drops_the_cycle() {
    let project = project("cycles");
    assert!(
        findings(&project, &[])
            .iter()
            .any(|line| line.contains("R0401"))
    );
    project.write("pkg/b.py", "\"\"\"B.\"\"\"\n");
    let after = findings(&project, &[]);
    assert!(
        after.iter().all(|line| !line.contains("pkg.a -> pkg.b")),
        "{after:?}"
    );
}

const BLOCK: &str = "    total = left + right\n    scaled = total * 3\n    shifted = scaled - left\n    \
                     doubled = shifted * 2\n    return doubled + right\n";

/// A committed tree with the cycles `pkg.a -> pkg.b -> pkg.a` and
/// `pkg.c -> pkg.d -> pkg.c`, and the copies `pkg/x.py` = `pkg/y.py` and
/// `pkg/m.py` = `pkg/n.py`.
fn committed() -> Project {
    let project = Project::git();
    project
        .write(".gitignore", ".home/\n.ferrolint-cache/\n")
        .write("ferrolint.toml", SELECT)
        .write("pkg/__init__.py", "")
        .write("pkg/a.py", "from pkg import b\n")
        .write("pkg/b.py", "from pkg import a\n")
        .write("pkg/c.py", "from pkg import d\n")
        .write("pkg/d.py", "from pkg import c\n");
    for (name, function) in [("x", "one"), ("y", "one"), ("m", "two"), ("n", "two")] {
        project.write(
            &format!("pkg/{name}.py"),
            format!("def {function}(left, right):\n{BLOCK}"),
        );
    }
    project.commit("base");
    project
}

#[test]
fn diff_mode_reports_what_involves_a_changed_file() {
    let project = committed();
    assert_eq!(findings(&project, &[]).len(), 4, "a full run sees all four");
    // `pkg/b.py` and `pkg/m.py` change; their partners and the rest do not.
    project
        .write("pkg/b.py", "from pkg import a\n\nVALUE = 1\n")
        .write(
            "pkg/m.py",
            format!("def two(left, right):\n{BLOCK}\nVALUE = 1\n"),
        );
    let found = findings(&project, &["--diff-base", "HEAD"]);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .any(|f| f.contains("R0401:Cyclic import (pkg.a -> pkg.b)"))
    );
    assert!(
        found
            .iter()
            .any(|f| f.contains("R0801") && f.contains("==pkg.m:") && f.contains("==pkg.n:")),
        "{found:?}"
    );
    // The report sits where a full run puts it, on the file checked last.
    let full = findings(&project, &[]);
    assert!(found.iter().all(|f| full.contains(f)), "{full:?}");
    // The warm run replays the same result from the one project entry.
    assert_eq!(findings(&project, &["--diff-base", "HEAD"]), found);
}

#[test]
fn diff_mode_drops_a_cycle_whose_files_did_not_change() {
    let project = committed();
    project.write("pkg/z.py", "VALUE = 1\n");
    assert_eq!(
        findings(&project, &["--diff-base", "HEAD"]),
        Vec::<String>::new()
    );
}
