//! Acceptance criteria, one test per row of `docs/requirements/ac-index.md`.
//!
//! A test still marked `#[ignore]` is a criterion whose feature has not
//! landed; its body is a single `unimplemented!` marker.

// A helper that panics is how a test fails, as `clippy.toml` allows
// inside `#[test]` functions themselves.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use serde_json::Value;
use support::{BARE_NOQA, CLEAN, Project, SYNTAX_ERROR};

/// The diagnostics array of a `--format json` report.
fn json_diagnostics(stdout: &str) -> Vec<Value> {
    let report: Value = serde_json::from_str(stdout).unwrap();
    report["diagnostics"].as_array().unwrap().clone()
}

/// A probe project: the probe plugin, with `select` limited to `codes`.
fn probe_project(codes: &[&str]) -> Project {
    let project = Project::new();
    let plugin = project.plugin("ferrolint-test-probe", "probe.wasm");
    let select: Vec<String> = codes.iter().map(|code| format!("{code:?}")).collect();
    project.write(
        "ferrolint.toml",
        format!("select = [{}]\n\n{plugin}", select.join(", ")),
    );
    project
}

// Configuration (AC-01..AC-06, FR-01, FR-02)

#[test]
fn ac_01_config_loads_from_pyproject() {
    let project = Project::new();
    project
        .write(
            "pyproject.toml",
            "[project]\nname = \"demo\"\n\n[tool.ferrolint]\nselect = [\"FL002\"]\n",
        )
        .write("bare.py", BARE_NOQA)
        .write("broken.py", SYNTAX_ERROR);
    let run = project.run(&[]);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(run.stdout.contains("FL002"), "{run:?}");
    assert!(
        !run.stdout.contains("FL003"),
        "the section's select applies: {run:?}"
    );
}

#[test]
fn ac_02_ferrolint_toml_wins_over_pyproject() {
    let project = Project::new();
    project
        .write(
            "pyproject.toml",
            "[tool.ferrolint]\nselect = [\"FL002\"]\nnot-a-key = true\n",
        )
        .write("ferrolint.toml", "select = [\"FL003\"]\n")
        .write("bare.py", BARE_NOQA)
        .write("broken.py", SYNTAX_ERROR);
    let run = project.run(&[]);
    assert_eq!(run.code, 1, "pyproject's bad key must not be read: {run:?}");
    assert!(run.stdout.contains("FL003"), "{run:?}");
    assert!(!run.stdout.contains("FL002"), "{run:?}");
}

#[test]
fn ac_03_plugin_sha_mismatch_is_config_error() {
    let project = Project::new();
    project
        .write("plugin.wasm", b"\0asm not really")
        .write(
            "ferrolint.toml",
            format!(
                "plugins = [{{ path = \"plugin.wasm\", sha256 = \"{}\" }}]\n",
                "0".repeat(64)
            ),
        )
        .write("bare.py", BARE_NOQA);
    let run = project.run(&[]);
    assert_eq!(run.code, 2, "{run:?}");
    assert!(run.stderr.contains("sha256 mismatch"), "{run:?}");
    assert!(run.stdout.is_empty(), "no file is linted: {run:?}");
}

#[test]
fn ac_04_select_and_ignore_filter_findings() {
    // Both host rules fire on one file: it does not parse and it has a bare noqa.
    let both = format!("{SYNTAX_ERROR}y = 2  # noqa\n");
    let project = Project::new();
    project
        .write(
            "ferrolint.toml",
            "select = [\"FL002\", \"FL003\"]\nignore = [\"FL003\"]\n\n[per-file-ignores]\n\"inside/*.py\" = [\"FL002\"]\n",
        )
        .write("inside/both.py", &both)
        .write("outside/both.py", &both);
    let run = project.run(&[]);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(
        !run.stdout.contains("FL003"),
        "ignored code never reports: {run:?}"
    );
    let lines = run.finding_lines();
    assert_eq!(lines.len(), 1, "{run:?}");
    assert!(
        lines[0].starts_with("outside/both.py:") && lines[0].contains("FL002"),
        "{run:?}"
    );
}

