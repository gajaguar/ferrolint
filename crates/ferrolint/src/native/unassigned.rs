//! `E0606` possibly-used-before-assignment: a name read where every
//! definition of it sits in an `if` branch that may not run.
//!
//! Pylint filters a name's definitions as it meets the first read of it
//! in a scope, and reports the read when none is left. This port runs the
//! filter of the `if` tests and of the `except` names, and reports the
//! read when the `if` filter decided it; the filters of `try` blocks, which
//! give `E0601`, are not run.
//!
//! shortcut: an `if` test is constant when it is a literal, `TYPE_CHECKING`
//! or a name bound once to a literal; Pylint infers any test. A read in a
//! comprehension is not checked, like Pylint's.

use std::collections::{HashMap, HashSet};

use ferrolint_core::Diagnostic;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ExceptHandlerExceptHandler, Expr, ExprCall, MatchCase, Number, Stmt, StmtIf, UnaryOp,
};
use ruff_text_size::Ranged;

use super::Ctx;
use super::classes::{Env, py_repr};
use super::scopes::{Def, DefKind, Info, Kind, Step, Use, UseKind};

pub(super) const POSSIBLY: &str = "E0606";

/// The read of a name that `E0606` reports, scope by scope.
pub(super) fn possibly_used(ctx: &Ctx<'_>, info: &Info<'_>, out: &mut Vec<Diagnostic>) {
    let postponed = ctx.py_version.is_some_and(|v| v >= (3, 14))
        || ctx.parsed.syntax().body.iter().any(|stmt| {
            matches!(stmt, Stmt::ImportFrom(i) if i.level == 0
                    && i.module.as_ref().is_some_and(|m| m.as_str() == "__future__")
                    && i.names.iter().any(|a| a.name.as_str() == "annotations"))
        });
    let env = Env::new(ctx);
    // The first read of each name in each scope it resolves to.
    let mut uses: Vec<&Use<'_>> = info.uses.iter().filter(|u| !u.self_assign).collect();
    uses.sort_by_key(|u| u.range.start());
    let mut firsts: HashMap<(usize, &str), &Use<'_>> = HashMap::new();
    for usage in uses {
        if let Some(scope) = info.lookup(usage.scope, usage.name) {
            firsts.entry((scope, usage.name)).or_insert(usage);
        }
    }
    for (id, scope) in info.scopes.iter().enumerate() {
        if !matches!(scope.kind, Kind::Module | Kind::Function | Kind::Class) {
            continue;
        }
        let mut seen = HashSet::new();
        for def in &scope.defs {
            if !seen.insert(def.name) || scope.globals.contains(def.name) {
                continue;
            }
            let Some(first) = firsts.get(&(id, def.name)) else {
                continue;
            };
            if first.in_comp {
                continue;
            }
            if (postponed && first.annotation)
                || ruff_python_stdlib::builtins::python_builtins(14, false).any(|b| b == first.name)
            {
                continue;
            }
            if all_filtered(&env, ctx, info, id, first) {
                out.push(ctx.report_at(
                    POSSIBLY,
                    first.range,
                    format!(
                        "Possibly using variable {} before assignment",
                        py_repr(first.name)
                    ),
                ));
            }
        }
    }
}

/// Whether the filters leave no definition of the use's name, and an `if`
/// with exactly one defining branch decided it.
fn all_filtered(
    env: &Env<'_>,
    ctx: &Ctx<'_>,
    info: &Info<'_>,
    scope: usize,
    usage: &Use<'_>,
) -> bool {
    let name = usage.name;
    let defs: Vec<&Def<'_>> = info.scopes[scope]
        .defs
        .iter()
        .filter(|d| d.name == name)
        .collect();
    let mut flagged = false;
    // A `del` of the name is a definition that no branch makes uncertain.
    let mut left = usize::from(
        info.uses
            .iter()
            .any(|u| u.kind == UseKind::Del && u.name == name && info.frame_of(u.scope) == scope),
    );
    for def in defs {
        // An `except` name belongs to its handler.
        if def.kind == DefKind::ExceptName
            && !def.handler.is_some_and(|h| h.contains_range(usage.range))
        {
            continue;
        }
        if !uncertain(env, ctx, info, scope, def, usage, &mut flagged) {
            left += 1;
        }
    }
    left == 0 && flagged
}

