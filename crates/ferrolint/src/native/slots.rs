//! `__slots__` rules: `E0236` invalid-slots-object, `E0238` invalid-slots,
//! `E0242` class-variable-slots-conflict, `E0245` declare-non-slot and
//! `W0244` redefined-slots-in-subclass.
//!
//! shortcut: the slots are read only where `__slots__` is a list, tuple, set
//! or dict written in the class body (the last assignment, like Pylint's
//! lookup) and each item is a literal; a name or a call is not followed.
//! Upgrade with a value inference.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;

use super::classes::{Env, Local, bases, blocks, const_repr, locals, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};

const OBJECT: &str = "E0236";
const INVALID: &str = "E0238";
const CONFLICT: &str = "E0242";
const NON_SLOT: &str = "E0245";
const REDEFINED: &str = "W0244";

pub struct Slots;

impl Checker for Slots {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                OBJECT,
                "invalid-slots-object",
                "An item of `__slots__` that is not a non-empty string.",
            ),
            rule(
                INVALID,
                "invalid-slots",
                "A `__slots__` that is not a string, an iterable or a sequence.",
            ),
            rule(
                CONFLICT,
                "class-variable-slots-conflict",
                "A slot with the name of a class variable, property or method.",
            ),
            rule(
                NON_SLOT,
                "declare-non-slot",
                "An annotated name absent from a class's `__slots__`.",
            ),
            rule(
                REDEFINED,
                "redefined-slots-in-subclass",
                "A slot that an ancestor's `__slots__` already declares.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![OBJECT, INVALID, CONFLICT, NON_SLOT, REDEFINED]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            hierarchy: enabled.contains(REDEFINED).then(|| Hierarchy::new(&env)),
            enabled,
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// The value of the class's last `__slots__ = ...` when every binding of the
/// name is such a statement of the class body.
pub(super) fn slots_value(class: &StmtClassDef) -> Option<&Expr> {
    let all = locals(&class.body)
        .into_iter()
        .filter(|(name, _)| name == "__slots__")
        .count();
    let direct: Vec<&Expr> = class
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Assign(a) if matches!(&a.targets[..], [Expr::Name(n)] if n.id.as_str() == "__slots__") => {
                Some(&*a.value)
            }
            Stmt::AnnAssign(a) if matches!(&*a.target, Expr::Name(n) if n.id.as_str() == "__slots__") => {
                a.value.as_deref()
            }
            _ => None,
        })
        .collect();
    (all == direct.len())
        .then(|| direct.last().copied())
        .flatten()
}

/// Every value assigned to `__slots__` in the class body, also under `if` and
/// `try`, when every binding of the name is such an assignment: astroid infers
/// each of them and joins the slots. Loops are not followed.
pub(super) fn slots_values(class: &StmtClassDef) -> Option<Vec<&Expr>> {
    fn collect<'a>(body: &'a [Stmt], out: &mut Vec<&'a Expr>) {
        for stmt in body {
            match stmt {
                Stmt::Assign(a) if matches!(&a.targets[..], [Expr::Name(n)] if n.id.as_str() == "__slots__") =>
                {
                    out.push(&a.value);
                }
                Stmt::AnnAssign(a) if matches!(&*a.target, Expr::Name(n) if n.id.as_str() == "__slots__") =>
                {
                    out.extend(a.value.as_deref());
                }
                Stmt::For(_) | Stmt::While(_) => {}
                _ => blocks(stmt).into_iter().for_each(|b| collect(b, out)),
            }
        }
    }
    let all = locals(&class.body)
        .into_iter()
        .filter(|(name, _)| name == "__slots__")
        .count();
    let mut found = Vec::new();
    collect(&class.body, &mut found);
    (all == found.len() && all > 0).then_some(found)
}

/// The items of a slots display (a dict's keys), when it is plain.
fn items(value: &Expr) -> Option<Vec<&Expr>> {
    let items: Option<Vec<&Expr>> = match value {
        Expr::List(l) => Some(l.elts.iter().collect()),
        Expr::Tuple(t) => Some(t.elts.iter().collect()),
        Expr::Set(s) => Some(s.elts.iter().collect()),
        Expr::Dict(d) => d.items.iter().map(|item| item.key.as_ref()).collect(),
        _ => None,
    };
    items.filter(|items| items.iter().all(|item| !matches!(item, Expr::Starred(_))))
}

