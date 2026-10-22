//! The method resolution order of a class, for the rules that ask what its
//! ancestors define. The MRO is built over the classes of the file, of the
//! project and, when `VIRTUAL_ENV` names one, of the standard library and
//! the installed packages ([`Project::with_packages`]).
//!
//! shortcut: a class with a base that none of those hold (a C module, a
//! package outside the environment, a base computed at run time) has no
//! MRO, so the rules skip it rather than guess; upgrade with a module index.

use std::cell::RefCell;
use std::collections::HashMap;

use ruff_python_ast::{Decorator, Expr, StmtClassDef};
use ruff_text_size::TextSize;

use super::classes::{Env, bases};
use super::project::{ClassRef, Project};
use crate::class_methods;

/// Bases that add no method to the MRO, though the project cannot read them.
pub(super) const EMPTY_BASES: [&str; 8] = [
    "object",
    // C classes with no method of their own that a rule reads.
    "ctypes.Structure",
    "ctypes.Union",
    "typing.Generic",
    // `typing` imports `Generic` from this C module.
    "_typing.Generic",
    "abc.ABC",
    "typing.Protocol",
    "typing_extensions.Protocol",
];

/// A class of an MRO.
#[derive(Clone)]
pub enum Class<'a> {
    Local(&'a StmtClassDef),
    Remote(ClassRef),
}

impl Class<'_> {
    pub fn def(&self) -> Option<&StmtClassDef> {
        match self {
            Class::Local(class) => Some(*class),
            Class::Remote(class) => class.class(),
        }
    }

    pub fn key(&self) -> String {
        match self {
            Class::Local(class) => format!("@{}", std::ptr::from_ref(*class).addr()),
            Class::Remote(class) => class.qname(),
        }
    }
}

/// A base as written, and what the MRO makes of it.
enum Entry<'a> {
    Class(Class<'a>),
    Object,
    /// A builtin, `Generic` or another base that adds no method.
    Empty,
    Unknown,
}

/// Why bases have no MRO, as astroid tells them apart.
#[derive(Clone, Copy, PartialEq)]
enum Fail {
    Duplicate,
    Inconsistent,
}

pub struct Hierarchy<'a, 'e> {
    pub env: &'e Env<'a>,
    pub project: Project,
    /// The modules the file imports with `from m import *`.
    stars: Vec<String>,
    /// The MRO of each class met, by key; `None` when it cannot be built.
    cache: RefCell<HashMap<String, Option<Vec<Class<'a>>>>>,
    /// The same, with the bases the project cannot read left out.
    known: RefCell<HashMap<String, Result<Vec<Class<'a>>, Fail>>>,
}

impl<'a, 'e> Hierarchy<'a, 'e> {
    pub fn new(env: &'e Env<'a>) -> Self {
        let project = Project::with_packages(env.ctx.path, env.ctx.module);
        Self {
            env,
            stars: project.stars(&env.ctx.parsed.syntax().body),
            project,
            cache: RefCell::default(),
            known: RefCell::default(),
        }
    }

    /// Whether a decorator names `typing.final`, as written in `owner`.
    pub fn is_final(&self, owner: &Class<'a>, decorators: &[Decorator]) -> bool {
        decorators.iter().any(|decorator| {
            let expr = match &decorator.expression {
                Expr::Call(call) => &*call.func,
                other => other,
            };
            self.qualify(owner, expr)
                .is_some_and(|name| name == "typing.final" || name == "typing_extensions.final")
        })
    }

    /// The dotted name `expr` stands for where `class` is written.
    pub fn qualify(&self, class: &Class<'a>, expr: &Expr) -> Option<String> {
        match class {
            Class::Local(_) => self.env.qualified(expr),
            Class::Remote(class) => class.qualify(expr),
        }
    }

