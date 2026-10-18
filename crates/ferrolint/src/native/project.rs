//! The classes of the other modules of the project, found by their dotted
//! name, for the rules that ask what an ancestor of a class defines. Pylint
//! infers a base class across modules; here the module is searched for in
//! the directories above the linted file (the `sys.path` of a project laid
//! out like a project's) and parsed on demand.
//!
//! shortcut: only modules that are files under those directories are read,
//! so the standard library and the installed packages stay unknown (a base
//! from them ends the chain); a name is followed through `import` and `from`
//! statements of the top level only; and a result that depends on another
//! module is cached with the file, so it is stale until the file changes or
//! the cache is cleared. Upgrade with the contract's module index.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ruff_python_ast::{Expr, ModModule, Stmt, StmtClassDef};
use ruff_python_parser::Parsed;

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
        let first = relative.iter().next()?;
        // The root is the nearest directory that holds the top package.
        let root = self.roots.iter().find(|root| {
            root.join(first).is_dir() || root.join(first).with_extension("py").is_file()
        })?;
        let file = root.join(&relative).with_extension("py");
        let (file, is_package) = if file.is_file() {
            (file, false)
        } else {
            (root.join(&relative).join("__init__.py"), true)
        };
        let source = std::fs::read_to_string(file).ok()?;
        let parsed = ruff_python_parser::parse_module(&source).ok()?;
        let package = if is_package {
            name.to_owned()
        } else {
            name.rsplit_once('.')
                .map_or(String::new(), |(p, _)| p.to_owned())
        };
        let mut imports = HashMap::new();
        collect_imports(&parsed.syntax().body, &package, &mut imports);
        Some(Module {
            name: name.to_owned(),
            parsed,
            package,
            imports,
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
        let origin = loaded.imports.get(name)?;
        (depth < 8).then(|| self.class_at(origin, depth + 1))?
    }
}

impl ClassRef {
    /// The qualified name of the class where it is defined.
    pub fn qname(&self) -> String {
        format!("{}.{}", self.module.name, self.name)
    }

    pub fn class(&self) -> Option<&StmtClassDef> {
        find_class(&self.module.parsed.syntax().body, &self.name)
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

fn collect_imports(body: &[Stmt], package: &str, out: &mut HashMap<String, String>) {
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
                collect_imports(&s.body, package, out);
                s.elif_else_clauses
                    .iter()
                    .for_each(|c| collect_imports(&c.body, package, out));
            }
            Stmt::Try(t) => {
                collect_imports(&t.body, package, out);
                for handler in &t.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(h) = handler;
                    collect_imports(&h.body, package, out);
                }
            }
            _ => {}
        }
    }
}
