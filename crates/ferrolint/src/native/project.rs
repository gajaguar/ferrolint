//! The classes of the other modules of the project, found by their dotted
//! name, for the rules that ask what an ancestor of a class defines. Pylint
//! infers a base class across modules; here the module is searched for in
//! the directories above the linted file (the `sys.path` of a project laid
//! out like a Django project's) and parsed on demand.
//!
//! shortcut: only modules that are files under those directories are read,
//! so the standard library and the installed packages stay unknown but for
//! [`Project::with_packages`] (a base from them ends the chain), and a C
//! module never is, except that `from m import *` knows its public names
//! from a table of one Python version (`make c-module-names`), and a `m.py`
//! with a compiled `m.*.so` beside it is not read, as astroid takes the
//! compiled one and finds it empty; a name is
//! followed through `import`, `from` and `from m import *` statements of the
//! top level only; and a result that depends on another module is cached
//! with the file, so it is stale until the file changes or the cache is
//! cleared. Upgrade with the contract's module index.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Comprehension, ExceptHandler, Expr, ExprContext, ModModule, Operator, Pattern, PySourceType,
    Stmt, StmtClassDef, StmtImportFrom,
};
use ruff_python_parser::{ParseOptions, Parsed};

use ferrolint_core::LineIndex;

use super::classes::{bases, blocks, locals};
use super::flow::{no_return, own_returns};
use super::project_phase::module_name;
use crate::c_module_names;
use crate::class_methods;
use crate::project_run::stamp_of;

pub struct Project {
    /// The directories modules are searched in, nearest first.
    roots: Vec<PathBuf>,
    /// The package of the linted file, for its relative imports.
    package: String,
    modules: RefCell<HashMap<String, Option<Rc<Module>>>>,
    /// Search the way astroid does for a whole run: the first package or
    /// module that a name finds ends the search, and a directory is a
    /// namespace package. Otherwise the nearest root that holds the whole
    /// module wins (see [`Project::new`]).
    strict: bool,
    /// The modules the run lints, by dotted name: astroid has them all built
    /// before it resolves an import, so they are found without a search.
    linted: HashMap<String, (PathBuf, bool)>,
    /// What each directory holds.
    listings: RefCell<HashMap<PathBuf, Rc<Listing>>>,
    specs: RefCell<HashMap<String, Option<Rc<Spec>>>>,
    /// Every name a module binds, private ones too.
    names: RefCell<HashMap<String, Option<Rc<HashSet<String>>>>>,
    /// The directories listed and the files read, for a cache to stamp.
    touched: RefCell<BTreeSet<PathBuf>>,
    stdlib: Option<PathBuf>,
    extensions: Extensions,
}

/// Which compiled modules outside the standard library astroid loads, and
/// so knows the names of: the ones `extension-pkg-allow-list` names, or all.
#[derive(Clone, Default)]
pub struct Extensions {
    pub allowed: Vec<String>,
    pub all: bool,
}

/// What a directory holds, by name (`true` for a directory).
#[derive(Default)]
struct Listing {
    entries: HashMap<String, bool>,
    /// The stems of the compiled modules (`m` for `m.cpython-314-darwin.so`).
    compiled: HashSet<String>,
    /// Each name by its lower case, where the file system ignores case (macOS,
    /// Windows): astroid tests `isfile`, which opens `ttFont.py` as `TTFont`.
    folded: HashMap<String, String>,
}

impl Listing {
    /// Whether `name` is a directory (`Some(true)`) or a file (`Some(false)`) here.
    fn entry(&self, name: &str) -> Option<bool> {
        let found = self.entries.get(name);
        found
            .or_else(|| self.entries.get(self.folded.get(&name.to_lowercase())?))
            .copied()
    }

    /// Whether a compiled module is called `stem` (by case where the file system ignores it).
    fn has_compiled(&self, stem: &str) -> bool {
        self.compiled.contains(stem)
            || (!self.folded.is_empty()
                && self.compiled.iter().any(|c| c.eq_ignore_ascii_case(stem)))
    }
}

/// A module found on the search path.
pub struct Spec {
    pub kind: Kind,
    /// Where a package's submodules are looked for.
    dirs: Vec<PathBuf>,
}

impl Spec {
    fn of(kind: Kind) -> Self {
        Self {
            kind,
            dirs: Vec::new(),
        }
    }

    fn source(file: PathBuf, package: bool) -> Self {
        let dirs = if package {
            file.parent().map(Path::to_path_buf).into_iter().collect()
        } else {
            Vec::new()
        };
        Self {
            kind: Kind::Source { file, package },
            dirs,
        }
    }
}

pub enum Kind {
    /// A `.py` file; `package` when it is an `__init__.py`.
    Source { file: PathBuf, package: bool },
    /// A compiled module outside the standard library, which astroid finds
    /// empty unless it loads it.
    Extension,
    /// A directory with no `__init__.py`.
    Namespace,
    /// A module of the standard library with no source, and its public names.
    Builtin(&'static [&'static str]),
    /// A module astroid makes up when the import fails (`six.moves`).
    Made,
}

/// What looking a name up in a module finds.
pub enum Attr {
    /// A name the module defines or astroid gives every module: the check
    /// stops there.
    Found,
    /// A submodule of the package.
    Module(Rc<Spec>),
    Missing,
}

pub struct Module {
    name: String,
    parsed: Parsed<ModModule>,
    /// The package that relative imports of the module start from.
    package: String,
    /// What each name the top level imports stands for, once per import.
    imports: HashMap<String, Vec<String>>,
    /// The modules that `from m import *` reads, in order.
    stars: Vec<String>,
    lines: LineIndex,
    /// Whether the file is a `.pyi` stub.
    stub: bool,
}

/// A class of a module of the project.
#[derive(Clone)]
pub struct ClassRef {
    module: Rc<Module>,
    name: String,
}

impl Project {
    pub fn new(path: &Path, module: &str, run: &[PathBuf]) -> Self {
        let is_package = path.file_stem().is_some_and(|stem| stem == "__init__");
        // Pylint names the module from the run's root, so `.x` is found there
        // even when a directory above has no `__init__.py`.
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let named = module_name(&real, run, true);
        let module = named.as_deref().unwrap_or(module);
        let package = if is_package {
            module.to_owned()
        } else {
            module
                .rsplit_once('.')
                .map_or(String::new(), |(p, _)| p.to_owned())
        };
        // Astroid looks in the importing file's directory, then on `sys.path`:
        // the run's roots. A package directory is no root there, so its
        // `collections.py` does not hide the standard library's.
        let mut roots: Vec<PathBuf> = path
            .parent()
            .filter(|dir| !dir.join("__init__.py").is_file())
            .map(Path::to_path_buf)
            .into_iter()
            .collect();
        roots.extend(
            run.iter()
                .filter(|root| !roots.contains(root))
                .cloned()
                .collect::<Vec<_>>(),
        );
        Self::with(roots, package, false)
    }

    /// The test root: the directory above the top package of `module`.
    #[cfg(test)]
    pub(crate) fn test_roots(path: &Path, module: &str) -> Vec<PathBuf> {
        let init = path.file_stem().is_some_and(|stem| stem == "__init__");
        let depth = module.split('.').count() + usize::from(init);
        path.ancestors()
            .nth(depth)
            .map(Path::to_path_buf)
            .into_iter()
            .collect()
    }

