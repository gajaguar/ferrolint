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

/// Bases that add no method to the MRO, though the project cannot read them.
pub(super) const EMPTY_BASES: [&str; 6] = [
    "object",
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
    /// The MRO of each class met, by key; `None` when it cannot be built.
    cache: RefCell<HashMap<String, Option<Vec<Class<'a>>>>>,
    /// The same, with the bases the project cannot read left out.
    known: RefCell<HashMap<String, Result<Vec<Class<'a>>, Fail>>>,
}

impl<'a, 'e> Hierarchy<'a, 'e> {
    pub fn new(env: &'e Env<'a>) -> Self {
        Self {
            env,
            project: Project::with_packages(env.ctx.path, env.ctx.module),
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
            Class::Remote(_) => {
                let qualified = self.qualify(class, expr)?;
                self.project.class(&qualified).map(Class::Remote)
            }
        }
    }

    /// The class `expr` names where it is written in this file.
    pub fn resolve_here(&self, expr: &Expr) -> Option<Class<'a>> {
        if let Some(local) = self.env.class_of(expr) {
            return Some(Class::Local(local));
        }
        let qualified = self.project.absolute(&self.env.qualified(expr)?);
        self.project.class(&qualified).map(Class::Remote)
    }

    /// The bases of `class` as written, each as the MRO reads it.
    fn entries(&self, class: &Class<'a>) -> Vec<Entry<'a>> {
        match class {
            Class::Local(def) => bases(def)
                .iter()
                .map(|base| {
                    // `Base[T]` derives from `Base`.
                    let base = match base {
                        Expr::Subscript(s) => &*s.value,
                        other => other,
                    };
                    if let Some(local) = self.env.class_of(base) {
                        return Entry::Class(Class::Local(local));
                    }
                    let Some(qualified) = self.env.qualified(base) else {
                        return Entry::Unknown;
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
                        .map_or(Entry::Unknown, |class| Entry::Class(Class::Remote(class)))
                })
                .collect(),
            Class::Remote(remote) => remote
                .bases(&self.project)
                .into_iter()
                .map(|base| match base {
                    Ok(base) => Entry::Class(Class::Remote(base)),
                    Err(name) if name == "object" => Entry::Object,
                    Err(name) if EMPTY_BASES.contains(&name.as_str()) => Entry::Empty,
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