/// One `if` statement seen from astroid: `if` and each `elif` is a node
/// nested in the `orelse` of the one before.
#[derive(Clone, Copy)]
struct IfView<'a> {
    stmt: &'a StmtIf,
    clause: usize,
}

impl<'a> IfView<'a> {
    /// How many clauses with a test the statement has.
    fn tests(stmt: &StmtIf) -> usize {
        1 + stmt
            .elif_else_clauses
            .iter()
            .filter(|c| c.test.is_some())
            .count()
    }

    fn test(&self) -> &'a Expr {
        if self.clause == 0 {
            &self.stmt.test
        } else {
            self.stmt.elif_else_clauses[self.clause - 1]
                .test
                .as_ref()
                .unwrap_or(&self.stmt.test)
        }
    }

    fn body(&self) -> Vec<A<'a>> {
        let body = if self.clause == 0 {
            &self.stmt.body
        } else {
            &self.stmt.elif_else_clauses[self.clause - 1].body
        };
        convert(body)
    }

    fn orelse(&self) -> Vec<A<'a>> {
        match self.stmt.elif_else_clauses.get(self.clause) {
            None => Vec::new(),
            Some(clause) if clause.test.is_some() => {
                vec![A::If(IfView {
                    stmt: self.stmt,
                    clause: self.clause + 1,
                })]
            }
            Some(clause) => convert(&clause.body),
        }
    }
}

/// A node of astroid's tree, for the statements the filter walks.
#[derive(Clone, Copy)]
enum A<'a> {
    Stmt(&'a Stmt),
    If(IfView<'a>),
    Handler(&'a ExceptHandlerExceptHandler),
    Case(&'a MatchCase),
}

fn convert(body: &[Stmt]) -> Vec<A<'_>> {
    body.iter()
        .map(|stmt| match stmt {
            Stmt::If(i) => A::If(IfView { stmt: i, clause: 0 }),
            other => A::Stmt(other),
        })
        .collect()
}

/// A call that ends the program: Pylint's `is_terminating_func`, for the
/// names it knows without inference: `exit`, `quit`, `sys.exit`,
/// `os._exit`, `assert_never` and a function of the file annotated
/// `NoReturn` or `Never`.
fn terminating(env: &Env<'_>, call: &ExprCall) -> bool {
    if let Some(name) = env.qualified(&call.func)
        && matches!(
            name.as_str(),
            "exit" | "quit" | "sys.exit" | "os._exit" | "typing.assert_never"
        )
    {
        return true;
    }
    match &*call.func {
        Expr::Name(n) if n.id.as_str() == "assert_never" => true,
        Expr::Name(n) => env.func_of(n).is_some_and(|f| {
            !f.is_async
                && f.returns.as_deref().is_some_and(|r| {
                    env.qualified(r).is_some_and(|q| {
                        matches!(
                            q.as_str(),
                            "typing.NoReturn"
                                | "typing.Never"
                                | "typing_extensions.NoReturn"
                                | "typing_extensions.Never"
                        )
                    })
                })
        }),
        _ => false,
    }
}

/// Whether `target` binds `name` (a tuple or starred target looked through).
fn binds(target: &Expr, name: &str) -> bool {
    match target {
        Expr::Name(n) => n.id.as_str() == name,
        Expr::Tuple(t) => t.elts.iter().any(|e| binds(e, name)),
        Expr::List(l) => l.elts.iter().any(|e| binds(e, name)),
        Expr::Starred(s) => binds(&s.value, name),
        _ => false,
    }
}

struct HasWalrus<'a>(&'a str, bool);

impl<'a> Visitor<'a> for HasWalrus<'_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Named(n) = expr
            && matches!(&*n.target, Expr::Name(t) if t.id.as_str() == self.0)
        {
            self.1 = true;
        }
        visitor::walk_expr(self, expr);
    }
}

/// Pylint writes `nodes_of_class(Break, Continue)`, whose second argument
/// is the class to skip: only a `break` makes an `if` permissive.
struct Jumps(bool);

