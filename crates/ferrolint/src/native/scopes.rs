//! Variable and scope rules: `W0601` global-variable-undefined, `W0621`
//! redefined-outer-name, `W0631` undefined-loop-variable and `W0641`
//! possibly-unused-variable. The collector also serves `E0606`, in
//! `unassigned.rs`. Ruff has `W0602`, `W0604` and `W0642`, so only the
//! first one's logic is here, to tell `W0601` from it.
//!
//! shortcut: a scope's names are read from the syntax, like astroid's
//! locals, but a value is followed only through a name bound once to a
//! literal display (`W0631`'s loop over a list). `W0631` knows a loop over
//! `range`, `enumerate` and non-empty displays; Pylint infers any
//! iterable. The `global` rules ignore a module attribute such as
//! `__name__`. Upgrade with a value inference.

use std::collections::{BTreeSet, HashMap, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    AnyParameterRef, Comprehension, ElifElseClause, ExceptHandler, Expr, ExprContext, Pattern,
    Stmt, StmtFunctionDef, StmtIf, StmtTry,
};
use ruff_text_size::{Ranged, TextRange};

use super::classes::py_repr;
use super::misc::py_match;
use super::project::Project;
use super::unassigned;
use super::{Checker, Ctx, rule};

const UNDEFINED: &str = "W0601";
const REDEFINED: &str = "W0621";
const LOOP_VARIABLE: &str = "W0631";
const POSSIBLY_UNUSED: &str = "W0641";

pub struct Scopes;

impl Checker for Scopes {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                UNDEFINED,
                "global-variable-undefined",
                "A `global` name that the module never defines.",
            ),
            rule(
                unassigned::POSSIBLY,
                "possibly-used-before-assignment",
                "A name read after definitions that only some branches run.",
            ),
            rule(
                REDEFINED,
                "redefined-outer-name",
                "A name that hides a module-level name or an outer `except` name.",
            ),
            rule(
                LOOP_VARIABLE,
                "undefined-loop-variable",
                "A loop variable used after a loop that may not run.",
            ),
            rule(
                POSSIBLY_UNUSED,
                "possibly-unused-variable",
                "An unused variable of a function that calls `locals()` later.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ids = [
            UNDEFINED,
            REDEFINED,
            LOOP_VARIABLE,
            POSSIBLY_UNUSED,
            unassigned::POSSIBLY,
        ];
        if !ids.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let info = Info::collect(ctx);
        let on = |id: &str| enabled.contains(id);
        if on(UNDEFINED) {
            globals(ctx, &info, out);
        }
        if on(unassigned::POSSIBLY) {
            unassigned::possibly_used(ctx, &info, out);
        }
        if on(REDEFINED) {
            redefined(ctx, &info, out);
        }
        if on(LOOP_VARIABLE) {
            loop_variables(ctx, &info, out);
        }
        if on(POSSIBLY_UNUSED) {
            possibly_unused(ctx, &info, out);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Module,
    Function,
    Class,
    Lambda,
    Comp,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DefKind {
    Param,
    /// An assignment, augmented assignment, `with` or `match` name.
    Plain,
    ForTarget,
    ExceptName,
    Import,
    FromImport,
    Func,
    Class,
}

/// A compound statement on the way to a node and the branch the way takes:
/// the clause of an `if` (`elif` and `else` count in order), the slot of a
/// `try`.
#[derive(Clone, Copy)]
pub(super) struct Step<'a> {
    pub stmt: &'a Stmt,
    pub slot: usize,
}

/// Whether two nodes sit in branches of an `if` or a `try` that never both
/// run: astroid's `are_exclusive`, from their paths.
pub(super) fn exclusive(a: &[Step<'_>], b: &[Step<'_>]) -> bool {
    let Some(at) = a
        .iter()
        .zip(b)
        .position(|(x, y)| !(std::ptr::eq(x.stmt, y.stmt) && x.slot == y.slot))
    else {
        return false;
    };
    let (x, y) = (a[at], b[at]);
    if !std::ptr::eq(x.stmt, y.stmt) {
        return false;
    }
    match x.stmt {
        Stmt::If(_) => true,
        Stmt::Try(_) => {
            // A handler excludes the body, the `else` block and each other
            // handler; the `finally` block excludes nothing.
            let handler = |slot: usize| (1..1000).contains(&slot);
            (handler(x.slot) || handler(y.slot)) && x.slot != 1001 && y.slot != 1001
        }
        _ => false,
    }
}

pub(super) struct Def<'a> {
    pub name: &'a str,
    pub kind: DefKind,
    /// Where Pylint reports it: the name, or the whole import statement.
    pub range: TextRange,
    /// The name as an unused import prints it.
    pub shown: &'a str,
    /// The start line of the statement that binds it.
    pub line: u32,
    /// The frame (module, function, class or lambda) the binding is in.
    pub frame: usize,
    /// Written directly in the module body, for a function or class.
    pub direct: bool,
    pub type_checking: bool,
    pub future: bool,
    pub path: Vec<Step<'a>>,
    /// The value of a plain `name = value`.
    pub value: Option<&'a Expr>,
    /// The `except` clause that binds it.
    pub handler: Option<TextRange>,
    /// The loop that binds it.
    pub for_idx: Option<usize>,
}