    fn with(roots: Vec<PathBuf>, package: String, strict: bool) -> Self {
        Self {
            roots,
            package,
            modules: RefCell::default(),
            strict,
            linted: HashMap::new(),
            listings: RefCell::default(),
            specs: RefCell::default(),
            names: RefCell::default(),
            touched: RefCell::default(),
            stdlib: None,
            extensions: Extensions::default(),
        }
    }

    /// The project of a whole run: `sys.path` as Pylint builds it, the
    /// `roots` of the linted files, then `PYTHONPATH`, the standard library
    /// and the installed packages (`environment`, see [`environment`]).
    pub fn for_run(
        mut roots: Vec<PathBuf>,
        linted: HashMap<String, (PathBuf, bool)>,
        extensions: Extensions,
        environment: Option<(PathBuf, Vec<PathBuf>)>,
    ) -> Self {
        roots.extend(
            std::env::var_os("PYTHONPATH")
                .iter()
                .flat_map(std::env::split_paths)
                .filter(|dir| !dir.as_os_str().is_empty()),
        );
        let mut stdlib = None;
        if let Some((library, packages)) = environment {
            roots.push(library.clone());
            stdlib = Some(library);
            for dir in packages {
                roots.push(dir.clone());
                roots.extend(path_files(&dir));
            }
        }
        let mut project = Self::with(roots, String::new(), true);
        project.linted = linted;
        project.stdlib = stdlib;
        project.extensions = extensions;
        project
    }

    /// As [`Project::new`], with the standard library and the installed
    /// packages of the environment that `VIRTUAL_ENV` names searched last,
    /// so a base class from either is read too.
    pub fn with_packages(path: &Path, module: &str, run: &[PathBuf]) -> Self {
        let mut project = Self::new(path, module, run);
        if let Some((library, packages)) = environment() {
            project.stdlib = Some(library.clone());
            project.roots.push(library);
            project.roots.extend(packages);
        }
        project
    }

    /// Whether the environment is known and holds no module `name`, so that
    /// astroid cannot import it either.
    pub fn is_missing(&self, name: &str) -> bool {
        // A bare `.` names the package itself.
        self.stdlib.is_some()
            && !name.trim_start_matches('.').is_empty()
            && self.spec(&self.absolute(name)).is_none()
    }

    /// The modules that `from m import *` names at the top level of `body`.
    pub fn stars(&self, body: &[Stmt]) -> Vec<String> {
        let mut stars = Vec::new();
        collect_imports(body, &self.package, &mut HashMap::new(), &mut stars);
        stars
    }

    /// `origin` of the linted file's imports with a relative module made
    /// absolute.
    pub fn absolute(&self, origin: &str) -> String {
        let dots = origin.chars().take_while(|c| *c == '.').count();
        if dots == 0 {
            return origin.to_owned();
        }
        let mut base = self.package.clone();
        for _ in 1..dots {
            base = base
                .rsplit_once('.')
                .map_or(String::new(), |(p, _)| p.to_owned());
        }
        let rest = &origin[dots..];
        if base.is_empty() {
            rest.to_owned()
        } else {
            format!("{base}.{rest}")
        }
    }

    fn load(&self, name: &str) -> Option<Rc<Module>> {
        if let Some(found) = self.modules.borrow().get(name) {
            return found.clone();
        }
        let module = self.read(name);
        self.modules
            .borrow_mut()
            .insert(name.to_owned(), module.clone());
        module
    }

    /// Whether a module or package called `name` is under the roots.
    pub fn has_module(&self, name: &str) -> bool {
        self.locate(name).is_some()
    }

    fn locate(&self, name: &str) -> Option<(PathBuf, bool)> {
        if self.strict {
            return match &self.spec(name)?.kind {
                Kind::Source { file, package } => Some((file.clone(), *package)),
                _ => None,
            };
        }
        let relative: PathBuf = name.split('.').collect();
        // The root is the nearest directory that holds the module itself: an
        // inner package may share the top package's name (`app/api` and `api`).
        self.roots.iter().find_map(|root| {
            let file = root.join(&relative).with_extension("py");
            let init = root.join(&relative).join("__init__.py");
            if file.is_file() && exact_case(&file) {
                Some((file, false))
            } else {
                (init.is_file() && init.parent().is_some_and(exact_case)).then_some((init, true))
            }
        })
    }

    fn read(&self, name: &str) -> Option<Rc<Module>> {
        let (file, is_package) = self.locate(name)?;
        // astroid prefers the compiled `m.*.so` to `m.py` and cannot read it
        if !self.strict && !is_package && has_extension(&file) {
            return None;
        }
        self.touched.borrow_mut().insert(file.clone());
        read_module(&file, name, is_package)
    }

    /// What `module.name` stands for when the module binds `name` by `name = X`, also under `if`
    /// and `try`, and `X` is a name it imports or a builtin: `no_limit_int = int` is `int`.
    // shortcut: only a name or an attribute of an import on the right; upgrade with inference.
    pub fn unalias(&self, qualified: &str) -> Vec<String> {
        fn walk(body: &[Stmt], name: &str, loaded: &Module, out: &mut Vec<String>) {
            for stmt in body {
                match stmt {
                    Stmt::Assign(a) if matches!(&a.targets[..], [Expr::Name(t)] if t.id.as_str() == name) => {
                        if let Expr::Name(value) = &*a.value {
                            let imported = loaded.imports.get(value.id.as_str());
                            out.extend(
                                imported.map_or_else(|| vec![value.id.to_string()], Clone::clone),
                            );
                        } else if let Expr::Attribute(_) = &*a.value {
                            out.extend(loaded.qualify(&a.value));
                        }
                    }
                    other => blocks(other)
                        .into_iter()
                        .for_each(|b| walk(b, name, loaded, out)),
                }
            }
        }
        let mut out = Vec::new();
        if let Some((module, name)) = qualified.rsplit_once('.')
            && let Some(loaded) = self.load(module)
        {
            walk(&loaded.parsed.syntax().body, name, &loaded, &mut out);
        }
        out
    }

    /// The class `qualified` names (`pkg.module.Class`), also when `pkg.module`
    /// only imports it.
    pub fn class(&self, qualified: &str) -> Option<ClassRef> {
        self.class_at(qualified, 0)
    }

    fn class_at(&self, qualified: &str, depth: u8) -> Option<ClassRef> {
        let (module, name) = qualified.rsplit_once('.')?;
        // `collections.abc` has no file of its own: the interpreter aliases it.
        let module = if module == "collections.abc" {
            "_collections_abc"
        } else {
            module
        };
        let loaded = self.load(module)?;
        if find_class(&loaded.parsed.syntax().body, name).is_some() {
            return Some(ClassRef {
                module: loaded,
                name: name.to_owned(),
            });
        }
        if depth >= 8 {
            return None;
        }
        if let Some(origin) = loaded.imports.get(name).and_then(|o| o.last()) {
            return self.class_at(origin, depth + 1);
        }
        if let Some(target) = alias(&loaded, module, name) {
            return self.class_at(&target, depth + 1);
        }
        loaded
            .stars
            .iter()
            .find_map(|star| self.class_at(&format!("{star}.{name}"), depth + 1))
    }
}