#[test]
fn ac_05_honours_gitignore_and_excludes() {
    let project = Project::git();
    project
        .write(".gitignore", ".home/\nignored.py\nbuild/\n")
        .write("ferrolint.toml", "exclude = [\"vendor/\", \"*_gen.py\"]\n")
        .write("kept.py", BARE_NOQA)
        .write("pkg/kept.py", BARE_NOQA)
        .write("ignored.py", BARE_NOQA)
        .write("build/out.py", BARE_NOQA)
        .write("vendor/lib.py", BARE_NOQA)
        .write("pkg/schema_gen.py", BARE_NOQA);
    let filtered = project.run(&[]);
    assert_eq!(filtered.code, 1, "{filtered:?}");
    assert_eq!(
        filtered.summary(),
        "2 files linted, 2 findings.",
        "{filtered:?}"
    );

    for path in [
        "ignored.py",
        "build/out.py",
        "vendor/lib.py",
        "pkg/schema_gen.py",
    ] {
        project.remove(path);
    }
    let removed = project.run(&[]);
    assert_eq!(filtered.stdout, removed.stdout);
}

#[test]
fn ac_06_named_files_bypass_discovery() {
    let project = Project::git();
    project
        .write(".gitignore", ".home/\nignored.py\n")
        .write("ferrolint.toml", "exclude = [\"excluded.py\"]\n")
        .write("ignored.py", BARE_NOQA)
        .write("excluded.py", BARE_NOQA);
    assert_eq!(project.run(&[]).summary(), "0 files linted, 0 findings.");
    let run = project.run(&["ignored.py", "excluded.py"]);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(run.stdout.contains("ignored.py:1:"), "{run:?}");
    assert!(run.stdout.contains("excluded.py:1:"), "{run:?}");
    assert_eq!(run.summary(), "2 files linted, 2 findings.");
}

// Plugins (AC-07..AC-10, FR-03, FR-04)

#[test]
fn ac_07_plugin_receives_visitor_events() {
    let project = probe_project(&["PRB001"]);
    project.write(
        "pkg/shapes.py",
        "import os\n\nclass Shape:\n    def area(self):\n        if self:\n            return 1\n        return 0\n\ndef main():\n    for item in [Shape()]:\n        print(item.area(), os.sep)\n",
    );
    let run = project.run(&["--format", "json"]);
    assert_eq!(run.code, 1, "{run:?}");
    let mut events: Vec<(String, u64)> = json_diagnostics(&run.stdout)
        .iter()
        .map(|d| {
            let message = d["message"].as_str().unwrap().to_owned();
            (message, d["range"]["start"]["offset"].as_u64().unwrap())
        })
        .collect();
    events.sort();
    let names: Vec<&str> = events.iter().map(|(message, _)| &message[5..]).collect();

    assert_eq!(names.first(), Some(&"start-file pkg.shapes"));
    assert_eq!(
        names.last(),
        Some(&"end-file"),
        "no event after the file ends"
    );
    assert_eq!(names[1], "visit module");

    let visits: Vec<&(String, u64)> = events
        .iter()
        .filter(|(m, _)| m[5..].starts_with("visit "))
        .collect();
    let leaves = names
        .iter()
        .filter(|name| name.starts_with("leave "))
        .count();
    assert_eq!(visits.len(), leaves, "every visit has its leave");
    assert!(
        visits.windows(2).all(|pair| pair[0].1 <= pair[1].1),
        "visits come in source order: {visits:?}"
    );
    let position = |event: &str| names.iter().position(|name| *name == event).unwrap();
    assert!(position("visit classdef Shape") < position("visit functiondef area"));
    assert!(position("visit functiondef area") < position("visit if"));
    assert!(position("leave classdef") < position("visit functiondef main"));
    assert!(position("visit for") > position("visit functiondef main"));
}