pub(super) struct Scope<'a> {
    pub kind: Kind,
    pub parent: Option<usize>,
    pub defs: Vec<Def<'a>>,
    /// The names of `defs`.
    pub names: HashSet<&'a str>,
    pub globals: HashSet<&'a str>,
    pub nonlocals: HashSet<&'a str>,
    pub func: Option<&'a StmtFunctionDef>,
    /// The lines of the `locals()` calls of a function.
    pub locals_calls: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UseKind {
    Load,
    Aug,
    Del,
}

pub(super) struct Use<'a> {
    pub name: &'a str,
    pub range: TextRange,
    pub scope: usize,
    pub kind: UseKind,
    /// The start line of the statement around the use.
    pub line: u32,
    pub stmt: TextRange,
    pub path: Vec<Step<'a>>,
    /// The tests of the `if` statements and expressions around it.
    pub tests: Vec<&'a Expr>,
    pub annotation: bool,
    /// A comprehension of its frame holds the use, its first iterable too.
    pub in_comp: bool,
    /// The use is the whole value of `name = value` for this very name.
    pub self_assign: bool,
}

/// A statement that binds a name: what `W0602` looks for after a `global`.
pub(super) struct Bind<'a> {
    pub name: &'a str,
    pub line: u32,
    pub scope: usize,
}

pub(super) struct GlobalStmt<'a> {
    pub range: TextRange,
    pub line: u32,
    pub names: Vec<&'a str>,
    pub scope: usize,
}

pub(super) struct ForInfo<'a> {
    pub range: TextRange,
    /// The `else` block, if any.
    pub else_range: Option<TextRange>,
    pub iter: &'a Expr,
    pub orelse: &'a [Stmt],
    pub scope: usize,
}

pub(super) struct Info<'a> {
    pub scopes: Vec<Scope<'a>>,
    pub uses: Vec<Use<'a>>,
    pub binds: Vec<Bind<'a>>,
    pub deletes: Vec<Bind<'a>>,
    pub globals: Vec<GlobalStmt<'a>>,
    pub loops: Vec<ForInfo<'a>>,
    /// `except ... as name` clauses inside another one binding the same
    /// name: the inner clause and the outer line.
    pub nested_handlers: Vec<(&'a str, TextRange, u32)>,
    /// Each `from m import *` of the module's scope: `m` as written and the line.
    pub stars: Vec<(String, u32)>,
    pub typing: HashSet<&'a str>,
}

impl<'a> Info<'a> {
    pub fn collect(ctx: &Ctx<'a>) -> Self {
        let mut typing: HashSet<&str> = HashSet::from(["typing"]);
        for stmt in &ctx.parsed.syntax().body {
            if let Stmt::Import(import) = stmt {
                for alias in &import.names {
                    if alias.name.as_str() == "typing"
                        && let Some(asname) = &alias.asname
                    {
                        typing.insert(asname.as_str());
                    }
                }
            }
        }
        let mut walk = Collect {
            ctx,
            info: Info {
                scopes: Vec::new(),
                uses: Vec::new(),
                binds: Vec::new(),
                deletes: Vec::new(),
                globals: Vec::new(),
                loops: Vec::new(),
                nested_handlers: Vec::new(),
                stars: Vec::new(),
                typing,
            },
            cur: 0,
            path: Vec::new(),
            tests: Vec::new(),
            stmt: TextRange::default(),
            stmt_line: 0,
            type_checking: 0,
            handlers: Vec::new(),
            annotation: false,
            comp_depth: 0,
            for_idx: None,
            plain_value: None,
        };
        walk.info.scopes.push(Scope::new(Kind::Module, None, None));
        walk.visit_body(&ctx.parsed.syntax().body);
        walk.info
    }

    /// The scope whose locals a use of `name` in `scope` reads.
    pub fn lookup(&self, scope: usize, name: &str) -> Option<usize> {
        let mut current = scope;
        let mut first = true;
        // A comprehension inside a class does not see the class names either.
        loop {
            let data = &self.scopes[current];
            let class_hidden = data.kind == Kind::Class && !first;
            if !class_hidden {
                if data.globals.contains(name) && data.kind != Kind::Module {
                    return Some(0);
                }
                if !data.nonlocals.contains(name) && data.names.contains(name) {
                    return Some(current);
                }
            }
            first = false;
            current = data.parent?;
        }
    }

    /// The frame that holds `scope`'s names: comprehensions share their
    /// parent's.
    pub fn frame_of(&self, mut scope: usize) -> usize {
        while self.scopes[scope].kind == Kind::Comp {
            scope = self.scopes[scope].parent.unwrap_or(0);
        }
        scope
    }

    /// Whether `scope` is `outer` or inside it.
    pub fn within(&self, scope: usize, outer: usize) -> bool {
        let mut current = Some(scope);
        while let Some(id) = current {
            if id == outer {
                return true;
            }
            current = self.scopes[id].parent;
        }
        false
    }
}

impl<'a> Scope<'a> {
    fn new(kind: Kind, parent: Option<usize>, func: Option<&'a StmtFunctionDef>) -> Self {
        Self {
            kind,
            parent,
            defs: Vec::new(),
            names: HashSet::new(),
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            func,
            locals_calls: Vec::new(),
        }
    }
}

