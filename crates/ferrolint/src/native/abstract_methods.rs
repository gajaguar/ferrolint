//! `W0223` abstract-method: a concrete class that leaves unimplemented a
//! method an ancestor declares abstract, by `raise NotImplementedError` or
//! `@abc.abstractmethod`. Pylint walks the class's MRO; here it is the
//! one [`super::hierarchy`] builds, and a class without one is skipped.

use std::collections::{BTreeMap, BTreeSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::Ranged;

use super::classes::{Env, Local, bases, locals, methods, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};

const ABSTRACT: &str = "W0223";

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
            hierarchy: Hierarchy::new(&env),
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: Hierarchy<'a, 'e>,
    out: &'e mut Vec<Diagnostic>,
}

impl<'a> Walk<'a, '_> {
    fn is_abstract(&self, owner: &Class<'a>, func: &StmtFunctionDef) -> bool {
        func.decorator_list.iter().any(|decorator| {
            self.hierarchy
                .qualify(owner, &decorator.expression)
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
        let Some(mro) = self.hierarchy.mro(&own) else {
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