    /// The dotted name of the module that defines `class`.
    pub fn module<'c>(&'c self, class: &'c Class<'a>) -> &'c str {
        match class {
            Class::Local(_) => self.env.ctx.module,
            Class::Remote(class) => class.module(),
        }
    }

    /// The 1-based line of `offset` in the module of `class`.
    pub fn line(&self, class: &Class<'a>, offset: TextSize) -> u32 {
        match class {
            Class::Local(_) => self.env.ctx.index.line(offset.to_u32()),
            Class::Remote(class) => class.line(offset.to_u32()),
        }
    }

    /// The class `expr` names where `class` is written, or `None` when it
    /// names none the project holds.
    pub fn resolve(&self, class: &Class<'a>, expr: &Expr) -> Option<Class<'a>> {
        match class {
            Class::Local(_) => self.resolve_here(expr),
            Class::Remote(remote) => remote.resolve(&self.project, expr)?.ok().map(Class::Remote),
        }
    }

    /// The class `expr` names where it is written in this file.
    pub fn resolve_here(&self, expr: &Expr) -> Option<Class<'a>> {
        if let Some(local) = self.env.class_of(expr) {
            return Some(Class::Local(local));
        }
        let Some(qualified) = self.env.qualified(expr) else {
            return self.star(expr).map(Class::Remote);
        };
        self.project
            .class(&self.project.absolute(&qualified))
            .map(Class::Remote)
    }

    /// The class a name of this file stands for through `from m import *`.
    fn star(&self, expr: &Expr) -> Option<ClassRef> {
        let Expr::Name(name) = expr else { return None };
        self.stars
            .iter()
            .find_map(|star| self.project.class(&format!("{star}.{}", name.id)))
    }

    /// The bases of `class` as written, each as the MRO reads it.
    fn entries(&self, class: &Class<'a>) -> Vec<Entry<'a>> {
        match class {
            Class::Local(def) => bases(def)
                .iter()
                .map(|base| {
                    // `Base[T]` derives from `Base`.
                    let (base, subscripted) = match base {
                        Expr::Subscript(s) => (&*s.value, true),
                        other => (other, false),
                    };
                    if let Some(local) = self.env.class_of(base) {
                        return Entry::Class(Class::Local(local));
                    }
                    let Some(qualified) = self.env.qualified(base) else {
                        return self
                            .star(base)
                            .map_or(Entry::Unknown, |class| Entry::Class(Class::Remote(class)));
                    };
                    if qualified == "object" {
                        return Entry::Object;
                    }
                    // A builtin has no module part and declares nothing abstract.
                    if !qualified.contains('.') || EMPTY_BASES.contains(&qualified.as_str()) {
                        return Entry::Empty;
                    }
                    let qualified = self.project.absolute(&qualified);
                    self.project
                        .class(&qualified)
                        .map_or(Entry::Unknown, |class| {
                            // astroid infers `typing.X[...]` as an empty class.
                            if subscripted && class.is_typing() {
                                Entry::Empty
                            } else {
                                Entry::Class(Class::Remote(class))
                            }
                        })
                })
                .collect(),
            Class::Remote(remote) => remote
                .bases(&self.project)
                .into_iter()
                .map(|base| match base {
                    Ok(base) => Entry::Class(Class::Remote(base)),
                    Err(name) if name == "object" => Entry::Object,
                    Err(name) if EMPTY_BASES.contains(&name.as_str()) => Entry::Empty,
                    // A builtin base is not modelled, and is not unknown either.
                    Err(name) if !name.contains('.') && class_methods::methods(&name).is_some() => {
                        Entry::Empty
                    }
                    Err(_) => Entry::Unknown,
                })
                .collect(),
        }
    }

    /// The direct bases of `class`, or `None` when one is unknown.
    pub fn bases(&self, class: &Class<'a>) -> Option<Vec<Class<'a>>> {
        self.entries(class)
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Class(base) => Some(Some(base)),
                Entry::Unknown => Some(None),
                Entry::Object | Entry::Empty => None,
            })
            .collect()
    }

    /// The direct bases of `class` that the project can read; astroid skips
    /// an uninferable one the same way.
    pub fn known_bases(&self, class: &Class<'a>) -> Vec<Class<'a>> {
        self.entries(class)
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Class(base) => Some(base),
                _ => None,
            })
            .collect()
    }

    /// The C3 linearization of `class`, as Python computes it.
    pub fn mro(&self, class: &Class<'a>) -> Option<Vec<Class<'a>>> {
        self.mro_at(class, 0)
    }

    fn mro_at(&self, class: &Class<'a>, depth: u8) -> Option<Vec<Class<'a>>> {
        let key = class.key();
        if let Some(found) = self.cache.borrow().get(&key) {
            return found.clone();
        }
        let found = (depth < 32)
            .then(|| {
                let bases = self.bases(class)?;
                let mut seqs = bases
                    .iter()
                    .map(|base| self.mro_at(base, depth + 1))
                    .collect::<Option<Vec<_>>>()?;
                seqs.push(bases);
                let mut out = vec![class.clone()];
                out.extend(merge(seqs)?);
                Some(out)
            })
            .flatten();
        self.cache.borrow_mut().insert(key, found.clone());
        found
    }
}

impl<'a> Hierarchy<'a, '_> {
    /// Whether the bases of `class`, or of one of its ancestors, cannot be
    /// merged into an MRO, as astroid's `InconsistentMroError`. A base the
    /// project cannot read is left out, as astroid leaves out an uninferable
    /// one; a builtin other than `object` is not modelled, so `class A(int,
    /// bool)` passes.
    pub fn inconsistent(&self, class: &Class<'a>) -> bool {
        matches!(self.known_mro(class, 0), Err(Fail::Inconsistent))
    }

    fn known_mro(&self, class: &Class<'a>, depth: u8) -> Result<Vec<Class<'a>>, Fail> {
        let key = class.key();
        if let Some(found) = self.known.borrow().get(&key) {
            return found.clone();
        }
        let found = if depth < 32 {
            self.merge_known(class, depth)
        } else {
            Ok(vec![class.clone()])
        };
        self.known.borrow_mut().insert(key, found.clone());
        found
    }

    fn merge_known(&self, class: &Class<'a>, depth: u8) -> Result<Vec<Class<'a>>, Fail> {
        let mut bases = Vec::new();
        let mut object_first = false;
        let mut seen_object = false;
        for entry in self.entries(class) {
            match entry {
                Entry::Class(base) => {
                    object_first |= seen_object;
                    bases.push(base);
                }
                Entry::Object => seen_object = true,
                Entry::Empty | Entry::Unknown => {}
            }
        }
        let mut seqs = bases
            .iter()
            .map(|base| self.known_mro(base, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        let mut keys: Vec<String> = bases.iter().map(Class::key).collect();
        keys.sort();
        if keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Fail::Duplicate);
        }
        // `object` heads the MRO of every base, so it cannot come first.
        if object_first {
            return Err(Fail::Inconsistent);
        }
        seqs.push(bases);
        let mut out = vec![class.clone()];
        out.extend(merge(seqs).ok_or(Fail::Inconsistent)?);
        Ok(out)
    }
}

/// Python's MRO merge of the bases' linearizations; `None` when they
/// disagree, as for `class A(B, B)`.
fn merge(mut seqs: Vec<Vec<Class<'_>>>) -> Option<Vec<Class<'_>>> {
    let mut out = Vec::new();
    loop {
        seqs.retain(|seq| !seq.is_empty());
        if seqs.is_empty() {
            return Some(out);
        }
        let head = seqs.iter().map(|seq| seq[0].key()).find(|head| {
            !seqs
                .iter()
                .any(|seq| seq[1..].iter().any(|c| &c.key() == head))
        })?;
        let pos = seqs.iter().position(|seq| seq[0].key() == head)?;
        out.push(seqs[pos][0].clone());
        for seq in &mut seqs {
            if seq[0].key() == head {
                seq.remove(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render_at;

    /// Lints `pkg/main.py` of a project made of `files`.
    fn run(files: &[(&str, &str)], ids: &[&str]) -> String {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let main = root.path().join("pkg/main.py");
        let source = std::fs::read_to_string(&main).unwrap();
        render_at(&source, &main, "pkg.main", &[], ids)
    }

    #[test]
    fn a_base_of_another_module_reads_the_names_of_its_own_module() {
        // `Mid` derives from `Old` (defined beside it, the way `object` is
        // implied), which a module-level alias also names.
        let files = [
            ("pkg/__init__.py", "from .base import *\n"),
            (
                "pkg/base.py",
                "class Old(object):\n    def run(self): ...\n\nclass Mid(Old):\n    pass\n\nAlias = Mid\n",
            ),
            (
                "pkg/main.py",
                "from . import Alias\n\nclass Child(Alias):\n    pass\n",
            ),
        ];
        assert_eq!(
            run(&files, &["R0903"]),
            "3:R0903:Too few public methods (1/2)"
        );
    }

    #[test]
    fn a_builtin_base_of_another_module_does_not_make_the_chain_unknown() {
        let files = [
            ("pkg/__init__.py", ""),
            (
                "pkg/base.py",
                "class Box(dict):\n    def run(self, a):\n        ...\n",
            ),
            (
                "pkg/main.py",
                "from .base import Box\n\nclass Child(Box):\n    def run(self):\n        ...\n",
            ),
        ];
        assert_eq!(
            run(&files, &["W0221"]),
            "4:W0221:Number of parameters was 2 in 'Box.run' and is now 1 in overriding 'Child.run' method"
        );
    }

    #[test]
    fn a_module_bound_beside_a_fallback_assignment_still_names_the_base() {
        let files = [
            ("pkg/__init__.py", ""),
            (
                "pkg/base.py",
                "import abc\n\nclass Base(abc.ABC):\n    @abc.abstractmethod\n    def run(self): ...\n",
            ),
            (
                "pkg/main.py",
                "try:\n    from pkg import base\nexcept ImportError:\n    base = None\n\nclass Child(base.Base):\n    pass\n",
            ),
        ];
        assert_eq!(
            run(&files, &["W0223"]),
            "6:W0223:Method 'run' is abstract in class 'Base' but is not overridden in child class 'Child'"
        );
    }

    #[test]
    fn a_base_bound_by_a_star_import_of_the_file_is_found() {
        let files = [
            ("pkg/__init__.py", ""),
            (
                "pkg/base.py",
                "class Base:\n    def __init__(self):\n        self.x = 1\n",
            ),
            (
                "pkg/main.py",
                "from .base import *\n\nclass Child(Base):\n    def __init__(self):\n        pass\n",
            ),
        ];
        assert_eq!(
            run(&files, &["W0231"]),
            "4:W0231:__init__ method from base class 'Base' is not called"
        );
    }

    #[test]
    fn a_class_derived_from_a_protocol_has_an_init_to_call() {
        let source = "from typing import Protocol\n\nclass P(Protocol):\n    def f(self): ...\n\nclass Q(P):\n    pass\n\nclass R(Q):\n    def __init__(self):\n        pass\n\nclass S(P):\n    def __init__(self):\n        pass\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", source)],
                &["W0231"]
            ),
            "10:W0231:__init__ method from base class 'Q' is not called"
        );
    }

    #[test]
    fn a_subscripted_typing_base_is_an_empty_class() {
        let typing = "import abc\n\nclass IO:\n    @abc.abstractmethod\n    def run(self): ...\n";
        let main =
            "from typing import IO\n\nclass A(IO[str]):\n    pass\n\nclass B(IO):\n    pass\n";
        let files = [
            ("typing.py", typing),
            ("pkg/__init__.py", ""),
            ("pkg/main.py", main),
        ];
        assert_eq!(
            run(&files, &["W0223"]),
            "6:W0223:Method 'run' is abstract in class 'IO' but is not overridden in child class 'B'"
        );
    }

    #[test]
    fn an_unreadable_base_does_not_hide_the_readable_ones() {
        let main = "from nowhere import Mystery\n\nclass Base:\n    def __init__(self):\n        self.x = 1\n\nclass Child(Mystery, Base):\n    def __init__(self):\n        Mystery.__init__(self)\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["W0231", "W0233"]
            ),
            "8:W0231:__init__ method from base class 'Base' is not called"
        );
    }

    #[test]
    fn a_ctypes_structure_adds_no_method_to_count() {
        let main = "import ctypes\n\nclass S(ctypes.Structure):\n    _fields_ = []\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["R0903"]
            ),
            "3:R0903:Too few public methods (0/2)"
        );
    }
}