struct Collect<'a, 'c> {
    ctx: &'c Ctx<'a>,
    info: Info<'a>,
    cur: usize,
    path: Vec<Step<'a>>,
    tests: Vec<&'a Expr>,
    stmt: TextRange,
    stmt_line: u32,
    /// How many `if TYPE_CHECKING` statements enclose the node.
    type_checking: usize,
    /// The `except ... as name` clauses around: name and line.
    handlers: Vec<(&'a str, u32)>,
    annotation: bool,
    /// How many comprehensions of the current frame hold the node.
    comp_depth: usize,
    for_idx: Option<usize>,
    plain_value: Option<&'a Expr>,
}

impl<'a> Collect<'a, '_> {
    fn line(&self, offset: u32) -> u32 {
        self.ctx
            .index
            .range(self.ctx.source, offset, offset)
            .start
            .line
    }

    fn frame_of(&self, scope: usize) -> usize {
        self.info.frame_of(scope)
    }

    fn push_scope(&mut self, kind: Kind, func: Option<&'a StmtFunctionDef>) {
        let id = self.info.scopes.len();
        self.info
            .scopes
            .push(Scope::new(kind, Some(self.cur), func));
        self.cur = id;
    }

    fn pop_scope(&mut self) {
        self.cur = self.info.scopes[self.cur].parent.unwrap_or(0);
    }

    fn is_type_checking(&self, test: &Expr) -> bool {
        match test {
            Expr::Name(name) => name.id.as_str() == "TYPE_CHECKING",
            Expr::Attribute(attr) => {
                attr.attr.as_str() == "TYPE_CHECKING"
                    && matches!(&*attr.value, Expr::Name(n) if self.info.typing.contains(n.id.as_str()))
            }
            _ => false,
        }
    }

    /// Record a binding of `name`. A walrus in a comprehension binds in the
    /// enclosing scope, and a `global` name binds in the module.
    fn bind(&mut self, name: &'a str, kind: DefKind, range: TextRange, event: bool, walrus: bool) {
        let lexical = self.cur;
        let mut target = lexical;
        if walrus {
            target = self.frame_of(target);
        }
        if target != 0 && self.info.scopes[target].globals.contains(name) {
            target = 0;
        }
        let imported = matches!(kind, DefKind::Import | DefKind::FromImport);
        let line = if matches!(
            kind,
            DefKind::Param | DefKind::Plain | DefKind::ForTarget | DefKind::ExceptName
        ) {
            self.line(range.start().to_u32())
        } else {
            self.stmt_line
        };
        let at = if imported { self.stmt } else { range };
        if event {
            self.info.binds.push(Bind {
                name,
                line: self.line(range.start().to_u32()),
                scope: lexical,
            });
        }
        let frame = self.frame_of(lexical);
        let direct = self.path.is_empty() && frame == 0;
        let def = Def {
            name,
            kind,
            range: at,
            shown: name,
            line,
            frame,
            direct,
            type_checking: self.type_checking > 0,
            future: false,
            path: self.path.clone(),
            value: self.plain_value,
            handler: None,
            for_idx: self.for_idx,
        };
        self.info.scopes[target].names.insert(name);
        self.info.scopes[target].defs.push(def);
    }

    fn use_name(&mut self, name: &'a str, range: TextRange, kind: UseKind, self_assign: bool) {
        self.info.uses.push(Use {
            name,
            range,
            scope: self.cur,
            kind,
            line: self.stmt_line,
            stmt: self.stmt,
            path: self.path.clone(),
            tests: self.tests.clone(),
            annotation: self.annotation,
            in_comp: self.comp_depth > 0,
            self_assign,
        });
    }

    /// Bind the names of an assignment target.
    fn bind_target(&mut self, target: &'a Expr, kind: DefKind) {
        match target {
            Expr::Name(name) => self.bind(name.id.as_str(), kind, name.range, true, false),
            Expr::Tuple(t) => t.elts.iter().for_each(|e| self.bind_target(e, kind)),
            Expr::List(l) => l.elts.iter().for_each(|e| self.bind_target(e, kind)),
            Expr::Starred(s) => self.bind_target(&s.value, kind),
            other => self.visit_expr(other),
        }
    }

    fn branch(&mut self, stmt: &'a Stmt, slot: usize, body: &'a [Stmt]) {
        self.path.push(Step { stmt, slot });
        self.visit_body(body);
        self.path.pop();
    }

    fn function(&mut self, def: &'a StmtFunctionDef) {
        for decorator in &def.decorator_list {
            self.visit_decorator(decorator);
        }
        self.visit_parameters(&def.parameters);
        if let Some(returns) = &def.returns {
            self.annotated(returns);
        }
        let range = def.name.range();
        self.bind(def.name.as_str(), DefKind::Func, range, true, false);
        let comps = std::mem::take(&mut self.comp_depth);
        self.push_scope(Kind::Function, Some(def));
        for parameter in &def.parameters {
            let name = parameter.name();
            // Pylint places `*args` and `**kwargs` on the arguments node.
            let at = if matches!(parameter, AnyParameterRef::Variadic(_)) {
                def.parameters
                    .iter()
                    .next()
                    .map_or(range, |first| first.range())
            } else {
                name.range()
            };
            self.bind(name.as_str(), DefKind::Param, at, true, false);
        }
        self.visit_body(&def.body);
        self.pop_scope();
        self.comp_depth = comps;
    }