impl Project {
    /// Whether the function `qualified` (`pkg.module.func`) is annotated
    /// `NoReturn` or `Never`, and then whether it is `async`.
    ///
    /// shortcut: the function must be defined in the module the name is given
    /// in, and the standard library is not read; upgrade with import resolution.
    pub fn never_returns(&self, qualified: &str) -> Option<bool> {
        /// The answers of the process, by search path and name, each with the
        /// stamp of the file it came from: the files of a package share them,
        /// and each would else parse the same module.
        type Answers = Mutex<HashMap<(Vec<PathBuf>, String), (String, Option<bool>)>>;
        static ANSWERS: LazyLock<Answers> = LazyLock::new(Mutex::default);
        let (module, name) = qualified.rsplit_once('.')?;
        if self.is_stdlib(module) {
            return None;
        }
        // The path is in the stamp: a package may replace the module file.
        let stamp = self.locate(module).map_or_else(String::new, |(file, _)| {
            format!("{}:{}", file.display(), stamp_of(&file))
        });
        let key = (self.roots.clone(), qualified.to_owned());
        if let Some((known, answer)) = ANSWERS.lock().ok()?.get(&key)
            && *known == stamp
        {
            return *answer;
        }
        let answer = self.load(module).and_then(|loaded| {
            loaded
                .parsed
                .syntax()
                .body
                .iter()
                .find_map(|stmt| match stmt {
                    Stmt::FunctionDef(f) if f.name.as_str() == name && no_return(f) => {
                        Some(f.is_async)
                    }
                    _ => None,
                })
        });
        ANSWERS.lock().ok()?.insert(key, (stamp, answer));
        answer
    }

    /// Whether every `return` of the function `qualified` names
    /// (`pkg.module.func`) gives a class of the project that the function
    /// imports or the module imports.
    ///
    /// shortcut: the function must be defined in the module the name is
    /// given in, not re-exported; upgrade with the contract's module index.
    pub fn returns_class(&self, qualified: &str) -> bool {
        let Some((module, name)) = qualified.rsplit_once('.') else {
            return false;
        };
        let Some(loaded) = self.load(module) else {
            return false;
        };
        let Some(func) = loaded
            .parsed
            .syntax()
            .body
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::FunctionDef(f) if f.name.as_str() == name => Some(f),
                _ => None,
            })
        else {
            return false;
        };
        let mut imports = loaded.imports.clone();
        collect_imports(&func.body, &loaded.package, &mut imports, &mut Vec::new());
        let returns = own_returns(&func.body);
        !returns.is_empty()
            && returns.iter().all(|r| {
                matches!(r.value.as_deref(), Some(Expr::Name(n))
                    if imports.get(n.id.as_str()).and_then(|q| q.last()).is_some_and(|q| self.class(q).is_some()))
            })
    }
}

/// The modules whose names astroid's brain plugins add to, so a name that
/// their source lacks may still be there.
const BRAINS: &[&str] = &[
    "ctypes",
    "crypt",
    "curses",
    "decimal",
    "datetime",
    "dateutil.parser",
    "hashlib",
    "mechanize",
    "http",
    "http.client",
    "multiprocessing",
    "pytest",
    "py.test",
    "pkg_resources",
    "PyQt4.QtCore",
    "re",
    "regex",
    "scipy.signal",
    "responses",
    "signal",
    "six",
    "six.moves",
    "sqlalchemy.orm.session",
    "subprocess",
    "ssl",
    "threading",
    "unittest",
    "typing",
    "numpy",
];

/// The attributes astroid gives every module, and its `__path__` when it is a package.
const SPECIAL: &[&str] = &[
    "__new__",
    "__init__",
    "__ne__",
    "__name__",
    "__doc__",
    "__file__",
    "__dict__",
    "__package__",
    "__spec__",
    "__loader__",
    "__cached__",
    "__builtins__",
];

impl Project {
    /// What the directory `dir` holds; a stale listing is what the cache stamps catch.
    fn listing(&self, dir: &Path) -> Rc<Listing> {
        if let Some(found) = self.listings.borrow().get(dir) {
            return found.clone();
        }
        self.touched.borrow_mut().insert(dir.to_path_buf());
        let mut listing = Listing::default();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.path().is_dir();
            if !is_dir
                && Path::new(&name)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("so") || e.eq_ignore_ascii_case("pyd"))
            {
                listing
                    .compiled
                    .insert(name.split('.').next().unwrap_or_default().to_owned());
            }
            listing.entries.insert(name, is_dir);
        }
        let probe = listing.entries.keys().find(|n| n.to_uppercase() != **n);
        if probe.is_some_and(|n| {
            let upper = n.to_uppercase();
            !listing.entries.contains_key(&upper) && dir.join(upper).exists()
        }) {
            listing.folded = listing
                .entries
                .keys()
                .map(|n| (n.to_lowercase(), n.clone()))
                .collect();
        }
        let listing = Rc::new(listing);
        self.listings
            .borrow_mut()
            .insert(dir.to_path_buf(), listing.clone());
        listing
    }

    /// The module or package a dotted `name` is on the search path, the way
    /// astroid finds it: each part is searched in the places the part before
    /// it leaves, and in each place a package beats a compiled module, which
    /// beats a `.py` file, which beats a namespace directory.
    pub fn spec(&self, name: &str) -> Option<Rc<Spec>> {
        if let Some(found) = self.specs.borrow().get(name) {
            return found.clone();
        }
        let found = self.find(name).map(Rc::new);
        self.specs
            .borrow_mut()
            .insert(name.to_owned(), found.clone());
        found
    }

    fn find(&self, name: &str) -> Option<Spec> {
        if let Some((file, package)) = self.linted.get(name) {
            return Some(Spec::source(file.clone(), *package));
        }
        // The running interpreter's `__main__` is in `sys.modules`, so astroid always finds it.
        if name == "__main__" {
            return Some(Spec::of(Kind::Made));
        }
        // astroid answers for `os.path` with the platform's module.
        if name == "os.path" {
            return self.find("posixpath");
        }
        let Some((parent, last)) = name.rsplit_once('.') else {
            return self.search(&self.roots, name, name);
        };
        let parent = self.spec(parent);
        let found = parent
            .as_deref()
            .filter(|parent| {
                matches!(
                    parent.kind,
                    Kind::Source { package: true, .. } | Kind::Namespace
                )
            })
            .and_then(|parent| self.search(&parent.dirs, last, name));
        // The module astroid makes up when a `six.moves` import fails.
        found
            .or_else(|| {
                // Python 3.14 has no `collections/abc.py`: importlib finds
                // the frozen `_collections_abc` under that name.
                (name == "collections.abc")
                    .then(|| self.find("_collections_abc"))
                    .flatten()
            })
            .or_else(|| {
                (name == "six.moves" || name.starts_with("six.moves."))
                    .then(|| Spec::of(Kind::Made))
            })
    }

    fn search(&self, dirs: &[PathBuf], part: &str, name: &str) -> Option<Spec> {
        let mut namespaces = Vec::new();
        for dir in dirs {
            let listing = self.listing(dir);
            let package = dir.join(part);
            if listing.entry(part) == Some(true) {
                // astroid's finder takes a package with only `__init__.pyi` for a source package.
                let inner = self.listing(&package);
                for init in ["__init__.py", "__init__.pyi"] {
                    if inner.entry(init) == Some(false) {
                        return Some(Spec::source(package.join(init), true));
                    }
                }
                if !listing.has_compiled(part)
                    && listing.entry(&format!("{part}.py")) != Some(false)
                {
                    namespaces.push(package);
                    continue;
                }
            }
            if listing.has_compiled(part) {
                return Some(Spec::of(Kind::Extension));
            }
            if listing.entry(&format!("{part}.py")) == Some(false) {
                return Some(Spec::source(dir.join(format!("{part}.py")), false));
            }
        }
        if !namespaces.is_empty() {
            return Some(Spec {
                kind: Kind::Namespace,
                dirs: namespaces,
            });
        }
        c_module_names::names(name).map(|names| Spec::of(Kind::Builtin(names)))
    }

    /// Whether astroid loads the compiled module `name` and so knows its names.
    fn loads_extension(&self, name: &str) -> bool {
        self.extensions.all
            || self.extensions.allowed.iter().any(|allowed| {
                name == allowed
                    || name
                        .strip_prefix(allowed.as_str())
                        .is_some_and(|rest| rest.starts_with('.'))
            })
    }

    /// Every name the source module `name` binds, private ones too.
    fn names_of(&self, name: &str) -> Option<Rc<HashSet<String>>> {
        if let Some(found) = self.names.borrow().get(name) {
            return found.clone();
        }
        let found = self.load(name).map(|loaded| {
            let mut names = Vec::new();
            self.bind_all(&loaded.package, &loaded.parsed.syntax().body, &mut names);
            Rc::new(names.into_iter().collect())
        });
        self.names
            .borrow_mut()
            .insert(name.to_owned(), found.clone());
        found
    }

    /// What looking `attr` up in the module `name` (found as `spec`) finds,
    /// as Pylint's `_check_module_attrs` does.
    pub fn attr(&self, name: &str, spec: &Spec, attr: &str) -> Attr {
        let package = matches!(spec.kind, Kind::Source { package: true, .. });
        if SPECIAL.contains(&attr)
            || (attr == "__path__" && (package || matches!(spec.kind, Kind::Namespace)))
        {
            return Attr::Found;
        }
        let brain = BRAINS.contains(&name) || name.starts_with("numpy.");
        let defined = brain
            || match &spec.kind {
                // A file that does not parse binds names that cannot be told.
                Kind::Source { .. } => self
                    .names_of(name)
                    .is_none_or(|names| names.contains(attr) || names.contains("*")),
                Kind::Builtin(names) => attr.starts_with('_') || names.contains(&attr),
                Kind::Extension => self.loads_extension(name),
                Kind::Made => true,
                Kind::Namespace => false,
            };
        if defined {
            return Attr::Found;
        }
        // A package's submodule is found by `getattr` last.
        let submodule = (package || matches!(spec.kind, Kind::Namespace))
            .then(|| self.spec(&format!("{name}.{attr}")))
            .flatten();
        submodule.map_or(Attr::Missing, Attr::Module)
    }

    /// Whether `name` is a module of the standard library (as its top-level
    /// name is one of the interpreter's own), when the library is known.
    pub fn is_stdlib(&self, name: &str) -> bool {
        let top = name.split('.').next().unwrap_or_default();
        c_module_names::names(top).is_some()
            || self.stdlib.as_ref().is_some_and(|dir| {
                let listing = self.listing(dir);
                listing.entries.contains_key(top)
                    || listing.entries.contains_key(&format!("{top}.py"))
            })
    }

    /// Whether the module `name` is found searching the `dirs` first (as
    /// `file_from_modpath` does with a context file), then the search path.
    pub fn exists_from(&self, dirs: &[PathBuf], name: &str) -> bool {
        let mut found = None;
        let mut places = dirs.to_vec();
        let mut path = String::new();
        for part in name.split('.') {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(part);
            let Some(spec) = self.search(&places, part, &path) else {
                found = None;
                break;
            };
            places.clone_from(&spec.dirs);
            found = Some(spec);
        }
        found.is_some() || self.on_disk(name)
    }

    /// Whether the module `name` is a file or directory of the search path:
    /// not one that astroid makes up (`six.moves`), which `file_from_modpath` cannot find,
    /// nor a linted `.pyi` stub, which it does not look for (an edge goes to the package).
    pub fn on_disk(&self, name: &str) -> bool {
        self.spec(name).is_some_and(|spec| match &spec.kind {
            Kind::Made => false,
            Kind::Source { file, .. } => file.extension().is_none_or(|ext| ext != "pyi"),
            _ => true,
        })
    }

    /// The directories listed and the files read so far, for a cache to stamp.
    pub fn touched(&self) -> Vec<PathBuf> {
        self.touched.borrow().iter().cloned().collect()
    }
}