impl<'a> Visitor<'a> for Jumps {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if matches!(stmt, Stmt::Break(_)) {
            self.0 = true;
        }
        visitor::walk_stmt(self, stmt);
    }
}

fn walrus_in(view: IfView<'_>, name: &str) -> bool {
    let mut find = HasWalrus(name, false);
    let stmt = view.stmt;
    for clause in view.clause..=stmt.elif_else_clauses.len() {
        if clause == 0 {
            find.visit_expr(&stmt.test);
            find.visit_body(&stmt.body);
        } else {
            let c = &stmt.elif_else_clauses[clause - 1];
            if let Some(test) = &c.test {
                find.visit_expr(test);
            }
            find.visit_body(&c.body);
        }
    }
    find.1
}

fn jumps_in(view: IfView<'_>) -> bool {
    let mut find = Jumps(false);
    let stmt = view.stmt;
    for clause in view.clause..=stmt.elif_else_clauses.len() {
        if clause == 0 {
            find.visit_body(&stmt.body);
        } else {
            find.visit_body(&stmt.elif_else_clauses[clause - 1].body);
        }
    }
    find.0
}

/// Pylint's `_defines_name_raises_or_returns`.
fn defines(env: &Env<'_>, name: &str, node: A<'_>) -> bool {
    match node {
        A::If(view) => walrus_in(view, name),
        A::Handler(h) => h.name.as_ref().is_some_and(|n| n.as_str() == name),
        A::Case(_) => false,
        A::Stmt(stmt) => match stmt {
            Stmt::Raise(_) | Stmt::Assert(_) | Stmt::Return(_) | Stmt::Continue(_) => true,
            Stmt::Expr(e) => matches!(&*e.value, Expr::Call(c) if terminating(env, c)),
            Stmt::AnnAssign(a) => {
                a.value.is_some() && matches!(&*a.target, Expr::Name(n) if n.id.as_str() == name)
            }
            Stmt::Assign(a) => a.targets.iter().any(|t| binds(t, name)),
            Stmt::Import(i) => i.names.iter().any(|alias| {
                alias.asname.as_ref().is_some_and(|a| a.as_str() == name)
                    || alias.name.as_str() == name
                    || alias.name.as_str().starts_with(&format!("{name}."))
            }),
            Stmt::ImportFrom(i) => i.names.iter().any(|alias| {
                alias.asname.as_ref().is_some_and(|a| a.as_str() == name)
                    || alias.name.as_str() == name
                    || alias.name.as_str().starts_with(&format!("{name}."))
            }),
            Stmt::With(w) => w.items.iter().any(|item| {
                matches!(item.optional_vars.as_deref(), Some(Expr::Name(n)) if n.id.as_str() == name)
            }),
            Stmt::ClassDef(c) => c.name.as_str() == name,
            Stmt::FunctionDef(f) => f.name.as_str() == name,
            _ => false,
        },
    }
}

