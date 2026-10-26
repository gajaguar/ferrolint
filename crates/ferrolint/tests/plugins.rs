//! The bundled plugins on small fixtures: each reports what its Pylint
//! checker reports there, and nothing on the code it leaves alone.

// A helper that panics is how a test fails, as `clippy.toml` allows
// inside `#[test]` functions themselves.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::Project;

/// Lint `source` with one plugin and return `line: message` per finding.
fn findings(package: &str, source: &str) -> Vec<String> {
    findings_beside(package, "", source)
}

/// As [`findings`], with a `tokens.py` of `tokens` beside `case.py` when it
/// is not empty. Its own findings would show, so it must have none.
fn findings_beside(package: &str, tokens: &str, source: &str) -> Vec<String> {
    let project = Project::new();
    let plugin = project.plugin(package, "plugin.wasm");
    project
        .write("ferrolint.toml", plugin)
        .write("case.py", source);
    if !tokens.is_empty() {
        project.write("tokens.py", tokens);
    }
    let run = project.run(&["--no-cache"]);
    assert!(run.code <= 1, "{run:?}");
    run.finding_lines()
        .iter()
        // the sources name things as they like, import `*` and declare bare
        // classes; `C0103`, `W0401` and `R0903` are not under test
        .filter(|line| {
            !(line.contains(" C0103 ") || line.contains(" W0401 ") || line.contains(" R0903 "))
        })
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
    let guarded = "try:\n    buffer\nexcept (OSError, NameError):\n    pass\ntry:\n    other\nexcept:\n    pass\n";
    assert_eq!(
        findings("ferrolint-plugin-undefined-variable", guarded),
        ["6: Undefined variable 'other'"]
    );
    let star = "from os import *\nprint(sep)\n";
    assert_eq!(
        findings("ferrolint-plugin-undefined-variable", star),
        Vec::<String>::new()
    );
}

#[test]
fn undefined_variable_leaves_a_name_bound_later_to_pylint_e0601() {
    let source = "import sys\nif sys.maxsize:\n    A = 1\nelse:\n    B = unicode\n    print(later)\nunicode = str\nclass C:\n    print(later)\n    print(inner)\n    inner = 1\nprint(never)\nlater = 1\n";
    assert_eq!(
        findings("ferrolint-plugin-undefined-variable", source),
        ["12: Undefined variable 'never'"]
    );
}

#[test]
fn undefined_variable_reads_a_del_as_a_use_of_the_name_before_it() {
    // A comprehension variable does not leak, a later binding is no help,
    // and a module name can be deleted in a function.
    let source = "G = 1\n\nclass C:\n    t = [x for x in (1,)]\n    del x\n\ndef f():\n    del G\n    del z\n    z = 1\n    y = 2\n    del y\n    del y\n";
    assert_eq!(
        findings("ferrolint-plugin-undefined-variable", source),
        ["5: Undefined variable 'x'", "9: Undefined variable 'z'"]
    );
}

#[test]
fn undefined_variable_reads_the_names_a_project_star_import_binds() {
    // Pylint binds the public names of the module and ignores its `__all__`.
    let tokens = "__all__ = ['alpha']\nalpha = 1\nbeta = 2\n_hidden = 3\n";
    let source = "from tokens import *\nprint(alpha, beta, _hidden, missing)\n";
    assert_eq!(
        findings_beside("ferrolint-plugin-undefined-variable", tokens, source),
        [
            "2: Undefined variable '_hidden'",
            "2: Undefined variable 'missing'"
        ]
    );
    // A module beside a star it cannot find keeps the plugin silent.
    let unknown = "from tokens import *\nfrom nowhere import *\nprint(alpha, beta, missing)\n";
    assert_eq!(
        findings_beside("ferrolint-plugin-undefined-variable", tokens, unknown),
        Vec::<String>::new()
    );
}