/// The module `import_name` names in the module `module` (the file `file`,
/// a package when `is_package`) when `level` dots start it, as astroid's
/// `relative_to_absolute_name` computes it, quirks included; `None` when it
/// climbs above the top.
pub fn relative_name(
    module: &str,
    file: &Path,
    is_package: bool,
    import_name: &str,
    level: u32,
) -> Option<String> {
    // `module.rsplit(".", n)[0]`
    let cut = |n: u32| {
        let mut name = module;
        for _ in 0..n {
            match name.rfind('.') {
                Some(at) => name = &name[..at],
                None => break,
            }
        }
        name.to_owned()
    };
    let package = if level > 0 {
        let mut level = level;
        let package = if is_package {
            level -= 1;
            cut(level)
        } else if file.parent().is_some_and(|dir| {
            !dir.join("__init__.py").exists()
                && dir
                    .join(import_name.split('.').next().unwrap_or_default())
                    .exists()
        }) {
            level -= 1;
            String::new()
        } else {
            cut(level)
        };
        if level > 0 && u32::try_from(module.matches('.').count()).is_ok_and(|dots| dots < level) {
            return None;
        }
        package
    } else if is_package {
        module.to_owned()
    } else {
        cut(1)
    };
    Some(match (package.is_empty(), import_name.is_empty()) {
        (true, _) => import_name.to_owned(),
        (false, true) => package,
        (false, false) => format!("{package}.{import_name}"),
    })
}

impl Project {
    /// The names `from module import *` binds, in astroid's order: the public
    /// names the module binds at its top level (also under compound
    /// statements), then those of its `from` imports and of the modules it
    /// star-imports, in statement order. `None` when the module is not
    /// found. Pylint ignores `__all__` here, so this does too.
    pub fn public_names(&self, module: &str) -> Option<Vec<String>> {
        let mut names = Vec::new();
        self.public_at(module, 0, &mut names)?;
        Some(names)
    }

    /// The names the first binding of `__all__` in `body` lists, as Pylint reads them to take
    /// them for used: a list or tuple of strings, `+` of two of the same kind, `*seq` inside
    /// one, and `m.__all__` of a module the project reads. `None` when a binding is there that
    /// this cannot read (a call, a name): the caller stays silent.
    pub fn listed_in_all(&self, body: &[Stmt]) -> Option<Vec<String>> {
        if matches!(first_all(body), None | Some(AllBinding::Other)) {
            return Some(Vec::new());
        }
        let mut imports = HashMap::new();
        collect_imports(body, &self.package, &mut imports, &mut Vec::new());
        Some(self.all_in(body, &imports, 0)?.1)
    }

    /// The names `__all__` lists in `body`, and whether it is a tuple.
    fn all_in(
        &self,
        body: &[Stmt],
        imports: &HashMap<String, Vec<String>>,
        depth: u8,
    ) -> Option<(bool, Vec<String>)> {
        match first_all(body)? {
            AllBinding::Value(value) => self.sequence(value, imports, depth),
            AllBinding::Imported => self.exported(imports.get("__all__")?.last()?, depth),
            AllBinding::Other => None,
        }
    }

    /// The `__all__` of the module `qualified` names (`pkg.m.__all__`).
    fn exported(&self, qualified: &str, depth: u8) -> Option<(bool, Vec<String>)> {
        let module = qualified.strip_suffix(".__all__")?;
        let loaded = self.load(module).filter(|_| depth < 8)?;
        self.all_in(&loaded.parsed.syntax().body, &loaded.imports, depth + 1)
    }