    fn annotated(&mut self, annotation: &'a Expr) {
        let outer = std::mem::replace(&mut self.annotation, true);
        self.visit_expr(annotation);
        self.annotation = outer;
    }

    fn comprehension(&mut self, generators: &'a [Comprehension], elements: &[&'a Expr]) {
        let Some((first, rest)) = generators.split_first() else {
            return;
        };
        self.comp_depth += 1;
        self.visit_expr(&first.iter);
        self.push_scope(Kind::Comp, None);
        for (index, generator) in std::iter::once(first).chain(rest).enumerate() {
            if index > 0 {
                self.visit_expr(&generator.iter);
            }
            self.bind_target(&generator.target, DefKind::Plain);
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        for element in elements {
            self.visit_expr(element);
        }
        self.pop_scope();
        self.comp_depth -= 1;
    }

    fn try_stmt(&mut self, stmt: &'a Stmt, t: &'a StmtTry) {
        self.branch(stmt, 0, &t.body);
        for (i, handler) in t.handlers.iter().enumerate() {
            let ExceptHandler::ExceptHandler(h) = handler;
            self.path.push(Step { stmt, slot: i + 1 });
            if let Some(type_) = &h.type_ {
                self.visit_expr(type_);
            }
            let line = self.line(h.range.start().to_u32());
            if let Some(name) = &h.name {
                if let Some((_, outer)) = self.handlers.iter().find(|(n, _)| *n == name.as_str()) {
                    self.info
                        .nested_handlers
                        .push((name.as_str(), h.range, *outer));
                }
                self.bind(
                    name.as_str(),
                    DefKind::ExceptName,
                    name.range(),
                    true,
                    false,
                );
                if let Some(def) = self.info.scopes[self.cur].defs.last_mut() {
                    def.handler = Some(h.range);
                }
                self.handlers.push((name.as_str(), line));
            }
            self.visit_body(&h.body);
            if h.name.is_some() {
                self.handlers.pop();
            }
            self.path.pop();
        }
        self.branch(stmt, 1000, &t.orelse);
        self.branch(stmt, 1001, &t.finalbody);
    }

    fn if_stmt(&mut self, stmt: &'a Stmt, s: &'a StmtIf) {
        let tests_before = self.tests.len();
        let tc_before = self.type_checking;
        let mut clauses: Vec<(Option<&'a Expr>, &'a [Stmt])> = vec![(Some(&*s.test), &s.body[..])];
        clauses.extend(
            s.elif_else_clauses
                .iter()
                .map(|ElifElseClause { test, body, .. }| (test.as_ref(), &body[..])),
        );
        let type_checking = self.is_type_checking(&s.test);
        if type_checking {
            self.type_checking += 1;
        }
        for (slot, (test, body)) in clauses.into_iter().enumerate() {
            if let Some(test) = test {
                self.tests.push(test);
                self.visit_expr(test);
            }
            self.branch(stmt, slot, body);
        }
        self.tests.truncate(tests_before);
        self.type_checking = tc_before;
    }
}

impl<'a> Collect<'a, '_> {
    /// The statements that bind names without a body of their own to walk
    /// by branch: definitions, imports and assignments. False for the rest.
    fn binding_stmt(&mut self, stmt: &'a Stmt) -> bool {
        match stmt {
            Stmt::FunctionDef(def) => self.function(def),
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
                let range = class.name.range();
                self.bind(class.name.as_str(), DefKind::Class, range, true, false);
                let comps = std::mem::take(&mut self.comp_depth);
                self.push_scope(Kind::Class, None);
                self.visit_body(&class.body);
                self.pop_scope();
                self.comp_depth = comps;
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    let (name, range) = match &alias.asname {
                        Some(asname) => (asname.as_str(), asname.range()),
                        None => (
                            alias.name.as_str().split('.').next().unwrap_or_default(),
                            alias.name.range(),
                        ),
                    };
                    self.bind(name, DefKind::Import, range, false, false);
                    if alias.asname.is_none()
                        && let Some(def) = self.info.scopes[self.cur].defs.last_mut()
                    {
                        def.shown = alias.name.as_str();
                    }
                }
            }
            Stmt::ImportFrom(import) => {
                let future = import.level == 0
                    && import
                        .module
                        .as_ref()
                        .is_some_and(|m| m.as_str() == "__future__");
                if self.cur == 0 && import.names.iter().any(|alias| &alias.name == "*") {
                    let origin = import.module.as_ref().map_or("", |m| m.as_str());
                    let origin = format!("{}{origin}", ".".repeat(import.level as usize));
                    self.info.stars.push((origin, self.stmt_line));
                }
                for alias in import
                    .names
                    .iter()
                    .filter(|alias| alias.name.as_str() != "*")
                {
                    let (name, range) = match &alias.asname {
                        Some(asname) => (asname.as_str(), asname.range()),
                        None => (alias.name.as_str(), alias.name.range()),
                    };
                    self.bind(name, DefKind::FromImport, range, false, false);
                    if let Some(def) = self.info.scopes[self.cur].defs.last_mut() {
                        def.future = future;
                    }
                }
            }
            Stmt::Assign(assign) => {
                self.visit_expr(&assign.value);
                let single = matches!(&assign.targets[..], [Expr::Name(_)]);
                let value_name = match &*assign.value {
                    Expr::Name(n) => Some(n.id.as_str()),
                    _ => None,
                };
                if let (Some(value), [Expr::Name(target)]) = (value_name, &assign.targets[..])
                    && value == target.id.as_str()
                    && let Some(last) = self.info.uses.last_mut()
                {
                    last.self_assign = true;
                }
                let outer_value = self.plain_value;
                self.plain_value = single.then_some(&*assign.value);
                for target in &assign.targets {
                    self.bind_target(target, DefKind::Plain);
                }
                self.plain_value = outer_value;
            }
            Stmt::AnnAssign(assign) => {
                self.annotated(&assign.annotation);
                if let Some(value) = &assign.value {
                    self.visit_expr(value);
                }
                self.bind_target(&assign.target, DefKind::Plain);
            }
            Stmt::AugAssign(assign) => {
                self.visit_expr(&assign.value);
                if let Expr::Name(name) = &*assign.target {
                    self.use_name(name.id.as_str(), name.range, UseKind::Aug, false);
                    self.bind(name.id.as_str(), DefKind::Plain, name.range, true, false);
                } else {
                    self.visit_expr(&assign.target);
                }
            }
            _ => return false,
        }
        true
    }
}

