//! `W0238` unused-private-member and `R0903` too-few-public-methods.
//!
//! W0238 follows Pylint's own reading of a class, name for name: a private
//! function, variable or attribute is used when some name, or some attribute
//! of `self`, `cls` or the class, spells it anywhere under the class, and a
//! variable is used by any attribute of something that is not a name.
//!
//! shortcut: R0903 judges a class only when every base is a class the
//! project can read, a builtin exception or `type`, so a class derived from
//! another builtin or from a class of a module that exists but whose class
//! the project cannot read is never reported (a base from a module the
//! environment lacks is left out, as Pylint does); and `type(self).__x` is
//! the one call W0238 takes for the class, where Pylint infers the callee.
//! Upgrade with inference.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Decorator, ExceptHandler, Expr, ExprContext, Parameters, Pattern, Stmt, StmtClassDef,
    StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};

use super::astring::as_string;
use super::classes::{Env, Local, bases, locals};
use super::hierarchy::{Class, EMPTY_BASES, Hierarchy};
use super::instance_attrs::is_private;
use super::misc::regexps;
use super::{Checker, Ctx, rule};

const UNUSED: &str = "W0238";
const FEW: &str = "R0903";

#[derive(Clone, Copy)]
enum Scope<'a> {
    Class(&'a StmtClassDef),
    Func(&'a StmtFunctionDef),
    /// A lambda or a comprehension.
    Other,
}

/// An attribute read.
struct Read<'a> {
    receiver: Option<&'a str>,
    /// The function the read is written in, by address, when that is the
    /// innermost scope.
    scope: Option<usize>,
    /// `type(self).name`.
    of_type: bool,
}

/// `receiver.attr = ...`.
struct Store<'a> {
    node: &'a Expr,
    attr: &'a str,
    receiver: &'a str,
    /// What a `__new__` that stores it returns by name.
    returned: Vec<&'a str>,
}

struct Private<'a> {
    func: &'a StmtFunctionDef,
    /// The function this one is nested in.
    parent: Option<&'a StmtFunctionDef>,
    /// The scopes between the class and the function.
    path: Vec<&'a str>,
}

#[derive(Default)]
struct Facts<'a> {
    names: HashSet<&'a str>,
    reads: HashMap<&'a str, Vec<Read<'a>>>,
    /// Some attribute is read off something that is not a name.
    chained: bool,
    variables: Vec<(&'a str, TextRange)>,
    stores: Vec<Store<'a>>,
    funcs: Vec<Private<'a>>,
}

struct Collect<'a> {
    facts: Facts<'a>,
    scopes: Vec<Scope<'a>>,
}

fn address(func: &StmtFunctionDef) -> usize {
    std::ptr::from_ref(func).addr()
}

/// The names `return`ed under `func`, nested functions included.
fn returned(func: &StmtFunctionDef) -> Vec<&str> {
    struct Find<'a>(Vec<&'a str>);
    impl<'a> Visitor<'a> for Find<'a> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if let Stmt::Return(r) = stmt
                && let Some(Expr::Name(name)) = r.value.as_deref()
            {
                self.0.push(name.id.as_str());
            }
            visitor::walk_stmt(self, stmt);
        }
    }
    let mut find = Find(Vec::new());
    find.visit_body(&func.body);
    find.0
}

/// The names loaded anywhere in `func`.
fn loaded(func: &StmtFunctionDef) -> HashSet<&str> {
    struct Find<'a>(HashSet<&'a str>);
    impl<'a> Visitor<'a> for Find<'a> {
        fn visit_expr(&mut self, expr: &'a Expr) {
            if let Expr::Name(name) = expr
                && name.ctx == ExprContext::Load
            {
                self.0.insert(name.id.as_str());
            }
            visitor::walk_expr(self, expr);
        }
    }
    let mut find = Find(HashSet::new());
    for decorator in &func.decorator_list {
        find.visit_decorator(decorator);
    }
    let parameters: &Parameters = &func.parameters;
    find.visit_parameters(parameters);
    if let Some(returns) = &func.returns {
        find.visit_expr(returns);
    }
    find.visit_body(&func.body);
    find.0
}