    fn sequence(
        &self,
        expr: &Expr,
        imports: &HashMap<String, Vec<String>>,
        depth: u8,
    ) -> Option<(bool, Vec<String>)> {
        let (tuple, elts) = match expr {
            Expr::List(l) => (false, &l.elts),
            Expr::Tuple(t) => (true, &t.elts),
            Expr::BinOp(b) if b.op == Operator::Add => {
                let (tuple, mut names) = self.sequence(&b.left, imports, depth)?;
                let (other, rest) = self.sequence(&b.right, imports, depth)?;
                names.extend(rest);
                return (tuple == other).then_some((tuple, names));
            }
            Expr::Attribute(a) if a.attr.as_str() == "__all__" => {
                return self.exported(&qualify(imports, expr)?, depth);
            }
            _ => return None,
        };
        let mut names = Vec::new();
        for elt in elts {
            match elt {
                Expr::Starred(s) => names.extend(self.sequence(&s.value, imports, depth)?.1),
                _ => names.push(elt.as_string_literal_expr()?.value.to_str().to_owned()),
            }
        }
        Some((tuple, names))
    }

    /// Whether the top level of `module` binds `__all__` (also under `if`
    /// and `try`); `false` when the module is not found.
    pub fn defines_all(&self, module: &str) -> bool {
        self.load(module)
            .is_some_and(|loaded| binds_all(&loaded.parsed.syntax().body))
    }

    /// The names a file with this `body` binds, in astroid's order, as
    /// [`Project::public_names`] lists them for a module.
    pub fn bound_names(&self, body: &[Stmt]) -> Vec<String> {
        let mut names = Vec::new();
        self.bind_at(&self.package, body, 0, &mut names);
        names
    }

    fn public_at(&self, module: &str, depth: u8, out: &mut Vec<String>) -> Option<()> {
        let Some(loaded) = self.load(module) else {
            // A C or builtin module has no source: its table is Python's own `dir`.
            c_module_names::names(module)?
                .iter()
                .for_each(|n| add_public(out, n));
            return Some(());
        };
        self.bind_at(&loaded.package, &loaded.parsed.syntax().body, depth, out);
        Some(())
    }

    fn bind_at(&self, package: &str, body: &[Stmt], depth: u8, out: &mut Vec<String>) {
        self.bind_in(package, body, depth, false, out);
    }

    /// As [`Project::bound_names`], with the private names too.
    fn bind_all(&self, package: &str, body: &[Stmt], out: &mut Vec<String>) {
        self.bind_in(package, body, 0, true, out);
    }

    fn bind_in(&self, package: &str, body: &[Stmt], depth: u8, all: bool, out: &mut Vec<String>) {
        if depth >= 8 {
            return;
        }
        let mut late = Vec::new();
        bound(package, body, all, &mut late, out);
        for item in late {
            match item {
                Late::Name(name) => add_name(out, &name, all),
                Late::Star(origin) => {
                    // A source that does not parse brings names that cannot be told: `*` stands for them.
                    let told = self.public_at(&origin, depth + 1, out).is_some();
                    if !told
                        && self
                            .spec(&origin)
                            .is_some_and(|s| matches!(s.kind, Kind::Source { .. }))
                    {
                        out.push("*".into());
                    }
                }
            }
        }
    }
}

/// What astroid binds after everything else a module binds: the names of
/// its `from` imports and what each star import brings, in statement order.
enum Late {
    Name(String),
    Star(String),
}

/// The public names `body` binds at its top level, also under compound
/// statements, in `out`, and the `from` imports astroid binds last in `late`.
fn bound(package: &str, body: &[Stmt], all: bool, late: &mut Vec<Late>, out: &mut Vec<String>) {
    let mut binder = Binder {
        package,
        all,
        globals: None,
        late,
        out,
    };
    binder.visit_body(body);
}

struct Binder<'o> {
    package: &'o str,
    /// Bind the private names too.
    all: bool,
    /// `None` at the module's scope; in a function or a class, the names it
    /// declares `global`, the only ones that reach the module.
    globals: Option<Vec<String>>,
    late: &'o mut Vec<Late>,
    out: &'o mut Vec<String>,
}

impl Binder<'_> {
    fn binds(&self, name: &str) -> bool {
        self.globals
            .as_ref()
            .is_none_or(|globals| globals.iter().any(|global| global == name))
    }

    fn bind(&mut self, name: &str) {
        if self.binds(name) {
            add_name(self.out, name, self.all);
        }
    }

    /// Reads `body` as a new scope, which declares no `global` yet.
    fn scope(&mut self, body: &[Stmt]) {
        let saved = self.globals.replace(Vec::new());
        self.visit_body(body);
        self.globals = saved;
    }
}

impl<'a> Visitor<'a> for Binder<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // A function binds its `global` names while its body is read, before its own name.
            Stmt::FunctionDef(f) => {
                self.visit_parameters(&f.parameters);
                self.scope(&f.body);
                if self.globals.is_none() {
                    add_name(self.out, f.name.as_str(), self.all);
                }
            }
            Stmt::ClassDef(c) if self.globals.is_none() => {
                self.scope(&c.body);
                add_name(self.out, c.name.as_str(), self.all);
            }
            Stmt::ClassDef(c) => self.visit_body(&c.body),
            Stmt::Global(g) => {
                if let Some(globals) = &mut self.globals {
                    globals.extend(g.names.iter().map(ToString::to_string));
                }
            }
            // astroid reads the targets before the value.
            Stmt::Assign(a) => {
                a.targets.iter().for_each(|t| self.visit_expr(t));
                self.visit_expr(&a.value);
            }
            Stmt::For(f) => {
                self.visit_expr(&f.target);
                self.visit_expr(&f.iter);
                self.visit_body(&f.body);
                self.visit_body(&f.orelse);
            }
            Stmt::Import(i) => i.names.iter().for_each(|alias| match &alias.asname {
                Some(asname) => self.bind(asname.as_str()),
                None => self.bind(alias.name.split('.').next().unwrap_or_default()),
            }),
            // `__future__` names are flags, not names of the module.
            Stmt::ImportFrom(i)
                if i.level == 0 && i.module.as_ref().is_some_and(|m| m == "__future__") => {}
            Stmt::ImportFrom(i) => {
                for alias in &i.names {
                    let name = alias.asname.as_ref().unwrap_or(&alias.name);
                    if &alias.name != "*" {
                        if self.binds(name) {
                            self.late.push(Late::Name(name.to_string()));
                        }
                    } else if self.globals.is_none() {
                        self.late.push(Late::Star(from_prefix(i, self.package)));
                    }
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(_) => {}
            // A store (a walrus' too) or a `del` of a name.
            Expr::Name(n) if n.ctx != ExprContext::Load => self.bind(n.id.as_str()),
            _ => visitor::walk_expr(self, expr),
        }
    }

    // The target of a comprehension is its own.
    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        self.visit_expr(&comprehension.iter);
        for test in &comprehension.ifs {
            self.visit_expr(test);
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(h) = handler;
        if let Some(type_) = &h.type_ {
            self.visit_expr(type_);
        }
        if let Some(name) = &h.name {
            self.bind(name.as_str());
        }
        self.visit_body(&h.body);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        visitor::walk_pattern(self, pattern);
        let name = match pattern {
            Pattern::MatchAs(p) => &p.name,
            Pattern::MatchStar(p) => &p.name,
            Pattern::MatchMapping(p) => &p.rest,
            _ => return,
        };
        if let Some(name) = name {
            self.bind(name.as_str());
        }
    }
}