impl<'a> Visitor<'a> for Collect<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let outer = (self.stmt, self.stmt_line);
        self.stmt = stmt.range();
        self.stmt_line = self.line(match stmt {
            Stmt::FunctionDef(f) => f.name.range().start().to_u32(),
            Stmt::ClassDef(c) => c.name.range().start().to_u32(),
            _ => stmt.range().start().to_u32(),
        });
        if !self.binding_stmt(stmt) {
            match stmt {
                Stmt::For(f) => {
                    self.visit_expr(&f.iter);
                    let idx = self.info.loops.len();
                    self.info.loops.push(ForInfo {
                        range: f.range,
                        else_range: f
                            .orelse
                            .first()
                            .zip(f.orelse.last())
                            .map(|(a, b)| a.range().cover(b.range())),
                        iter: &f.iter,
                        orelse: &f.orelse,
                        scope: self.cur,
                    });
                    let outer = self.for_idx.replace(idx);
                    self.path.push(Step { stmt, slot: 0 });
                    self.bind_target(&f.target, DefKind::ForTarget);
                    self.for_idx = outer;
                    self.visit_body(&f.body);
                    self.path.pop();
                    self.branch(stmt, 1, &f.orelse);
                }
                Stmt::While(w) => {
                    self.visit_expr(&w.test);
                    self.branch(stmt, 0, &w.body);
                    self.branch(stmt, 1, &w.orelse);
                }
                Stmt::With(w) => {
                    for item in &w.items {
                        self.visit_expr(&item.context_expr);
                        if let Some(vars) = &item.optional_vars {
                            self.bind_target(vars, DefKind::Plain);
                        }
                    }
                    self.branch(stmt, 0, &w.body);
                }
                Stmt::If(s) => self.if_stmt(stmt, s),
                Stmt::Try(t) => self.try_stmt(stmt, t),
                Stmt::Match(m) => {
                    self.visit_expr(&m.subject);
                    for (i, case) in m.cases.iter().enumerate() {
                        self.path.push(Step { stmt, slot: i });
                        self.visit_pattern(&case.pattern);
                        if let Some(guard) = &case.guard {
                            self.visit_expr(guard);
                        }
                        self.visit_body(&case.body);
                        self.path.pop();
                    }
                }
                Stmt::Global(g) => {
                    let names: Vec<&str> = g
                        .names
                        .iter()
                        .map(ruff_python_ast::Identifier::as_str)
                        .collect();
                    self.info.scopes[self.cur]
                        .globals
                        .extend(names.iter().copied());
                    self.info.globals.push(GlobalStmt {
                        range: g.range,
                        line: self.stmt_line,
                        names,
                        scope: self.cur,
                    });
                }
                Stmt::Nonlocal(n) => {
                    let names = n.names.iter().map(ruff_python_ast::Identifier::as_str);
                    self.info.scopes[self.cur].nonlocals.extend(names);
                }
                Stmt::Delete(d) => {
                    for target in &d.targets {
                        self.visit_expr(target);
                    }
                }
                _ => visitor::walk_stmt(self, stmt),
            }
        }
        (self.stmt, self.stmt_line) = outer;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match name.ctx {
                ExprContext::Load => {
                    self.use_name(name.id.as_str(), name.range, UseKind::Load, false);
                }
                ExprContext::Del => {
                    self.use_name(name.id.as_str(), name.range, UseKind::Del, false);
                    self.info.deletes.push(Bind {
                        name: name.id.as_str(),
                        line: self.line(name.range.start().to_u32()),
                        scope: self.cur,
                    });
                }
                ExprContext::Store | ExprContext::Invalid => {
                    self.bind(name.id.as_str(), DefKind::Plain, name.range, true, false);
                }
            },
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = &*named.target {
                    self.bind(target.id.as_str(), DefKind::Plain, target.range, true, true);
                }
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
                let comps = std::mem::take(&mut self.comp_depth);
                self.push_scope(Kind::Lambda, None);
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters {
                        let name = parameter.name();
                        self.bind(name.as_str(), DefKind::Param, name.range(), true, false);
                    }
                }
                self.visit_expr(&lambda.body);
                self.pop_scope();
                self.comp_depth = comps;
            }
            Expr::ListComp(c) => self.comprehension(&c.generators, &[&c.elt]),
            Expr::SetComp(c) => self.comprehension(&c.generators, &[&c.elt]),
            Expr::Generator(c) => self.comprehension(&c.generators, &[&c.elt]),
            Expr::DictComp(c) => {
                let elements: Vec<&Expr> = c
                    .key
                    .as_deref()
                    .into_iter()
                    .chain([c.value.as_ref()])
                    .collect();
                self.comprehension(&c.generators, &elements);
            }
            Expr::If(e) => {
                self.tests.push(&e.test);
                visitor::walk_expr(self, expr);
                self.tests.pop();
            }
            Expr::Call(call) => {
                if let Expr::Name(name) = &*call.func
                    && name.id.as_str() == "locals"
                {
                    let mut scope = self.cur;
                    loop {
                        match self.info.scopes[scope].kind {
                            Kind::Function => {
                                let line = self.line(call.range().start().to_u32());
                                self.info.scopes[scope].locals_calls.push(line);
                                break;
                            }
                            Kind::Lambda | Kind::Comp => {
                                scope = self.info.scopes[scope].parent.unwrap_or(0);
                            }
                            _ => break,
                        }
                    }
                }
                visitor::walk_expr(self, expr);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(p) => p.name.as_ref(),
            Pattern::MatchStar(p) => p.name.as_ref(),
            Pattern::MatchMapping(p) => p.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.bind(name.as_str(), DefKind::Plain, name.range(), true, false);
        }
        visitor::walk_pattern(self, pattern);
    }

    fn visit_annotation(&mut self, annotation: &'a Expr) {
        self.annotated(annotation);
    }
}

