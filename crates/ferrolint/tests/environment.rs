//! Rules that depend on the environment `VIRTUAL_ENV` names: a base that
//! comes from a module the environment lacks is left out of the ancestors, as
//! Pylint's astroid leaves it out.

#![allow(clippy::unwrap_used)]

mod support;

use support::Project;

const SOURCE: &str = "\
from typing import Generic, TypeVar

from missing import Gone

T = TypeVar(\"T\")


class Box(Generic[T]):
    def __init__(self):
        self.shared = 1


class Named(Gone):
    def run(self):
        self.a = 1


class Drops(Box[Gone]):
    def run(self):
        self.shared = 2


class Keeps(Box[T]):
    def run(self):
        self.shared = 2
";

/// A virtualenv whose standard library holds only `os.py`.
fn venv() -> Project {
    let venv = Project::new();
    venv.write("base/lib/python3.14/os.py", "")
        .write("env/pyvenv.cfg", {
            let home = venv.join("base/bin");
            format!("home = {}\nversion = 3.14.0\n", home.display())
        });
    venv
}

fn lines(project: &Project, venv: Option<&std::path::Path>) -> Vec<String> {
    let run = project.run_in(&["--no-cache"], venv);
    run.finding_lines()
        .into_iter()
        .filter(|line| line.contains("W0201"))
        .map(|line| line.split_whitespace().next().unwrap().to_owned())
        .collect()
}

#[test]
fn a_base_from_a_missing_module_is_dropped_where_the_environment_is_known() {
    let project = Project::new();
    project
        .write("ferrolint.toml", "select = [\"W0201\"]\n")
        .write("m.py", SOURCE);
    // Without an environment the module may exist, so the class is skipped.
    assert_eq!(lines(&project, None), Vec::<String>::new());
    let venv = venv();
    assert_eq!(
        lines(&project, Some(&venv.join("env"))),
        ["m.py:15:9:", "m.py:20:9:"]
    );
}