/// What binds `__all__` first at the top level of a module, also under `if` and `try`.
enum AllBinding<'a> {
    Value(&'a Expr),
    /// `from m import __all__`.
    Imported,
    /// A binding that tells no names (`__all__ += x`).
    Other,
}

fn first_all(body: &[Stmt]) -> Option<AllBinding<'_>> {
    let is_all = |target: &Expr| matches!(target, Expr::Name(n) if n.id.as_str() == "__all__");
    body.iter().find_map(|stmt| match stmt {
        Stmt::Assign(a) if a.targets.iter().any(is_all) => Some(AllBinding::Value(&a.value)),
        Stmt::AnnAssign(a) if is_all(&a.target) => Some(
            a.value
                .as_deref()
                .map_or(AllBinding::Other, AllBinding::Value),
        ),
        Stmt::AugAssign(a) if is_all(&a.target) => Some(AllBinding::Other),
        Stmt::ImportFrom(i)
            if i.names
                .iter()
                .any(|a| a.asname.as_ref().unwrap_or(&a.name).as_str() == "__all__") =>
        {
            Some(AllBinding::Imported)
        }
        other => blocks(other).into_iter().find_map(first_all),
    })
}

/// The dotted name `expr` (a name or its attributes) stands for through `imports`.
fn qualify(imports: &HashMap<String, Vec<String>>, expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => imports.get(name.id.as_str())?.last().cloned(),
        Expr::Attribute(a) => Some(format!("{}.{}", qualify(imports, &a.value)?, a.attr)),
        _ => None,
    }
}

/// Whether the top level of a module binds `__all__`, also under `if` and `try`.
fn binds_all(body: &[Stmt]) -> bool {
    let is_all = |target: &Expr| matches!(target, Expr::Name(n) if n.id.as_str() == "__all__");
    body.iter().any(|stmt| match stmt {
        Stmt::Assign(a) => a.targets.iter().any(is_all),
        Stmt::AnnAssign(a) => is_all(&a.target),
        Stmt::AugAssign(a) => is_all(&a.target),
        Stmt::If(s) => binds_all(&s.body) || s.elif_else_clauses.iter().any(|c| binds_all(&c.body)),
        Stmt::Try(t) => {
            binds_all(&t.body)
                || t.handlers
                    .iter()
                    .any(|ExceptHandler::ExceptHandler(h)| binds_all(&h.body))
        }
        _ => false,
    })
}

fn add_public(out: &mut Vec<String>, name: &str) {
    add_name(out, name, false);
}

fn add_name(out: &mut Vec<String>, name: &str, all: bool) {
    if (all || !name.starts_with('_')) && !out.iter().any(|seen| seen == name) {
        out.push(name.to_owned());
    }
}

/// The absolute module a `from` statement of a module in `package` reads.
fn from_prefix(import: &StmtImportFrom, package: &str) -> String {
    let mut base = package.to_owned();
    for _ in 1..import.level {
        base = base
            .rsplit_once('.')
            .map_or(String::new(), |(p, _)| p.to_owned());
    }
    let module = import.module.as_ref().map(ToString::to_string);
    match (import.level, module) {
        (0, Some(module)) => module,
        (_, Some(module)) if base.is_empty() => module,
        (_, Some(module)) => format!("{base}.{module}"),
        (_, None) => base,
    }
}

impl ClassRef {
    /// The dotted name of the module that defines the class.
    pub fn module(&self) -> &str {
        &self.module.name
    }

    /// The 1-based line of `offset` in the class's module.
    pub fn line(&self, offset: u32) -> u32 {
        self.module.lines.line(offset)
    }

    /// The qualified name of the class where it is defined.
    pub fn qname(&self) -> String {
        format!("{}.{}", self.module.name, self.name)
    }

    /// Whether the class is written in a `.pyi` stub.
    pub fn is_stub(&self) -> bool {
        self.module.stub
    }

    pub fn class(&self) -> Option<&StmtClassDef> {
        find_class(&self.module.parsed.syntax().body, &self.name)
    }

    /// Whether the module binds the class's name only once, as astroid needs
    /// to infer a single class from it.
    pub fn is_sole(&self) -> bool {
        let bound = locals(&self.module.parsed.syntax().body);
        bound.iter().filter(|(name, _)| *name == self.name).count() == 1
    }

    /// The dotted name `expr` stands for in the class's module.
    pub fn qualify(&self, expr: &Expr) -> Option<String> {
        self.module.qualify(expr)
    }

    /// The classes that this one derives from and the project holds, and the
    /// qualified names of the rest.
    pub fn bases(&self, project: &Project) -> Vec<Result<ClassRef, String>> {
        self.bases_by(project, false)
    }

    /// Like [`Self::bases`], but a base imported in several branches
    /// (`try: from a import B` / `except: from b import B`) stands for each import, as Pylint infers it.
    pub fn bases_all(&self, project: &Project) -> Vec<Result<ClassRef, String>> {
        self.bases_by(project, true)
    }

    fn bases_by(&self, project: &Project, all: bool) -> Vec<Result<ClassRef, String>> {
        let Some(class) = self.class() else {
            return Vec::new();
        };
        bases(class)
            .iter()
            .flat_map(|base| {
                // `Base[T]` derives from `Base`.
                let (base, subscripted) = match base {
                    Expr::Subscript(s) => (&*s.value, true),
                    other => (other, false),
                };
                let found = self.resolve(project, base).or_else(|| {
                    // A builtin class no import names.
                    let Expr::Name(name) = base else {
                        return None;
                    };
                    class_methods::methods(&name.id).map(|_| Err(name.id.to_string()))
                });
                // astroid infers `typing.X[...]` as an empty class.
                let Some(found) = found else {
                    return Vec::new();
                };
                if subscripted && found.as_ref().is_ok_and(Self::is_typing) {
                    return Vec::new();
                }
                match (&found, base) {
                    (Err(_), Expr::Name(name)) if all => {
                        match self.module.imports.get(name.id.as_str()) {
                            Some(origins) => origins
                                .iter()
                                .map(|q| project.class(q).ok_or_else(|| q.clone()))
                                .collect(),
                            None => vec![found],
                        }
                    }
                    _ => vec![found],
                }
            })
            .collect()
    }

    /// Whether the class is one of the `typing` module, `Generic` apart.
    pub fn is_typing(&self) -> bool {
        self.module.name == "typing" && self.name != "Generic"
    }

    /// The class `expr` names in the class's module: one the module defines,
    /// one it imports, or one a `from m import *` binds; otherwise the
    /// qualified name, or `None` when `expr` has none.
    pub fn resolve(&self, project: &Project, expr: &Expr) -> Option<Result<ClassRef, String>> {
        if let Expr::Name(name) = expr
            && find_class(&self.module.parsed.syntax().body, name.id.as_str()).is_some()
        {
            return Some(Ok(Self {
                module: self.module.clone(),
                name: name.id.to_string(),
            }));
        }
        let Some(qualified) = self.module.qualify(expr) else {
            // A name that `from module import *` binds, or `Alias = Class`.
            let Expr::Name(name) = expr else {
                return None;
            };
            return project
                .class(&format!("{}.{}", self.module.name, name.id))
                .map(Ok);
        };
        Some(project.class(&qualified).ok_or(qualified))
    }
}

impl Module {
    fn qualify(&self, expr: &Expr) -> Option<String> {
        match expr {
            // `object` is the one builtin a base names without importing it.
            Expr::Name(name) => self
                .imports
                .get(name.id.as_str())
                .and_then(|origins| origins.last())
                .cloned()
                .or_else(|| (name.id.as_str() == "object").then(|| "object".to_owned())),
            Expr::Attribute(attribute) => Some(format!(
                "{}.{}",
                self.qualify(&attribute.value)?,
                attribute.attr
            )),
            _ => None,
        }
    }
}