/// Pylint's `_is_name_ignored`: the dummy-variable pattern, or the
/// ignored-argument pattern for a parameter.
fn ignored(ctx: &Ctx<'_>, kind: DefKind, name: &str) -> bool {
    let (key, default) = if kind == DefKind::Param {
        ("ignored-argument-names", "_.*|^ignored_|^unused_")
    } else {
        (
            "dummy-variables-rgx",
            "_+$|(_[a-zA-Z0-9_]*[a-zA-Z0-9]+?$)|dummy|^ignored_|^unused_",
        )
    };
    py_match(ctx.option(key).unwrap_or(default), name)
}

const MODULE_ATTRIBUTES: &[&str] = &[
    "__name__",
    "__doc__",
    "__file__",
    "__dict__",
    "__path__",
    "__package__",
    "__spec__",
    "__loader__",
    "__cached__",
];

/// `W0601`: a `global` name the module never defines. A name that the
/// function never assigns is Pylint's `W0602`, which is left to Ruff.
fn globals(ctx: &Ctx<'_>, info: &Info<'_>, out: &mut Vec<Diagnostic>) {
    let module = &info.scopes[0];
    for stmt in &info.globals {
        if info.scopes[stmt.scope].kind == Kind::Module {
            continue;
        }
        for &name in &stmt.names {
            // A module attribute such as `__doc__` is Pylint's `W0622` there.
            if MODULE_ATTRIBUTES.contains(&name) {
                continue;
            }
            let defs: Vec<&Def<'_>> = module.defs.iter().filter(|d| d.name == name).collect();
            let imported = defs
                .iter()
                .any(|d| matches!(d.kind, DefKind::Import | DefKind::FromImport));
            let reassigned = info
                .binds
                .iter()
                .any(|b| b.name == name && b.line > stmt.line && b.scope == stmt.scope);
            let deleted = info
                .deletes
                .iter()
                .any(|d| d.name == name && d.line > stmt.line && info.within(d.scope, stmt.scope));
            if !reassigned && !deleted && !imported {
                continue;
            }
            let defined = defs.iter().any(|d| match d.kind {
                DefKind::Func | DefKind::Class => d.direct,
                _ => d.frame == 0,
            });
            if !defined && !imported {
                out.push(ctx.report_at(
                    UNDEFINED,
                    stmt.range,
                    format!(
                        "Global variable {} undefined at the module level",
                        py_repr(name)
                    ),
                ));
            }
        }
    }
}