#[test]
fn ac_08_scope_and_binding_queries_resolve_names() {
    let project = probe_project(&["PRB002", "PRB003"]);
    project.write(
        "scopes.py",
        "counter = 0\n\ndef shadow():\n    counter = 1\n    return counter\n\ndef reader():\n    return counter\n\ndef importer():\n    import json.decoder as jd\n    return jd\n",
    );
    let run = project.run(&[]);
    assert_eq!(run.code, 1, "{run:?}");
    let lines = run.finding_lines();
    let line = |prefix: &str| -> &str {
        lines
            .iter()
            .find(|line| line.starts_with(prefix))
            .unwrap_or_else(|| panic!("no line {prefix}: {run:?}"))
    };
    assert!(
        line("scopes.py:5:12: info PRB002")
            .ends_with("counter -> assignment in function shadow line 4")
    );
    assert!(
        line("scopes.py:8:12: info PRB002").ends_with("counter -> assignment in module - line 1")
    );
    assert!(
        line("scopes.py:12:12: info PRB002")
            .ends_with("jd -> import in function importer line 11 from json.decoder")
    );

    let visible = line("scopes.py:5:12: info PRB003");
    assert!(visible.contains("counter=assignment@4"), "{visible}");
    assert!(
        !visible.contains("counter=assignment@1"),
        "the shadowed global is hidden: {visible}"
    );
    let visible = line("scopes.py:8:12: info PRB003");
    assert!(visible.contains("counter=assignment@1"), "{visible}");
}

#[test]
fn ac_09_diagnostic_records_full_field_set() {
    let project = probe_project(&["PRB004", "PRB005"]);
    let source = "class Box:\n    pass\n\ndef make():\n    return Box()\n";
    project.write("defs.py", source);
    let run = project.run(&["--format", "json"]);
    assert_eq!(run.code, 1, "{run:?}");
    let diagnostics = json_diagnostics(&run.stdout);
    assert_eq!(diagnostics.len(), 2, "{run:?}");
    let keys =
        |value: &Value| -> Vec<String> { value.as_object().unwrap().keys().cloned().collect() };

    let class = &diagnostics[0];
    assert_eq!(class["code"], "PRB005");
    assert_eq!(
        keys(class),
        ["aliases", "code", "message", "path", "range", "severity"]
    );
    assert_eq!(class["severity"], "error");
    assert_eq!(class["message"], "class Box");
    assert_eq!(
        class["range"]["start"],
        serde_json::json!({"column": 1, "line": 1, "offset": 0})
    );

    let function = &diagnostics[1];
    assert_eq!(function["code"], "PRB004");
    assert_eq!(
        keys(function),
        [
            "aliases", "code", "fix", "message", "path", "range", "severity"
        ]
    );
    assert_eq!(function["severity"], "warning");
    assert_eq!(
        keys(&function["fix"]),
        ["description", "range", "replacement"]
    );
    assert_eq!(function["fix"]["replacement"], "MAKE");
    assert_eq!(
        std::fs::read_to_string(project.join("defs.py")).unwrap(),
        source,
        "the fix is not applied"
    );
}

#[test]
#[ignore = "AC-10: the sample plugin matches its real Pylint checker"]
fn ac_10_sample_plugin_matches_pylint_checker() {
    // Given the sample plugin ported from the designated Pylint checker and annotated fixtures,
    // when findings are resolved through the code mapping,
    // then every expected finding appears mapped, no unexpected ones, asserted in `make test`.
    unimplemented!("AC-10");
}

// Suppressions (AC-11..AC-13, FR-05)

#[test]
#[ignore = "AC-11: ferrolint-disable comments suppress their codes"]
fn ac_11_ferrolint_disable_suppresses() {
    // Given a fixture line whose comment is `# ferrolint: disable=CODE` for the finding's code,
    // when a run reports,
    // then that finding is absent and a second code on the line is suppressed only if listed.
    unimplemented!("AC-11");
}