/// The qualified `Y` of the last `name = Y` of the top level, where `Y` is a name of the module
/// or an attribute of a module it imports (`name = decoder.Decoder`), that may be a class; also
/// `name = _alias(Y, n)`.
///
/// shortcut: a name bound in the module itself, not under `if` or `try`;
/// upgrade with inference.
fn alias(loaded: &Module, module: &str, name: &str) -> Option<String> {
    loaded.parsed.syntax().body.iter().rev().find_map(|stmt| match stmt {
        Stmt::Assign(a)
            if matches!(&a.targets[..], [Expr::Name(target)] if target.id.as_str() == name) =>
        {
            // astroid reads `X = _alias(Y, n)`, as typing writes it, as a class `X` that derives from `Y`.
            let value = match &*a.value {
                Expr::Call(call)
                    if matches!(&*call.func, Expr::Name(f) if matches!(f.id.as_str(), "_alias" | "_DeprecatedGenericAlias"))
                        && call.arguments.args.len() == 2 =>
                {
                    &call.arguments.args[0]
                }
                other => other,
            };
            match value {
                Expr::Name(value) => Some(format!("{module}.{}", value.id)),
                Expr::Attribute(_) => loaded.qualify(value),
                _ => None,
            }
        }
        _ => None,
    })
}

/// The last class `name` of the top level, also under `if` and `try`.
fn find_class<'m>(body: &'m [Stmt], name: &str) -> Option<&'m StmtClassDef> {
    let mut found = None;
    for stmt in body {
        match stmt {
            Stmt::ClassDef(class) if class.name.as_str() == name => found = Some(class),
            Stmt::If(s) => {
                let nested =
                    std::iter::once(&s.body).chain(s.elif_else_clauses.iter().map(|c| &c.body));
                found = nested.filter_map(|b| find_class(b, name)).last().or(found);
            }
            Stmt::Try(t) => {
                let handlers = t.handlers.iter().map(|h| {
                    let ExceptHandler::ExceptHandler(h) = h;
                    &h.body
                });
                let nested = std::iter::once(&t.body)
                    .chain(handlers)
                    .chain([&t.orelse, &t.finalbody]);
                found = nested.filter_map(|b| find_class(b, name)).last().or(found);
            }
            _ => {}
        }
    }
    found
}

/// Parsed modules by file and name, once per worker thread: the next file
/// linted on the thread reads the same modules again. An entry is valid while
/// the file keeps its size and date.
type Kept = HashMap<(PathBuf, String), (String, Option<Rc<Module>>)>;

thread_local! {
    static PARSED: RefCell<Kept> = RefCell::new(HashMap::new());
}

/// Most modules a thread keeps; past it the thread starts over.
const KEPT: usize = 512;

fn read_module(file: &Path, name: &str, is_package: bool) -> Option<Rc<Module>> {
    let stamp = stamp_of(file);
    let key = (file.to_path_buf(), name.to_owned());
    let hit = PARSED.with(|kept| {
        kept.borrow()
            .get(&key)
            .filter(|(known, _)| *known == stamp)
            .map(|(_, module)| module.clone())
    });
    if let Some(module) = hit {
        return module;
    }
    let module = parse_module(file, name, is_package).map(Rc::new);
    PARSED.with(|kept| {
        let mut kept = kept.borrow_mut();
        if kept.len() >= KEPT {
            kept.clear();
        }
        kept.insert(key, (stamp, module.clone()));
    });
    module
}

fn parse_module(file: &Path, name: &str, is_package: bool) -> Option<Module> {
    let source = std::fs::read_to_string(file).ok()?;
    let parsed = crate::checks::parse(&source, ParseOptions::from(PySourceType::Python));
    if !parsed.errors().is_empty() {
        return None;
    }
    let package = if is_package {
        name.to_owned()
    } else {
        name.rsplit_once('.')
            .map_or(String::new(), |(p, _)| p.to_owned())
    };
    let (mut imports, mut stars) = (HashMap::new(), Vec::new());
    collect_imports(&parsed.syntax().body, &package, &mut imports, &mut stars);
    Some(Module {
        name: name.to_owned(),
        lines: LineIndex::new(&source),
        parsed,
        package,
        imports,
        stars,
        stub: file.extension().is_some_and(|ext| ext == "pyi"),
    })
}

/// Whether the last component of `path` is spelled as its directory lists
/// it: macOS and Windows would open `iterTools.py` as `itertools.py`.
fn exact_case(path: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    std::fs::read_dir(dir).is_ok_and(|mut entries| {
        entries.any(|entry| entry.is_ok_and(|entry| entry.file_name() == name))
    })
}

/// Whether a compiled module (`m.cpython-314-darwin.so`, `m.pyd`) sits next
/// to the source file `m.py`.
fn has_extension(file: &Path) -> bool {
    let (Some(dir), Some(stem)) = (file.parent(), file.file_stem().and_then(|s| s.to_str())) else {
        return false;
    };
    std::fs::read_dir(dir).is_ok_and(|mut entries| {
        entries.any(|entry| {
            entry.is_ok_and(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.strip_prefix(stem).is_some_and(|rest| {
                    rest.starts_with('.')
                        && Path::new(rest).extension().is_some_and(|e| {
                            e.eq_ignore_ascii_case("so") || e.eq_ignore_ascii_case("pyd")
                        })
                })
            })
        })
    })
}

/// The standard library and the `site-packages` directories of the
/// environment that `VIRTUAL_ENV` names.
pub fn environment() -> Option<(PathBuf, Vec<PathBuf>)> {
    let env = std::env::var_os("VIRTUAL_ENV").map(PathBuf::from)?;
    let library = stdlib(&env)?;
    // `lib/pythonX.Y/site-packages` on Unix, `Lib/site-packages` on Windows.
    let unix = std::fs::read_dir(env.join("lib"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path().join("site-packages"));
    let packages = unix
        .chain([env.join("Lib").join("site-packages")])
        .filter(|dir| dir.is_dir())
        .collect();
    Some((library, packages))
}

/// The directories the `.pth` files of a `site-packages` directory add to
/// the search path, in file order (`site.addpackage`).
pub fn path_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|e| e == "pth")
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty() && !line.starts_with('#') && !line.starts_with("import")
                })
                .map(|line| dir.join(line))
                .collect::<Vec<_>>()
        })
        .filter(|path| path.is_dir())
        .collect()
}

/// The standard library of the interpreter that made the environment
/// `env`, read from its `pyvenv.cfg`.
fn stdlib(env: &Path) -> Option<PathBuf> {
    let config = std::fs::read_to_string(env.join("pyvenv.cfg")).ok()?;
    let home = config.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "base-prefix" || key.trim() == "home").then(|| value.trim().to_owned())
    })?;
    let home = PathBuf::from(home);
    // `home` names the `bin` directory; `base-prefix` the one above it.
    let prefix = if home.ends_with("bin") {
        home.parent()?.to_path_buf()
    } else {
        home
    };
    let version = config.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "version" || key.trim() == "version_info").then(|| value.trim().to_owned())
    })?;
    let minor: Vec<&str> = version.split('.').take(2).collect();
    [
        prefix
            .join("lib")
            .join(format!("python{}", minor.join("."))),
        prefix.join("Lib"),
    ]
    .into_iter()
    .find(|dir| dir.join("os.py").is_file())
}