#[test]
fn redefined_builtin() {
    let source = "from builtins import str\nfrom six.moves import range\nfrom os import (\n    path,\n    open,\n)\nlist = []\nlist = [1]\n__doc__ = ''\n\ndef sorted(): ...\n\ndef f(id, value, sorted):\n    max = value\n    squares = [map for map in value]\n    return id, max, lambda input: input\n\ndef g(point):\n    match point:\n        case Point(type=type):\n            return type\n\nclass C:\n    format = 1\n\ndef h():\n    global input\n    input = 1\n\nstr = 1\n";
    // The import is reported at its statement, not at the name; the first
    // binding of `str` is an import Pylint lets be, so a later one is too; `sorted` is
    // the module's, so a parameter of it is the native `W0621`, not this rule.
    assert_eq!(
        findings("ferrolint-plugin-redefined-builtin", source),
        [
            "3: Redefining built-in 'open'",
            "7: Redefining built-in 'list'",
            "11: Redefining built-in 'sorted'",
            "13: Redefining built-in 'id'",
            "13: Redefining name 'sorted' from outer scope (line 11)",
            "14: Redefining built-in 'max'",
            "20: Redefining built-in 'type'",
            "27: Global variable 'input' undefined at the module level",
            "28: Redefining built-in 'input'",
        ]
    );
}

#[test]
fn redefined_builtin_override() {
    let source = "from typing import Generic, TypeVar, overload\n\nT = TypeVar('T')\n\nclass Base(Generic[T]):\n    def run(self, id, input):\n        return id, input\n\nclass Child(Base[int]):\n    def run(self, id, value):\n        return id, value\n\nclass Grand(Child):\n    def run(self, id, input):\n        return id, input\n\nclass Other(Missing):\n    def run(self, id):\n        return id\n\nclass PBase:\n    def pos(self, id, /):\n        return id\n\nclass PChild(PBase):\n    def pos(self, id):\n        return id\n\nclass IChild(Base):\n    if True:\n        def run(self, id):\n            return id\n\nclass OBase:\n    @overload\n    def get(self) -> int: ...\n    @overload\n    def get(self, type: int) -> int: ...\n    def get(self, type=None): ...\n\nclass OChild(OBase):\n    def get(self, type=None): ...\n";
    // Pylint excuses a parameter that the method of the first ancestor with
    // that method repeats: `Child.run` has `id` but not `input`, `Missing`
    // is not a class, a position-only parameter of the ancestor is no name, a
    // method under an `if` of the class body overrides nothing, and the first overload
    // stub is the method that counts.
    assert_eq!(
        findings("ferrolint-plugin-redefined-builtin", source)
            .into_iter()
            .filter(|line| line.contains("built-in"))
            .collect::<Vec<_>>(),
        [
            "6: Redefining built-in 'id'",
            "6: Redefining built-in 'input'",
            "14: Redefining built-in 'input'",
            "18: Redefining built-in 'id'",
            "22: Redefining built-in 'id'",
            "26: Redefining built-in 'id'",
            "31: Redefining built-in 'id'",
            "38: Redefining built-in 'type'",
            "39: Redefining built-in 'type'",
            "42: Redefining built-in 'type'",
        ]
    );
}

#[test]
fn redefined_builtin_override_of_another_module() {
    // Only the base's own `id` shows: the override of `case.py` repeats it.
    let tokens = "class Base:\n    def run(self, id=None): ...\n";
    let source = "from tokens import Base\n\nclass C(Base):\n    def run(self, id=None): ...\n";
    assert_eq!(
        findings_beside("ferrolint-plugin-redefined-builtin", tokens, source),
        ["2: Redefining built-in 'id'"]
    );
}

