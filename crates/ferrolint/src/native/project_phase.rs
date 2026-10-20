//! The rules that need the whole run: E0401 import-error, E0402
//! relative-beyond-top-level, E0611 no-name-in-module and R0401 cyclic-import.
//! Each one depends on files other than the linted one (the module an import
//! finds, the modules that import it), so they do not fit the per-file model
//! and its cache: the engine runs this phase after the per-file one and
//! caches its result as one entry, stamped with everything the resolver read.
//!
//! The resolver follows astroid's: see [`Project::spec`].
//!
//! shortcut: a name that a module binds is read statically, as astroid does,
//! but its brain plugins are not, so the modules they add names to (`BRAINS`
//! in `project.rs`) accept any name; a name that a module imports from a
//! module is not followed through to what it infers to; and without a
//! `VIRTUAL_ENV` the standard library and the installed packages are
//! unknown, so only relative imports can be judged missing.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrolint_core::{Diagnostic, LineIndex, Position, Range, Registry, Severity};
use rayon::prelude::*;
use regex::Regex;
use ruff_python_parser::ParseOptions;

use super::imports_scan::{self, Import};
use super::project::{Attr, Extensions, Project, Spec, relative_name};
use super::rule;
use crate::pragma;
use crate::suppress::{self, Span, Suppressions};

pub const IMPORT_ERROR: &str = "E0401";
pub const RELATIVE_BEYOND_TOP: &str = "E0402";
pub const NO_NAME: &str = "E0611";
pub const CYCLIC: &str = "R0401";

/// The rules of this phase; the per-file checkers do not own them.
pub fn rules() -> Vec<ferrolint_core::RuleInfo> {
    vec![
        rule(IMPORT_ERROR, "import-error", "Unable to import a module"),
        rule(
            RELATIVE_BEYOND_TOP,
            "relative-beyond-top-level",
            "Relative import goes beyond the top-level package",
        ),
        rule(NO_NAME, "no-name-in-module", "Name not found in a module"),
        rule(
            CYCLIC,
            "cyclic-import",
            "Modules import each other in a cycle",
        ),
    ]
}

/// What of the options this phase reads.
pub struct Opts {
    ignored: Vec<Regex>,
    analyse_fallback: bool,
    pub extensions: Extensions,
    pub source_roots: Vec<String>,
}

impl Opts {
    pub fn new(options: &[(String, String)]) -> Self {
        let get = |key: &str| {
            options
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        let csv = |key: &str| -> Vec<String> {
            get(key)
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let yes = |key: &str| {
            get(key).is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "y" | "yes" | "true" | "1"
                )
            })
        };
        let mut allowed = csv("extension-pkg-allow-list");
        allowed.extend(csv("extension-pkg-whitelist"));
        Self {
            ignored: csv("ignored-modules")
                .iter()
                .filter_map(|pattern| glob(pattern))
                .collect(),
            analyse_fallback: yes("analyse-fallback-blocks"),
            extensions: Extensions {
                allowed,
                all: yes("unsafe-load-any-extension"),
            },
            source_roots: csv("source-roots"),
        }
    }

    /// Pylint's `is_module_ignored`: a pattern of `ignored-modules` matches
    /// the name or any package above it.
    fn is_ignored(&self, name: &str) -> bool {
        let mut prefix = String::new();
        name.split('.').any(|part| {
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(part);
            self.ignored.iter().any(|pattern| pattern.is_match(&prefix))
        })
    }
}

/// A Python `fnmatch` pattern as a regular expression.
fn glob(pattern: &str) -> Option<Regex> {
    let mut out = String::from("(?s)^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '[' => {
                let mut set = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == ']' && !set.is_empty() && set != "!" {
                        closed = true;
                        break;
                    }
                    set.push(next);
                }
                if closed {
                    let set = set
                        .strip_prefix('!')
                        .map_or(set.clone(), |rest| format!("^{rest}"));
                    out.push('[');
                    out.push_str(&set.replace('\\', "\\\\"));
                    out.push(']');
                } else {
                    out.push_str("\\[");
                    out.push_str(&regex::escape(&set));
                }
            }
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    Regex::new(&out).ok()
}

