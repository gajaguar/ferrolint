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
//! from a table of one Python version (`make c-module-names`); a name is
//! followed through `import`, `from` and `from m import *` statements of the
//! top level only; and a result that depends on another module is cached
//! with the file, so it is stale until the file changes or the cache is
//! cleared. Upgrade with the contract's module index.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Comprehension, ExceptHandler, Expr, ExprContext, ModModule, Pattern, Stmt, StmtClassDef,
    StmtImportFrom,
};
use ruff_python_parser::Parsed;

use ferrolint_core::LineIndex;

use super::classes::bases;
use crate::c_module_names;

/// How far up from the linted file a project root is looked for.
const LEVELS: usize = 8;

pub struct Project {
    /// The directories modules are searched in, nearest first.
    roots: Vec<PathBuf>,
    /// The package of the linted file, for its relative imports.
    package: String,
    modules: RefCell<HashMap<String, Option<Rc<Module>>>>,
}

pub struct Module {
    name: String,
    parsed: Parsed<ModModule>,
    /// The package that relative imports of the module start from.
    package: String,
    /// What each name the top level imports stands for.
    imports: HashMap<String, String>,
    /// The modules that `from m import *` reads, in order.
    stars: Vec<String>,
    lines: LineIndex,
}

/// A class of a module of the project.
#[derive(Clone)]
pub struct ClassRef {
    module: Rc<Module>,
    name: String,
}

impl Project {
    pub fn new(path: &Path, module: &str) -> Self {
        let is_package = path.file_stem().is_some_and(|stem| stem == "__init__");
        let package = if is_package {
            module.to_owned()
        } else {
            module
                .rsplit_once('.')
                .map_or(String::new(), |(p, _)| p.to_owned())
        };
        let roots = path
            .ancestors()
            .skip(1)
            .take(LEVELS)
            .map(Path::to_path_buf)
            .collect();
        Self {
            roots,
            package,
            modules: RefCell::default(),
        }
    }

    /// As [`Project::new`], with the standard library and the installed
    /// packages of the environment that `VIRTUAL_ENV` names searched last,
    /// so a base class from either is read too.
    pub fn with_packages(path: &Path, module: &str) -> Self {
        let mut project = Self::new(path, module);
        if let Some(env) = std::env::var_os("VIRTUAL_ENV").map(PathBuf::from) {
            project.roots.extend(stdlib(&env));
            // `lib/pythonX.Y/site-packages` on Unix, `Lib/site-packages` on Windows.
            let unix = std::fs::read_dir(env.join("lib"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path().join("site-packages"));
            project.roots.extend(
                unix.chain([env.join("Lib").join("site-packages")])
                    .filter(|dir| dir.is_dir()),
            );
        }
        project
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
        let module = self.read(name).map(Rc::new);
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

    fn read(&self, name: &str) -> Option<Module> {
        let (file, is_package) = self.locate(name)?;
        let source = std::fs::read_to_string(file).ok()?;
        let parsed = ruff_python_parser::parse_module(&source).ok()?;
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
        })
    }

    /// The class `qualified` names (`pkg.module.Class`), also when `pkg.module`
    /// only imports it.
    pub fn class(&self, qualified: &str) -> Option<ClassRef> {
        self.class_at(qualified, 0)
    }

