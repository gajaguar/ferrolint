//! The rules on the bases a class names: `E0240` inconsistent-mro, `W0240`
//! subclassed-final-class and `E0244` invalid-enum-extension. The bases come
//! from [`super::hierarchy`]; one it cannot read gives no verdict, except for
//! `E0240`, which Pylint also decides over the bases it can infer.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;

use super::classes::{Env, bases, blocks, locals, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};

const INCONSISTENT: &str = "E0240";
const FINAL_CLASS: &str = "W0240";
const ENUM_EXTENSION: &str = "E0244";
const IDS: [&str; 3] = [INCONSISTENT, FINAL_CLASS, ENUM_EXTENSION];

/// The classes of `enum` that make a class an enumeration.
const ENUMS: [&str; 6] = [
    "enum.Enum",
    "enum.IntEnum",
    "enum.Flag",
    "enum.IntFlag",
    "enum.StrEnum",
    "enum.ReprEnum",
];

pub struct ClassBases;

impl Checker for ClassBases {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                INCONSISTENT,
                "inconsistent-mro",
                "A class whose bases cannot be merged into a method resolution order.",
            ),
            rule(
                FINAL_CLASS,
                "subclassed-final-class",
                "A class that derives from a class decorated with `typing.final`.",
            ),
            rule(
                ENUM_EXTENSION,
                "invalid-enum-extension",
                "A class that derives from an enumeration that already has members.",
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

impl<'a> Walk<'a, '_> {
    fn class(&mut self, class: &'a StmtClassDef) {
        let own = Class::Local(class);
        let resolved: Vec<Class<'a>> = bases(class)
            .iter()
            .filter_map(|base| {
                // `Base[T]` derives from `Base`.
                let base = match base {
                    Expr::Subscript(s) => &*s.value,
                    other => other,
                };
                self.hierarchy.resolve(&own, base)
            })
            .collect();
        if self.enabled.contains(ENUM_EXTENSION) {
            for base in &resolved {
                if let Some(def) = base.def()
                    && self.hierarchy.module(base) != "enum"
                    && self.is_enum(base)
                    && has_members(&def.body)
                {
                    let message = format!("Extending inherited Enum class \"{}\"", def.name);
                    self.report(ENUM_EXTENSION, class, message);
                }
            }
        }
        if self.enabled.contains(FINAL_CLASS) && self.env.ctx.py_version.is_none_or(|v| v >= (3, 8))
        {
            for base in &resolved {
                if let Some(def) = base.def()
                    && self.hierarchy.is_final(base, &def.decorator_list)
                {
                    let message = format!(
                        "Class {} is a subclass of a class decorated with typing.final: {}",
                        py_repr(&class.name),
                        py_repr(&def.name)
                    );
                    self.report(FINAL_CLASS, class, message);
                }
            }
        }
        if self.enabled.contains(INCONSISTENT) && self.hierarchy.inconsistent(&own) {
            let message = format!(
                "Inconsistent method resolution order for class {}",
                py_repr(&class.name)
            );
            self.report(INCONSISTENT, class, message);
        }
    }

    fn report(&mut self, id: &str, class: &StmtClassDef, message: String) {
        self.out
            .push(self.env.ctx.report_at(id, class.name.range(), message));
    }

    /// Whether `class` derives from `enum.Enum`, by its own bases or by its MRO.
    fn is_enum(&self, class: &Class<'a>) -> bool {
        let named = |name: &str| ENUMS.contains(&name);
        matches!(class, Class::Local(def) if bases(def).iter().any(|b| self.env.base_is(b, &named)))
            || self
                .hierarchy
                .mro(class)
                .is_some_and(|mro| mro.iter().any(|c| c.key() == "enum.Enum"))
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

/// Whether astroid's enum brain finds a member in `body`: a name bound only
/// by plain assignments, the first of them with a value, other than `_ignore_`.
/// It also counts the helpers a body assigns (`_order_ = ...`).
fn has_members(body: &[Stmt]) -> bool {
    // (name, whether the binding is an assignment with a value, a bare
    // annotation or something that is not an assignment at all)
    #[derive(Clone, Copy, PartialEq)]
    enum Bind {
        Value,
        Bare,
        Other,
    }
    fn collect(body: &[Stmt], out: &mut Vec<(String, Bind)>) {
        for stmt in body {
            match stmt {
                Stmt::Assign(assign) => {
                    let mut names = Vec::new();
                    assign
                        .targets
                        .iter()
                        .for_each(|target| super::classes::target_names(target, &mut names));
                    out.extend(names.into_iter().map(|(name, _)| (name, Bind::Value)));
                }
                Stmt::AnnAssign(assign) => {
                    if let Expr::Name(name) = &*assign.target {
                        let bind = if assign.value.is_some() {
                            Bind::Value
                        } else {
                            Bind::Bare
                        };
                        out.push((name.id.to_string(), bind));
                    }
                }
                Stmt::FunctionDef(_)
                | Stmt::ClassDef(_)
                | Stmt::Import(_)
                | Stmt::ImportFrom(_) => {
                    out.extend(
                        locals(std::slice::from_ref(stmt))
                            .into_iter()
                            .map(|(name, _)| (name, Bind::Other)),
                    );
                }
                _ => {}
            }
            for block in blocks(stmt) {
                collect(block, out);
            }
        }
    }
    let mut binds = Vec::new();
    collect(body, &mut binds);
    binds.iter().any(|(name, _)| {
        let own: Vec<Bind> = binds
            .iter()
            .filter(|(other, _)| other == name)
            .map(|(_, bind)| *bind)
            .collect();
        name != "_ignore_" && own[0] == Bind::Value && !own.contains(&Bind::Other)
    })
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn inconsistent_mro() {
        let source = include_str!("fixtures/inconsistent_mro.py");
        let expected = include_str!("fixtures/inconsistent_mro.expected").trim_end();
        assert_eq!(render(source, &[], &["E0240"]), expected);
    }

    #[test]
    fn subclassed_final_class() {
        let source = include_str!("fixtures/subclassed_final_class.py");
        let expected = include_str!("fixtures/subclassed_final_class.expected").trim_end();
        assert_eq!(render(source, &[], &["W0240"]), expected);
    }

    #[test]
    fn invalid_enum_extension() {
        let source = include_str!("fixtures/invalid_enum_extension.py");
        let expected = include_str!("fixtures/invalid_enum_extension.expected").trim_end();
        assert_eq!(render(source, &[], &["E0244"]), expected);
    }
}