#[test]
fn redefined_builtin_options() {
    let source = "from six.moves import range\nfrom os import open\n\ndef f(id, input):\n    return id, input\n";
    let project = Project::new();
    let plugin = project.plugin("ferrolint-plugin-redefined-builtin", "plugin.wasm");
    let options =
        "[options]\nredefining-builtins-modules = \"os\"\nallowed-redefined-builtins = \"id\"\n";
    project
        .write("ferrolint.toml", format!("{options}{plugin}"))
        .write("case.py", source);
    let run = project.run(&["--no-cache"]);
    let lines = run.finding_lines();
    let found = |text: &str| lines.iter().any(|line| line.contains(text));
    assert!(found("Redefining built-in 'range'"), "{lines:?}");
    assert!(found("Redefining built-in 'input'"), "{lines:?}");
    assert!(!found("Redefining built-in 'open'"), "{lines:?}");
    assert!(!found("Redefining built-in 'id'"), "{lines:?}");
}

#[test]
fn unused_variable() {
    let source = "def f(values):\n    total = 0\n    unused = 1\n    _ignored = 2\n    for item in values:\n        total += item\n    return total\n\ndef g():\n    hidden = 1\n    return locals()\n\ndef counter():\n    n = 0\n    def inc():\n        nonlocal n\n        n += 1\n    return inc\n";
    assert_eq!(
        findings("ferrolint-plugin-unused-variable", source),
        [
            "3: Unused variable 'unused'",
            // The plugin leaves a function that calls `locals()` to the
            // native `W0641`, as Pylint splits the two.
            "10: Possibly unused variable 'hidden'",
        ]
    );
}

