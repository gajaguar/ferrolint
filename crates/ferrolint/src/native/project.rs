//! The classes of the other modules of the project, found by their dotted
//! name, for the rules that ask what an ancestor of a class defines. Pylint
//! infers a base class across modules; here the module is searched for in
//! the directories above the linted file (the `sys.path` of a project laid
//! out like a project's) and parsed on demand.
//!
//! shortcut: only modules that are files under those directories are read,
//! so the standard library and the installed packages stay unknown but for
//! [`Project::with_packages`] (a base from them ends the chain), and a C
//! module never is; a name is followed through `import`, `from` and
//! `from m import *` statements of the top level only; and a result that depends on another
//! module is cached with the file, so it is stale until the file changes or
//! the cache is cleared. Upgrade with the contract's module index.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ruff_python_ast::{Expr, ModModule, Stmt, StmtClassDef};
use ruff_python_parser::Parsed;

use ferrolint_core::LineIndex;

use super::classes::bases;

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

    fn read(&self, name: &str) -> Option<Module> {
        let relative: PathBuf = name.split('.').collect();
        // The root is the nearest directory that holds the module itself: an
        // inner package may share the top package's name (`app/api` and `api`).
        let (file, is_package) = self.roots.iter().find_map(|root| {
            let file = root.join(&relative).with_extension("py");
            let init = root.join(&relative).join("__init__.py");
            if file.is_file() {
                Some((file, false))
            } else {
                init.is_file().then_some((init, true))
            }
        })?;
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
                let qualified = self.module.qualify(base)?;
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
                let mut base = package.to_owned();
                for _ in 1..import.level {
                    base = base
                        .rsplit_once('.')
                        .map_or(String::new(), |(p, _)| p.to_owned());
                }
                let module = import.module.as_ref().map(ToString::to_string);
                let prefix = match (import.level, module) {
                    (0, Some(module)) => module,
                    (_, Some(module)) if base.is_empty() => module,
                    (_, Some(module)) => format!("{base}.{module}"),
                    (_, None) => base,
                };
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
                    let ruff_python_ast::ExceptHandler::ExceptHandler(h) = handler;
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
}