/// The statement children of a node, as astroid's `get_children` has them.
fn children(node: A<'_>) -> Vec<A<'_>> {
    match node {
        A::If(view) => view.body().into_iter().chain(view.orelse()).collect(),
        A::Handler(h) => convert(&h.body),
        A::Case(c) => convert(&c.body),
        A::Stmt(stmt) => match stmt {
            Stmt::Try(t) => {
                let handlers = t.handlers.iter().map(|h| {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(h) = h;
                    A::Handler(h)
                });
                convert(&t.body)
                    .into_iter()
                    .chain(handlers)
                    .chain(convert(&t.orelse))
                    .chain(convert(&t.finalbody))
                    .collect()
            }
            Stmt::With(w) => convert(&w.body),
            Stmt::For(f) => convert(&f.body)
                .into_iter()
                .chain(convert(&f.orelse))
                .collect(),
            Stmt::While(w) => convert(&w.body)
                .into_iter()
                .chain(convert(&w.orelse))
                .collect(),
            Stmt::Match(m) => m.cases.iter().map(A::Case).collect(),
            _ => Vec::new(),
        },
    }
}

/// Pylint's `_defines_name_raises_or_returns_recursive`.
fn recursive(env: &Env<'_>, name: &str, node: A<'_>) -> bool {
    for child in children(node) {
        if defines(env, name, child) {
            return true;
        }
        match child {
            A::If(_) => {
                if children(child)
                    .into_iter()
                    .any(|nested| defines(env, name, nested))
                {
                    return true;
                }
            }
            A::Stmt(Stmt::With(_)) => {
                if children(child)
                    .into_iter()
                    .any(|nested| defines(env, name, nested))
                {
                    return true;
                }
            }
            A::Stmt(Stmt::Try(t)) if t.finalbody.is_empty() && recursive(env, name, child) => {
                return true;
            }
            A::Stmt(Stmt::Match(_)) => {
                return children(child)
                    .into_iter()
                    .all(|case| recursive(env, name, case));
            }
            _ => {}
        }
    }
    false
}

/// The truth of an `if` test Pylint infers as a constant.
fn truthiness(info: &Info<'_>, scope: usize, test: &Expr, depth: u8) -> Option<bool> {
    match test {
        Expr::BooleanLiteral(b) => Some(b.value),
        Expr::NoneLiteral(_) => Some(false),
        Expr::EllipsisLiteral(_) => Some(true),
        Expr::StringLiteral(s) => Some(!s.value.to_str().is_empty()),
        Expr::BytesLiteral(b) => Some(b.value.bytes().next().is_some()),
        Expr::NumberLiteral(n) => Some(match &n.value {
            Number::Int(i) => i.as_u64() != Some(0),
            Number::Float(f) => *f != 0.0,
            Number::Complex { real, imag } => *real != 0.0 || *imag != 0.0,
        }),
        Expr::UnaryOp(u) if u.op == UnaryOp::Not => {
            truthiness(info, scope, &u.operand, depth).map(|v| !v)
        }
        Expr::Name(n) if n.id.as_str() == "TYPE_CHECKING" => Some(false),
        Expr::Attribute(a) if a.attr.as_str() == "TYPE_CHECKING" => Some(false),
        Expr::Name(n) if depth == 0 => {
            let found = info.lookup(scope, n.id.as_str())?;
            let defs: Vec<&Def<'_>> = info.scopes[found]
                .defs
                .iter()
                .filter(|d| d.name == n.id.as_str())
                .collect();
            match &defs[..] {
                [only] => truthiness(info, scope, only.value?, depth + 1),
                _ => None,
            }
        }
        _ => None,
    }
}

struct Filter<'i, 'a> {
    env: &'i Env<'a>,
    info: &'i Info<'a>,
    scope: usize,
    name: &'i str,
    flagged: bool,
}

impl<'a> Filter<'_, 'a> {
    fn branch_handles(&mut self, body: &[A<'a>]) -> bool {
        for item in body {
            if defines(self.env, self.name, *item) {
                return true;
            }
            let compound = matches!(
                item,
                A::If(_)
                    | A::Stmt(
                        Stmt::Try(_)
                            | Stmt::With(_)
                            | Stmt::For(_)
                            | Stmt::While(_)
                            | Stmt::Match(_)
                    )
            );
            if compound && self.inferred(*item) {
                return true;
            }
        }
        false
    }

    /// Pylint's `_inferred_to_define_name_raise_or_return`.
    fn inferred(&mut self, node: A<'a>) -> bool {
        match node {
            A::If(view) => self.if_node(view),
            A::Stmt(Stmt::Try(t)) => {
                recursive(self.env, self.name, node)
                    || t.handlers.iter().all(|h| {
                        let ruff_python_ast::ExceptHandler::ExceptHandler(h) = h;
                        recursive(self.env, self.name, A::Handler(h))
                    })
            }
            A::Stmt(Stmt::With(_) | Stmt::For(_) | Stmt::While(_)) => {
                recursive(self.env, self.name, node)
            }
            A::Stmt(Stmt::Match(m)) => m
                .cases
                .iter()
                .all(|c| recursive(self.env, self.name, A::Case(c))),
            _ => false,
        }
    }

    fn if_node(&mut self, view: IfView<'a>) -> bool {
        // Be permissive if there is a break or a continue.
        if jumps_in(view) || defines(self.env, self.name, A::If(view)) {
            return true;
        }
        let test = match view.test() {
            Expr::Named(n) => &*n.value,
            other => other,
        };
        let always_false = truthiness(self.info, self.scope, test, 0) == Some(false);
        if always_false {
            return self.branch_handles(&view.orelse());
        }
        let if_branch = self.branch_handles(&view.body());
        let else_branch = self.branch_handles(&view.orelse());
        if if_branch && else_branch {
            self.flagged = false;
        } else if if_branch ^ else_branch {
            self.flagged = true;
        }
        if_branch && else_branch
    }
}

/// A call whose inference Pylint gives up on, which then reads as the same
/// as any other such call: builtin functions of an unknown argument.
/// shortcut: the list is what Pylint 4 does with each builtin function of
/// an unknown argument; upgrade with a call inference.
fn fails_inference(info: &Info<'_>, scope: usize, test: &Expr) -> bool {
    const FUNCTIONS: &[&str] = &[
        "all",
        "any",
        "isinstance",
        "issubclass",
        "len",
        "id",
        "hash",
        "repr",
        "str",
        "sorted",
        "sum",
        "min",
        "max",
        "abs",
        "round",
        "ord",
        "chr",
        "vars",
        "dir",
        "iter",
        "next",
        "open",
        "print",
        "input",
        "format",
        "ascii",
        "bin",
        "hex",
        "oct",
        "divmod",
        "pow",
    ];
    matches!(test, Expr::Call(call) if matches!(&*call.func, Expr::Name(f)
        if FUNCTIONS.contains(&f.id.as_str()) && info.lookup(scope, f.id.as_str()).is_none()))
}

/// The text of a test as `as_string` would compare it: no spaces.
fn squeezed(ctx: &Ctx<'_>, range: ruff_text_size::TextRange) -> String {
    ctx.source[range]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Pylint's `_uncertain_nodes_if_tests` for one definition.
fn uncertain(
    env: &Env<'_>,
    ctx: &Ctx<'_>,
    info: &Info<'_>,
    scope: usize,
    def: &Def<'_>,
    usage: &Use<'_>,
    flagged: &mut bool,
) -> bool {
    // The `if` nodes around the definition that do not hold the read,
    // innermost first.
    let mut around: Vec<IfView<'_>> = Vec::new();
    for step in def.path.iter().rev() {
        let Stmt::If(stmt) = step.stmt else { continue };
        let tests = IfView::tests(stmt);
        let top = step.slot.min(tests - 1);
        let inside = usage
            .path
            .iter()
            .find(|s: &&Step<'_>| std::ptr::eq(s.stmt, step.stmt))
            .map(|s| s.slot.min(tests - 1));
        let floor = inside.map_or(0, |u| u + 1);
        around.extend((floor..=top).rev().map(|clause| IfView { stmt, clause }));
    }
    let (Some(closest), Some(outer)) = (around.first(), around.last()) else {
        return false;
    };
    // An augmented assignment in another frame than the `if` is not held.
    if usage.kind == UseKind::Aug && info.frame_of(usage.scope) != def.frame {
        return false;
    }
    let _ = closest;
    // A read under an `if` with the same test is as certain.
    let wanted = match outer.test() {
        Expr::Named(n) => squeezed(ctx, n.target.range()),
        other => squeezed(ctx, other.range()),
    };
    let outer_fails = fails_inference(info, scope, outer.test());
    let outer_truth = truthiness(info, scope, outer.test(), 1);
    if usage.tests.iter().any(|t| {
        squeezed(ctx, t.range()) == wanted
            // Tests Pylint infers alike: a name never counts.
            || (!matches!(t, Expr::Name(_))
                && ((outer_fails && fails_inference(info, scope, t))
                    || (outer_truth.is_some() && truthiness(info, scope, t, 1) == outer_truth)))
    }) {
        return false;
    }
    let mut filter = Filter {
        env,
        info,
        scope,
        name: usage.name,
        flagged: *flagged,
    };
    let defined = filter.inferred(A::If(*outer));
    *flagged = filter.flagged;
    !defined
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    #[test]
    fn possibly_used_before_assignment() {
        let source = include_str!("fixtures/possibly_used_before_assignment.py");
        let expected = include_str!("fixtures/possibly_used_before_assignment.expected").trim_end();
        assert_eq!(render(source, &[], &["E0606"]), expected);
    }
}
