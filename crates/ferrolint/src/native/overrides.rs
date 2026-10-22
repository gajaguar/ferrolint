//! The rules on a method against the one it overrides: `W0221`
//! arguments-differ, `W0222` signature-differs, `W0237` arguments-renamed,
//! `W0236` invalid-overridden-method, `W0239` overridden-final-method,
//! `W0246` useless-parent-delegation, and on the `__init__` calls of a
//! constructor, `W0231` super-init-not-called and `W0233`
//! non-parent-init-called. The ancestors come from [`super::hierarchy`]: a
//! class without an MRO is not checked, nor a base Pylint reads from a running
//! interpreter (a builtin such as `dict`), whose methods it does not compare
//! either.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Expr, ExprAttribute, ExprCall, Parameters, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::Ranged;

use super::classes::{Env, Local, is_classmethod, is_static, locals, methods, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::instance_attrs::is_private;
use super::misc::py_match;
use super::{Checker, Ctx, rule};

const DIFFER: &str = "W0221";
const SIGNATURE: &str = "W0222";
const RENAMED: &str = "W0237";
const SUPER_INIT: &str = "W0231";
const NON_PARENT_INIT: &str = "W0233";
const INVALID_OVERRIDE: &str = "W0236";
const USELESS_DELEGATION: &str = "W0246";
const FINAL_METHOD: &str = "W0239";
const IDS: [&str; 8] = [
    DIFFER,
    SIGNATURE,
    RENAMED,
    SUPER_INIT,
    NON_PARENT_INIT,
    INVALID_OVERRIDE,
    USELESS_DELEGATION,
    FINAL_METHOD,
];

/// The builtin classes a `Name.__init__(self)` call can name.
const BUILTIN_CLASSES: &[&str] = &[
    "object",
    "dict",
    "list",
    "set",
    "frozenset",
    "tuple",
    "str",
    "int",
    "float",
    "bytes",
    "bytearray",
    "bool",
    "complex",
    "type",
    "property",
    "Exception",
    "BaseException",
];

/// The attributes of `object`, whose signatures Pylint reads from the
/// running interpreter.
const OBJECT_ATTRIBUTES: &[&str] = &[
    "__class__",
    "__delattr__",
    "__dir__",
    "__doc__",
    "__eq__",
    "__format__",
    "__ge__",
    "__getattribute__",
    "__getstate__",
    "__gt__",
    "__hash__",
    "__init__",
    "__init_subclass__",
    "__le__",
    "__lt__",
    "__ne__",
    "__new__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__setattr__",
    "__sizeof__",
    "__str__",
    "__subclasshook__",
];

/// The methods of `object` whose delegation Pylint judges useless.
const OBJECT_PLAIN: &[&str] = &[
    "__init__",
    "__repr__",
    "__str__",
    "__dir__",
    "__sizeof__",
    "__reduce__",
    "__getstate__",
];

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
                SIGNATURE,
                "signature-differs",
                "A method with fewer default values than the method it overrides.",
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
            rule(
                NON_PARENT_INIT,
                "non-parent-init-called",
                "An `__init__` that calls the `__init__` of a class that is not a direct base.",
            ),
            rule(
                INVALID_OVERRIDE,
                "invalid-overridden-method",
                "A method overridden as a property, or the reverse, or as async, or the reverse.",
            ),
            rule(
                USELESS_DELEGATION,
                "useless-parent-delegation",
                "A method that only calls the same method of `super()` with its own parameters.",
            ),
            rule(
                FINAL_METHOD,
                "overridden-final-method",
                "A method that overrides a method decorated with `typing.final`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !IDS.iter().any(|id| enabled.contains(*id)) {
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
pub(super) fn is_overload(func: &StmtFunctionDef) -> bool {
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
        // The constructor check reads the bases one by one, so it does not need an MRO.
        let mro = self.hierarchy.mro(&Class::Local(class));
        for method in methods(&class.body) {
            if let Some(mro) = &mro
                && self.enabled.contains(USELESS_DELEGATION)
            {
                self.delegation(class, mro, method);
            }
            if method.name.as_str() == "__init__" {
                if self.enabled.contains(SUPER_INIT) || self.enabled.contains(NON_PARENT_INIT) {
                    self.constructor(class, method);
                }
            } else if let Some(mro) = &mro
                && [DIFFER, SIGNATURE, RENAMED, INVALID_OVERRIDE, FINAL_METHOD]
                    .iter()
                    .any(|id| self.enabled.contains(*id))
            {
                self.signature(class, mro, method);
            }
        }
    }

    /// The method `method` overrides: the first binding of its name in the
    /// first ancestor that binds it to a function, as astroid's
    /// `local_attr_ancestors`.
    fn signature(&mut self, class: &'a StmtClassDef, mro: &[Class<'a>], method: &StmtFunctionDef) {
        let name = method.name.as_str();
        let found = mro[1..].iter().find_map(|owner| {
            let def = owner.def()?;
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
            Some((owner, def, parent))
        });
        let Some((owner, def, parent)) = found else {
            return;
        };
        self.compare(class, def, parent, method);
        if self.enabled.contains(INVALID_OVERRIDE) {
            self.invalid_override(&Class::Local(class), owner, parent, method);
        }
        if self.enabled.contains(FINAL_METHOD)
            && self.env.ctx.py_version.is_none_or(|v| v >= (3, 8))
            && self.hierarchy.is_final(owner, &parent.decorator_list)
        {
            let message = format!(
                "Method {} overrides a method decorated with typing.final which is defined in class {}",
                py_repr(name),
                py_repr(&def.name)
            );
            self.out.push(
                self.env
                    .ctx
                    .report_at(FINAL_METHOD, method.name.range(), message),
            );
        }
    }

    /// Pylint's `_check_signature`: `W0221`, `W0237` and `W0222`.
    fn compare(
        &mut self,
        class: &StmtClassDef,
        owner: &StmtClassDef,
        parent: &StmtFunctionDef,
        method: &StmtFunctionDef,
    ) {
        let name = method.name.as_str();
        if is_private(name) || accessor(method, &["setter"]) || is_overload(method) {
            return;
        }
        let dummy = self
            .env
            .ctx
            .option("dummy-variables-rgx")
            .unwrap_or("_+$|(_[a-zA-Z0-9_]*[a-zA-Z0-9]+?$)|dummy|^ignored_|^unused_");
        let here = format!("'{}.{name}'", class.name);
        let found = differences(parent, method, dummy);
        if found.is_empty() {
            if self.enabled.contains(SIGNATURE)
                && defaults(&method.parameters) < defaults(&parent.parameters)
                && method.parameters.vararg.is_none()
            {
                self.out.push(self.env.ctx.report_at(
                    SIGNATURE,
                    method.name.range(),
                    format!("Signature differs from overridden {} method", py_repr(name)),
                ));
            }
            return;
        }
        for difference in found {
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

    /// Pylint's `_check_invalid_overridden_method`: `W0236`.
    fn invalid_override(
        &mut self,
        own: &Class<'a>,
        owner: &Class<'a>,
        parent: &StmtFunctionDef,
        method: &StmtFunctionDef,
    ) {
        let name = method.name.as_str();
        let (was, is) = (
            self.is_property(owner, parent),
            self.is_property(own, method),
        );
        let kind = if is_classmethod(method) {
            "classmethod"
        } else if is_static(method) {
            "staticmethod"
        } else {
            "method"
        };
        let mut found = Vec::new();
        if was && !is {
            found.push(("property", kind));
        } else if !was && is {
            found.push(("method", "property"));
        }
        match (parent.is_async, method.is_async) {
            (true, false) => found.push(("async", "non-async")),
            (false, true) => found.push(("non-async", "async")),
            _ => {}
        }
        for (expected, got) in found {
            self.out.push(self.env.ctx.report_at(
                INVALID_OVERRIDE,
                method.name.range(),
                format!(
                    "Method {} was expected to be {}, found it instead as {}",
                    py_repr(name),
                    py_repr(expected),
                    py_repr(got)
                ),
            ));
        }
    }

    /// Pylint's `decorated_with_property` or a setter or deleter.
    fn is_property(&self, owner: &Class<'a>, func: &StmtFunctionDef) -> bool {
        accessor(func, &["setter", "deleter"])
            || func.decorator_list.iter().any(|d| {
                !matches!(d.expression, Expr::Call(_)) && self.property_class(owner, &d.expression)
            })
    }

    /// Whether `expr` names `property`, `functools.cached_property` or a
    /// class derived from `property`.
    fn property_class(&self, owner: &Class<'a>, expr: &Expr) -> bool {
        const PROPERTIES: [&str; 4] = [
            "property",
            "builtins.property",
            "functools.cached_property",
            "abc.abstractproperty",
        ];
        let named = |class: &Class<'a>, expr: &Expr| {
            matches!(expr, Expr::Name(n) if n.id.as_str() == "property")
                || self
                    .hierarchy
                    .qualify(class, expr)
                    .is_some_and(|name| PROPERTIES.contains(&name.as_str()))
        };
        named(owner, expr)
            || self
                .hierarchy
                .resolve(owner, expr)
                .and_then(|class| self.hierarchy.mro(&class))
                .is_some_and(|mro| {
                    mro.iter().any(|class| {
                        class.def().is_some_and(|def| {
                            super::classes::bases(def)
                                .iter()
                                .any(|base| named(class, base))
                        })
                    })
                })
    }

    /// Pylint's `_check_useless_super_delegation`.
    fn delegation(&mut self, class: &'a StmtClassDef, mro: &[Class<'a>], func: &StmtFunctionDef) {
        let name = func.name.as_str();
        // astroid keeps the docstring out of the body.
        let body = match &func.body[..] {
            [Stmt::Expr(doc), rest @ ..] if doc.value.is_string_literal_expr() => rest,
            all => all,
        };
        if !func.decorator_list.is_empty()
            || body.len() != 1
            || matches!(name, "__new__" | "__init_subclass__" | "__class_getitem__")
        {
            return;
        }
        let value = match &body[0] {
            Stmt::Expr(stmt) => Some(&*stmt.value),
            Stmt::Return(stmt) => stmt.value.as_deref(),
            _ => None,
        };
        let Some(Expr::Call(call)) = value else {
            return;
        };
        let Expr::Attribute(callee) = &*call.func else {
            return;
        };
        if callee.attr.as_str() != name || !self.is_super(class, func, &callee.value) {
            return;
        }
        // A class that defines `__eq__` redefines `__hash__` on purpose.
        if name == "__hash__"
            && methods(&class.body)
                .iter()
                .any(|m| m.name.as_str() == "__eq__")
        {
            return;
        }
        let found = mro[1..].iter().find_map(|owner| {
            let def = owner.def()?;
            let (_, kind) = locals(&def.body).into_iter().find(|(n, _)| n == name)?;
            Some((def, kind))
        });
        let parent = match found {
            Some((_, kind)) if kind != Local::Function => return,
            Some((def, _)) => methods(&def.body)
                .into_iter()
                .find(|f| f.name.as_str() == name),
            // shortcut: Pylint finds the name on a builtin base and reads its
            // signature; only a plain class of the project, whose bases add no
            // name `object` lacks, is judged here, and of `object` only the
            // methods whose signature is just `self`.
            None if !self.plain(mro) => return,
            None if OBJECT_ATTRIBUTES.contains(&name) => {
                let p = &func.parameters;
                if !OBJECT_PLAIN.contains(&name)
                    || p.args.len() != 1
                    || p.vararg.is_some()
                    || p.kwarg.is_some()
                    || !p.posonlyargs.is_empty()
                    || !p.kwonlyargs.is_empty()
                {
                    return;
                }
                None
            }
            None => None,
        };
        if let Some(parent) = parent {
            let (o, n) = (&parent.parameters, &func.parameters);
            let (old, new) = (annotations(o), annotations(n));
            if different_defaults(o, n)
                || (o.vararg.is_some() && (n.vararg.is_none() || n.args.len() > o.args.len()))
                || (!old.is_empty() && !new.is_empty() && old != new)
                || matches!((&func.returns, &parent.returns), (Some(a), Some(b)) if ComparableExpr::from(&**a) != ComparableExpr::from(&**b))
            {
                return;
            }
        }
        if equivalent(&func.parameters, call) {
            self.out.push(self.env.ctx.report_at(
                USELESS_DELEGATION,
                func.name.range(),
                format!(
                    "Useless parent or super() delegation in method {}",
                    py_repr(name)
                ),
            ));
        }
    }

    /// Whether `expr` is `super()` or `super(Class, self)` in `func`, a
    /// method of `class`.
    fn is_super(&self, class: &StmtClassDef, func: &StmtFunctionDef, expr: &Expr) -> bool {
        let Expr::Call(call) = expr else {
            return false;
        };
        if !matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "super")
            || self.env.qualified(&call.func).is_none_or(|q| q != "super")
            || !call.arguments.keywords.is_empty()
        {
            return false;
        }
        match &call.arguments.args[..] {
            [] => true,
            [pointer, owner] => {
                let first = func
                    .parameters
                    .posonlyargs
                    .first()
                    .or(func.parameters.args.first());
                let is_self = |expr: &Expr| matches!((expr, first), (Expr::Name(n), Some(p)) if n.id == p.parameter.name.id);
                let class_pointer = self
                    .env
                    .class_of(pointer)
                    .is_some_and(|c| std::ptr::eq(c, class))
                    || matches!(pointer, Expr::Attribute(a) if a.attr.as_str() == "__class__" && is_self(&a.value));
                class_pointer && is_self(owner)
            }
            _ => false,
        }
    }

    /// Whether the classes of `mro` have no base but the project's own and
    /// the ones that add no name `object` lacks.
    fn plain(&self, mro: &[Class<'a>]) -> bool {
        mro.iter().all(|class| {
            class.def().is_none_or(|def| {
                super::classes::bases(def).iter().all(|base| {
                    let base = match base {
                        Expr::Subscript(s) => &*s.value,
                        other => other,
                    };
                    let last = match base {
                        Expr::Name(n) => Some(n.id.as_str()),
                        Expr::Attribute(a) => Some(a.attr.as_str()),
                        _ => None,
                    };
                    self.hierarchy.resolve(class, base).is_some()
                        || last
                            .is_some_and(|n| ["object", "ABC", "Generic", "Protocol"].contains(&n))
                })
            })
        })
    }

    /// The class that gives `base` its `__init__`, when it is a method of a
    /// class the project reads and not abstract: Pylint's `_ancestors_to_call`.
    fn init_of(&self, base: &Class<'a>) -> Option<String> {
        let mro = self.hierarchy.mro(base)?;
        let found = mro.iter().find_map(|class| {
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
        });
        // astroid finds an `__init__` on a class derived from a protocol.
        let Some((owner, init)) = found else {
            return mro
                .iter()
                .any(|class| self.is_protocol(class))
                .then(|| "typing.Protocol".to_owned());
        };
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

    /// The identity of the direct bases of `class`, or `None` when one is unknown.
    fn direct_bases(&self, class: &'a StmtClassDef) -> Option<Vec<String>> {
        let own = Class::Local(class);
        let bases = super::classes::bases(class);
        if bases.is_empty() {
            return Some(vec!["object".to_owned()]);
        }
        bases
            .iter()
            .map(|base| {
                let base = match base {
                    Expr::Subscript(s) => &*s.value,
                    other => other,
                };
                self.hierarchy
                    .resolve(&own, base)
                    .map(|c| c.key())
                    .or_else(|| self.env.qualified(base))
            })
            .collect()
    }

    /// `W0231` and `W0233`, Pylint's `_check_init`.
    fn constructor(&mut self, class: &'a StmtClassDef, init: &StmtFunctionDef) {
        if self.env.ctx.is_stub() {
            return;
        }
        let own = Class::Local(class);
        let bases = self.hierarchy.known_bases(&own);
        let direct = self.direct_bases(class);
        // (base, the class that gives it its `__init__`), in base order.
        let mut not_called: Vec<(Class<'a>, String)> = Vec::new();
        for base in bases {
            if !not_called.iter().any(|(b, _)| b.key() == base.key())
                && let Some(owner) = self.init_of(&base)
            {
                not_called.push((base, owner));
            }
        }
        let first = init
            .parameters
            .posonlyargs
            .first()
            .or(init.parameters.args.first())
            .map(|p| p.parameter.name.as_str());
        let mut find = InitCalls::default();
        find.visit_body(&init.body);
        // Pylint stops at the first `super().__init__`.
        let before = find.super_at.unwrap_or(find.calls.len());
        let mut called = BTreeSet::new();
        for call in &find.calls[..before] {
            // The class called (its identity and name), or the class of an
            // instance, which is never a base.
            let (key, name) = match &*call.value {
                Expr::Name(n) if first == Some(n.id.as_str()) => {
                    (None, Some(class.name.to_string()))
                }
                Expr::Call(inner) => match self.hierarchy.resolve(&own, &inner.func) {
                    Some(instance) => (None, instance.def().map(|d| d.name.to_string())),
                    None => continue,
                },
                value => {
                    let target = self.hierarchy.resolve(&own, value);
                    if let Some(target) = &target
                        && let Some(pos) =
                            not_called.iter().position(|(b, _)| b.key() == target.key())
                    {
                        called.insert(not_called.remove(pos).1);
                        continue;
                    }
                    match (&target, self.env.qualified(value)) {
                        (Some(target), _) => {
                            (Some(target.key()), target.def().map(|d| d.name.to_string()))
                        }
                        (None, Some(name)) if BUILTIN_CLASSES.contains(&name.as_str()) => {
                            (Some(name.clone()), Some(name))
                        }
                        _ => continue,
                    }
                }
            };
            if let (Some(direct), Some(name)) = (&direct, name)
                && self.enabled.contains(NON_PARENT_INIT)
                && key.is_none_or(|key| !direct.contains(&key))
            {
                self.out.push(self.env.ctx.report_at(
                    NON_PARENT_INIT,
                    call.range(),
                    format!(
                        "__init__ method from a non direct base class {} is called",
                        py_repr(&name)
                    ),
                ));
            }
        }
        if find.super_at.is_some() || !self.enabled.contains(SUPER_INIT) {
            return;
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

/// Whether a decorator of `func` is an attribute called one of `names`.
fn accessor(func: &StmtFunctionDef, names: &[&str]) -> bool {
    func.decorator_list
        .iter()
        .any(|d| matches!(&d.expression, Expr::Attribute(a) if names.contains(&a.attr.as_str())))
}

/// The positional parameters with a default, as astroid's `args.defaults`.
fn defaults(p: &Parameters) -> usize {
    p.posonlyargs
        .iter()
        .chain(&p.args)
        .filter(|a| a.default.is_some())
        .count()
}

/// The annotations of the positional parameters, in order.
fn annotations(p: &Parameters) -> Vec<ComparableExpr<'_>> {
    p.posonlyargs
        .iter()
        .chain(&p.args)
        .filter_map(|a| a.parameter.annotation.as_deref())
        .map(ComparableExpr::from)
        .collect()
}

/// Pylint's `_has_different_parameters_default_value`: whether a parameter
/// of `original` has a default that `overriding` lacks, gives another or
/// does not have.
fn different_defaults(original: &Parameters, overriding: &Parameters) -> bool {
    original.iter_non_variadic_params().any(|param| {
        let name = param.parameter.name.as_str();
        let (old, new) = (
            param.default.as_deref(),
            overriding
                .iter_non_variadic_params()
                .find(|a| a.parameter.name.as_str() == name)
                .and_then(|a| a.default.as_deref()),
        );
        match (old, new) {
            (None, None) => false,
            (Some(old), Some(new)) => !same_default(old, new),
            _ => true,
        }
    })
}

/// Pylint's `ASTROID_TYPE_COMPARATORS` on two defaults: a literal against a
/// literal, a name against a name, and two empty containers are the same;
/// anything else counts as different.
fn same_default(old: &Expr, new: &Expr) -> bool {
    match (old, new) {
        (Expr::Name(a), Expr::Name(b)) => a.id == b.id,
        (Expr::Tuple(a), Expr::Tuple(b)) => a.elts.is_empty() && b.elts.is_empty(),
        (Expr::List(a), Expr::List(b)) => a.elts.is_empty() && b.elts.is_empty(),
        (Expr::Dict(a), Expr::Dict(b)) => a.items.is_empty() && b.items.is_empty(),
        _ => {
            let literal = |e: &Expr| {
                matches!(
                    e,
                    Expr::NumberLiteral(_)
                        | Expr::StringLiteral(_)
                        | Expr::BytesLiteral(_)
                        | Expr::BooleanLiteral(_)
                        | Expr::NoneLiteral(_)
                        | Expr::EllipsisLiteral(_)
                )
            };
            literal(old) && literal(new) && ComparableExpr::from(old) == ComparableExpr::from(new)
        }
    }
}

/// Pylint's `_definition_equivalent_to_call`: whether `call` passes exactly
/// the parameters of `params` along, by name.
fn equivalent(params: &Parameters, call: &ExprCall) -> bool {
    let (mut args, mut starred_args, mut starred_kws) = (Vec::new(), Vec::new(), Vec::new());
    let mut kws: Vec<(Option<&str>, Option<&str>)> = Vec::new();
    for arg in &call.arguments.args {
        match arg {
            Expr::Starred(s) if matches!(&*s.value, Expr::Name(_)) => {
                if let Expr::Name(n) = &*s.value {
                    starred_args.push(n.id.as_str());
                }
            }
            Expr::Name(n) => args.push(Some(n.id.as_str())),
            _ => args.push(None),
        }
    }
    for keyword in &call.arguments.keywords {
        let name = keyword
            .arg
            .as_ref()
            .map(ruff_python_ast::Identifier::as_str);
        match (&keyword.value, name) {
            (Expr::Name(n), None) => starred_kws.push(n.id.as_str()),
            (Expr::Name(n), name) => kws.push((name, Some(n.id.as_str()))),
            (_, name) => kws.push((name, None)),
        }
    }
    let rest = |variadic: &Option<Box<ruff_python_ast::Parameter>>, starred: &[&str]| match variadic
    {
        Some(p) => starred.contains(&p.name.as_str()),
        None => starred.is_empty(),
    };
    if !rest(&params.kwarg, &starred_kws) || !rest(&params.vararg, &starred_args) {
        return false;
    }
    let kwonly: Vec<&str> = params
        .kwonlyargs
        .iter()
        .map(|p| p.parameter.name.as_str())
        .collect();
    // A repeated keyword keeps its last value, as in a dict.
    let value = |kw: &str| {
        kws.iter()
            .rev()
            .find(|(name, _)| *name == Some(kw))
            .and_then(|(_, value)| *value)
    };
    if kwonly.iter().any(|kw| value(kw) != Some(*kw)) {
        return false;
    }
    let positional: Vec<Option<&str>> = params
        .posonlyargs
        .iter()
        .chain(&params.args)
        .map(|p| p.parameter.name.as_str())
        .filter(|name| *name != "self")
        .map(Some)
        .collect();
    positional == args
        && kws.iter().all(|(name, _)| {
            name.is_some_and(|kw| args.contains(&Some(kw)) || kwonly.contains(&kw))
        })
}

/// The `__init__` calls of a body, and where a `super().__init__` stands.
#[derive(Default)]
struct InitCalls<'a> {
    calls: Vec<&'a ExprAttribute>,
    /// The number of calls that come before the first `super().__init__`.
    super_at: Option<usize>,
}

impl<'a> Visitor<'a> for InitCalls<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr
            && let Expr::Attribute(a) = &*call.func
            && a.attr.as_str() == "__init__"
        {
            match &*a.value {
                Expr::Call(inner) if matches!(&*inner.func, Expr::Name(n) if n.id.as_str() == "super") =>
                {
                    self.super_at.get_or_insert(self.calls.len());
                }
                _ => self.calls.push(a),
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
    use std::path::Path;

    use crate::native::classes::{render, render_at};

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

    #[test]
    fn a_stub_is_not_checked_for_super_init() {
        let source = "class A:\n    def __init__(self):\n        self.a = 1\n\n\nclass B(A):\n    def __init__(self):\n        self.b = 1\n";
        let at = |name| render_at(source, Path::new(name), "mod", &[], &["W0231"]);
        assert_eq!(
            at("mod.py"),
            "7:W0231:__init__ method from base class 'A' is not called"
        );
        assert_eq!(at("mod.pyi"), "");
    }

    #[test]
    fn signature_differs() {
        let source = include_str!("fixtures/signature_differs.py");
        let expected = include_str!("fixtures/signature_differs.expected").trim_end();
        assert_eq!(render(source, &[], &["W0221", "W0222", "W0237"]), expected);
    }

    #[test]
    fn non_parent_init_called() {
        let source = include_str!("fixtures/non_parent_init_called.py");
        let expected = include_str!("fixtures/non_parent_init_called.expected").trim_end();
        assert_eq!(render(source, &[], &["W0231", "W0233"]), expected);
    }

    #[test]
    fn invalid_overridden_method() {
        let source = include_str!("fixtures/invalid_overridden_method.py");
        let expected = include_str!("fixtures/invalid_overridden_method.expected").trim_end();
        assert_eq!(render(source, &[], &["W0236"]), expected);
    }

    #[test]
    fn overridden_final_method() {
        let source = include_str!("fixtures/overridden_final_method.py");
        let expected = include_str!("fixtures/overridden_final_method.expected").trim_end();
        assert_eq!(render(source, &[], &["W0239"]), expected);
    }

    #[test]
    fn useless_parent_delegation() {
        let source = include_str!("fixtures/useless_parent_delegation.py");
        let expected = include_str!("fixtures/useless_parent_delegation.expected").trim_end();
        assert_eq!(render(source, &[], &["W0246"]), expected);
    }
}