/// The slot names of a display of strings, `None` when any item is not one.
pub(super) fn names(value: &Expr) -> Option<Vec<&str>> {
    items(value)?
        .into_iter()
        .map(|item| match item {
            Expr::StringLiteral(s) => Some(s.value.to_str()),
            _ => None,
        })
        .collect()
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: Option<Hierarchy<'a, 'e>>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    /// The slots of `class` and of every class of its MRO, as astroid's
    /// `slots()` gives them: none when one of those has no plain slots.
    fn all_slots(&self, hierarchy: &Hierarchy<'a, '_>, class: &Class<'a>) -> Option<Vec<String>> {
        let mut all = Vec::new();
        for member in hierarchy.mro(class)? {
            // a builtin base other than `object` has no slots
            if let Class::Local(def) = &member
                && bases(def).iter().any(|base| {
                    self.env
                        .qualified(base)
                        .is_some_and(|q| !q.contains('.') && q != "object")
                })
            {
                return None;
            }
            let slots = names(slots_value(member.def()?)?)?;
            all.extend(slots.into_iter().map(str::to_owned));
        }
        Some(all)
    }

    /// `W0244`: a slot of the class that an ancestor with `__slots__` has.
    fn redefined(&mut self, class: &'a StmtClassDef, value: &Expr) {
        let Some(hierarchy) = &self.hierarchy else {
            return;
        };
        let (Some(items), Some(mro)) = (items(value), hierarchy.mro(&Class::Local(class))) else {
            return;
        };
        let inherited: Vec<String> = mro
            .iter()
            .skip(1)
            .filter(|ancestor| ancestor.def().is_some_and(|d| slots_value(d).is_some()))
            .filter_map(|ancestor| self.all_slots(hierarchy, ancestor))
            .flatten()
            .collect();
        let shown: Vec<String> = items
            .iter()
            .filter_map(|item| match item {
                Expr::StringLiteral(s) => Some(s.value.to_str()),
                _ => None,
            })
            .filter(|name| inherited.iter().any(|slot| slot == name))
            .map(py_repr)
            .collect();
        if !shown.is_empty() {
            self.out.push(self.env.ctx.report(
                REDEFINED,
                value,
                format!("Redefined slots [{}] in subclass", shown.join(", ")),
            ));
        }
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        if let Some(value) = slots_value(class) {
            self.check_slots(class, value);
            if self.on(REDEFINED) {
                self.redefined(class, value);
            }
        }
        if self.on(NON_SLOT) {
            self.declare_non_slot(class);
        }
    }

    fn check_slots(&mut self, class: &StmtClassDef, value: &Expr) {
        let ctx = self.env.ctx;
        // Constants that are not iterable; a string or bytes is one slot.
        let not_iterable = matches!(
            value,
            Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_)
                | Expr::Lambda(_)
        );
        if not_iterable {
            if self.on(INVALID) {
                self.out.push(ctx.report_at(
                    INVALID,
                    class.name.range(),
                    "Invalid __slots__ object",
                ));
            }
            return;
        }
        let Some(items) = items(value) else { return };
        let class_locals = locals(&class.body);
        for item in items {
            match item {
                Expr::StringLiteral(s) if !s.value.to_str().is_empty() => {
                    let name = s.value.to_str();
                    let bound: Vec<Local> = class_locals
                        .iter()
                        .filter(|(local, _)| local == name)
                        .map(|(_, kind)| *kind)
                        .collect();
                    // A bare annotation alone does not conflict with a slot.
                    if self.on(CONFLICT) && !bound.is_empty() && bound != [Local::Bare] {
                        self.out.push(ctx.report(
                            CONFLICT,
                            item,
                            format!(
                                "Value {} in slots conflicts with class variable",
                                py_repr(name)
                            ),
                        ));
                    }
                }
                Expr::StringLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_) => {
                    if self.on(OBJECT)
                        && let Some(shown) = const_repr(item, "...")
                    {
                        self.out.push(ctx.report(
                            OBJECT,
                            item,
                            format!(
                                "Invalid object {} in __slots__, must contain only non empty strings",
                                py_repr(&shown)
                            ),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    /// An annotation without a value that no slot of the class or of its
    /// bases, all of this file, declares.
    fn declare_non_slot(&mut self, class: &StmtClassDef) {
        let Some(mut slots) = slots_value(class).and_then(names) else {
            return;
        };
        if slots.is_empty() || slots.contains(&"__dict__") {
            return;
        }
        for base in bases(class) {
            // A base that is not a class of this file, or has no plain
            // slots, may allow other attributes: Pylint's stop.
            let Some(base) = self.env.class_of(base) else {
                return;
            };
            let Some(inherited) = slots_value(base).and_then(names) else {
                return;
            };
            if inherited.contains(&"__dict__") {
                return;
            }
            slots.extend(inherited);
        }
        for stmt in &class.body {
            if let Stmt::AnnAssign(a) = stmt
                && let Expr::Name(name) = &*a.target
                && a.value.is_none()
                && !slots.contains(&name.id.as_str())
                && !class_var(&a.annotation)
            {
                self.out.push(self.env.ctx.report(
                    NON_SLOT,
                    &*a.target,
                    format!("No such name {} in __slots__", py_repr(name.id.as_str())),
                ));
            }
        }
    }
}

/// Whether the annotation is `ClassVar` or a subscript of it.
fn class_var(annotation: &Expr) -> bool {
    let annotation = match annotation {
        Expr::Subscript(s) => &*s.value,
        other => other,
    };
    match annotation {
        Expr::Name(n) => n.id.as_str() == "ClassVar",
        Expr::Attribute(a) => a.attr.as_str() == "ClassVar",
        _ => false,
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
    use super::super::classes::render;

    #[test]
    fn invalid_slots_objects_and_conflicts() {
        let source = include_str!("fixtures/invalid_slots_objects_and_conflicts.py");
        let expected =
            include_str!("fixtures/invalid_slots_objects_and_conflicts.expected").trim_end();
        assert_eq!(render(source, &[], &["E0236", "E0238", "E0242"]), expected);
    }

    #[test]
    fn redefined_slots_in_subclass() {
        let source = include_str!("fixtures/redefined_slots.py");
        let expected = include_str!("fixtures/redefined_slots.expected").trim_end();
        assert_eq!(render(source, &[], &["W0244"]), expected);
    }

    #[test]
    fn declare_non_slot() {
        let source = include_str!("fixtures/declare_non_slot.py");
        let expected = include_str!("fixtures/declare_non_slot.expected").trim_end();
        assert_eq!(render(source, &[], &["E0245"]), expected);
    }
}