#[test]
#[ignore = "AC-12: pylint-disable comments suppress their codes"]
fn ac_12_pylint_disable_suppresses() {
    // Given the same fixture line with `# pylint: disable=CODE` in msg-id, symbol or plugin spelling,
    // when a run reports,
    // then the finding is suppressed identically to the ferrolint spelling of the same code.
    unimplemented!("AC-12");
}

#[test]
#[ignore = "AC-13: noqa comments suppress their codes"]
fn ac_13_noqa_suppresses() {
    // Given the same fixture line with `# noqa: CODE` and a two-code comma variant,
    // when a run reports,
    // then exactly the listed findings are suppressed and inert comments suppress nothing.
    unimplemented!("AC-13");
}

// Output (AC-14..AC-18, FR-06, FR-10)

#[test]
#[ignore = "AC-14: text output is the default and per-diagnostic"]
fn ac_14_text_output_default() {
    // Given a run with findings and no `--format` argument,
    // when the report prints,
    // then one line per diagnostic plus a summary line, and `--format text` prints the same bytes.
    unimplemented!("AC-14");
}

#[test]
#[ignore = "AC-15: json output is machine-parseable"]
fn ac_15_json_output_machine_parseable() {
    // Given the same run with `--format json`,
    // when the report is produced,
    // then it is canonical JSON with one object per diagnostic and a summary, no extra top-level keys.
    unimplemented!("AC-15");
}

#[test]
#[ignore = "AC-16: sarif output is SARIF 2.1.0 shaped"]
fn ac_16_sarif_output_sarif_2_1() {
    // Given the same run with `--format sarif`,
    // when the report is produced,
    // then a SARIF 2.1.0 reader parses it with rule reference, location and mapped severity per result.
    unimplemented!("AC-16");
}

#[test]
#[ignore = "AC-17: junit output reads as a test report"]
fn ac_17_junit_output_test_report() {
    // Given the same run with `--format junit`,
    // when the report is produced,
    // then it parses as JUnit XML, one testcase per file with one failure per diagnostic, CI-consistent.
    unimplemented!("AC-17");
}

#[test]
#[ignore = "AC-18: Pylint aliases resolve through the run"]
fn ac_18_pylint_aliases_resolve() {
    // Given a `select` citing a ported checker by Pylint msg-id and symbol,
    // when a firing fixture is linted and reported in every format,
    // then findings carry plugin code plus both aliases, and `ignore` removes them by any spelling.
    unimplemented!("AC-18");
}

// Exit codes (AC-19..AC-21, FR-07)

#[test]
fn ac_19_clean_run_exits_zero() {
    let project = Project::new();
    project.write("a.py", CLEAN).write("pkg/b.py", CLEAN);
    let run = project.run(&[]);
    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(run.stdout, "2 files linted, 0 findings.\n");
}

#[test]
fn ac_20_findings_run_exits_one() {
    let project = Project::new();
    project
        .write("a.py", BARE_NOQA)
        .write("b.py", SYNTAX_ERROR)
        .write("c.py", CLEAN);
    let run = project.run(&[]);
    assert_eq!(run.code, 1, "{run:?}");
    let lines = run.finding_lines();
    assert_eq!(lines.len(), 2, "{run:?}");
    assert!(lines[0].starts_with("a.py:1:8: warning FL002 "), "{run:?}");
    assert!(
        lines[1].starts_with("b.py:1:12: error FL003 (E0001, syntax-error) "),
        "{run:?}"
    );
    assert_eq!(run.summary(), "3 files linted, 2 findings.");
}

