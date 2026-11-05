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

use super::classes::{Env, Local, bases, locals};
use super::project::{ClassRef, Project};
use crate::class_methods;
use crate::wit::BindingKind;

/// Bases that add no method to the MRO, though the project cannot read them.
pub(super) const EMPTY_BASES: [&str; 11] = [
    "object",
    // C classes with no method of their own that a rule reads.
    "ctypes.Structure",
    "ctypes.Union",
    // `ast.AST` is a C class that astroid cannot build.
    "ast.AST",
    // `typing` makes these lazily, so astroid reads the context manager classes they stand for.
    "typing.ContextManager",
    "typing.AsyncContextManager",
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
    /// `Generic` or another base that adds no method.
    Empty,
    /// A builtin class other than `object`, whose methods are not modelled.
    Builtin,
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
    /// The name Pylint gives the linted module.
    module: String,
    /// The modules the file imports with `from m import *`.
    stars: Vec<String>,
    /// The MRO of each class met, by key; `None` when it cannot be built.
    cache: RefCell<HashMap<String, Option<Vec<Class<'a>>>>>,
    /// The same, with the bases the project cannot read left out.
    known: RefCell<HashMap<String, Result<Vec<Class<'a>>, Fail>>>,
}

impl<'a, 'e> Hierarchy<'a, 'e> {
    pub fn new(env: &'e Env<'a>) -> Self {
        let project = Project::with_packages(env.ctx.path, env.ctx.module, env.ctx.roots);
        Self {
            module: Project::named(env.ctx.path, env.ctx.module, env.ctx.roots),
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
            Class::Local(_) => &self.module,
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

    /// Whether `expr` is a name that nothing in this file binds and that adds no
    /// method: no star import can bring it, as every module they read is one
    /// astroid cannot import, or a star module aliases it to an empty class
    /// (`Structure = ctypes.Structure`).
    pub fn lost(&self, expr: &Expr) -> bool {
        let Expr::Name(name) = expr else { return false };
        self.env.binding(name).is_none()
            && (self.stars.iter().all(|star| self.project.is_missing(star))
                || self.stars.iter().any(|star| {
                    let aliased = self.project.unalias(&format!("{star}.{}", name.id));
                    !aliased.is_empty() && aliased.iter().all(|n| EMPTY_BASES.contains(&n.as_str()))
                }))
    }

    /// The class a name of this file stands for through `from m import *`.
    fn star(&self, expr: &Expr) -> Option<ClassRef> {
        let Expr::Name(name) = expr else { return None };
        self.stars
            .iter()
            .find_map(|star| self.project.class(&format!("{star}.{}", name.id)))
    }

    /// Whether astroid certainly fails to infer the base `expr` and so leaves
    /// it out of the ancestors: a name that only imports of a module the
    /// environment lacks bind, or a parameter with no default, or a subscript with such a name in its value or
    /// in a single index (a tuple index infers).
    ///
    /// shortcut: only a missing module is certain; a base computed by a call
    /// or bound otherwise stays unknown, upgrade with a value inference.
    pub fn dropped(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Subscript(s) => {
                self.dropped(&s.value) || (!s.slice.is_tuple_expr() && self.dropped(&s.slice))
            }
            Expr::Attribute(a) => self.dropped(&a.value),
            Expr::Name(name) => self.env.binding(name).is_some_and(|binding| {
                // A builtin has no binding in the file, so the list is empty.
                let all = self.env.ctx.model.same_name(binding.id);
                !all.is_empty()
                    && all.iter().all(|b| {
                        self.env.is_bare_param(b)
                            || matches!(b.kind, BindingKind::Import | BindingKind::FromImport)
                                && b.origin.as_deref().is_some_and(|origin| {
                                    !origin.starts_with('.')
                                        && self
                                            .project
                                            .is_missing(origin.split('.').next().unwrap_or(origin))
                                })
                    })
            }),
            _ => false,
        }
    }

    /// The bases of `class` as written, each as the MRO reads it.
    fn entries(&self, class: &Class<'a>) -> Vec<Entry<'a>> {
        match class {
            Class::Local(def) => bases(def)
                .iter()
                .map(|base| {
                    if self.dropped(base) || self.lost(base) {
                        return Entry::Empty;
                    }
                    // `Base[T]` derives from `Base`.
                    let (base, subscripted) = match base {
                        Expr::Subscript(s) => (&*s.value, true),
                        other => (other, false),
                    };
                    if let Some(local) = self
                        .env
                        .class_of(base)
                        .or_else(|| self.env.made_class(base))
                    {
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
                    if !qualified.contains('.') {
                        return Entry::Builtin;
                    }
                    if EMPTY_BASES.contains(&qualified.as_str()) {
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
            Class::Remote(remote) => remote_entries(&self.project, remote, false),
        }
    }

    /// The direct bases of `class`, or `None` when one is unknown.
    pub fn bases(&self, class: &Class<'a>) -> Option<Vec<Class<'a>>> {
        self.entries(class)
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Class(base) => Some(Some(base)),
                Entry::Unknown => Some(None),
                Entry::Object | Entry::Empty | Entry::Builtin => None,
            })
            .collect()
    }

    /// Whether `class` derives directly from a builtin class other than
    /// `object`, whose methods the MRO does not hold.
    pub fn has_builtin_base(&self, class: &Class<'a>) -> bool {
        self.entries(class)
            .iter()
            .any(|entry| matches!(entry, Entry::Builtin))
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

    /// The C3 linearization of `class` without the bases the project cannot
    /// read, `None` when the rest cannot be merged.
    pub fn known_order(&self, class: &Class<'a>) -> Option<Vec<Class<'a>>> {
        self.known_mro(class, 0).ok()
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
                Entry::Empty | Entry::Builtin | Entry::Unknown => {}
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

/// The bases of a class of the project as written, each as the MRO reads it.
/// With `exact`, a base that names nothing is `Unknown` instead of left out.
fn remote_entries(project: &Project, remote: &ClassRef, exact: bool) -> Vec<Entry<'static>> {
    let bases = if exact {
        remote.bases_exact(project)
    } else {
        remote.bases(project)
    };
    bases
        .into_iter()
        .map(|base| match base {
            Ok(base) => Entry::Class(Class::Remote(base)),
            Err(name) if name == "object" => Entry::Object,
            Err(name) if EMPTY_BASES.contains(&name.as_str()) => Entry::Empty,
            // A builtin base is not modelled, and is not unknown either.
            Err(name) if !name.contains('.') && class_methods::methods(&name).is_some() => {
                Entry::Builtin
            }
            Err(_) => Entry::Unknown,
        })
        .collect()
}

/// The MRO of the class `qualified` names (`pkg.module.Class`), the class
/// first, by qualified name; `object` and the builtin bases are left out.
/// `None` when the project does not hold the class, a base cannot be read or
/// the bases cannot be merged.
///
/// shortcut: no cache, so a deep hierarchy is merged again on every call;
/// upgrade by sharing `Hierarchy`'s cache when a plugin asks a lot.
pub fn ancestors(project: &Project, qualified: &str) -> Option<Vec<String>> {
    let class = project.class(qualified)?;
    let mro = remote_mro(project, &class, 0)?;
    Some(mro.iter().map(Class::key).collect())
}

/// Who binds a member of a class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The MRO is unknown, as for [`ancestors`].
    Unknown,
    /// No class of the MRO binds it (`object`, which the MRO leaves out, may).
    Nobody,
    /// The first class of the MRO that does, by qualified name.
    Class(String),
}

/// The first class of the MRO of `qualified` whose body binds `member`. A
/// bare annotation (`member: int`) binds nothing.
pub fn member_owner(project: &Project, qualified: &str, member: &str) -> Owner {
    let Some(mro) = project
        .class(qualified)
        .and_then(|class| remote_mro(project, &class, 0))
    else {
        return Owner::Unknown;
    };
    let binds = |class: &Class<'_>| {
        class.def().is_some_and(|def| {
            locals(&def.body)
                .iter()
                .any(|(name, kind)| name == member && !matches!(kind, Local::Bare))
        })
    };
    mro.iter()
        .find(|class| binds(class))
        .map_or(Owner::Nobody, |class| Owner::Class(class.key()))
}

fn remote_mro(project: &Project, class: &ClassRef, depth: u8) -> Option<Vec<Class<'static>>> {
    if depth >= 32 {
        return None;
    }
    let bases = remote_entries(project, class, true)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Class(Class::Remote(base)) => Some(Some(base)),
            Entry::Unknown => Some(None),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let mut seqs = bases
        .iter()
        .map(|base| remote_mro(project, base, depth + 1))
        .collect::<Option<Vec<_>>>()?;
    seqs.push(bases.into_iter().map(Class::Remote).collect());
    let mut out = vec![Class::Remote(class.clone())];
    out.extend(merge(seqs)?);
    Some(out)
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
    use super::Owner;

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
    fn a_subscripted_protocol_base_has_no_init_to_call() {
        let source = "from typing import Protocol, TypeVar\n\nT = TypeVar(\"T\")\n\nclass P(Protocol[T]):\n    def __init__(self, a):\n        self.a = a\n\nclass C(P[int]):\n    def __init__(self):\n        self.b = 1\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", source)],
                &["W0231"]
            ),
            ""
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
    fn a_typing_alias_is_the_class_it_aliases() {
        let files = [
            (
                "_collections_abc.py",
                "class Mapping:\n    def get(self, key, default=None): ...\n",
            ),
            (
                "typing.py",
                "import collections.abc\n\nMapping = _alias(collections.abc.Mapping, 2)\n",
            ),
            ("pkg/__init__.py", ""),
            (
                "pkg/main.py",
                "import typing\n\nclass A(typing.Mapping[str, str]):\n    def get(self, name, default=None): ...\n",
            ),
        ];
        assert_eq!(
            run(&files, &["W0237"]),
            "4:W0237:Parameter 'key' has been renamed to 'name' in overriding 'A.get' method"
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

    #[test]
    fn an_undefined_base_adds_no_method_to_count() {
        let main = "class S(Gone):\n    x = 1\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["R0903"]
            ),
            "1:R0903:Too few public methods (0/2)"
        );
    }

    #[test]
    fn a_star_alias_of_an_empty_class_adds_no_method_to_count() {
        let main = "from .defs import *\n\nclass S(Structure):\n    _fields_ = []\n";
        assert_eq!(
            run(
                &[
                    ("pkg/__init__.py", ""),
                    (
                        "pkg/defs.py",
                        "import ctypes\n\nStructure = ctypes.Structure\n"
                    ),
                    ("pkg/main.py", main)
                ],
                &["R0903"]
            ),
            "3:R0903:Too few public methods (0/2)"
        );
    }

    #[test]
    fn a_name_bound_to_an_empty_class_adds_no_method_to_count() {
        let main = "import ctypes\n\nStructure = ctypes.Structure\n\nclass S(Structure):\n    _fields_ = []\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["R0903"]
            ),
            "5:R0903:Too few public methods (0/2)"
        );
    }

    #[test]
    fn a_parameter_base_adds_no_method_to_count() {
        // astroid cannot infer a parameter with no default; `self` and a default it can.
        let main = "import io\n\ndef make(base, other=io.IOBase):\n    class A(base):\n        x = 1\n    class B(other):\n        x = 1\n    return A, B\n\nclass M:\n    def make(self, base):\n        class C(self):\n            x = 1\n        class D(base):\n            x = 1\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["R0903"]
            ),
            "4:R0903:Too few public methods (0/2)\n10:R0903:Too few public methods (1/2)\n14:R0903:Too few public methods (0/2)"
        );
    }

    #[test]
    fn a_c_class_without_public_method_adds_none_to_count() {
        let main = "import ast\nimport types\nimport typing\n\nclass A(ast.AST):\n    x = 1\n\nclass B(types.ModuleType):\n    x = 1\n\nclass C(typing.ContextManager[\"C\"]):\n    x = 1\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["R0903"]
            ),
            "5:R0903:Too few public methods (0/2)\n8:R0903:Too few public methods (0/2)\n11:R0903:Too few public methods (0/2)"
        );
    }

    #[test]
    fn an_undefined_base_hides_no_attribute() {
        let main = "class S(Gone):\n    def __init__(self):\n        self.a = 1\n\n    def run(self):\n        self.b = 2\n";
        assert_eq!(
            run(
                &[("pkg/__init__.py", ""), ("pkg/main.py", main)],
                &["W0201"]
            ),
            "6:W0201:Attribute 'b' defined outside __init__"
        );
    }

    /// The MRO of `qualified` in a project made of `files`, with `site/` as
    /// a second root standing for the installed packages.
    fn mro(files: &[(&str, &str)], qualified: &str) -> Option<Vec<String>> {
        on(files, |project| super::ancestors(project, qualified))
    }

    fn member_owner(files: &[(&str, &str)], qualified: &str, member: &str) -> Owner {
        on(files, |project| {
            super::member_owner(project, qualified, member)
        })
    }

    fn on<T>(files: &[(&str, &str)], ask: impl FnOnce(&super::Project) -> T) -> T {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let main = root.path().join("pkg/main.py");
        let roots = [root.path().to_path_buf(), root.path().join("site")];
        ask(&super::Project::new(&main, "pkg.main", &roots))
    }

    const BASE: [(&str, &str); 2] = [("pkg/__init__.py", ""), ("pkg/main.py", "")];

    fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
        BASE.iter().chain(extra).copied().collect()
    }

    #[test]
    fn ancestors_follow_the_project_bases_and_their_mixins() {
        let files = with(&[(
            "pkg/a.py",
            "class A: ...\nclass M: ...\nclass B(A): ...\nclass C(B, M): ...\n",
        )]);
        assert_eq!(
            mro(&files, "pkg.a.C").unwrap(),
            ["pkg.a.C", "pkg.a.B", "pkg.a.A", "pkg.a.M"]
        );
    }

    #[test]
    fn ancestors_read_a_base_of_the_installed_packages() {
        let files = with(&[
            ("site/lib/__init__.py", ""),
            ("site/lib/base.py", "class Base: ...\n"),
            (
                "pkg/a.py",
                "from lib.base import Base\nclass V(Base): ...\n",
            ),
        ]);
        assert_eq!(
            mro(&files, "pkg.a.V").unwrap(),
            ["pkg.a.V", "lib.base.Base"]
        );
    }

    #[test]
    fn ancestors_leave_out_object_and_the_builtin_bases() {
        let files = with(&[("pkg/a.py", "class B(dict): ...\nclass C(object): ...\n")]);
        assert_eq!(mro(&files, "pkg.a.B").unwrap(), ["pkg.a.B"]);
        assert_eq!(mro(&files, "pkg.a.C").unwrap(), ["pkg.a.C"]);
    }

    #[test]
    fn ancestors_are_unknown_when_a_base_cannot_be_read() {
        let files = with(&[(
            "pkg/a.py",
            "from nowhere import X\nclass B(X): ...\nclass C(B): ...\n",
        )]);
        assert_eq!(mro(&files, "pkg.a.B"), None);
        assert_eq!(mro(&files, "pkg.a.C"), None);
    }

    #[test]
    fn ancestors_are_unknown_when_a_base_names_nothing() {
        // An undefined name and a call may stand for any class, as astroid
        // cannot infer them: the other bases are not all of them.
        let files = with(&[(
            "pkg/a.py",
            "class A: ...\nclass B(A, Gone): ...\nclass C(make()): ...\n",
        )]);
        assert_eq!(mro(&files, "pkg.a.B"), None);
        assert_eq!(mro(&files, "pkg.a.C"), None);
    }

    #[test]
    fn ancestors_are_unknown_for_a_class_the_project_does_not_hold() {
        assert_eq!(mro(&with(&[]), "pkg.a.Gone"), None);
    }

    #[test]
    fn the_member_owner_is_the_first_class_of_the_mro_that_binds_it() {
        let files = with(&[(
            "pkg/a.py",
            "class A:\n    perm = 1\n    def run(self): ...\nclass M:\n    perm = 2\nclass B(A, M):\n    pass\n",
        )]);
        let owner = |member| member_owner(&files, "pkg.a.B", member);
        assert_eq!(owner("perm"), Owner::Class("pkg.a.A".to_owned()));
        assert_eq!(owner("run"), Owner::Class("pkg.a.A".to_owned()));
        assert_eq!(owner("absent"), Owner::Nobody);
    }

    #[test]
    fn a_bare_annotation_does_not_own_the_member() {
        let files = with(&[(
            "pkg/a.py",
            "class A:\n    perm = 1\nclass B(A):\n    perm: int\n",
        )]);
        assert_eq!(
            member_owner(&files, "pkg.a.B", "perm"),
            Owner::Class("pkg.a.A".to_owned())
        );
    }

    #[test]
    fn the_member_owner_is_unknown_when_a_base_cannot_be_read() {
        let files = with(&[(
            "pkg/a.py",
            "from nowhere import X\nclass B(X):\n    perm = 1\n",
        )]);
        assert_eq!(member_owner(&files, "pkg.a.B", "perm"), Owner::Unknown);
    }
}