/// `W0621`: a name of a function that a module-level name or an `except`
/// name already holds.
fn redefined(ctx: &Ctx<'_>, info: &Info<'_>, out: &mut Vec<Diagnostic>) {
    let module = &info.scopes[0];
    let mut first_module: HashMap<&str, Vec<&Def<'_>>> = HashMap::new();
    for def in &module.defs {
        first_module.entry(def.name).or_default().push(def);
    }
    // The names of a star import, with the line of the first star that binds each.
    let mut starred: HashMap<String, u32> = HashMap::new();
    if !info.stars.is_empty() {
        let project = Project::with_packages(ctx.path, ctx.module);
        for (origin, line) in &info.stars {
            let names = project.public_names(&project.absolute(origin));
            for name in names.into_iter().flatten() {
                starred.entry(name).or_insert(*line);
            }
        }
    }
    for scope in &info.scopes {
        if scope.kind != Kind::Function {
            continue;
        }
        let mut seen = HashSet::new();
        for def in &scope.defs {
            // astroid keeps the assignment under a `nonlocal` as a local.
            if !seen.insert(def.name) || scope.globals.contains(def.name) {
                continue;
            }
            let outer = first_module.get(def.name);
            if outer.is_some_and(|o| o.iter().any(|d| d.type_checking)) {
                continue;
            }
            // astroid sorts the locals of a name by line, so a star import above wins.
            let line = match (outer.map(|o| o[0]), starred.get(def.name)) {
                (Some(d), Some(&star)) if star < d.line => star,
                (Some(d), _) if d.future || d.kind == DefKind::ExceptName => continue,
                (Some(d), _) => d.line,
                (None, Some(&star)) => star,
                (None, None) => continue,
            };
            if !ignored(ctx, def.kind, def.name) {
                out.push(ctx.report_at(
                    REDEFINED,
                    def.range,
                    format!(
                        "Redefining name {} from outer scope (line {})",
                        py_repr(def.name),
                        line
                    ),
                ));
            }
        }
    }
    for (name, range, line) in &info.nested_handlers {
        out.push(ctx.report_at(
            REDEFINED,
            *range,
            format!(
                "Redefining name {} from outer scope (line {line})",
                py_repr(name)
            ),
        ));
    }
}

/// Whether Pylint infers the iterable as a range, or as a non-empty display.
fn safe_iterable(info: &Info<'_>, scope: usize, iter: &Expr, depth: u8) -> bool {
    match iter {
        Expr::Call(call) => match &*call.func {
            Expr::Name(f) if f.id.as_str() == "range" => info.lookup(scope, "range").is_none(),
            Expr::Name(f)
                if f.id.as_str() == "enumerate" && info.lookup(scope, "enumerate").is_none() =>
            {
                call.arguments
                    .args
                    .first()
                    .is_some_and(|arg| safe_iterable(info, scope, arg, depth))
            }
            _ => false,
        },
        Expr::If(e) => safe_iterable(info, scope, &e.body, depth),
        Expr::List(l) => !l.elts.is_empty(),
        Expr::Tuple(t) => !t.elts.is_empty(),
        Expr::Set(s) => !s.elts.is_empty(),
        Expr::Dict(d) => !d.items.is_empty(),
        Expr::Name(name) if depth < 4 => {
            let Some(found) = info.lookup(scope, name.id.as_str()) else {
                return false;
            };
            // The first value Pylint infers is that of the first binding.
            info.scopes[found]
                .defs
                .iter()
                .find(|d| d.name == name.id.as_str())
                .and_then(|d| d.value)
                .is_some_and(|value| safe_iterable(info, scope, value, depth + 1))
        }
        _ => false,
    }
}

/// `W0631`: a loop variable read after its loop, when it is the only
/// binding of the name and the loop may not run.
fn loop_variables(ctx: &Ctx<'_>, info: &Info<'_>, out: &mut Vec<Diagnostic>) {
    let looped: HashSet<(usize, &str)> = info
        .scopes
        .iter()
        .enumerate()
        .flat_map(|(id, scope)| {
            scope
                .defs
                .iter()
                .filter(|d| d.kind == DefKind::ForTarget)
                .map(move |d| (id, d.name))
        })
        .collect();
    for usage in &info.uses {
        let Some(found) = info
            .lookup(usage.scope, usage.name)
            .filter(|&found| looped.contains(&(found, usage.name)))
        else {
            continue;
        };
        let scope = &info.scopes[found];
        // A nested function or lambda reads a loop variable of an outer
        // scope only after the loop has run.
        let user = &info.scopes[usage.scope];
        if matches!(user.kind, Kind::Function | Kind::Lambda) && usage.scope != found {
            continue;
        }
        let same_frame = info.scopes[usage.scope].kind == Kind::Comp || usage.scope == found;
        let defs: Vec<&Def<'_>> = scope
            .defs
            .iter()
            .filter(|d| d.name == usage.name)
            .filter(|d| !(same_frame && d.line > usage.line))
            // An augmented assignment does not count as its own binding.
            .filter(|d| d.range != usage.range)
            .filter(|d| !exclusive(&d.path, &usage.path))
            .collect();
        let Some(def) = defs.first() else { continue };
        let Some(index) = def.for_idx.filter(|_| def.kind == DefKind::ForTarget) else {
            continue;
        };
        let looped = &info.loops[index];
        // Bindings inside the loop, its `else` block apart, do not count.
        if defs[1..].iter().any(|d| {
            !looped.range.contains_range(d.range)
                || looped.else_range.is_some_and(|e| e.contains_range(d.range))
        }) {
            continue;
        }
        if looped.range.contains_range(usage.range) {
            continue;
        }
        if looped.orelse.iter().any(|stmt| {
            matches!(
                stmt,
                Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_) | Stmt::Continue(_)
            )
        }) {
            continue;
        }
        if safe_iterable(info, looped.scope, looped.iter, 0) {
            continue;
        }
        out.push(ctx.report_at(
            LOOP_VARIABLE,
            usage.range,
            format!(
                "Using possibly undefined loop variable {}",
                py_repr(usage.name)
            ),
        ));
    }
}

