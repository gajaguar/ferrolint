//! Class members: `R0202` no-classmethod-decorator, `R0203`
//! no-staticmethod-decorator and `W3201` bad-dunder-name
//! (`pylint.extensions.dunder`, so it reports only when `load-plugins` names
//! it).

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;

use super::classes::{Local, blocks, locals, methods};
use super::extensions::loaded;
use super::{Checker, Ctx, rule};

const CLASSMETHOD: &str = "R0202";
const STATICMETHOD: &str = "R0203";
const DUNDER: &str = "W3201";

/// Pylint's `DUNDER_PROPERTIES`, `EXTRA_DUNDER_METHODS` and `DUNDER_METHODS`
/// for every version.
const GOOD: &[&str] = &[
    "__class__",
    "__dict__",
    "__doc__",
    "__format__",
    "__module__",
    "__sizeof__",
    "__subclasshook__",
    "__suppress_context__",
    "__weakref__",
    "__new__",
    "__subclasses__",
    "__init_subclass__",
    "__set_name__",
    "__class_getitem__",
    "__missing__",
    "__exit__",
    "__await__",
    "__aexit__",
    "__getnewargs_ex__",
    "__getnewargs__",
    "__getstate__",
    "__index__",
    "__setstate__",
    "__reduce__",
    "__reduce_ex__",
    "__post_init__",
    "_generate_next_value_",
    "_missing_",
    "_numeric_repr_",
    "__init__",
    "__del__",
    "__repr__",
    "__str__",
    "__bytes__",
    "__lt__",
    "__le__",
    "__eq__",
    "__ne__",
    "__gt__",
    "__ge__",
    "__hash__",
    "__bool__",
    "__getattr__",
    "__getattribute__",
    "__setattr__",
    "__delattr__",
    "__dir__",
    "__get__",
    "__set__",
    "__delete__",
    "__instancecheck__",
    "__subclasscheck__",
    "__call__",
    "__len__",
    "__length_hint__",
    "__getitem__",
    "__setitem__",
    "__delitem__",
    "__iter__",
    "__next__",
    "__reversed__",
    "__contains__",
    "__add__",
    "__sub__",
    "__mul__",
    "__matmul__",
    "__truediv__",
    "__floordiv__",
    "__mod__",
    "__divmod__",
    "__pow__",
    "__lshift__",
    "__rshift__",
    "__and__",
    "__xor__",
    "__or__",
    "__radd__",
    "__rsub__",
    "__rmul__",
    "__rmatmul__",
    "__rtruediv__",
    "__rfloordiv__",
    "__rmod__",
    "__rdivmod__",
    "__rpow__",
    "__rlshift__",
    "__rrshift__",
    "__rand__",
    "__rxor__",
    "__ror__",
    "__iadd__",
    "__isub__",
    "__imul__",
    "__imatmul__",
    "__itruediv__",
    "__ifloordiv__",
    "__imod__",
    "__ipow__",
    "__ilshift__",
    "__irshift__",
    "__iand__",
    "__ixor__",
    "__ior__",
    "__neg__",
    "__pos__",
    "__abs__",
    "__invert__",
    "__complex__",
    "__int__",
    "__float__",
    "__round__",
    "__trunc__",
    "__floor__",
    "__ceil__",
    "__enter__",
    "__aenter__",
    "__copy__",
    "__deepcopy__",
    "__fspath__",
];
/// The names added from a Python version on.
const SINCE: &[((u32, u32), &[&str])] = &[
    ((3, 10), &["__aiter__", "__anext__"]),
    ((3, 13), &["_add_alias_", "_add_value_alias_"]),
];

pub struct Members;

impl Checker for Members {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                CLASSMETHOD,
                "no-classmethod-decorator",
                "`name = classmethod(name)` in a class body for a method of the class.",
            ),
            rule(
                STATICMETHOD,
                "no-staticmethod-decorator",
                "`name = staticmethod(name)` in a class body for a method of the class.",
            ),
            rule(
                DUNDER,
                "bad-dunder-name",
                "A method named like a dunder method that Python does not define; needs `pylint.extensions.dunder`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let decorators = enabled.contains(CLASSMETHOD) || enabled.contains(STATICMETHOD);
        let dunder = enabled.contains(DUNDER) && loaded(ctx, "dunder");
        if !decorators && !dunder {
            return;
        }
        Walk {
            ctx,
            enabled,
            dunder,
            out,
        }
        .visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'e> {
    ctx: &'e Ctx<'e>,
    enabled: &'e BTreeSet<String>,
    dunder: bool,
    out: &'e mut Vec<Diagnostic>,
}

impl Walk<'_> {
    /// The assignments of the class scope: the body and what is nested in
    /// its `if`, `try` and the like, but not in a function or class.
    fn calls(&mut self, class: &StmtClassDef, body: &[Stmt]) {
        for stmt in body {
            if let Stmt::Assign(assign) = stmt
                && let Expr::Call(call) = &*assign.value
                && let Expr::Name(callee) = &*call.func
                && let id @ ("classmethod" | "staticmethod") = callee.id.as_str()
                && let Some(Expr::Name(method)) = call.arguments.args.first()
                && let Some(target) = assign.targets.first()
                // astroid's `mymethods` holds the first binding of each name
                && locals(&class.body)
                    .iter()
                    .find(|(name, _)| name == method.id.as_str())
                    .is_some_and(|(_, kind)| *kind == Local::Function)
            {
                let (code, what) = if id == "classmethod" {
                    (CLASSMETHOD, "classmethod")
                } else {
                    (STATICMETHOD, "staticmethod")
                };
                if self.enabled.contains(code) {
                    self.out.push(self.ctx.report(
                        code,
                        target,
                        format!("Consider using a decorator instead of calling {what}"),
                    ));
                }
            }
            for block in blocks(stmt) {
                self.calls(class, block);
            }
        }
    }

    fn good(&self, name: &str) -> bool {
        GOOD.contains(&name)
            || SINCE.iter().any(|(since, names)| {
                self.ctx.py_version.is_none_or(|v| v >= *since) && names.contains(&name)
            })
            || self
                .ctx
                .option("good-dunder-names")
                .is_some_and(|list| list.split(',').any(|good| good.trim() == name))
    }
}

impl<'a> Visitor<'a> for Walk<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            self.calls(class, &class.body);
            if self.dunder {
                // `visit_functiondef` alone: Pylint skips `async def`
                for method in methods(&class.body).into_iter().filter(|m| !m.is_async) {
                    let name = method.name.as_str();
                    if name.starts_with('_') && name.ends_with('_') && !self.good(name) {
                        self.out.push(self.ctx.report_at(
                            DUNDER,
                            method.name.range(),
                            format!("Bad or misspelled dunder method name {name}."),
                        ));
                    }
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn decorator_calls() {
        let source = include_str!("fixtures/decorator_calls.py");
        let expected = include_str!("fixtures/decorator_calls.expected").trim_end();
        assert_eq!(render(source, &[], &["R0202", "R0203"]), expected);
    }

    #[test]
    fn bad_dunder_names() {
        let source = include_str!("fixtures/bad_dunder_names.py");
        let expected = include_str!("fixtures/bad_dunder_names.expected").trim_end();
        let options = [
            ("load-plugins", "pylint.extensions.dunder"),
            ("good-dunder-names", "__ok__, __fine__"),
        ];
        assert_eq!(render(source, &options, &["W3201"]), expected);
        assert_eq!(render(source, &[], &["W3201"]), "");
    }
}