    fn class_at(&self, qualified: &str, depth: u8) -> Option<ClassRef> {
        let (module, name) = qualified.rsplit_once('.')?;
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
        if let Some(origin) = loaded.imports.get(name) {
            return self.class_at(origin, depth + 1);
        }
        loaded
            .stars
            .iter()
            .find_map(|star| self.class_at(&format!("{star}.{name}"), depth + 1))
    }
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
        if depth >= 8 {
            return;
        }
        let mut late = Vec::new();
        bound(package, body, &mut late, out);
        for item in late {
            match item {
                Late::Name(name) => add_public(out, &name),
                Late::Star(origin) => {
                    let _ = self.public_at(&origin, depth + 1, out);
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
fn bound(package: &str, body: &[Stmt], late: &mut Vec<Late>, out: &mut Vec<String>) {
    let mut binder = Binder {
        package,
        globals: None,
        late,
        out,
    };
    binder.visit_body(body);
}

struct Binder<'o> {
    package: &'o str,
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
            add_public(self.out, name);
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
                    add_public(self.out, f.name.as_str());
                }
            }
            Stmt::ClassDef(c) if self.globals.is_none() => {
                self.scope(&c.body);
                add_public(self.out, c.name.as_str());
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
    if !name.starts_with('_') && !out.iter().any(|seen| seen == name) {
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

    pub fn class(&self) -> Option<&StmtClassDef> {
        find_class(&self.module.parsed.syntax().body, &self.name)
    }

    /// The dotted name `expr` stands for in the class's module.
    pub fn qualify(&self, expr: &Expr) -> Option<String> {
        self.module.qualify(expr)
    }

    /// The classes that this one derives from and the project holds, and the
    /// qualified names of the rest.
    pub fn bases(&self, project: &Project) -> Vec<Result<ClassRef, String>> {
        let Some(class) = self.class() else {
            return Vec::new();
        };
        bases(class)
            .iter()
            .filter_map(|base| {
                // `Base[T]` derives from `Base`.
                let base = match base {
                    Expr::Subscript(s) => &*s.value,
                    other => other,
                };
                if let Expr::Name(name) = base
                    && find_class(&self.module.parsed.syntax().body, name.id.as_str()).is_some()
                {
                    return Some(Ok(Self {
                        module: self.module.clone(),
                        name: name.id.to_string(),
                    }));
                }
                let Some(qualified) = self.module.qualify(base) else {
                    // A name that `from module import *` binds.
                    let Expr::Name(name) = base else {
                        return None;
                    };
                    return self
                        .module
                        .stars
                        .iter()
                        .find_map(|star| project.class(&format!("{star}.{}", name.id)))
                        .map(Ok);
                };
                Some(project.class(&qualified).ok_or(qualified))
            })
            .collect()
    }
}

impl Module {
    fn qualify(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Name(name) => self.imports.get(name.id.as_str()).cloned(),
            Expr::Attribute(attribute) => Some(format!(
                "{}.{}",
                self.qualify(&attribute.value)?,
                attribute.attr
            )),
            _ => None,
        }
    }
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
            Stmt::Try(t) => found = find_class(&t.body, name).or(found),
            _ => {}
        }
    }
    found
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
    out: &mut HashMap<String, String>,
    stars: &mut Vec<String>,
) {
    for stmt in body {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let first = alias.name.split('.').next().unwrap_or_default();
                    match &alias.asname {
                        Some(asname) => out.insert(asname.to_string(), alias.name.to_string()),
                        None => out.insert(first.to_owned(), first.to_owned()),
                    };
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
                    out.insert(local, origin);
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
        let project = Project::new(&root.path().join("app/views.py"), "app.views");
        let class = project.class("api.base.Base").unwrap();
        assert_eq!(class.qname(), "api.base.Base");
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
        let project = Project::new(&root.path().join("app/views.py"), "app.views");
        let bases = project.class("app.child.Child").unwrap().bases(&project);
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].as_ref().unwrap().qname(), "app.base.Base");
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
        let project = Project::new(&root.path().join("app/views.py"), "app.views");
        assert_eq!(
            project.public_names("app.m").unwrap(),
            ["os", "a", "f", "K", "c", "z"]
        );
        assert!(project.public_names("app.missing").is_none());
    }

    #[test]
    fn a_module_is_found_by_the_case_it_is_spelled_in() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("misc/Inner")).unwrap();
        std::fs::write(root.path().join("misc/Inner/__init__.py"), "").unwrap();
        std::fs::write(root.path().join("misc/iterTools.py"), "").unwrap();
        let project = Project::new(&root.path().join("misc/user.py"), "misc.user");
        assert!(project.has_module("iterTools") && project.has_module("Inner"));
        assert!(!project.has_module("itertools") && !project.has_module("inner"));
    }
}