#[test]
fn ac_21_broken_run_exits_two() {
    let project = Project::new();
    project.write("clean.py", CLEAN);
    let assert_broken = |run: support::Run, cause: &str| {
        assert_eq!(run.code, 2, "{cause}: {run:?}");
        assert!(run.stdout.is_empty(), "{cause}: {run:?}");
        assert!(run.stderr.contains(cause), "{cause}: {run:?}");
    };

    assert_broken(project.run(&["--threads", "many"]), "invalid value");
    assert_broken(project.run(&["missing.py"]), "no such file");

    project.write("ferrolint.toml", "select = [\n");
    assert_broken(project.run(&[]), "config error");
    project.write("ferrolint.toml", "select = [\"NOPE1\"]\n");
    assert_broken(project.run(&[]), "no rule is named");

    project.write(
        "ferrolint.toml",
        "plugins = [{ path = \"gone.wasm\", sha256 = \"00\" }]\n",
    );
    assert_broken(project.run(&[]), "gone.wasm");

    project.write("plugin.wasm", b"not wasm");
    project.write(
        "ferrolint.toml",
        "plugins = [{ path = \"plugin.wasm\", sha256 = \"00\" }]\n",
    );
    assert_broken(project.run(&[]), "sha256 mismatch");

    // Bytes that are not WASM, a core module, and a component without the
    // contract's exports all fail to load.
    let empty_component = b"\0asm\x0d\0\x01\0".to_vec();
    let core_module = b"\0asm\x01\0\0\0".to_vec();
    for (bytes, cause) in [
        (b"not wasm".to_vec(), "not a WASM component"),
        (core_module, "not a WASM component"),
        (empty_component, "does not implement the plugin contract"),
    ] {
        let digest = support::sha256(&bytes);
        project.write("plugin.wasm", bytes);
        project.write(
            "ferrolint.toml",
            format!("plugins = [{{ path = \"plugin.wasm\", sha256 = \"{digest}\" }}]\n"),
        );
        assert_broken(project.run(&[]), cause);
    }
}

// Diff mode (AC-22..AC-23, FR-08)

#[test]
#[ignore = "AC-22: --diff-base lints only changed files"]
fn ac_22_diff_base_lints_only_changed() {
    // Given a tree with changed, added and untouched files since the merge base of `--diff-base`,
    // when a run starts with that argument,
    // then only changed and added files are walked and report, and the summary names diff mode and count.
    unimplemented!("AC-22");
}

#[test]
#[ignore = "AC-23: an unresolvable diff base is a usage error"]
fn ac_23_unresolvable_diff_base_usage_error() {
    // Given `--diff-base` naming a ref that does not resolve,
    // when the argument is processed,
    // then no file is linted, the failure is on stderr, and the exit code is 2.
    unimplemented!("AC-23");
}

// Cache (AC-24..AC-25, FR-09)

#[test]
#[ignore = "AC-24: a warm run replays unchanged files"]
fn ac_24_warm_run_replays_unchanged() {
    // Given a first run over a tree and a second identical run,
    // when cache keys match for unchanged files,
    // then they skip parse and plugins and replay diagnostics, with a byte-identical report.
    unimplemented!("AC-24");
}

#[test]
#[ignore = "AC-25: each key-part change invalidates"]
fn ac_25_key_part_change_invalidates() {
    // Given a warm cache and variants changing one key part each (file bytes, plugin binary, config),
    // when each variant runs,
    // then touched files re-lint, untouched files keep stored diagnostics, and the report equals a cold run.
    unimplemented!("AC-25");
}

// Performance (AC-26..AC-28, NFR-01, NFR-02, NFR-05)

#[test]
#[ignore = "AC-26: full-run budgets on a million lines"]
fn ac_26_full_run_budgets_million_lines() {
    // Given a one-million-line corpus with ten plugins run cold against Pylint,
    // when both finish,
    // then ferrolint is under a tenth of Pylint and ten seconds, and a warm run is five times faster again.
    unimplemented!("AC-26");
}

#[test]
#[ignore = "AC-27: peak memory stays under the ceiling"]
fn ac_27_peak_memory_under_ceiling() {
    // Given the corpus and cold full run of AC-26,
    // when the run completes,
    // then peak resident RSS is below 2 GB as measured by the benchmark harness.
    unimplemented!("AC-27");
}

#[test]
#[ignore = "AC-28: startup, compile-once and parallelism hold"]
fn ac_28_startup_compile_once_parallelism() {
    // Given a run with a configuration naming several plugins,
    // when it proceeds through parse and walk,
    // then first parse is under 100 ms, each plugin compiles once, and all eight cores carry work.
    unimplemented!("AC-28");
}

