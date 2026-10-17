//! The bundled plugins on small fixtures: each reports what its Pylint
//! checker reports there, and nothing on the code it leaves alone.

// A helper that panics is how a test fails, as `clippy.toml` allows
// inside `#[test]` functions themselves.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::Project;

/// Lint `source` with one plugin and return `line: message` per finding.
fn findings(package: &str, source: &str) -> Vec<String> {
    let project = Project::new();
    let plugin = project.plugin(package, "plugin.wasm");
    project
        .write("ferrolint.toml", plugin)
        .write("case.py", source);
    let run = project.run(&["--no-cache"]);
    assert!(run.code <= 1, "{run:?}");
    run.finding_lines()
        .iter()
        .map(|line| {
            let (location, rest) = line.split_once(": ").unwrap();
            let message = rest.split_once(") ").map_or(rest, |(_, message)| message);
            format!("{}: {message}", location.split(':').nth(1).unwrap())
        })
        .collect()
}

#[test]
fn undefined_variable() {
    let source =
        "import os\n\ndef f(a):\n    return a + b + os.sep + len(__file__)\n\nprint(missing)\n";
    assert_eq!(
        findings("ferrolint-plugin-undefined-variable", source),
        [
            "4: Undefined variable 'b'",
            "6: Undefined variable 'missing'"
        ]
    );
    let star = "from os import *\nprint(sep)\n";
    assert_eq!(
        findings("ferrolint-plugin-undefined-variable", star),
        Vec::<String>::new()
    );
}

#[test]
fn redefined_builtin() {
    let source = "from builtins import str\nlist = []\nlist = [1]\n\ndef f(id, value):\n    max = value\n    return id, max\n\nclass C:\n    type = 1\n";
    assert_eq!(
        findings("ferrolint-plugin-redefined-builtin", source),
        [
            "2: Redefining built-in 'list'",
            "5: Redefining built-in 'id'",
            "6: Redefining built-in 'max'",
        ]
    );
}

#[test]
fn unused_variable() {
    let source = "def f(values):\n    total = 0\n    unused = 1\n    _ignored = 2\n    for item in values:\n        total += item\n    return total\n\ndef g():\n    hidden = 1\n    return locals()\n\ndef counter():\n    n = 0\n    def inc():\n        nonlocal n\n        n += 1\n    return inc\n";
    assert_eq!(
        findings("ferrolint-plugin-unused-variable", source),
        ["3: Unused variable 'unused'"]
    );
}

#[test]
fn global_statement() {
    let source = "global top\ncounter = 0\n\ndef bump():\n    global counter\n    counter += 1\n";
    assert_eq!(
        findings("ferrolint-plugin-global-statement", source),
        ["5: Using the global statement"]
    );
}

#[test]
fn bare_except() {
    let source =
        "try:\n    pass\nexcept:\n    pass\n\ntry:\n    pass\nexcept ValueError:\n    pass\n";
    assert_eq!(
        findings("ferrolint-plugin-bare-except", source),
        ["3: No exception type(s) specified"]
    );
}

#[test]
fn pointless_statement() {
    let source = "\"\"\"Docstring.\"\"\"\nx = 1\nx\nx == 2\nprint(x)\n...\n";
    assert_eq!(
        findings("ferrolint-plugin-pointless-statement", source),
        [
            "3: Statement seems to have no effect",
            "4: Statement seems to have no effect"
        ]
    );
}

#[test]
fn unnecessary_pass() {
    let source = "class Empty:\n    pass\n\nclass Documented:\n    \"\"\"Doc.\"\"\"\n    pass\n";
    assert_eq!(
        findings("ferrolint-plugin-unnecessary-pass", source),
        ["6: Unnecessary pass statement"]
    );
}

#[test]
fn singleton_comparison() {
    let source =
        "x = None\nif x == None:\n    pass\nif x != True:\n    pass\nif x is None:\n    pass\n";
    assert_eq!(
        findings("ferrolint-plugin-singleton-comparison", source),
        [
            "2: Comparison 'x == None' should be 'x is None'",
            "4: Comparison 'x != True' should be 'x is not True' if checking for the singleton value True, or 'not x' if testing for falsiness",
        ]
    );
}

#[test]
fn self_assigning_variable() {
    let source = "x = 1\nx = x\ny = x\n";
    assert_eq!(
        findings("ferrolint-plugin-self-assigning-variable", source),
        ["2: Assigning the same variable 'x' to itself"]
    );
}