#[test]
fn unused_variable_follows_pylint_targets_and_dummy_names() {
    let source = "def f(items, ctx, p):\n    for i in items:\n        pass\n    with ctx as c:\n        pass\n    try:\n        pass\n    except ValueError as e:\n        pass\n    (n := 5)\n    dummy = _x_ = xdummy = __tracebackhide__ = unused_a = _ok = 1\n    p = 2\n    for k in items:\n        pass\n    [k for k in items]\n    def inner():\n        pass\n    class Inner:\n        pass\n    @items\n    def decorated():\n        pass\n    match items:\n        case [m, *rest]:\n            print(m)\n\ndef g(items):\n    before = 1\n    after = 2\n    return locals(), items\n\ndef h():\n    try:\n        pass\n    except OSError as e:\n        pass\n    try:\n        pass\n    except KeyError as e:\n        print(e)\n";
    assert_eq!(
        findings("ferrolint-plugin-unused-variable", source),
        [
            "1: Too many local variables (19/15)",
            "2: Unused variable 'i'",
            "4: Unused variable 'c'",
            "8: Unused variable 'e'",
            "10: Unused variable 'n'",
            "11: Unused variable '_x_'",
            "11: Unused variable 'xdummy'",
            "11: Unused variable '__tracebackhide__'",
            "16: Unused variable 'inner'",
            "18: Unused variable 'Inner'",
            "24: Unused variable 'rest'",
            "28: Possibly unused variable 'before'",
            "29: Possibly unused variable 'after'",
            "35: Unused variable 'e'",
        ]
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
fn global_statement_leaves_w0601_and_w0602_to_pylint() {
    // Pylint: 10 and 25 are W0602 (read only, or the statement has a W0602
    // name), 20 is W0601 (never defined at module level); 15, 30, 35, 40 and 45 are W0603.
    let source = "import os\n\nro = 1\nrw = 1\nundef = None\nfrom os import sep\n\n\ndef read_only():\n    global ro\n    return ro\n\n\ndef reassigned():\n    global rw\n    rw = 2\n\n\ndef undefined_name():\n    global nowhere\n    nowhere = 3\n\n\ndef mixed():\n    global ro, rw\n    rw = 4\n\n\ndef imported_then_set():\n    global sep\n    sep = '/'\n\n\ndef local_import():\n    global json\n    import json\n\n\ndef module_attribute():\n    global __file__\n    __file__ = 'x'\n\n\ndef except_name():\n    global ro\n    try:\n        pass\n    except ValueError as ro:\n        pass\n";
    assert_eq!(
        findings("ferrolint-plugin-global-statement", source),
        [
            "15: Using the global statement",
            "20: Global variable 'nowhere' undefined at the module level",
            "30: Using the global statement",
            "35: Using the global statement",
            "36: Import outside toplevel (json)",
            "40: Using the global statement",
            "45: Using the global statement"
        ]
    );
}

#[test]
fn bare_except() {
    let source = "try:\n    pass\nexcept:\n    pass\n\ntry:\n    pass\nexcept ValueError:\n    pass\n\ntry:\n    pass\nexcept:\n    cleanup()\n    raise\n\ntry:\n    pass\nexcept:\n    if x:\n        raise\n";
    assert_eq!(
        findings("ferrolint-plugin-bare-except", source),
        [
            "3: No exception type(s) specified",
            "19: No exception type(s) specified"
        ]
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
fn not_callable_reports_resolved_builtin_instances() {
    let source = "x = 1\nx()\nname: str = 'a'\nname()\nNone()\nitems = [x]\nitems()\n\ndef f(y):\n    return y()\n";
    assert_eq!(
        findings("ferrolint-plugin-not-callable", source),
        [
            "2: x is not callable",
            "4: name is not callable",
            "5: None is not callable",
            "7: items is not callable",
        ]
    );
}

#[test]
fn not_callable_stays_silent_when_the_type_is_unresolved() {
    let source = "import random\nfrom os import sep\n\nclass C:\n    pass\n\ndef make() -> C:\n    return C()\n\nfn = random.choice([len, 1])\nfn()\nsep()\nC()\nmake()\nlen('a')\nint('1')\na, b = 1, 2\na()\ntry:\n    from _speedups import fast\nexcept ImportError:\n    fast = None\nfast()\ncallback: Callable[[], None] | None = None\ncallback()\nflag = 1 == 2\nflag()\n\ndef g(validate_host: bool):\n    validate_host(1)\ng(True)\n";
    assert_eq!(
        findings("ferrolint-plugin-not-callable", source),
        Vec::<String>::new()
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

/// The fixture holds the quoting, operand order and `bool()` cases;
/// `messages.txt` is Pylint's output on it (`line: message`).
#[test]
fn singleton_comparison_messages_match_pylint() {
    let fixtures = support::workspace().join("plugins/singleton-comparison/fixtures");
    let source = std::fs::read_to_string(fixtures.join("messages.py")).unwrap();
    let recorded = std::fs::read_to_string(fixtures.join("messages.txt")).unwrap();
    let found = findings("ferrolint-plugin-singleton-comparison", &source);
    assert_eq!(found, recorded.lines().collect::<Vec<_>>());
}

#[test]
fn self_assigning_variable() {
    let source = "x = 1\nx = x\ny = x\n";
    assert_eq!(
        findings("ferrolint-plugin-self-assigning-variable", source),
        ["2: Assigning the same variable 'x' to itself"]
    );
}

#[test]
fn self_assigning_variable_pairs_a_tuple_target() {
    let source = "a = b = 1\na, b = a, 2\nc, d = 3, d\n";
    assert_eq!(
        findings("ferrolint-plugin-self-assigning-variable", source),
        [
            "2: Assigning the same variable 'a' to itself",
            "3: Assigning the same variable 'd' to itself"
        ]
    );
}

#[test]
fn self_assigning_variable_skips_a_class_body() {
    let source = "x = 1\nclass A:\n    x = x\n\n\ndef f(y):\n    y = y\n";
    assert_eq!(
        findings("ferrolint-plugin-self-assigning-variable", source),
        ["7: Assigning the same variable 'y' to itself"]
    );
}

#[test]
fn unused_import_leaves_a_name_an_earlier_project_star_import_binds() {
    let tokens = "alpha = 1\nbeta = 2\n";
    // `import os` comes before the star, so it owns the name; `beta` and
    // `alpha` come after it, so the star does, and Pylint reports them as `W0614`.
    let source =
        "import os\nfrom tokens import *\nimport beta\nfrom tokens import alpha\nimport gamma\n";
    assert_eq!(
        findings_beside("ferrolint-plugin-unused-import", tokens, source),
        [
            "1: Unused import os",
            "2: Unused import(s) beta and alpha from wildcard import of tokens",
            "5: Unused import gamma"
        ]
    );
}

#[test]
fn unused_import_reads_the_all_the_host_reads() {
    // The host resolves `m.__all__` and `list(...)`; `sorted(...)` is no list for astroid.
    let tokens = "__all__ = ['os']\n";
    let used =
        "import os\nimport sys\nimport tokens\n__all__ = list(tokens.__all__)\nprint(tokens)\n";
    assert_eq!(
        findings_beside("ferrolint-plugin-unused-import", tokens, used),
        ["2: Unused import sys"]
    );
    let sorted = "import os\n__all__ = sorted(['os'])\n";
    assert_eq!(
        findings_beside("ferrolint-plugin-unused-import", tokens, sorted),
        ["1: Unused import os"]
    );
}

#[test]
fn unused_import_takes_the_first_package_of_a_dotted_import_in_a_function_for_the_name() {
    // `import _pkg.sub` binds `_pkg`, a dummy name to Pylint in a function.
    let source = "def f():\n    import _pkg.sub\n    import pkg.sub\n";
    let found = findings("ferrolint-plugin-unused-import", source);
    assert_eq!(
        found
            .iter()
            .filter(|line| line.contains("Unused"))
            .collect::<Vec<_>>(),
        ["3: Unused import pkg.sub"]
    );
}

/// The unused-import component with every `@0.4.2` interface name spelled
/// with `version` instead, as a build against that contract names them;
/// the strings have one length, so the binary stays valid.
fn respelled_plugin(version: [u8; 6]) -> Vec<u8> {
    let mut bytes = support::plugin_bytes("ferrolint-plugin-unused-import");
    let (prefix, current) = (b"ferrolint:plugin/", b"@0.4.2");
    let mut rewritten = 0;
    for start in 0..bytes.len() {
        if bytes[start..].starts_with(prefix)
            && let Some(at) = bytes[start..]
                .iter()
                .take(48)
                .position(|&byte| byte == b'@')
            && bytes[start + at..].starts_with(current)
        {
            bytes[start + at..start + at + 6].copy_from_slice(&version);
            rewritten += 1;
        }
    }
    assert!(rewritten > 0);
    bytes
}

fn lint_with(bytes: &[u8]) -> support::Run {
    let project = Project::new();
    let sha = support::sha256(bytes);
    project
        .write("old.wasm", bytes)
        .write(
            "ferrolint.toml",
            format!("[[plugins]]\npath = \"old.wasm\"\nsha256 = \"{sha}\"\n"),
        )
        .write("case.py", "import os\n");
    project.run(&["--no-cache"])
}

#[test]
fn a_plugin_built_against_the_patch_before_is_loaded() {
    // 0.4.2 only adds a function, which a 0.4.1 build does not import.
    let run = lint_with(&respelled_plugin(*b"@0.4.1"));
    assert_eq!(run.code, 1, "{run:?}");
    assert!(run.stdout.contains("Unused import os"), "{run:?}");
}

#[test]
fn a_plugin_built_against_an_older_contract_is_refused() {
    // Before 1.0 a minor bump of the contract is incompatible, whatever it
    // adds, so 0.4.2 refuses a 0.3.x build as it does a 0.2.x one.
    for version in [*b"@0.2.1", *b"@0.3.1"] {
        let run = lint_with(&respelled_plugin(version));
        assert_eq!(run.code, 2, "{run:?}");
        assert!(run.stderr.contains("ferrolint:plugin"), "{run:?}");
    }
}
