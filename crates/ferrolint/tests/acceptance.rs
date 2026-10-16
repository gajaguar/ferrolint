//! Acceptance criteria stubs, one test per row of `docs/requirements/ac-index.md`.
//!
//! Each test is ignored until its criterion is implemented; the body is a
//! single `unimplemented!` marker so the suite compiles before the feature
//! work lands.

// Configuration (AC-01..AC-06, FR-01, FR-02)

#[test]
#[ignore = "AC-01: configuration loads from pyproject.toml"]
fn ac_01_config_loads_from_pyproject() {
    // Given a repository with a `[tool.ferrolint]` section in `pyproject.toml`,
    // when a run starts in that directory,
    // then it uses that section's settings and exits 0 or 1, not a config error.
    unimplemented!("AC-01");
}

#[test]
#[ignore = "AC-02: ferrolint.toml wins over pyproject.toml"]
fn ac_02_ferrolint_toml_wins_over_pyproject() {
    // Given both a `ferrolint.toml` and a `[tool.ferrolint]` table with differing settings,
    // when a run starts,
    // then the effective configuration is `ferrolint.toml`'s and nothing from pyproject applies.
    unimplemented!("AC-02");
}

#[test]
#[ignore = "AC-03: plugin SHA mismatch is a configuration error"]
fn ac_03_plugin_sha_mismatch_is_config_error() {
    // Given a configuration whose plugin entry's `sha256` does not match the file at `path`,
    // when a run starts,
    // then no file is linted, the mismatch is on stderr, and the exit code is 2.
    unimplemented!("AC-03");
}

#[test]
#[ignore = "AC-04: select and ignore filter findings"]
fn ac_04_select_and_ignore_filter_findings() {
    // Given selected and ignored codes firing on the same file plus a per-file-ignores glob,
    // when the file is linted inside and outside the glob,
    // then the ignored code never appears, the selected code only outside the glob, in every format.
    unimplemented!("AC-04");
}

#[test]
#[ignore = "AC-05: a run honours gitignore and configured excludes"]
fn ac_05_honours_gitignore_and_excludes() {
    // Given a target directory whose files are matched by `.gitignore` and by configured `exclude`,
    // when the directory is linted,
    // then ignored files are absent from report and count, byte-identical to removing them from the tree.
    unimplemented!("AC-05");
}

#[test]
#[ignore = "AC-06: explicitly named files bypass discovery filters"]
fn ac_06_named_files_bypass_discovery() {
    // Given a `.gitignore`-covered Python file named explicitly on the command line,
    // when a run starts,
    // then the file is linted and its findings report normally.
    unimplemented!("AC-06");
}

// Plugins (AC-07..AC-10, FR-03, FR-04)

#[test]
#[ignore = "AC-07: a plugin receives AST visitor events"]
fn ac_07_plugin_receives_visitor_events() {
    // Given a plugin recording every visitor event for a file,
    // when a fixture with a function, a class and nested statements is linted,
    // then events cover module body, class and function in source order, no visit after the file ends.
    unimplemented!("AC-07");
}

#[test]
#[ignore = "AC-08: scope and binding queries resolve names"]
fn ac_08_scope_and_binding_queries_resolve_names() {
    // Given a plugin querying visible bindings and resolution targets at each name,
    // when a fixture with a global, a shadowing local and a function-local import is linted,
    // then the answers are the local, the global and the import origin - never the shadowed binding.
    unimplemented!("AC-08");
}

#[test]
#[ignore = "AC-09: diagnostic records carry the full field set"]
fn ac_09_diagnostic_records_full_field_set() {
    // Given one diagnostic with severity, message, code, range and fix data, one without fix data,
    // when linting with `--format json`,
    // then both appear with exactly those fields, the fix is a host-reported suggestion, exit code 1.
    unimplemented!("AC-09");
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
#[ignore = "AC-19: a clean run exits 0"]
fn ac_19_clean_run_exits_zero() {
    // Given a run over files with no findings under the active configuration,
    // when it completes,
    // then the exit code is 0 and the report carries the file count and zero findings.
    unimplemented!("AC-19");
}

#[test]
#[ignore = "AC-20: a run with findings exits 1"]
fn ac_20_findings_run_exits_one() {
    // Given a run over files with un-suppressed findings,
    // when it completes,
    // then the exit code is 1, findings are on stdout in the chosen format, count matches.
    unimplemented!("AC-20");
}

#[test]
#[ignore = "AC-21: a broken run exits 2"]
fn ac_21_broken_run_exits_two() {
    // Given a run whose startup fails (bad flag, malformed config, missing plugin, SHA mismatch),
    // when it fails,
    // then exit code 2, cause on stderr, stdout empty of the report, regardless of prior runnable files.
    unimplemented!("AC-21");
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
#[ignore = "AC-29: a plugin touches nothing it should not"]
fn ac_29_plugin_sandbox_enforced() {
    // Given a plugin attempting a filesystem open, socket, environment read and host-disabled builtin,
    // when those attempts execute,
    // then each fails inside the instance, the failure is reported, and remaining files still lint.
    unimplemented!("AC-29");
}

#[test]
#[ignore = "AC-30: a runaway plugin cannot hang a run"]
fn ac_30_runaway_plugin_aborts() {
    // Given a plugin whose visitor loops forever and a host-chosen fuel or epoch budget,
    // when the cap is hit,
    // then the work aborts, the file reports termination, and the run completes over remaining files.
    unimplemented!("AC-30");
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
