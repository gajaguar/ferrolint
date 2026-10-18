//! `W0223` abstract-method: a concrete class that leaves unimplemented a
//! method an ancestor declares abstract, by `raise NotImplementedError` or
//! `@abc.abstractmethod`. Pylint walks the class's MRO; here the MRO is
//! built over the classes of the file, of the project and, when
//! `VIRTUAL_ENV` names one, of the installed packages.
//!
//! shortcut: a class with a base that none of those hold (the standard
//! library, a package outside the environment, a base computed at run time)
//! is skipped rather than guessed at, so its findings are missed; upgrade
//! with a module index that covers the standard library.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::Ranged;

use super::classes::{Env, Local, bases, locals, methods, py_repr};
use super::project::{ClassRef, Project};
use super::{Checker, Ctx, rule};

const ABSTRACT: &str = "W0223";

/// Bases that add no method to the MRO, though the project cannot read them.
const EMPTY_BASES: [&str; 5] = [
    "object",
    "typing.Generic",
    "abc.ABC",
    "typing.Protocol",
    "typing_extensions.Protocol",
];

pub struct AbstractMethods;

impl Checker for AbstractMethods {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![rule(
            ABSTRACT,
            "abstract-method",
            "A concrete class that does not override a method its ancestors declare abstract.",
        )]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(ABSTRACT) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            project: Project::with_packages(ctx.path, ctx.module),
            cache: RefCell::default(),
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// A class of an MRO.
#[derive(Clone)]
enum Class<'a> {
    Local(&'a StmtClassDef),
    Remote(ClassRef),
}

impl Class<'_> {
    fn def(&self) -> Option<&StmtClassDef> {
        match self {
            Class::Local(class) => Some(*class),
            Class::Remote(class) => class.class(),
        }
    }

    fn key(&self) -> String {
        match self {
            Class::Local(class) => format!("@{}", std::ptr::from_ref(*class).addr()),
            Class::Remote(class) => class.qname(),
        }
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    project: Project,
    /// The MRO of each class met, by key; `None` when it cannot be built.
    cache: RefCell<HashMap<String, Option<Vec<Class<'a>>>>>,
    out: &'e mut Vec<Diagnostic>,
}

impl<'a> Walk<'a, '_> {
    /// The dotted name `expr` stands for where `class` is written.
    fn qualify(&self, class: &Class<'a>, expr: &Expr) -> Option<String> {
        match class {
            Class::Local(_) => self.env.qualified(expr),
            Class::Remote(class) => class.qualify(expr),
        }
    }

    /// The direct bases of `class`, or `None` when one is unknown.
    fn bases(&self, class: &Class<'a>) -> Option<Vec<Class<'a>>> {
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
    fn mro(&self, class: &Class<'a>, depth: u8) -> Option<Vec<Class<'a>>> {
        let key = class.key();
        if let Some(found) = self.cache.borrow().get(&key) {
            return found.clone();
        }
        let found = (depth < 32)
            .then(|| {
                let bases = self.bases(class)?;
                let mut seqs = bases
                    .iter()
                    .map(|base| self.mro(base, depth + 1))
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

    fn is_abstract(&self, owner: &Class<'a>, func: &StmtFunctionDef) -> bool {
        func.decorator_list.iter().any(|decorator| {
            self.qualify(owner, &decorator.expression)
                .is_some_and(|name| name == "abc.abstractmethod" || name == "abc.abstractproperty")
        }) || raises_not_implemented(func)
    }

    /// Pylint's `class_is_abstract`: a protocol, an `ABCMeta` or `ABC`
    /// class, or one with an abstract method of its own.
    fn is_abstract_class(&self, class: &'a StmtClassDef) -> bool {
        let own = Class::Local(class);
        let named = |expr: &Expr, names: &[&str]| {
            let expr = match expr {
                Expr::Subscript(s) => &*s.value,
                other => other,
            };
            self.env
                .qualified(expr)
                .is_some_and(|name| names.contains(&name.as_str()))
        };
        let protocols = ["typing.Protocol", "typing_extensions.Protocol"];
        bases(class)
            .iter()
            .any(|base| named(base, &protocols) || named(base, &["abc.ABC", "_py_abc.ABC"]))
            || class.arguments.as_ref().is_some_and(|arguments| {
                arguments.keywords.iter().any(|keyword| {
                    keyword.arg.as_ref().is_some_and(|arg| arg == "metaclass")
                        && named(&keyword.value, &["abc.ABCMeta", "_py_abc.ABCMeta"])
                })
            })
            || methods(&class.body)
                .iter()
                .any(|method| self.is_abstract(&own, method))
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        if self.is_abstract_class(class) {
            return;
        }
        let own = Class::Local(class);
        let Some(mro) = self.mro(&own, 0) else {
            return;
        };
        // Name -> (owner key, owner name); later ancestors override earlier ones.
        let mut abstract_methods: BTreeMap<String, (String, String)> = BTreeMap::new();
        for ancestor in mro.iter().rev() {
            let Some(def) = ancestor.def() else { continue };
            let mut seen = BTreeSet::new();
            for (name, kind) in locals(&def.body) {
                // Pylint reads the first binding of each name.
                if !seen.insert(name.clone()) {
                    continue;
                }
                let func = (kind == Local::Function)
                    .then(|| {
                        methods(&def.body)
                            .into_iter()
                            .find(|f| f.name.as_str() == name)
                    })
                    .flatten();
                if func.is_some_and(|func| self.is_abstract(ancestor, func)) {
                    abstract_methods.insert(name, (ancestor.key(), def.name.to_string()));
                } else {
                    abstract_methods.remove(&name);
                }
            }
        }
        let own_key = own.key();
        let own_names: BTreeSet<String> = locals(&class.body).into_iter().map(|(n, _)| n).collect();
        for (name, (owner_key, owner)) in abstract_methods {
            if owner_key == own_key || own_names.contains(&name) {
                continue;
            }
            let message = format!(
                "Method {} is abstract in class {} but is not overridden in child class {}",
                py_repr(&name),
                py_repr(&owner),
                py_repr(&class.name)
            );
            self.out.push(
                self.env
                    .ctx
                    .report_at(ABSTRACT, class.name.range(), message),
            );
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            self.class(class);
        }
        visitor::walk_stmt(self, stmt);
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

/// Whether the first statement after the docstring raises
/// `NotImplementedError`, named anywhere in the raised expression.
fn raises_not_implemented(func: &StmtFunctionDef) -> bool {
    struct Find(bool);
    impl Visitor<'_> for Find {
        fn visit_expr(&mut self, expr: &Expr) {
            if let Expr::Name(name) = expr
                && name.id.as_str() == "NotImplementedError"
            {
                self.0 = true;
            }
            visitor::walk_expr(self, expr);
        }
    }
    let docstring =
        matches!(func.body.first(), Some(Stmt::Expr(e)) if e.value.is_string_literal_expr());
    let Some(Stmt::Raise(raise)) = func.body.get(usize::from(docstring)) else {
        return false;
    };
    raise.exc.as_deref().is_some_and(|exc| {
        let mut find = Find(false);
        find.visit_expr(exc);
        find.0
    })
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn abstract_methods() {
        let source = include_str!("fixtures/abstract_methods.py");
        let expected = include_str!("fixtures/abstract_methods.expected").trim_end();
        assert_eq!(render(source, &[], &["W0223"]), expected);
    }
}
