//! The rules on a method against the one it overrides: `W0221`
//! arguments-differ, `W0237` arguments-renamed and `W0231`
//! super-init-not-called. The ancestors come from [`super::hierarchy`]: a
//! class without an MRO is not checked, nor a base Pylint reads from a
//! running interpreter (a builtin such as `dict`), whose methods it does not
//! compare either.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Parameters, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::Ranged;

use super::classes::{Env, Local, is_classmethod, is_static, locals, methods, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::instance_attrs::is_private;
use super::misc::py_match;
use super::{Checker, Ctx, rule};

const DIFFER: &str = "W0221";
const RENAMED: &str = "W0237";
const SUPER_INIT: &str = "W0231";

/// Pylint's `PYMETHODS`: the special methods whose parameters keep only
/// their number.
const SPECIAL: &[&str] = &[
    "__abs__",
    "__add__",
    "__aenter__",
    "__aexit__",
    "__aiter__",
    "__and__",
    "__anext__",
    "__await__",
    "__bool__",
    "__bytes__",
    "__call__",
    "__ceil__",
    "__cmp__",
    "__complex__",
    "__contains__",
    "__copy__",
    "__deepcopy__",
    "__del__",
    "__delattr__",
    "__delete__",
    "__delitem__",
    "__dir__",
    "__div__",
    "__divmod__",
    "__enter__",
    "__eq__",
    "__exit__",
    "__float__",
    "__floor__",
    "__floordiv__",
    "__format__",
    "__fspath__",
    "__ge__",
    "__get__",
    "__getattr__",
    "__getattribute__",
    "__getitem__",
    "__getnewargs__",
    "__getnewargs_ex__",
    "__getstate__",
    "__gt__",
    "__hash__",
    "__iadd__",
    "__iand__",
    "__ifloordiv__",
    "__ilshift__",
    "__imatmul__",
    "__imod__",
    "__imul__",
    "__index__",
    "__init__",
    "__init_subclass__",
    "__instancecheck__",
    "__int__",
    "__invert__",
    "__ior__",
    "__ipow__",
    "__irshift__",
    "__isub__",
    "__iter__",
    "__itruediv__",
    "__ixor__",
    "__le__",
    "__len__",
    "__length_hint__",
    "__lshift__",
    "__lt__",
    "__matmul__",
    "__missing__",
    "__mod__",
    "__mul__",
    "__ne__",
    "__neg__",
    "__new__",
    "__nonzero__",
    "__or__",
    "__pos__",
    "__pow__",
    "__radd__",
    "__rand__",
    "__rdivmod__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__reversed__",
    "__rfloordiv__",
    "__rlshift__",
    "__rmatmul__",
    "__rmod__",
    "__rmul__",
    "__ror__",
    "__round__",
    "__rpow__",
    "__rrshift__",
    "__rshift__",
    "__rsub__",
    "__rtruediv__",
    "__rxor__",
    "__set__",
    "__set_name__",
    "__setattr__",
    "__setitem__",
    "__setstate__",
    "__str__",
    "__sub__",
    "__subclasscheck__",
    "__subclasses__",
    "__truediv__",
    "__trunc__",
    "__unicode__",
    "__xor__",
];

/// Pylint's `CONSTRUCTOR_METHODS`, whose signatures it does not compare.
const CONSTRUCTORS: &[&str] = &["__new__", "__init__", "__init_subclass__", "__post_init__"];

pub struct Overrides;

impl Checker for Overrides {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                DIFFER,
                "arguments-differ",
                "A method whose parameters differ in number or kind from the method it overrides.",
            ),
            rule(
                RENAMED,
                "arguments-renamed",
                "A method that renames a parameter of the method it overrides.",
            ),
            rule(
                SUPER_INIT,
                "super-init-not-called",
                "An `__init__` that calls neither `super().__init__` nor that of a base.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![DIFFER, RENAMED, SUPER_INIT]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            hierarchy: Hierarchy::new(&env),
            enabled,
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: Hierarchy<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
}

/// Whether a decorator of `func` is `typing.overload`, by its last name.
fn is_overload(func: &StmtFunctionDef) -> bool {
    func.decorator_list.iter().any(|d| {
        let expr = match &d.expression {
            Expr::Call(call) => &*call.func,
            other => other,
        };
        match expr {
            Expr::Name(n) => n.id.as_str() == "overload",
            Expr::Attribute(a) => a.attr.as_str() == "overload",
            _ => false,
        }
    })
}

/// A parameter with whether it has a default.
type Param<'f> = (&'f str, bool);

/// The positional parameters Pylint compares: the non-positional-only ones,
/// less the first of a class method, which is bound.
fn positional(func: &StmtFunctionDef) -> Vec<Param<'_>> {
    let all: Vec<Param<'_>> = func
        .parameters
        .args
        .iter()
        .map(|p| (p.parameter.name.as_str(), p.default.is_some()))
        .collect();
    let skip = usize::from(is_classmethod(func) && !is_static(func));
    all.into_iter().skip(skip).collect()
}

fn kwonly(p: &Parameters) -> Vec<Param<'_>> {
    p.kwonlyargs
        .iter()
        .map(|p| (p.parameter.name.as_str(), p.default.is_some()))
        .collect()
}