/// A linted file, as the phase sees it.
pub struct Unit {
    pub path: PathBuf,
    pub display: String,
    /// The dotted name Pylint gives the module.
    pub module: String,
    pub is_package: bool,
}

/// What reading one file gives the phase.
pub struct Prepared {
    imports: Vec<Import>,
    spans: Vec<Span>,
}

/// Parse a file and read its imports; `None` when it does not parse, is not
/// UTF-8, or a pragma ends it (Pylint checks nothing in such a file).
pub fn prepare(path: &Path) -> Option<Prepared> {
    let bytes = std::fs::read(path).ok()?;
    let source = std::str::from_utf8(&bytes).ok()?;
    let options = ParseOptions::from(ruff_python_ast::PySourceType::Python)
        .with_target_version(ruff_python_ast::PythonVersion::latest());
    let parsed = ruff_python_parser::parse_unchecked(source, options)
        .try_into_module()
        .unwrap_or_else(|| unreachable!("Python source parses to a module"));
    if parsed.has_syntax_errors() {
        return None;
    }
    let index = LineIndex::new(source);
    let comments = suppress::comments(source, parsed.tokens());
    if pragma::informational(source, &comments, &index)
        .iter()
        .any(|d| d.code == "I0013")
    {
        return None;
    }
    Some(Prepared {
        imports: imports_scan::scan(&parsed.syntax().body, source, &index),
        spans: suppress::spelled(source, &parsed, &index),
    })
}

/// The files read in parallel.
pub fn prepare_all(units: &[Unit]) -> Vec<Option<Prepared>> {
    units.par_iter().map(|unit| prepare(&unit.path)).collect()
}

/// Everything the phase found: the diagnostics of each file, by position in
/// the units, and the import cycles.
#[derive(Default)]
pub struct Found {
    pub files: Vec<Vec<Diagnostic>>,
    pub cycles: Vec<String>,
}

fn line_start(line: u32) -> Range {
    let at = Position {
        offset: 0,
        line,
        column: 1,
    };
    Range { start: at, end: at }
}

fn diagnostic(code: &str, message: String, range: Range) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message,
        range,
        severity: rule(code, "", "").severity,
        fix: None,
    }
}

/// Run the phase over `units`. A `# pylint: disable=` comment drops the
/// diagnostic it names, and an import it covers for `cyclic-import` drops
/// its edge from the graph.
pub fn analyse(
    units: &[Unit],
    prepared: &[Option<Prepared>],
    project: &Project,
    known: bool,
    opts: &Opts,
    registry: &Registry,
) -> Found {
    let mut found = Found {
        files: vec![Vec::new(); units.len()],
        cycles: Vec::new(),
    };
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut excluded: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (at, (unit, prepared)) in units.iter().zip(prepared).enumerate() {
        let Some(prepared) = prepared else { continue };
        let file = File {
            unit,
            project,
            known,
            opts,
        };
        let suppressions = Suppressions::resolve(&prepared.spans, registry);
        let mut own = Vec::new();
        for import in &prepared.imports {
            file.import(import, &mut own);
            for edge in file.edges(import) {
                let line = import.range.start.line;
                let skip = import.type_checking
                    || suppressions.suppresses(&diagnostic(
                        CYCLIC,
                        String::new(),
                        line_start(line),
                    ));
                if !graph.contains_key(&unit.module) {
                    order.push(unit.module.clone());
                }
                graph
                    .entry(unit.module.clone())
                    .or_default()
                    .insert(edge.clone());
                if skip {
                    excluded
                        .entry(unit.module.clone())
                        .or_default()
                        .insert(edge);
                }
            }
        }
        own.retain(|d| !suppressions.suppresses(d));
        found.files[at] = own;
    }
    for (module, edges) in &mut graph {
        if let Some(drop) = excluded.get(module) {
            edges.retain(|edge| !drop.contains(edge));
        }
    }
    found.cycles = cycles(&graph, &order);
    found
}