/// `W0641`: a name a function never reads, in a function that calls
/// `locals()` on a later line.
fn possibly_unused(ctx: &Ctx<'_>, info: &Info<'_>, out: &mut Vec<Diagnostic>) {
    for (id, scope) in info.scopes.iter().enumerate() {
        if scope.kind != Kind::Function || scope.locals_calls.is_empty() {
            continue;
        }
        let Some(func) = scope.func else { continue };
        // A function that only raises is not checked.
        if matches!(&func.body[..], [Stmt::Raise(_)]) {
            continue;
        }
        let comp_targets: HashSet<&str> = info
            .scopes
            .iter()
            .enumerate()
            .filter(|(i, s)| s.kind == Kind::Comp && info.within(*i, id))
            .flat_map(|(_, s)| s.defs.iter().map(|d| d.name))
            .collect();
        let nonlocals: HashSet<&str> = info
            .scopes
            .iter()
            .enumerate()
            .filter(|(i, _)| info.within(*i, id))
            .flat_map(|(_, s)| s.nonlocals.iter().copied())
            .collect();
        let mut seen = HashSet::new();
        for def in &scope.defs {
            if !seen.insert(def.name) || def.kind == DefKind::Param {
                continue;
            }
            if scope.globals.contains(def.name)
                || nonlocals.contains(def.name)
                || comp_targets.contains(def.name)
                || ignored(ctx, def.kind, def.name)
            {
                continue;
            }
            let used = info
                .uses
                .iter()
                .any(|u| info.lookup(u.scope, u.name) == Some(id) && u.name == def.name);
            if used {
                continue;
            }
            if !scope.locals_calls.iter().any(|&line| def.line < line) {
                continue;
            }
            // A decorated function, and an `except` name its handler reads.
            if def.kind == DefKind::Func
                && let Some(inner) = find_function(ctx, def.range)
                && !inner.decorator_list.is_empty()
            {
                continue;
            }
            if let Some(handler) = def.handler
                && info
                    .uses
                    .iter()
                    .any(|u| u.name == def.name && handler.contains_range(u.range))
            {
                continue;
            }
            out.push(ctx.report_at(
                POSSIBLY_UNUSED,
                def.range,
                format!("Possibly unused variable {}", py_repr(def.shown)),
            ));
        }
    }
}

/// The function whose name sits at `range`.
fn find_function<'a>(ctx: &Ctx<'a>, range: TextRange) -> Option<&'a StmtFunctionDef> {
    struct Find<'a>(TextRange, Option<&'a StmtFunctionDef>);
    impl<'a> Visitor<'a> for Find<'a> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if let Stmt::FunctionDef(f) = stmt
                && f.name.range() == self.0
            {
                self.1 = Some(f);
            }
            visitor::walk_stmt(self, stmt);
        }
    }
    let mut find = Find(range, None);
    find.visit_body(&ctx.parsed.syntax().body);
    find.1
}

#[cfg(test)]
mod tests {
    use super::super::classes::{render, render_at};

    #[test]
    fn global_statements() {
        let source = include_str!("fixtures/global_statements.py");
        let expected = include_str!("fixtures/global_statements.expected").trim_end();
        assert_eq!(render(source, &[], &["W0601"]), expected);
    }

    #[test]
    fn redefined_outer_names() {
        let source = include_str!("fixtures/redefined_outer_name.py");
        let expected = include_str!("fixtures/redefined_outer_name.expected").trim_end();
        assert_eq!(render(source, &[], &["W0621"]), expected);
    }

    #[test]
    fn a_star_import_binds_the_outer_name() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("m.py"), "Loader = 1\nother = 2\n").unwrap();
        let path = root.path().join("main.py");
        let source = "from m import *\n\ndef load(stream, Loader=Loader):\n    return stream\n\n\
                      def late(other):\n    return other\n\nother = 3\n\ndef fine(unrelated):\n    return unrelated\n";
        assert_eq!(
            render_at(source, &path, "main", &[], &["W0621"]),
            "3:W0621:Redefining name 'Loader' from outer scope (line 1)\n\
             6:W0621:Redefining name 'other' from outer scope (line 1)"
        );
    }

    #[test]
    fn loop_variables() {
        let source = include_str!("fixtures/undefined_loop_variable.py");
        let expected = include_str!("fixtures/undefined_loop_variable.expected").trim_end();
        assert_eq!(render(source, &[], &["W0631"]), expected);
    }

    #[test]
    fn possibly_unused_variables() {
        let source = include_str!("fixtures/possibly_unused_variable.py");
        let expected = include_str!("fixtures/possibly_unused_variable.expected").trim_end();
        assert_eq!(render(source, &[], &["W0641"]), expected);
    }
}
