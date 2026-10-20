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

use ruff_python_ast::{Expr, StmtClassDef};
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

pub struct Hierarchy<'a, 'e> {
    pub env: &'e Env<'a>,
    pub project: Project,
    /// The MRO of each class met, by key; `None` when it cannot be built.
    cache: RefCell<HashMap<String, Option<Vec<Class<'a>>>>>,
}

impl<'a, 'e> Hierarchy<'a, 'e> {
    pub fn new(env: &'e Env<'a>) -> Self {
        Self {
            env,
            project: Project::with_packages(env.ctx.path, env.ctx.module),
            cache: RefCell::default(),
        }
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
        if let Class::Local(_) = class
            && let Some(local) = self.env.class_of(expr)
        {
            return Some(Class::Local(local));
        }
        let qualified = self.qualify(class, expr)?;
        let qualified = match class {
            Class::Local(_) => self.project.absolute(&qualified),
            Class::Remote(_) => qualified,
        };
        self.project.class(&qualified).map(Class::Remote)
    }

    /// The direct bases of `class`, or `None` when one is unknown.
    pub fn bases(&self, class: &Class<'a>) -> Option<Vec<Class<'a>>> {
        match class {
            Class::Local(def) => bases(def)
                .iter()
                .filter_map(|base| {
                    // `Base[T]` derives from `Base`.
                    let base = match base {
                        Expr::Subscript(s) => &*s.value,
                        other => other,
                    };
                    if let Some(local) = self.env.class_of(base) {
                        return Some(Some(Class::Local(local)));
                    }
                    let Some(qualified) = self.env.qualified(base) else {
                        return Some(None);
                    };
                    // A builtin has no module part and declares nothing abstract.
                    if !qualified.contains('.') || EMPTY_BASES.contains(&qualified.as_str()) {
                        return None;
                    }
                    let qualified = self.project.absolute(&qualified);
                    Some(self.project.class(&qualified).map(Class::Remote))
                })
                .collect(),
            Class::Remote(remote) => remote
                .bases(&self.project)
                .into_iter()
                .filter_map(|base| match base {
                    Ok(base) => Some(Some(Class::Remote(base))),
                    Err(name) if EMPTY_BASES.contains(&name.as_str()) => None,
                    Err(_) => Some(None),
                })
                .collect(),
        }
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