/// Pylint's `get_cycles`, with a module's imports taken in name order.
fn cycles(graph: &BTreeMap<String, BTreeSet<String>>, vertices: &[String]) -> Vec<String> {
    fn walk(
        graph: &BTreeMap<String, BTreeSet<String>>,
        path: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
        result: &mut Vec<Vec<String>>,
        vertex: &str,
    ) {
        if let Some(at) = path.iter().position(|seen| seen == vertex) {
            let mut cycle: Vec<String> = path[at..].to_vec();
            let start = (0..cycle.len()).min_by_key(|&i| &cycle[i]).unwrap_or(0);
            cycle.rotate_left(start);
            if !result.contains(&cycle) {
                result.push(cycle);
            }
            return;
        }
        path.push(vertex.to_owned());
        if let Some(next) = graph.get(vertex) {
            for node in next {
                if !visited.contains(node) {
                    walk(graph, path, visited, result, node);
                    visited.insert(node.clone());
                }
            }
        }
        path.pop();
    }
    let mut result = Vec::new();
    for vertex in vertices {
        walk(
            graph,
            &mut Vec::new(),
            &mut Default::default(),
            &mut result,
            vertex,
        );
    }
    result.into_iter().map(|cycle| cycle.join(" -> ")).collect()
}

struct File<'a> {
    unit: &'a Unit,
    project: &'a Project,
    known: bool,
    opts: &'a Opts,
}

/// Why a module could not be imported.
enum Failure {
    /// The relative import climbs above the top.
    TooFar,
    Missing,
}

