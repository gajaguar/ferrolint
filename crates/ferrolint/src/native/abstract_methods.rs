//! `W0223` abstract-method: a concrete class that leaves unimplemented a
//! method an ancestor declares abstract, by `raise NotImplementedError` or
//! `@abc.abstractmethod`. And `E0110` abstract-class-instantiated: a call of
//! a class that still has a method decorated with an `abc` decorator and
//! whose metaclass is `ABCMeta`. Pylint walks the class's MRO; here it is the
//! one [`super::hierarchy`] builds, and a class without one is skipped.

use std::collections::{BTreeMap, BTreeSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::Ranged;

use super::classes::{Env, Local, bases, locals, methods, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};

const ABSTRACT: &str = "W0223";
const INSTANTIATED: &str = "E0110";

/// Pylint's `ABC_METHODS`: the decorators that make a method abstract for
/// `E0110`.
const ABC_METHODS: [&str; 4] = [
    "abc.abstractproperty",
    "abc.abstractmethod",
    "abc.abstractclassmethod",
    "abc.abstractstaticmethod",
];

pub struct AbstractMethods;

impl Checker for AbstractMethods {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                ABSTRACT,
                "abstract-method",
                "A concrete class that does not override a method its ancestors declare abstract.",
            ),
            rule(
                INSTANTIATED,
                "abstract-class-instantiated",
                "A call of a class with an `ABCMeta` metaclass that has abstract methods left.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(ABSTRACT) && !enabled.contains(INSTANTIATED) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            hierarchy: Hierarchy::new(&env),
            enabled,
            enclosing: Vec::new(),
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: Hierarchy<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    /// The classes whose body holds the node visited, the innermost last.
    enclosing: Vec<&'a StmtClassDef>,
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

    /// Pylint's `unimplemented_abstract_methods`: by name, the key and the
    /// name of the class that declares each method `is_abstract` and no
    /// later class of the MRO overrides.
    fn unimplemented(
        mro: &[Class<'a>],
        is_abstract: &dyn Fn(&Class<'a>, &StmtFunctionDef) -> bool,
    ) -> BTreeMap<String, (String, String)> {
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
                if func.is_some_and(|func| is_abstract(ancestor, func)) {
                    abstract_methods.insert(name, (ancestor.key(), def.name.to_string()));
                } else {
                    abstract_methods.remove(&name);
                }
            }
        }
        abstract_methods
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        if self.is_abstract_class(class) {
            return;
        }
        let own = Class::Local(class);
        let Some(mro) = self.hierarchy.mro(&own) else {
            return;
        };
        let abstract_methods =
            Self::unimplemented(&mro, &|owner, func| self.is_abstract(owner, func));
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

impl<'a> Walk<'a, '_> {
    /// Pylint's `_check_inferred_class_is_abstract`, for a call of a class
    /// that is not the one the call is written in.
    fn instantiation(&mut self, call: &ExprCall) {
        let Some(class) = self.hierarchy.resolve_here(&call.func) else {
            return;
        };
        let inside = self.enclosing.last().copied();
        if let (Class::Local(def), Some(inside)) = (&class, inside)
            && std::ptr::eq(*def, inside)
        {
            return;
        }
        let (Some(def), Some(mro)) = (class.def(), self.hierarchy.mro(&class)) else {
            return;
        };
        if Self::unimplemented(&mro, &|owner, func| self.is_abc_abstract(owner, func)).is_empty()
            || !self.abc_metaclass(&mro)
        {
            return;
        }
        let message = format!(
            "Abstract class {} with abstract methods instantiated",
            py_repr(&def.name)
        );
        self.out
            .push(self.env.ctx.report_at(INSTANTIATED, call.range(), message));
    }

    /// Whether a method carries one of Pylint's `ABC_METHODS` decorators.
    fn is_abc_abstract(&self, owner: &Class<'a>, func: &StmtFunctionDef) -> bool {
        func.decorator_list.iter().any(|decorator| {
            let expr = match &decorator.expression {
                Expr::Call(call) => &*call.func,
                other => other,
            };
            self.hierarchy
                .qualify(owner, expr)
                .is_some_and(|name| ABC_METHODS.contains(&name.as_str()))
        })
    }

    /// Whether the metaclass of the class with this MRO is `ABCMeta` itself:
    /// the first `metaclass=` keyword along the MRO, or else a base `ABC`.
    fn abc_metaclass(&self, mro: &[Class<'a>]) -> bool {
        let declared = mro.iter().find_map(|class| {
            let keywords = &class.def()?.arguments.as_ref()?.keywords;
            keywords
                .iter()
                .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg == "metaclass"))
                .map(|keyword| (class, &keyword.value))
        });
        match declared {
            Some((class, value)) => self
                .hierarchy
                .qualify(class, value)
                .is_some_and(|name| name == "abc.ABCMeta" || name == "_py_abc.ABCMeta"),
            None => mro.iter().any(|class| {
                class.def().is_some_and(|def| {
                    bases(def).iter().any(|base| {
                        self.hierarchy
                            .qualify(class, base)
                            .is_some_and(|name| name == "abc.ABC")
                    })
                })
            }),
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            if self.enabled.contains(ABSTRACT) {
                self.class(class);
            }
            // Bases and decorators are evaluated outside the class.
            class
                .decorator_list
                .iter()
                .for_each(|decorator| self.visit_decorator(decorator));
            if let Some(arguments) = &class.arguments {
                self.visit_arguments(arguments);
            }
            self.enclosing.push(class);
            self.visit_body(&class.body);
            self.enclosing.pop();
        } else {
            visitor::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr
            && self.enabled.contains(INSTANTIATED)
        {
            self.instantiation(call);
        }
        visitor::walk_expr(self, expr);
    }
}

/// Whether the first statement after the docstring raises
/// `NotImplementedError`, named anywhere in the raised expression.
pub(super) fn raises_not_implemented(func: &StmtFunctionDef) -> bool {
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

    #[test]
    fn abstract_class_instantiated() {
        let source = include_str!("fixtures/abstract_class_instantiated.py");
        let expected = include_str!("fixtures/abstract_class_instantiated.expected").trim_end();
        assert_eq!(render(source, &[], &["E0110"]), expected);
    }
}
