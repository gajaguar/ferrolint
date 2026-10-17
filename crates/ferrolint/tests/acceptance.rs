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

/// A project loading the unused-import sample plugin, with `config` on
/// top of the plugin table.
fn sample_project(config: &str) -> Project {
    let project = Project::new();
    let plugin = project.plugin("ferrolint-plugin-unused-import", "unused_import.wasm");
    project.write("ferrolint.toml", format!("{config}\n{plugin}"));
    project
}

/// The (path, line, code) of every finding of a run, read from each of the
/// four formats, which must agree one for one.
fn every_format(project: &Project, args: &[&str]) -> Vec<(String, u64, String)> {
    let run = |format: &str| {
        let mut with_format = args.to_vec();
        with_format.extend(["--format", format]);
        project.run(&with_format).stdout
    };
    let text: Vec<(String, u64, String)> = project
        .run(args)
        .finding_lines()
        .iter()
        .map(|line| text_finding(line))
        .collect();
    let json: Vec<(String, u64, String)> = json_diagnostics(&run("json"))
        .iter()
        .map(|d| {
            (
                d["path"].as_str().unwrap().to_owned(),
                d["range"]["start"]["line"].as_u64().unwrap(),
                d["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let sarif: Value = serde_json::from_str(&run("sarif")).unwrap();
    let sarif: Vec<(String, u64, String)> = sarif["runs"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let location = &r["locations"][0]["physicalLocation"];
            (
                location["artifactLocation"]["uri"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                location["region"]["startLine"].as_u64().unwrap(),
                r["ruleId"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let junit_xml = run("junit");
    let junit_doc = roxmltree::Document::parse(&junit_xml).unwrap();
    let junit: Vec<(String, u64, String)> = junit_doc
        .descendants()
        .filter(|node| node.has_tag_name("failure"))
        .map(|failure| {
            let (path, line, _) = text_finding(failure.text().unwrap());
            (path, line, failure.attribute("type").unwrap().to_owned())
        })
        .collect();
    assert_eq!(text, json, "text and json agree");
    assert_eq!(json, sarif, "json and sarif agree");
    assert_eq!(json, junit, "json and junit agree");
    json
}

/// The path, line and code of one text report line.
fn text_finding(line: &str) -> (String, u64, String) {
    let fields: Vec<&str> = line.split(' ').collect();
    let mut location = fields[0].split(':');
    let path = location.next().unwrap().to_owned();
    let line_number = location.next().unwrap().parse().unwrap();
    (path, line_number, fields[2].to_owned())
}

/// The (line, code) pairs a run reports in every format.
fn reported(project: &Project, args: &[&str]) -> Vec<(u64, String)> {
    every_format(project, args)
        .into_iter()
        .map(|(_, line, code)| (line, code))
        .collect()
}

/// The run every output format test shares: the sample plugin and the host
/// rules firing over four files, one of them clean.
fn output_project() -> Project {
    let project = sample_project("");
    project
        .write("a.py", "import os\nimport sys  # noqa\n")
        .write("b.py", "print(1)\n")
        .write("pkg/c.py", "from typing import Any\n")
        .write("d.py", SYNTAX_ERROR);
    project
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
    assert_eq!(project.run(&[]).code, 1);
    assert_eq!(
        every_format(&project, &[]),
        [("outside/both.py".to_owned(), 3, "FL002".to_owned())],
        "the ignored code never reports, the selected one only outside the glob"
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
    project.write("pkg/__init__.py", "");
    project.write(
        "pkg/shapes.py",
        "import os\n\nclass Shape:\n    def area(self):\n        if self:\n            return 1\n        return 0\n\ndef main():\n    for item in [Shape()]:\n        print(item.area(), os.sep)\n",
    );
    let run = project.run(&["pkg/shapes.py", "--format", "json"]);
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
fn ac_10_sample_plugin_matches_pylint_checker() {
    // Each fixture's `.txt` sibling holds Pylint's own W0611 findings on it,
    // recorded by `make sample-fixtures`: line:column:msg-id:symbol:message.
    let fixtures = support::workspace().join("plugins/unused-import/fixtures");
    let project = Project::new();
    let plugin = project.plugin("ferrolint-plugin-unused-import", "unused_import.wasm");
    project
        .write("ferrolint.toml", plugin)
        .copy_dir(&fixtures, "fixtures");

    let mut expected = Vec::new();
    for entry in support::walk(&fixtures) {
        if entry.extension().is_some_and(|ext| ext == "txt") {
            let relative = entry.strip_prefix(&fixtures).unwrap().with_extension("py");
            let path = format!("fixtures/{}", relative.to_string_lossy().replace('\\', "/"));
            for line in std::fs::read_to_string(&entry).unwrap().lines() {
                let fields: Vec<&str> = line.splitn(5, ':').collect();
                let column: u64 = fields[1].parse().unwrap();
                expected.push(format!(
                    "{path}:{}:{}:{}:{}:{}",
                    fields[0],
                    column + 1,
                    fields[2],
                    fields[3],
                    fields[4]
                ));
            }
        }
    }
    expected.sort();
    assert!(
        expected.len() >= 20,
        "the fixtures exercise the checker: {expected:?}"
    );

    let run = project.run(&["fixtures", "--format", "json"]);
    assert_eq!(run.code, 1, "{run:?}");
    let mut actual: Vec<String> = json_diagnostics(&run.stdout)
        .iter()
        .map(|d| {
            let aliases = d["aliases"].as_array().unwrap();
            format!(
                "{}:{}:{}:{}:{}:{}",
                d["path"].as_str().unwrap(),
                d["range"]["start"]["line"],
                d["range"]["start"]["column"],
                aliases[0].as_str().unwrap(),
                aliases[1].as_str().unwrap(),
                d["message"].as_str().unwrap()
            )
        })
        .collect();
    actual.sort();
    assert_eq!(actual, expected);
}

// Suppressions (AC-11..AC-13, FR-05)

#[test]
fn ac_11_ferrolint_disable_suppresses() {
    let project = sample_project("");
    project.write(
        "a.py",
        "import os  # ferrolint: disable=IMP001\nimport sys  # ferrolint: disable=IMP001  # noqa\nimport re  # ferrolint: disable=IMP001,FL002  # noqa\nimport json\n",
    );
    assert_eq!(
        reported(&project, &[]),
        [(2, "FL002".to_owned()), (4, "IMP001".to_owned())],
        "a listed code is gone, an unlisted one on the same line stays"
    );
}

#[test]
fn ac_12_pylint_disable_suppresses() {
    let project = sample_project("");
    project.write(
        "a.py",
        "import os  # pylint: disable=W0611\nimport sys  # pylint: disable=unused-import\nimport re  # pylint: disable=IMP001\nimport json  # pylint: disable=W0612\nimport csv  # Pylint: disable = unused-import, W0612\n",
    );
    assert_eq!(reported(&project, &[]), [(4, "IMP001".to_owned())]);
}

#[test]
fn ac_13_noqa_suppresses() {
    let project = probe_project(&["PRB002", "PRB004"]);
    project.write(
        "a.py",
        "def one(): return len  # noqa: PRB004\ndef two(): return len  # noqa: PRB004, PRB002\ndef three(): return len  # noqa: NOPE999\n",
    );
    assert_eq!(
        reported(&project, &[]),
        [
            (1, "PRB002".to_owned()),
            (3, "PRB004".to_owned()),
            (3, "PRB002".to_owned()),
        ],
        "exactly the listed codes are suppressed; an unknown code is inert"
    );
}

// Output (AC-14..AC-18, FR-06, FR-10)

#[test]
fn ac_14_text_output_default() {
    let project = output_project();
    let default = project.run(&[]);
    let text = project.run(&["--format", "text"]);
    assert_eq!(default.code, 1, "{default:?}");
    assert_eq!(default.stdout, text.stdout, "--format text is the default");
    assert_eq!(
        default.finding_lines(),
        [
            "a.py:1:1: warning IMP001 (W0611, unused-import) Unused import os",
            "a.py:2:1: warning IMP001 (W0611, unused-import) Unused import sys",
            "a.py:2:13: warning FL002 `# noqa` without codes suppresses nothing; name the codes",
            "d.py:1:12: error FL003 (E0001, syntax-error) Expected a parameter or the end of the parameter list",
            "pkg/c.py:1:1: warning IMP001 (W0611, unused-import) Unused Any imported from typing",
        ]
    );
    assert_eq!(default.summary(), "4 files linted, 5 findings.");
}

#[test]
fn ac_15_json_output_machine_parseable() {
    let project = output_project();
    let run = project.run(&["--format", "json"]);
    let report: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(
        serde_json::to_string(&report).unwrap() + "\n",
        run.stdout,
        "sorted keys, no whitespace: canonical JSON"
    );
    let keys: Vec<&String> = report.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["diagnostics", "summary"]);
    assert_eq!(
        report["summary"],
        serde_json::json!({"files": 4, "findings": 5})
    );
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 5);
    for diagnostic in diagnostics {
        for key in ["code", "message", "range", "severity", "path", "aliases"] {
            assert!(diagnostic.get(key).is_some(), "{key} in {diagnostic}");
        }
    }
}

#[test]
fn ac_16_sarif_output_sarif_2_1() {
    let project = output_project();
    let run = project.run(&["--format", "sarif"]);
    let sarif: Value = serde_json::from_str(&run.stdout).unwrap();
    let schema_path =
        support::workspace().join("crates/ferrolint/tests/data/sarif-schema-2.1.0.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::draft4::new(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&sarif)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");

    let json = project.run(&["--format", "json"]);
    let diagnostics = json_diagnostics(&json.stdout);
    let sarif_run = &sarif["runs"][0];
    let rules = sarif_run["tool"]["driver"]["rules"].as_array().unwrap();
    let results = sarif_run["results"].as_array().unwrap();
    assert_eq!(results.len(), diagnostics.len());
    for (result, diagnostic) in results.iter().zip(&diagnostics) {
        assert_eq!(result["ruleId"], diagnostic["code"]);
        let index = usize::try_from(result["ruleIndex"].as_u64().unwrap()).unwrap();
        assert_eq!(rules[index]["id"], diagnostic["code"]);
        let level = match diagnostic["severity"].as_str().unwrap() {
            "error" => "error",
            "warning" => "warning",
            _ => "note",
        };
        assert_eq!(result["level"], level);
        let location = &result["locations"][0]["physicalLocation"];
        assert_eq!(location["artifactLocation"]["uri"], diagnostic["path"]);
        assert_eq!(
            location["region"]["startLine"],
            diagnostic["range"]["start"]["line"]
        );
        assert_eq!(result["properties"]["aliases"], diagnostic["aliases"]);
    }
}

#[test]
fn ac_17_junit_output_test_report() {
    let project = output_project();
    let run = project.run(&["--format", "junit"]);
    let document = roxmltree::Document::parse(&run.stdout).unwrap();
    let root = document.root_element();
    assert!(root.has_tag_name("testsuites"));
    assert_eq!(root.attribute("tests"), Some("4"));
    assert_eq!(root.attribute("failures"), Some("3"), "three files fail");
    let cases: Vec<roxmltree::Node> = root
        .descendants()
        .filter(|node| node.has_tag_name("testcase"))
        .collect();
    let names: Vec<&str> = cases
        .iter()
        .map(|case| case.attribute("name").unwrap())
        .collect();
    assert_eq!(
        names,
        ["a.py", "b.py", "d.py", "pkg/c.py"],
        "one testcase per linted file"
    );

    let json = project.run(&["--format", "json"]);
    for case in cases {
        let path = case.attribute("name").unwrap();
        let failures: Vec<&str> = case
            .children()
            .filter(|node| node.has_tag_name("failure"))
            .map(|failure| failure.attribute("type").unwrap())
            .collect();
        let expected: Vec<String> = json_diagnostics(&json.stdout)
            .iter()
            .filter(|d| d["path"] == path)
            .map(|d| d["code"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(failures, expected, "{path}");
    }
}

#[test]
fn ac_18_pylint_aliases_resolve() {
    let source = "import os\nimport sys\n\nprint(sys)\n";
    for select in ["W0611", "unused-import"] {
        let project = sample_project(&format!("select = [\"{select}\"]"));
        project.write("a.py", source);
        let text = project.run(&[]);
        assert_eq!(
            text.finding_lines(),
            ["a.py:1:1: warning IMP001 (W0611, unused-import) Unused import os"],
            "{select}: {text:?}"
        );
        let json = project.run(&["--format", "json"]);
        let diagnostics = json_diagnostics(&json.stdout);
        assert_eq!(diagnostics[0]["code"], "IMP001");
        assert_eq!(
            diagnostics[0]["aliases"],
            serde_json::json!(["W0611", "unused-import"])
        );
    }
    for ignore in ["W0611", "unused-import", "IMP001"] {
        let project = sample_project(&format!("ignore = [\"{ignore}\"]"));
        project.write("a.py", source);
        assert!(reported(&project, &[]).is_empty(), "{ignore}");
    }
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
fn ac_31_deterministic_across_schedules() {
    let project = output_project();
    project.write(
        "ferrolint.toml",
        format!(
            "{}{}",
            project.plugin("ferrolint-test-probe", "probe.wasm"),
            project.plugin("ferrolint-plugin-unused-import", "unused_import.wasm")
        ),
    );
    for index in 0..20 {
        project.write(
            &format!("many/m{index:02}.py"),
            format!("import os\ndef f{index}(x):\n    return x\n"),
        );
    }
    let formats = ["text", "json", "sarif", "junit"];
    let mut first: Vec<Option<String>> = vec![None; formats.len()];
    for run_number in 0..50_usize {
        let threads = (run_number % 8 + 1).to_string();
        let slot = run_number % formats.len();
        let run = project.run(&["--threads", &threads, "--format", formats[slot]]);
        assert_eq!(run.code, 1, "{run:?}");
        match &first[slot] {
            None => first[slot] = Some(run.stdout),
            Some(expected) => {
                assert_eq!(&run.stdout, expected, "run {run_number}, {threads} threads");
            }
        }
    }
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