impl File<'_> {
    /// The module a `level`-dotted `name` stands for and where it is.
    fn find(&self, level: u32, name: &str) -> Result<(String, Rc<Spec>), Failure> {
        let absolute = if level == 0 {
            name.to_owned()
        } else {
            relative_name(
                &self.unit.module,
                &self.unit.path,
                self.unit.is_package,
                name,
                level,
            )
            .ok_or(Failure::TooFar)?
        };
        self.project
            .spec(&absolute)
            .map(|spec| (absolute, spec))
            .ok_or(Failure::Missing)
    }

    /// Whether a missing `name` goes unreported here.
    fn forgiven(&self, import: &Import, name: &str) -> bool {
        self.opts.is_ignored(name)
            || import.type_checking
            || import.sys_guard
            || import.ignores_import_error
    }

    fn import(&self, import: &Import, out: &mut Vec<Diagnostic>) {
        let report = |code: &str, message: String| diagnostic(code, message, import.range);
        if let Some((module, level)) = &import.from
            && *level > 0
            && module.is_empty()
            && relative_name(
                &self.unit.module,
                &self.unit.path,
                self.unit.is_package,
                "",
                *level,
            )
            .is_some_and(|name| name.is_empty())
        {
            // The directory is no package: astroid finds a namespace module
            // without a name, which has none of the names asked of it.
            if self.opts.analyse_fallback || !import.fallback {
                let missing = import.names.iter().filter(|name| *name != "*");
                out.extend(
                    missing.map(|name| report(NO_NAME, format!("No name '{name}' in module ''"))),
                );
            }
            return;
        }
        match &import.from {
            None => {
                for name in &import.names {
                    if self.find(0, name).is_err()
                        && self.known
                        && !self.forgiven(import, name)
                        && (self.opts.analyse_fallback || !import.fallback)
                    {
                        out.push(report(IMPORT_ERROR, format!("Unable to import '{name}'")));
                    }
                }
            }
            Some((module, level)) => match self.find(*level, module) {
                Ok(_) => {}
                Err(Failure::TooFar) => {
                    if !self.opts.is_ignored(module)
                        && !import.type_checking
                        && !import.sys_guard
                        && !import.ignores_import_error
                    {
                        out.push(report(
                            RELATIVE_BEYOND_TOP,
                            "Attempted relative import beyond top-level package".into(),
                        ));
                    }
                }
                Err(Failure::Missing) => {
                    // An absolute import can only be judged when the
                    // environment's libraries are known.
                    if (*level > 0 || self.known)
                        && !self.forgiven(import, module)
                        && (self.opts.analyse_fallback || !import.fallback)
                    {
                        let shown = relative_name(
                            &self.unit.module,
                            &self.unit.path,
                            self.unit.is_package,
                            module,
                            *level,
                        )
                        .unwrap_or_else(|| module.clone());
                        out.push(report(IMPORT_ERROR, format!("Unable to import '{shown}'")));
                    }
                }
            },
        }
        if (self.opts.analyse_fallback || !import.fallback)
            && !import.type_checking
            && !import.sys_guard
        {
            self.names(import, out);
        }
    }

    /// E0611: the names an import takes from a module that does not have them.
    fn names(&self, import: &Import, out: &mut Vec<Diagnostic>) {
        let mut check =
            |module: (String, Rc<Spec>), path: &[&str]| self.attrs(import, module, path, out);
        match &import.from {
            None => {
                for name in &import.names {
                    let parts: Vec<&str> = name.split('.').collect();
                    if let Ok(module) = self.find(0, parts[0]) {
                        check(module, &parts[1..]);
                    }
                }
            }
            Some((module, level)) => {
                let parts: Vec<&str> = module.split('.').collect();
                let Ok(first) = self.find(*level, parts[0]) else {
                    return;
                };
                let Some(module) = check(first, &parts[1..]) else {
                    return;
                };
                for name in import.names.iter().filter(|name| *name != "*") {
                    check(module.clone(), &[name.as_str()]);
                }
            }
        }
    }

    /// Pylint's `_check_module_attrs`: follow `path` from `module`, reporting
    /// the first name it lacks; the module it ends in, if it is one.
    fn attrs(
        &self,
        import: &Import,
        module: (String, Rc<Spec>),
        path: &[&str],
        out: &mut Vec<Diagnostic>,
    ) -> Option<(String, Rc<Spec>)> {
        let (mut name, mut spec) = module;
        for (at, part) in path.iter().enumerate() {
            if *part == "__dict__" {
                if at + 1 < path.len() {
                    let rest = path[at + 1..].join(".");
                    out.push(diagnostic(
                        NO_NAME,
                        format!("No name '{rest}' in module '__dict__'"),
                        import.range,
                    ));
                }
                return None;
            }
            match self.project.attr(&name, &spec, part) {
                Attr::Found => return None,
                Attr::Module(next) => {
                    name = format!("{name}.{part}");
                    spec = next;
                }
                Attr::Missing => {
                    if !self.opts.is_ignored(&format!("{name}.{part}")) {
                        out.push(diagnostic(
                            NO_NAME,
                            format!("No name '{part}' in module '{name}'"),
                            import.range,
                        ));
                    }
                    return None;
                }
            }
        }
        Some((name, spec))
    }

    /// The modules of the project this import depends on, as Pylint records
    /// them (`_add_imported_module`).
    fn edges(&self, import: &Import) -> Vec<String> {
        let (level, imported): (u32, Vec<String>) = match &import.from {
            None => (
                0,
                import
                    .names
                    .iter()
                    .filter(|name| self.find(0, name).is_ok())
                    .cloned()
                    .collect(),
            ),
            Some((module, level)) => match self.find(*level, module) {
                Ok((absolute, _)) => (
                    *level,
                    import
                        .names
                        .iter()
                        .map(|name| {
                            if name == "*" {
                                absolute.clone()
                            } else {
                                format!("{absolute}.{name}")
                            }
                        })
                        .collect(),
                ),
                Err(_) => return Vec::new(),
            },
        };
        let context = (level > 0)
            .then(|| self.unit.path.parent().map(Path::to_path_buf))
            .flatten();
        imported
            .iter()
            .map(|name| self.module_part(name, context.as_deref()))
            .filter(|part| *part != self.unit.module && !self.project.is_stdlib(part))
            .collect()
    }

    /// astroid's `get_module_part`: the longest leading part of a dotted
    /// name that is a module (the rest names something inside it).
    fn module_part(&self, dotted: &str, context: Option<&Path>) -> String {
        if dotted.starts_with("os.path") {
            return "os.path".into();
        }
        let parts: Vec<&str> = dotted.split('.').collect();
        let dirs: Vec<PathBuf> = context.map(Path::to_path_buf).into_iter().collect();
        for at in 0..parts.len() {
            let name = parts[..=at].join(".");
            let exists = if dirs.is_empty() {
                self.project.spec(&name).is_some()
            } else {
                self.project.exists_from(&dirs, &name)
            };
            if !exists {
                // Too early a miss raises, and the name stands as it is.
                return if at < 1.max(parts.len().saturating_sub(2)) {
                    dotted.to_owned()
                } else {
                    parts[..at].join(".")
                };
            }
        }
        dotted.to_owned()
    }
}