impl<'a> Collect<'a> {
    fn scope_name(scope: Scope<'a>) -> &'a str {
        match scope {
            Scope::Class(class) => class.name.as_str(),
            Scope::Func(func) => func.name.as_str(),
            Scope::Other => "<lambda>",
        }
    }

    fn nested(&mut self, scope: Scope<'a>, walk: impl FnOnce(&mut Self)) {
        self.scopes.push(scope);
        walk(self);
        self.scopes.pop();
    }

    fn variable(&mut self, name: &'a str, range: TextRange) {
        if is_private(name) {
            self.facts.variables.push((name, range));
        }
    }
}

impl<'a> Visitor<'a> for Collect<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                self.nested(Scope::Class(class), |this| visitor::walk_stmt(this, stmt));
            }
            Stmt::FunctionDef(func) => {
                if is_private(func.name.as_str()) {
                    let parent = match self.scopes.last() {
                        Some(Scope::Func(parent)) => Some(*parent),
                        _ => None,
                    };
                    let path = self.scopes[1..]
                        .iter()
                        .map(|scope| Self::scope_name(*scope))
                        .collect();
                    self.facts.funcs.push(Private { func, parent, path });
                }
                self.nested(Scope::Func(func), |this| visitor::walk_stmt(this, stmt));
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match name.ctx {
                ExprContext::Load => {
                    self.facts.names.insert(name.id.as_str());
                }
                ExprContext::Store => self.variable(name.id.as_str(), name.range()),
                _ => {}
            },
            Expr::Attribute(attribute) => {
                let receiver = match &*attribute.value {
                    Expr::Name(name) => Some(name.id.as_str()),
                    _ => None,
                };
                let innermost = match self.scopes.last() {
                    Some(Scope::Func(func)) => Some(*func),
                    _ => None,
                };
                match attribute.ctx {
                    ExprContext::Load => {
                        self.facts.chained |= receiver.is_none();
                        let of_type = matches!(&*attribute.value, Expr::Call(call)
                            if matches!(&*call.func, Expr::Name(f) if f.id.as_str() == "type")
                                && matches!(&call.arguments.args[..], [Expr::Name(a)] if a.id.as_str() == "self"));
                        self.facts
                            .reads
                            .entry(attribute.attr.as_str())
                            .or_default()
                            .push(Read {
                                receiver,
                                scope: innermost.map(address),
                                of_type,
                            });
                    }
                    ExprContext::Store => {
                        if let Some(receiver) = receiver
                            && is_private(attribute.attr.as_str())
                        {
                            let returned = innermost
                                .filter(|func| func.name.as_str() == "__new__")
                                .map_or_else(Vec::new, returned);
                            self.facts.stores.push(Store {
                                node: expr,
                                attr: attribute.attr.as_str(),
                                receiver,
                                returned,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Expr::Lambda(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_) => {
                self.nested(Scope::Other, |this| visitor::walk_expr(this, expr));
                return;
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(h) = handler;
        if let Some(name) = &h.name {
            self.variable(name.as_str(), name.range());
        }
        visitor::walk_except_handler(self, handler);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(p) => p.name.as_ref(),
            Pattern::MatchStar(p) => p.name.as_ref(),
            Pattern::MatchMapping(p) => p.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.variable(name.as_str(), name.range());
        }
        visitor::walk_pattern(self, pattern);
    }
}

pub struct ClassMembers;

impl Checker for ClassMembers {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                UNUSED,
                "unused-private-member",
                "A private function, variable or attribute of a class that nothing in the class uses.",
            ),
            rule(
                FEW,
                "too-few-public-methods",
                "A class with fewer public methods than `min-public-methods`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(UNUSED) && !enabled.contains(FEW) {
            return;
        }
        let env = Env::new(ctx);
        let hierarchy = Hierarchy::new(&env);
        let mut walk = Walk {
            hierarchy: &hierarchy,
            enabled,
            out,
            exclude: regexps(
                ctx.option("exclude-too-few-public-methods")
                    .unwrap_or_default(),
            ),
            module_names: locals(&ctx.parsed.syntax().body)
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e, 'o> {
    hierarchy: &'e Hierarchy<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    out: &'o mut Vec<Diagnostic>,
    exclude: Vec<regex::Regex>,
    /// The names the module binds at its top level.
    module_names: HashSet<String>,
}

impl<'a> Walk<'a, '_, '_> {
    fn ctx(&self) -> &'a Ctx<'a> {
        self.hierarchy.env.ctx
    }

    fn unused(&mut self, stmt: &'a Stmt, class: &'a StmtClassDef) {
        let mut collect = Collect {
            facts: Facts::default(),
            scopes: Vec::new(),
        };
        collect.visit_stmt(stmt);
        let facts = collect.facts;
        let owner = class.name.as_str();
        let ctx = self.ctx();
        let mut report = |range: TextRange, member: String| {
            self.out.push(ctx.report_at(
                UNUSED,
                range,
                format!("Unused private member `{owner}.{member}`"),
            ));
        };
        for private in &facts.funcs {
            let name = private.func.name.as_str();
            if private
                .parent
                .is_some_and(|parent| loaded(parent).contains(name))
                || facts.names.contains(name)
            {
                continue;
            }
            let called = facts.reads.get(name).is_some_and(|reads| {
                reads.iter().any(|read| {
                    read.scope != Some(address(private.func))
                        && (read.of_type
                            || read
                                .receiver
                                .is_some_and(|r| matches!(r, "self" | "cls") || r == owner))
                })
            });
            if called {
                continue;
            }
            let mut shown = private.path.join(".");
            if !shown.is_empty() {
                shown.push('.');
            }
            let _ = write!(
                shown,
                "{name}({})",
                format_parameters(ctx.source, &private.func.parameters)
            );
            report(private.func.name.range(), shown);
        }
        for &(name, range) in &facts.variables {
            let used = facts.names.contains(name)
                || facts.chained
                || facts.reads.get(name).is_some_and(|reads| {
                    reads.iter().any(|read| {
                        read.receiver
                            .is_some_and(|r| matches!(r, "self" | "cls") || r == owner)
                    })
                });
            if !used {
                report(range, name.to_owned());
            }
        }
        for store in &facts.stores {
            let used = facts.reads.get(store.attr).is_some_and(|reads| {
                reads.iter().filter_map(|read| read.receiver).any(|r| {
                    let on_class = store.receiver == "cls" || store.receiver == owner;
                    (on_class && (matches!(r, "cls" | "self") || r == owner))
                        || ((store.receiver == "self" || store.returned.contains(&store.receiver))
                            && r == "self")
                        || (store.receiver == owner && r == owner)
                })
            });
            if !used {
                report(store.node.range(), store.attr.to_owned());
            }
        }
    }

    fn few(&mut self, class: &'a StmtClassDef) {
        let ctx = self.ctx();
        let min = ctx
            .option("min-public-methods")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(2);
        let start = Class::Local(class);
        let mut scan = Scan {
            walk: self,
            exempt: class.name.ends_with("Exception"),
            unknown: false,
            ancestors: Vec::new(),
            seen: HashSet::new(),
        };
        scan.class(&start, 0);
        let (exempt, unknown, ancestors) = (scan.exempt, scan.unknown, scan.ancestors);
        if exempt || unknown || self.dataclass(class) {
            return;
        }
        if ancestors
            .iter()
            .any(|name| self.exclude.iter().any(|re| re.is_match(name)))
        {
            return;
        }
        let Some(mro) = self.hierarchy.known_order(&start) else {
            return;
        };
        let mut done = HashSet::new();
        let mut count = 0;
        for (index, owner) in mro.iter().enumerate() {
            let Some(def) = owner.def() else { return };
            let mut seen = HashSet::new();
            for (name, kind) in locals(&def.body) {
                if !seen.insert(name.clone()) || kind != Local::Function {
                    continue;
                }
                // Special methods count for the class alone, `__init__` never.
                let special = name.len() > 4
                    && name.starts_with("__")
                    && name.ends_with("__")
                    && name[2..name.len() - 2]
                        .chars()
                        .all(|c| c.is_ascii_lowercase())
                    && name != "__init__";
                if index == 0 && special {
                    count += 1;
                }
                if done.insert(name.clone()) && !name.starts_with('_') {
                    count += 1;
                }
            }
        }
        if count < min {
            self.out.push(ctx.report_at(
                FEW,
                class.name.range(),
                format!("Too few public methods ({count}/{min})"),
            ));
        }
    }

    /// Pylint's `_is_exempt_from_public_methods` for a dataclass or an
    /// `attrs` class, read from the decorator and the imports of the module.
    fn dataclass(&self, class: &StmtClassDef) -> bool {
        let bound = |names: &[&str]| names.iter().any(|name| self.module_names.contains(*name));
        class
            .decorator_list
            .iter()
            .any(|Decorator { expression, .. }| {
                let expression = match expression {
                    Expr::Call(call) => &*call.func,
                    other => other,
                };
                let name = match expression {
                    Expr::Name(name) => name.id.as_str(),
                    Expr::Attribute(attribute) => attribute.attr.as_str(),
                    _ => return false,
                };
                (matches!(name, "dataclass" | "attrs")
                    && bound(&["dataclass", "attrs", "dataclasses"]))
                    || (matches!(name, "define" | "frozen")
                        && bound(&["define", "frozen", "attrs"]))
            })
    }
}

/// What the bases of a class say about it: an exception, a metaclass, an
/// enum, a named tuple or a typed dict is not judged, and a base the project
/// cannot read leaves the methods uncounted.
struct Scan<'w, 'a, 'e, 'o> {
    walk: &'w Walk<'a, 'e, 'o>,
    exempt: bool,
    unknown: bool,
    /// The qualified names of the ancestors, for `exclude-too-few-public-methods`.
    ancestors: Vec<String>,
    seen: HashSet<String>,
}

fn is_exception(name: &str) -> bool {
    !name.contains('.')
        && (name.ends_with("Error")
            || name.ends_with("Exception")
            || name.ends_with("Warning")
            || name.ends_with("Group")
            || matches!(
                name,
                "KeyboardInterrupt"
                    | "SystemExit"
                    | "StopIteration"
                    | "StopAsyncIteration"
                    | "GeneratorExit"
            ))
}

impl<'a> Scan<'_, 'a, '_, '_> {
    /// Whether the name that `base` starts with has one binding in a class of this file.
    fn bound_once(&self, class: &Class<'a>, base: &Expr) -> bool {
        let mut head = base;
        while let Expr::Attribute(attribute) = head {
            head = &attribute.value;
        }
        let env = self.walk.hierarchy.env;
        match (class, head) {
            (Class::Local(_), Expr::Name(name)) => env
                .binding(name)
                .is_some_and(|binding| env.ctx.model.is_sole_binding(binding.id)),
            // shortcut: a remote file's rebinding is not checked, upgrade with its own bindings
            (Class::Remote(_), _) => true,
            _ => false,
        }
    }

    fn class(&mut self, class: &Class<'a>, depth: u8) {
        let hierarchy = self.walk.hierarchy;
        let Some(def) = class.def() else {
            self.unknown = true;
            return;
        };
        if depth > 16 || !self.seen.insert(class.key()) {
            return;
        }
        for base in bases(def) {
            let base = match base {
                Expr::Subscript(s) => &*s.value,
                other => other,
            };
            let qualified = hierarchy.qualify(class, base);
            if let Some(name) = &qualified
                && (name.starts_with("enum.")
                    || matches!(
                        name.as_str(),
                        "typing.NamedTuple" | "typing.TypedDict" | "typing_extensions.TypedDict"
                    ))
            {
                self.exempt = true;
            }
            if let Some(parent) = hierarchy.resolve(class, base) {
                let name = match &parent {
                    Class::Local(def) => format!("{}.{}", hierarchy.module(&parent), def.name),
                    Class::Remote(_) => parent.key(),
                };
                self.exempt |= parent.def().is_some_and(|d| d.name.ends_with("Exception"));
                self.ancestors.push(name);
                self.class(&parent, depth + 1);
                continue;
            }
            match qualified.as_deref() {
                Some(name) if EMPTY_BASES.contains(&name) => {
                    self.ancestors.push(if name == "object" {
                        "builtins.object".to_owned()
                    } else {
                        name.to_owned()
                    });
                }
                // a C class whose methods are all private, like `types.ModuleType`
                Some(name)
                    if crate::class_methods::methods(name)
                        .is_some_and(|all| all.iter().all(|m| m.starts_with('_'))) =>
                {
                    self.ancestors.push(name.to_owned());
                }
                Some(name) if is_exception(name) || name == "type" => self.exempt = true,
                // astroid cannot import the module, so Pylint counts the class without the
                // base; a name bound twice may stand for another import there.
                Some(name)
                    if name
                        .rsplit_once('.')
                        .is_some_and(|(module, _)| hierarchy.project.is_missing(module))
                        && self.bound_once(class, base) => {}
                // a name only a missing module's star import could bind, or a parameter, is lost the same way
                _ if matches!(class, Class::Local(_))
                    && (hierarchy.lost(base) || hierarchy.dropped(base)) => {}
                // an enum, named tuple or typed dict is exempt already
                _ => self.unknown = true,
            }
        }
        if bases(def).is_empty() {
            self.ancestors.push("builtins.object".to_owned());
        }
    }
}

/// `Arguments.format_args` of astroid: annotations and defaults spelled the
/// way `as_string` does.
fn format_parameters(source: &str, parameters: &Parameters) -> String {
    let one = |p: &ruff_python_ast::ParameterWithDefault| {
        let mut text = p.parameter.name.to_string();
        if let Some(annotation) = &p.parameter.annotation {
            let _ = write!(text, ": {}", as_string(source, annotation));
        }
        if let Some(default) = &p.default {
            let separator = if p.parameter.annotation.is_some() {
                " = "
            } else {
                "="
            };
            let _ = write!(text, "{separator}{}", as_string(source, default));
        }
        text
    };
    let mut parts = Vec::new();
    if !parameters.posonlyargs.is_empty() {
        parts.push(
            parameters
                .posonlyargs
                .iter()
                .map(one)
                .collect::<Vec<_>>()
                .join(", "),
        );
        parts.push("/".to_owned());
    }
    if !parameters.args.is_empty() {
        parts.push(
            parameters
                .args
                .iter()
                .map(one)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if let Some(vararg) = &parameters.vararg {
        parts.push(format!("*{}", vararg.name));
    }
    if !parameters.kwonlyargs.is_empty() {
        if parameters.vararg.is_none() {
            parts.push("*".to_owned());
        }
        parts.push(
            parameters
                .kwonlyargs
                .iter()
                .map(one)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if let Some(kwarg) = &parameters.kwarg {
        parts.push(format!("**{}", kwarg.name));
    }
    parts.join(", ")
}

impl<'a> Visitor<'a> for Walk<'a, '_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            if self.enabled.contains(UNUSED) {
                self.unused(stmt, class);
            }
            if self.enabled.contains(FEW) {
                self.few(class);
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn unused_private_member() {
        let source = include_str!("fixtures/unused_private_member.py");
        let expected = include_str!("fixtures/unused_private_member.expected").trim_end();
        assert_eq!(render(source, &[], &["W0238"]), expected);
    }

    #[test]
    fn too_few_public_methods() {
        let source = include_str!("fixtures/too_few_public_methods.py");
        let expected = include_str!("fixtures/too_few_public_methods.expected").trim_end();
        assert_eq!(render(source, &[], &["R0903"]), expected);
        assert_eq!(
            render(
                source,
                &[
                    ("min-public-methods", "1"),
                    ("exclude-too-few-public-methods", "mod\\.Excluded")
                ],
                &["R0903"]
            ),
            include_str!("fixtures/too_few_public_methods_options.expected").trim_end()
        );
    }
}