/// The number Pylint gives a signature in its message.
fn total(p: &Parameters) -> usize {
    p.args.len()
        + usize::from(p.vararg.is_some())
        + usize::from(p.kwarg.is_some())
        + p.kwonlyargs.len()
}

const NUMBER: &str = "Number of parameters ";

/// Pylint's `_different_parameters`: the differences of `overriding` from
/// `original`, each the start of a message.
fn differences(
    original: &StmtFunctionDef,
    overriding: &StmtFunctionDef,
    dummy: &str,
) -> Vec<String> {
    let mut original_positional = positional(original);
    let overriding_positional = positional(overriding);
    let mut original_kwonly = kwonly(&original.parameters);
    let (o, n) = (&original.parameters, &overriding.parameters);
    if n.vararg.is_some() {
        original_positional
            .retain(|(name, _)| overriding_positional.iter().any(|(m, _)| m == name));
    }
    let overriding_kwonly = kwonly(n);
    if n.kwarg.is_some() {
        original_kwonly.retain(|(name, _)| overriding_kwonly.iter().any(|(m, _)| m == name));
    }
    let positional = (|| {
        let mut out = Vec::new();
        for i in 0..original_positional.len().max(overriding_positional.len()) {
            let Some(&(new, default)) = overriding_positional.get(i) else {
                return vec![NUMBER.to_owned()];
            };
            let Some(&(old, _)) = original_positional.get(i) else {
                if default {
                    continue;
                }
                return vec![NUMBER.to_owned()];
            };
            if py_match(dummy, old) || py_match(dummy, new) {
                continue;
            }
            if old != new {
                out.push(format!("Parameter '{old}' has been renamed to '{new}' in"));
            }
        }
        out
    })();
    let keyword = if original_kwonly
        .iter()
        .any(|(name, _)| !overriding_kwonly.iter().any(|(m, _)| m == name))
        || overriding_kwonly
            .iter()
            .any(|(name, default)| !default && !original_kwonly.iter().any(|(m, _)| m == name))
    {
        vec![NUMBER.to_owned()]
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    if !keyword.is_empty() && !positional.is_empty() && positional[0].starts_with("Number ") {
        out.push(NUMBER.to_owned());
        out.extend(positional[1..].iter().cloned());
    } else {
        out.extend(positional);
        out.extend(keyword);
    }
    if (o.kwarg.is_some() && n.kwarg.is_none()) || (o.vararg.is_some() && n.vararg.is_none()) {
        out.push("Variadics removed in".to_owned());
    }
    let name = original.name.as_str();
    if CONSTRUCTORS.contains(&name) {
        out.clear();
    } else if SPECIAL.contains(&name) {
        out.retain(|message| message.contains("Number"));
    }
    out
}

impl<'a> Walk<'a, '_> {
    fn class(&mut self, class: &'a StmtClassDef) {
        let own = Class::Local(class);
        let Some(mro) = self.hierarchy.mro(&own) else {
            return;
        };
        for method in methods(&class.body) {
            if method.name.as_str() == "__init__" {
                if self.enabled.contains(SUPER_INIT) {
                    self.super_init(class, method);
                }
            } else if self.enabled.contains(DIFFER) || self.enabled.contains(RENAMED) {
                self.signature(class, &mro, method);
            }
        }
    }

    /// The method `method` overrides: the first binding of its name in the
    /// first ancestor that binds it to a function, as astroid's
    /// `local_attr_ancestors`.
    fn signature(&mut self, class: &StmtClassDef, mro: &[Class<'a>], method: &StmtFunctionDef) {
        let name = method.name.as_str();
        let found = mro[1..].iter().filter_map(Class::def).find_map(|def| {
            let (_, kind) = locals(&def.body).into_iter().find(|(n, _)| n == name)?;
            if kind != Local::Function {
                return None;
            }
            let all: Vec<&StmtFunctionDef> = methods(&def.body)
                .into_iter()
                .filter(|f| f.name.as_str() == name)
                .collect();
            let first = *all.first()?;
            // Compare with the implementation, not the first overload stub.
            let parent = if is_overload(first) {
                all.iter()
                    .rev()
                    .find(|f| !is_overload(f))
                    .copied()
                    .unwrap_or(first)
            } else {
                first
            };
            Some((def, parent))
        });
        let Some((owner, parent)) = found else { return };
        let setter = method
            .decorator_list
            .iter()
            .any(|d| matches!(&d.expression, Expr::Attribute(a) if a.attr.as_str() == "setter"));
        if is_private(name) || setter || is_overload(method) {
            return;
        }
        let dummy = self
            .env
            .ctx
            .option("dummy-variables-rgx")
            .unwrap_or("_+$|(_[a-zA-Z0-9_]*[a-zA-Z0-9]+?$)|dummy|^ignored_|^unused_");
        let here = format!("'{}.{name}'", class.name);
        for difference in differences(parent, method, dummy) {
            let (id, message) = if difference.contains("Number") {
                (
                    DIFFER,
                    format!(
                        "{difference}was {} in '{}.{name}' and is now {} in overriding {here} method",
                        total(&parent.parameters),
                        owner.name,
                        total(&method.parameters)
                    ),
                )
            } else if difference.contains("renamed") {
                (RENAMED, format!("{difference} overriding {here} method"))
            } else {
                (DIFFER, format!("{difference} overriding {here} method"))
            };
            if self.enabled.contains(id) {
                self.out
                    .push(self.env.ctx.report_at(id, method.name.range(), message));
            }
        }
    }

    /// The class that gives `base` its `__init__`, when it is a method of a
    /// class the project reads and not abstract: Pylint's `_ancestors_to_call`.
    fn init_of(&self, base: &Class<'a>) -> Option<String> {
        let mro = self.hierarchy.mro(base)?;
        let (owner, init) = mro.iter().find_map(|class| {
            let def = class.def()?;
            locals(&def.body)
                .iter()
                .any(|(n, _)| n == "__init__")
                .then(|| {
                    let init = methods(&def.body)
                        .into_iter()
                        .find(|f| f.name.as_str() == "__init__");
                    (class.key(), init)
                })
        })?;
        let init = init?;
        // astroid's `is_abstract`: the first statement after the docstring
        // is `pass` or raises `NotImplementedError`, or there is none.
        let docstring =
            matches!(init.body.first(), Some(Stmt::Expr(e)) if e.value.is_string_literal_expr());
        let abstract_ = match init.body.get(usize::from(docstring)) {
            None | Some(Stmt::Pass(_)) => true,
            Some(Stmt::Raise(raise)) => raise.exc.as_deref().is_some_and(|exc| {
                let exc = match exc {
                    Expr::Call(call) => &*call.func,
                    other => other,
                };
                matches!(exc, Expr::Name(n) if n.id.as_str() == "NotImplementedError")
            }),
            Some(_) => false,
        } || init.decorator_list.iter().any(|d| {
            self.hierarchy
                .qualify(base, &d.expression)
                .is_some_and(|name| name == "abc.abstractmethod")
        });
        (!abstract_).then_some(owner)
    }

    fn super_init(&mut self, class: &'a StmtClassDef, init: &StmtFunctionDef) {
        if self
            .env
            .ctx
            .path
            .extension()
            .is_some_and(|ext| ext == "pyi")
        {
            return;
        }
        let own = Class::Local(class);
        let Some(bases) = self.hierarchy.bases(&own) else {
            return;
        };
        // (base, the class that gives it its `__init__`), in base order.
        let mut not_called: Vec<(Class<'a>, String)> = Vec::new();
        for base in bases {
            if !not_called.iter().any(|(b, _)| b.key() == base.key())
                && let Some(owner) = self.init_of(&base)
            {
                not_called.push((base, owner));
            }
        }
        let mut find = InitCalls(Vec::new(), false);
        find.visit_body(&init.body);
        if find.1 {
            return;
        }
        let mut called = BTreeSet::new();
        for expr in find.0 {
            let Some(target) = self.hierarchy.resolve(&own, expr) else {
                continue;
            };
            if let Some(pos) = not_called.iter().position(|(b, _)| b.key() == target.key()) {
                called.insert(not_called.remove(pos).1);
            }
        }
        for (base, owner) in &not_called {
            if called.contains(owner) || self.is_protocol(base) {
                return;
            }
            if is_overload(init) {
                continue;
            }
            let Some(def) = base.def() else { continue };
            self.out.push(self.env.ctx.report_at(
                SUPER_INIT,
                init.name.range(),
                format!(
                    "__init__ method from base class {} is not called",
                    py_repr(&def.name)
                ),
            ));
        }
    }

    fn is_protocol(&self, class: &Class<'a>) -> bool {
        let protocols = ["typing.Protocol", "typing_extensions.Protocol"];
        class.def().is_some_and(|def| {
            super::classes::bases(def).iter().any(|base| {
                self.hierarchy
                    .qualify(class, base)
                    .is_some_and(|name| protocols.contains(&name.as_str()))
            })
        })
    }
}

/// The objects whose `__init__` a body calls, and whether one is `super()`.
struct InitCalls<'a>(Vec<&'a Expr>, bool);

impl<'a> Visitor<'a> for InitCalls<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr
            && let Expr::Attribute(a) = &*call.func
            && a.attr.as_str() == "__init__"
        {
            match &*a.value {
                Expr::Call(inner) if matches!(&*inner.func, Expr::Name(n) if n.id.as_str() == "super") =>
                {
                    self.1 = true;
                }
                other => self.0.push(other),
            }
        }
        visitor::walk_expr(self, expr);
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

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn overridden_signatures() {
        let source = include_str!("fixtures/overridden_signatures.py");
        let expected = include_str!("fixtures/overridden_signatures.expected").trim_end();
        assert_eq!(render(source, &[], &["W0221", "W0237"]), expected);
    }

    #[test]
    fn super_init_not_called() {
        let source = include_str!("fixtures/super_init_not_called.py");
        let expected = include_str!("fixtures/super_init_not_called.expected").trim_end();
        assert_eq!(render(source, &[], &["W0231"]), expected);
    }
}