/// The root of the modules that an argument names: the directory above its
/// top package, or the source root that holds it.
pub fn package_root(argument: &Path, source_roots: &[PathBuf]) -> PathBuf {
    let real = std::fs::canonicalize(argument).unwrap_or_else(|_| argument.to_path_buf());
    let dir = if real.is_dir() {
        real
    } else {
        real.parent().map(Path::to_path_buf).unwrap_or(real)
    };
    for root in source_roots {
        if dir.starts_with(root) || root.starts_with(&dir) {
            return root.clone();
        }
    }
    let mut dir = dir;
    while dir.join("__init__.py").exists() {
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
    dir
}

/// A file's module name relative to `bases`, the first that holds it and
/// whose directories down to the file are packages (or all are accepted).
pub fn module_name(path: &Path, bases: &[PathBuf], namespace: bool) -> Option<String> {
    for base in bases {
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let mut parts: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let file = parts.pop()?;
        let mut dir = base.clone();
        let packages = parts.iter().all(|part| {
            dir.push(part);
            dir.join("__init__.py").exists()
        });
        if !packages && !namespace {
            continue;
        }
        let stem = file.strip_suffix(".py").unwrap_or(&file).to_owned();
        if stem != "__init__" {
            parts.push(stem);
        }
        return Some(parts.join("."));
    }
    None
}

/// The position every project-wide message sits at: the start of a file.
pub fn file_start() -> Range {
    let at = Position {
        offset: 0,
        line: 1,
        column: 1,
    };
    Range { start: at, end: at }
}

/// R0401's diagnostic.
pub fn cycle_diagnostic(cycle: &str) -> Diagnostic {
    Diagnostic {
        code: CYCLIC.into(),
        message: format!("Cyclic import ({cycle})"),
        range: file_start(),
        severity: Severity::Info,
        fix: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::Selection;

    fn write(root: &Path, files: &[(&str, &str)]) {
        for (name, text) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    }

    /// Lint every `.py` of `root` as one run (names relative to `root`), with
    /// a fake standard library and `options`.
    fn run(root: &Path, options: &[(&str, &str)]) -> Vec<(String, u32, String, String)> {
        let _ = Selection::new;
        let mut paths: Vec<PathBuf> = walk(root);
        paths.sort();
        let units: Vec<Unit> = paths
            .iter()
            .map(|path| Unit {
                path: path.clone(),
                display: path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                module: module_name(path, &[root.to_path_buf()], true).unwrap(),
                is_package: path.file_name().is_some_and(|n| n == "__init__.py"),
            })
            .collect();
        let options: Vec<(String, String)> = options
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let opts = Opts::new(&options);
        let linted = units
            .iter()
            .map(|u| (u.module.clone(), (u.path.clone(), u.is_package)))
            .collect();
        let library = root.join("stdlib");
        let project = Project::for_run(
            vec![root.to_path_buf()],
            linted,
            Extensions::default(),
            Some((library, Vec::new())),
        );
        let prepared = prepare_all(&units);
        let registry = Registry::with_owners(super::rules(), &BTreeMap::new()).unwrap();
        let found = analyse(&units, &prepared, &project, true, &opts, &registry);
        let mut out = Vec::new();
        for (unit, diagnostics) in units.iter().zip(found.files) {
            for d in diagnostics {
                out.push((unit.display.clone(), d.range.start.line, d.code, d.message));
            }
        }
        for cycle in found.cycles {
            out.push((String::new(), 1, "R0401".into(), cycle));
        }
        out
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else if path.extension().is_some_and(|e| e == "py")
                && !path.starts_with(dir.join("stdlib"))
            {
                out.push(path);
            }
        }
        out
    }

    fn codes(found: &[(String, u32, String, String)]) -> Vec<String> {
        found
            .iter()
            .map(|f| format!("{}:{}:{}", f.0, f.1, f.2))
            .collect()
    }

    #[test]
    fn missing_modules_names_and_relative_imports_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &[
                ("stdlib/os.py", "sep = '/'\n"),
                ("pkg/__init__.py", ""),
                (
                    "pkg/a.py",
                    "import os\nimport nothing\nfrom pkg import b, c\nfrom os import sep, nope\nfrom ... import far\nfrom . import b\n",
                ),
                ("pkg/b.py", "VALUE = 1\n"),
            ],
        );
        let found = run(dir.path(), &[]);
        assert_eq!(
            found
                .iter()
                .map(|f| (f.1, f.2.as_str(), f.3.as_str()))
                .collect::<Vec<_>>(),
            [
                (2, "E0401", "Unable to import 'nothing'"),
                (3, "E0611", "No name 'c' in module 'pkg'"),
                (4, "E0611", "No name 'nope' in module 'os'"),
                (
                    5,
                    "E0402",
                    "Attempted relative import beyond top-level package"
                ),
            ]
        );
    }

    #[test]
    fn guards_and_fallbacks_silence_a_missing_import() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &[
                ("stdlib/sys.py", ""),
                ("stdlib/typing.py", "TYPE_CHECKING = False\n"),
                (
                    "a.py",
                    "import sys\nfrom typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    import m1\nif sys.version_info > (3,):\n    import m2\ntry:\n    import m3\nexcept ImportError:\n    m3 = None\ntry:\n    import m4\nexcept ValueError:\n    pass\nimport m5\n",
                ),
            ],
        );
        assert_eq!(
            codes(&run(dir.path(), &[])),
            ["a.py:12:E0401", "a.py:15:E0401"]
        );
        assert_eq!(
            codes(&run(dir.path(), &[("ignored-modules", "m4,m*")])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn cycles_are_found_unless_the_edge_is_a_type_checking_import() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &[
                ("pkg/__init__.py", ""),
                ("pkg/a.py", "from pkg import b\n"),
                ("pkg/b.py", "from pkg import a\n"),
                (
                    "pkg/c.py",
                    "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    from pkg import d\n",
                ),
                ("pkg/d.py", "from pkg import c\n"),
            ],
        );
        let found = run(dir.path(), &[]);
        assert_eq!(
            found
                .iter()
                .filter(|f| f.2 == "R0401")
                .map(|f| f.3.as_str())
                .collect::<Vec<_>>(),
            ["pkg.a -> pkg.b"]
        );
    }
}