fn collect_imports(
    body: &[Stmt],
    package: &str,
    out: &mut HashMap<String, Vec<String>>,
    stars: &mut Vec<String>,
) {
    for stmt in body {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let first = alias.name.split('.').next().unwrap_or_default();
                    let (local, origin) = match &alias.asname {
                        Some(asname) => (asname.to_string(), alias.name.to_string()),
                        None => (first.to_owned(), first.to_owned()),
                    };
                    out.entry(local).or_default().push(origin);
                }
            }
            Stmt::ImportFrom(import) => {
                let prefix = from_prefix(import, package);
                if import.names.iter().any(|alias| &alias.name == "*") {
                    stars.push(prefix.clone());
                }
                for alias in import.names.iter().filter(|alias| &alias.name != "*") {
                    let local = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                    let origin = if prefix.is_empty() {
                        alias.name.to_string()
                    } else {
                        format!("{prefix}.{}", alias.name)
                    };
                    out.entry(local).or_default().push(origin);
                }
            }
            Stmt::If(s) => {
                collect_imports(&s.body, package, out, stars);
                s.elif_else_clauses
                    .iter()
                    .for_each(|c| collect_imports(&c.body, package, out, stars));
            }
            Stmt::Try(t) => {
                collect_imports(&t.body, package, out, stars);
                for handler in &t.handlers {
                    let ExceptHandler::ExceptHandler(h) = handler;
                    collect_imports(&h.body, package, out, stars);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Project;

    #[test]
    fn a_function_annotated_no_return_is_found_again_from_another_file() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "lib/stop.py",
            "def die() -> NoReturn:\n    raise SystemExit\n\n\ndef ok():\n    pass\n",
        );
        write("lib/a.py", "");
        write("lib/b.py", "");
        for file in ["a", "b"] {
            let project = Project::new(
                &root.path().join(format!("lib/{file}.py")),
                "lib.x",
                &[root.path().to_path_buf()],
            );
            assert_eq!(project.never_returns("lib.stop.die"), Some(false));
            assert_eq!(project.never_returns("lib.stop.ok"), None);
        }
    }

    #[test]
    fn an_edited_or_new_module_gives_a_fresh_answer() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("edit")).unwrap();
        std::fs::write(root.path().join("edit/a.py"), "").unwrap();
        let ask = || {
            Project::new(
                &root.path().join("edit/a.py"),
                "edit.x",
                &[root.path().to_path_buf()],
            )
            .never_returns("edit.stop.die")
        };
        assert_eq!(ask(), None);
        let stop = root.path().join("edit/stop.py");
        std::fs::write(&stop, "def die() -> NoReturn:\n    raise SystemExit\n").unwrap();
        assert_eq!(ask(), Some(false));
        std::fs::write(&stop, "async def die() -> Never:\n    raise SystemExit\n").unwrap();
        assert_eq!(ask(), Some(true));
        std::fs::write(&stop, "def die():\n    raise SystemExit\n").unwrap();
        assert_eq!(ask(), None);
    }

    #[test]
    fn an_inner_package_named_like_the_top_one_does_not_hide_it() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("app/api/__init__.py", "");
        write("app/views.py", "");
        write("api/base.py", "class Base:\n    pass\n");
        let project = Project::new(
            &root.path().join("app/views.py"),
            "app.views",
            &[root.path().to_path_buf()],
        );
        let class = project.class("api.base.Base").unwrap();
        assert_eq!(class.qname(), "api.base.Base");
    }

    #[test]
    fn a_module_of_the_package_does_not_hide_one_of_the_same_name() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("app/__init__.py", "");
        write("app/views.py", "");
        write("app/base.py", "class Local:\n    pass\n");
        write("base.py", "class Top:\n    pass\n");
        let project = Project::new(
            &root.path().join("app/views.py"),
            "app.views",
            &[root.path().to_path_buf()],
        );
        assert!(project.class("base.Top").is_some());
        assert!(project.class("base.Local").is_none());
    }

    #[test]
    fn a_base_bound_by_a_star_import_is_found() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("app/__init__.py", "");
        write("app/views.py", "");
        write("app/base.py", "class Base:\n    pass\n");
        write(
            "app/child.py",
            "from .base import *\n\nclass Child(Base):\n    pass\n",
        );
        let project = Project::new(
            &root.path().join("app/views.py"),
            "app.views",
            &[root.path().to_path_buf()],
        );
        let bases = project.class("app.child.Child").unwrap().bases(&project);
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].as_ref().unwrap().qname(), "app.base.Base");
    }

    #[test]
    fn a_base_bound_by_an_alias_of_its_module_is_found() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("app/__init__.py", "");
        write("app/views.py", "");
        write(
            "app/child.py",
            "class Base:\n    pass\n\nAlias = Base\n\nclass Child(Alias):\n    pass\n",
        );
        let project = Project::new(
            &root.path().join("app/views.py"),
            "app.views",
            &[root.path().to_path_buf()],
        );
        let bases = project.class("app.child.Child").unwrap().bases(&project);
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].as_ref().unwrap().qname(), "app.child.Base");
    }

    #[test]
    fn a_module_is_missing_only_where_the_environment_is_known() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("here.py"), "").unwrap();
        let mut project = Project::new(
            &root.path().join("mod.py"),
            "mod",
            &[root.path().to_path_buf()],
        );
        assert!(!project.is_missing("absent"));
        project.stdlib = Some(root.path().to_path_buf());
        assert!(project.is_missing("absent"));
        assert!(!project.is_missing("here"));
        assert!(!project.is_missing("."));
    }

    #[test]
    fn public_names_follow_stars_and_skip_private_ones() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("app/__init__.py", "");
        write("app/views.py", "");
        write(
            "app/m.py",
            "import os.path\nfrom .n import *\n__all__ = ['a']\na, _b = 1, 2\n\ndef f(): ...\n\nclass K: ...\n\nif a:\n    c: int = 1\n",
        );
        write("app/n.py", "from .m import *\nz = 1\n_p = 2\n");
        let project = Project::new(
            &root.path().join("app/views.py"),
            "app.views",
            &[root.path().to_path_buf()],
        );
        assert_eq!(
            project.public_names("app.m").unwrap(),
            ["os", "a", "f", "K", "c", "z"]
        );
        assert!(project.public_names("app.missing").is_none());
    }

    #[test]
    fn a_compiled_module_beside_the_source_hides_it() {
        let root = tempfile::tempdir().unwrap();
        for file in [
            "app/m.py",
            "app/m.cpython-314-darwin.so",
            "app/n.py",
            "app/o.py",
        ] {
            let path = root.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "a = 1\n").unwrap();
        }
        let project = Project::new(
            &root.path().join("app/views.py"),
            "app.views",
            &[root.path().to_path_buf()],
        );
        assert!(project.has_module("app.m") && project.public_names("app.m").is_none());
        assert!(project.public_names("app.n").is_some());
        // `o.py` is not `m.py`'s sibling by prefix alone
        assert!(project.public_names("app.o").is_some());
    }

    #[test]
    fn a_module_is_found_by_the_case_it_is_spelled_in() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("misc/Inner")).unwrap();
        std::fs::write(root.path().join("misc/Inner/__init__.py"), "").unwrap();
        std::fs::write(root.path().join("misc/iterTools.py"), "").unwrap();
        let project = Project::new(
            &root.path().join("misc/user.py"),
            "misc.user",
            &[root.path().to_path_buf()],
        );
        assert!(project.has_module("iterTools") && project.has_module("Inner"));
        assert!(!project.has_module("itertools") && !project.has_module("inner"));
    }
}