// Sandbox (AC-29..AC-30, NFR-03)

#[test]
fn ac_29_plugin_sandbox_enforced() {
    let project = Project::new();
    let plugin = project.plugin("ferrolint-test-sandbox", "sandbox.wasm");
    project
        .write("ferrolint.toml", plugin)
        .write("a.py", CLEAN)
        .write("b.py", "SANDBOX_EXIT = True\n")
        .write("c.py", CLEAN);
    let run = project.run(&["--threads", "1"]);
    assert_eq!(
        run.code, 2,
        "a plugin abort makes the verdict incomplete: {run:?}"
    );
    for file in ["a.py", "c.py"] {
        for attempt in ["filesystem", "network", "environment"] {
            let expected = format!("{file}:1:1: info SBX001 {attempt}: denied");
            assert!(run.stdout.contains(&expected), "{expected}: {run:?}");
        }
    }
    assert!(
        run.stdout.contains("b.py:1:1: error FL001 plugin sandbox aborted on this file: called the disabled builtin exit("),
        "{run:?}"
    );
    assert!(!project.join("escape.txt").exists());
}

#[test]
fn ac_30_runaway_plugin_aborts() {
    let project = Project::new();
    let plugin = project.plugin("ferrolint-test-runaway", "runaway.wasm");
    project
        .write("ferrolint.toml", plugin)
        .write("a.py", CLEAN)
        .write("b.py", "RUNAWAY = True\n")
        .write("c.py", CLEAN);
    let started = std::time::Instant::now();
    let run = project.run(&[]);
    assert!(started.elapsed().as_secs() < 60, "the cap stops the loop");
    assert_eq!(run.code, 2, "{run:?}");
    assert!(
        run.stdout.contains("a.py:1:1: info RUN001 checked"),
        "{run:?}"
    );
    assert!(
        run.stdout.contains("c.py:1:1: info RUN001 checked"),
        "{run:?}"
    );
    let abort = run
        .finding_lines()
        .into_iter()
        .find(|line| line.starts_with("b.py:"))
        .unwrap();
    assert!(
        abort.contains("FL001 plugin runaway aborted on this file: ran out of fuel")
            || abort.contains("exceeded its time budget"),
        "{abort}"
    );
    assert_eq!(run.summary(), "3 files linted, 3 findings.");
}

// Determinism (AC-31, NFR-04)

#[test]
#[ignore = "AC-31: same input, same output, any schedule"]
fn ac_31_deterministic_across_schedules() {
    // Given one tree and configuration, run 50 times with varying threads, a warm cache in the second half,
    // when the reports are compared,
    // then every text and json report is byte-identical, including plugin order, across two machines.
    unimplemented!("AC-31");
}

// Platforms (AC-32..AC-34, NFR-06, NFR-07, NFR-08)

#[test]
#[ignore = "AC-32: one static binary per target"]
fn ac_32_static_binary_per_target() {
    // Given the three release targets (linux x86_64, linux aarch64, macos aarch64),
    // when a release build runs,
    // then each static binary starts run, help and config validation with no system package or runtime.
    unimplemented!("AC-32");
}

#[test]
#[ignore = "AC-33: Ruff crates move only one-way"]
fn ac_33_ruff_crates_single_pin() {
    // Given the three re-used Ruff parser crates,
    // when `cargo tree` is resolved,
    // then every use goes through the single git-tag pin and the upgrade procedure is tag-plus-fixtures.
    unimplemented!("AC-33");
}

#[test]
#[ignore = "AC-34: the gate is green over the whole workspace"]
fn ac_34_gate_green_whole_workspace() {
    // Given `make check` and `make test` on a workspace identical to a release's,
    // when both run,
    // then they pass on every plugin, the SDK crate and the linter with pedantic clippy and cargo-deny.
    unimplemented!("AC-34");
}
