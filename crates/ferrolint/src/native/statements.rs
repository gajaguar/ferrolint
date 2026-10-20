//! Statements and the text around them: `W0105` pointless-string-statement,
//! `W0124` confusing-with-statement, `C0200` consider-using-enumerate,
//! `R1713` consider-using-join, `W4701` modified-iterating-list, `E4702`
//! modified-iterating-dict, `E4703` modified-iterating-set, `W4801`
//! looping-through-iterator, `E3102`
//! positional-only-arguments-expected, `R5102`
//! http-response-with-content-type-json (pylint-django), `E0113`
//! invalid-star-assignment-target, `C0302`
//! too-many-lines and `C0325` superfluous-parens.
//!
//! shortcut: Pylint infers the type of what a loop iterates and what a call
//! calls. Here a list or dict is a name bound once to a literal; `E3102`
//! follows only a name bound once to a function of this file. Upgrade with
//! an inference of values.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Expr, ExprCall, HasNodeIndex, Operator, Stmt, StmtAugAssign, StmtFor, StmtFunctionDef, StmtWith,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use super::classes::Env;
use super::flow::exits_early;
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const STRING_STATEMENT: &str = "W0105";
const CONFUSING_WITH: &str = "W0124";
const ENUMERATE: &str = "C0200";
const JOIN: &str = "R1713";
const LIST: &str = "W4701";
const DICT: &str = "E4702";
const SET: &str = "E4703";
const ITERATOR: &str = "W4801";
const POSITIONAL_ONLY: &str = "E3102";
const STAR_TARGET: &str = "E0113";
const JSON_RESPONSE: &str = "R5102";
const TOO_MANY_LINES: &str = "C0302";
const PARENS: &str = "C0325";

pub struct Statements;

impl Checker for Statements {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                STRING_STATEMENT,
                "pointless-string-statement",
                "A string used as a statement that is not a docstring.",
            ),
            rule(
                CONFUSING_WITH,
                "confusing-with-statement",
                "`with a() as b, c:`, where `c` is not a call.",
            ),
            rule(
                ENUMERATE,
                "consider-using-enumerate",
                "`for i in range(len(x))` that indexes `x[i]`.",
            ),
            rule(
                JOIN,
                "consider-using-join",
                "Strings concatenated in a loop into a string.",
            ),
            rule(
                LIST,
                "modified-iterating-list",
                "A list modified in the loop that iterates it.",
            ),
            rule(
                DICT,
                "modified-iterating-dict",
                "A dict modified in the loop that iterates it.",
            ),
            rule(
                SET,
                "modified-iterating-set",
                "A set modified in the loop that iterates it.",
            ),
            rule(
                ITERATOR,
                "looping-through-iterator",
                "An iterator consumed in a loop nested in another loop.",
            ),
            rule(
                POSITIONAL_ONLY,
                "positional-only-arguments-expected",
                "A positional-only parameter passed as a keyword.",
            ),
            rule(
                JSON_RESPONSE,
                "http-response-with-content-type-json",
                "`HttpResponse(content_type='application/json')`.",
            ),
            rule(
                STAR_TARGET,
                "invalid-star-assignment-target",
                "`*a = b`, a starred target that is not in a list or tuple.",
            ),
            rule(
                TOO_MANY_LINES,
                "too-many-lines",
                "A module with more than `max-module-lines` lines.",
            ),
            rule(
                PARENS,
                "superfluous-parens",
                "A single item in parentheses after `if`, `for`, `return` and other keywords.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if enabled.contains(TOO_MANY_LINES) {
            too_many_lines(ctx, out);
        }
        if enabled.contains(PARENS) {
            superfluous_parens(ctx, out);
        }
        let ours = [
            STRING_STATEMENT,
            CONFUSING_WITH,
            ENUMERATE,
            JOIN,
            LIST,
            DICT,
            SET,
            ITERATOR,
            POSITIONAL_ONLY,
            JSON_RESPONSE,
            STAR_TARGET,
        ];
        if !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            assigned: HashMap::new(),
            docstrings: Vec::new(),
            scopes: vec![HashMap::new()],
            loops: Vec::new(),
            frames: Vec::new(),
            scope: Scope::Module,
        };
        let body = &ctx.parsed.syntax().body;
        walk.prepare(body);
        walk.visit_body(body);
    }
}

/// `C0302`: the lines of the module are counted like `wc -l`.
fn too_many_lines(ctx: &Ctx<'_>, out: &mut Vec<Diagnostic>) {
    let max: usize = ctx
        .option("max-module-lines")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(1000);
    let lines = ctx.source.matches('\n').count()
        + usize::from(!ctx.source.is_empty() && !ctx.source.ends_with('\n'));
    if lines > max {
        out.push(ctx.report_at(
            TOO_MANY_LINES,
            TextRange::default(),
            format!("Too many lines in module ({lines}/{max})"),
        ));
    }
}

const KEYWORDS: [&str; 15] = [
    "assert", "del", "elif", "except", "for", "if", "in", "not", "raise", "return", "while",
    "yield", "with", "=", ":=",
];

/// `C0325`: Pylint's `_check_keyword_parentheses` over the tokens.
fn superfluous_parens(ctx: &Ctx<'_>, out: &mut Vec<Diagnostic>) {
    let tokens: Vec<(TokenKind, &str, TextSize)> = ctx
        .parsed
        .tokens()
        .iter()
        .map(|t| (t.kind(), &ctx.source[t.range()], t.start()))
        .collect();
    for start in 0..tokens.len() {
        if KEYWORDS.contains(&tokens[start].1) {
            parens_after(ctx, &tokens, start, out);
        }
    }
}

fn parens_after(
    ctx: &Ctx<'_>,
    tokens: &[(TokenKind, &str, TextSize)],
    start: usize,
    out: &mut Vec<Diagnostic>,
) {
    let text = |i: usize| tokens.get(i).map_or("", |t| t.1);
    if text(start + 1) != "(" {
        return;
    }
    if text(start) == "not" && start > 0 && text(start - 1) == "is" {
        return;
    }
    let (mut and_or, mut walrus, mut walrus_depth) = (false, false, 0_i32);
    let (mut double, mut depth) = (0_i32, 0_i32);
    let keyword = text(start);
    for i in start..tokens.len().saturating_sub(1) {
        let (kind, string, _) = tokens[i];
        if kind == TokenKind::NonLogicalNewline {
            return;
        }
        if string == ":=" {
            walrus = true;
            walrus_depth = depth;
        }
        if string == "(" {
            depth += 1;
            if text(i + 1) == "(" {
                double = 1;
            }
        } else if string == ")" {
            depth -= 1;
            if depth != 0 {
                if double != 0 && text(i + 1) == ")" {
                    // for walrus operators in `if (not)` conditions and comprehensions
                    if matches!(keyword, "in" | "if" | "not") {
                        continue;
                    }
                    return;
                }
                double -= 1;
                continue;
            }
            let next = tokens[i + 1];
            if matches!(next.1, ":" | ")" | "]" | "}" | "in")
                || matches!(
                    next.0,
                    TokenKind::Newline | TokenKind::EndOfFile | TokenKind::Comment
                )
            {
                if walrus && walrus_depth - 1 == depth {
                    return;
                }
                // the empty tuple is always accepted
                if i == start + 2 || and_or {
                    return;
                }
                if keyword == "in" {
                    let single = i == start + 3
                        && matches!(
                            tokens[start + 2].0,
                            TokenKind::String
                                | TokenKind::Int
                                | TokenKind::Float
                                | TokenKind::Complex
                        );
                    if !single {
                        return;
                    }
                }
                out.push(ctx.report_at(
                    PARENS,
                    TextRange::at(
                        tokens[start].2,
                        TextSize::from(u32::try_from(keyword.len()).unwrap_or(1)),
                    ),
                    format!("Unnecessary parens after '{keyword}' keyword"),
                ));
            }
            return;
        } else if depth == 1 {
            match string {
                "," | "yield" | "for" => return,
                "and" | "or" => and_or = true,
                "else" => {
                    // the rest of a conditional expression may have its own
                    if tokens[i..].iter().any(|t| t.1 == "(") {
                        let rest: Vec<_> = tokens[i..].to_vec();
                        parens_after(ctx, &rest, 0, out);
                    }
                    return;
                }
                _ => {}
            }
        }
    }
}

/// Where a statement list is.
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Module,
    Class,
    /// A function; whether it is `__init__`.
    Function(bool),
}

/// A name a loop-nesting check tracks.
#[derive(Clone)]
enum Def<'a> {
    Safe,
    /// An iterator, with the loops around its definition, innermost first.
    Iterator(Vec<&'a StmtFor>),
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    /// The value assigned to each name written as an assignment target, by
    /// the node id of the target.
    assigned: HashMap<u32, &'a Expr>,
    /// The string statements that are docstrings, by address.
    docstrings: Vec<usize>,
    /// W4801: what the names of each scope stand for.
    scopes: Vec<HashMap<String, Def<'a>>>,
    /// The `for` loops around the statement, innermost last.
    loops: Vec<&'a StmtFor>,
    /// The names of the enclosing functions.
    frames: Vec<&'a str>,
    /// The module, class or function around the statement being walked.
    scope: Scope,
}

fn address<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn text(&self, expr: &Expr) -> &'a str {
        &self.env.ctx.source[expr.range()]
    }

    /// Record the assigned values and the docstrings.
    fn prepare(&mut self, body: &'a [Stmt]) {
        struct Prepare<'a, 'm> {
            assigned: &'m mut HashMap<u32, &'a Expr>,
            docstrings: &'m mut Vec<usize>,
        }
        fn docstring(body: &[Stmt], into: &mut Vec<usize>) {
            if let Some(Stmt::Expr(first)) = body.first()
                && matches!(&*first.value, Expr::StringLiteral(_))
            {
                into.push(address(first));
            }
        }
        impl<'a> Visitor<'a> for Prepare<'a, '_> {
            fn visit_stmt(&mut self, stmt: &'a Stmt) {
                match stmt {
                    Stmt::FunctionDef(f) => docstring(&f.body, self.docstrings),
                    Stmt::ClassDef(c) => docstring(&c.body, self.docstrings),
                    Stmt::Assign(assign) => {
                        for target in &assign.targets {
                            if let Expr::Name(name) = target
                                && let Some(id) = name.node_index().load().as_u32()
                            {
                                self.assigned.insert(id, &assign.value);
                            }
                        }
                    }
                    _ => {}
                }
                visitor::walk_stmt(self, stmt);
            }
        }
        let mut prepare = Prepare {
            assigned: &mut self.assigned,
            docstrings: &mut self.docstrings,
        };
        docstring(body, prepare.docstrings);
        prepare.visit_body(body);
    }

    /// The rules about a list of statements and their neighbors.
    fn statements(&mut self, body: &'a [Stmt], scope: Scope) {
        let ctx = self.env.ctx;
        for (i, stmt) in body.iter().enumerate() {
            match stmt {
                Stmt::Expr(e)
                    if self.on(STRING_STATEMENT)
                        && matches!(&*e.value, Expr::StringLiteral(_))
                        && !self.docstrings.contains(&address(e)) =>
                {
                    // an attribute docstring follows an assignment
                    let attribute = scope != Scope::Function(false)
                        && i > 0
                        && matches!(
                            body[i - 1],
                            Stmt::Assign(_) | Stmt::AnnAssign(_) | Stmt::TypeAlias(_)
                        );
                    if !attribute {
                        self.out.push(ctx.report(
                            STRING_STATEMENT,
                            stmt,
                            "String statement has no effect",
                        ));
                    }
                }
                Stmt::For(f) if i > 0 && self.on(JOIN) => self.join(f, &body[i - 1]),
                _ => {}
            }
        }
    }

    fn with(&mut self, node: &StmtWith) {
        // `with a() as b, c:` reads as if `c` were bound by `as`
        if node.is_async {
            return;
        }
        for pair in node.items.windows(2) {
            if matches!(pair[0].optional_vars.as_deref(), Some(Expr::Name(_)))
                && pair[1].optional_vars.is_none()
                && !matches!(pair[1].context_expr, Expr::Call(_))
            {
                self.out.push(self.env.ctx.report(
                    CONFUSING_WITH,
                    node,
                    "Following \"as\" with another context manager looks like a tuple.",
                ));
            }
        }
    }

    fn enumerate(&mut self, node: &'a StmtFor) {
        let Expr::Call(call) = &*node.iter else {
            return;
        };
        if !matches!(
            self.env.qualified(&call.func).as_deref(),
            Some("range" | "builtins.range" | "six.moves.range")
        ) {
            return;
        }
        let args = &call.arguments.args;
        let zero = matches!(args.first(), Some(Expr::NumberLiteral(n)) if matches!(&n.value, ruff_python_ast::Number::Int(i) if i.as_u64() == Some(0)));
        if args.is_empty() || args.len() > 2 || (args.len() == 2 && !zero) {
            return;
        }
        let Some(Expr::Call(len)) = args.last() else {
            return;
        };
        let [object] = &len.arguments.args[..] else {
            return;
        };
        if !matches!(
            self.env.qualified(&len.func).as_deref(),
            Some("len" | "builtins.len")
        ) {
            return;
        }
        if !matches!(object, Expr::Name(_) | Expr::Attribute(_)) {
            return;
        }
        // `__iter__` of `self` cannot use `enumerate`
        if matches!(object, Expr::Name(n) if n.id.as_str() == "self")
            && self.frames.last() == Some(&"__iter__")
        {
            return;
        }
        let Expr::Name(target) = &*node.target else {
            return;
        };
        let mut subscripts = Subscripts {
            object,
            target: target.id.as_str(),
            found: false,
        };
        subscripts.visit_body(&node.body);
        if subscripts.found {
            self.out.push(self.env.ctx.report(
                ENUMERATE,
                node,
                "Consider using enumerate instead of iterating with range and len",
            ));
        }
    }

    fn join(&mut self, node: &StmtFor, previous: &Stmt) {
        let [
            Stmt::AugAssign(StmtAugAssign {
                op: Operator::Add,
                target,
                value,
                ..
            }),
        ] = &node.body[..]
        else {
            return;
        };
        let Stmt::Assign(assign) = previous else {
            return;
        };
        let (Expr::Name(result), Expr::Name(item)) = (&**target, &*node.target) else {
            return;
        };
        let initial = matches!(&*assign.value, Expr::StringLiteral(_));
        let starts = assign
            .targets
            .iter()
            .any(|t| matches!(t, Expr::Name(n) if n.id == result.id));
        let suggest = self
            .env
            .ctx
            .option("suggest-join-with-non-empty-separator")
            .is_none_or(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "y" | "yes" | "true" | "1"
                )
            });
        let name = match &**value {
            Expr::Name(n) => Some(&n.id),
            Expr::FString(f) => {
                let (mut formatted, mut literal) = (Vec::new(), false);
                for part in &f.value {
                    match part {
                        ruff_python_ast::FStringPartRef::Literal(l) => literal |= !l.is_empty(),
                        ruff_python_ast::FStringPartRef::FString(s) => {
                            for element in &s.elements {
                                match element {
                                    ruff_python_ast::InterpolatedStringElement::Literal(l) => {
                                        literal |= !l.value.is_empty();
                                    }
                                    ruff_python_ast::InterpolatedStringElement::Interpolation(
                                        e,
                                    ) => formatted.push(e),
                                }
                            }
                        }
                    }
                }
                match formatted[..] {
                    [only] if !literal || suggest => match &*only.expression {
                        Expr::Name(n) => Some(&n.id),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        };
        if initial && starts && name == Some(&item.id) {
            self.out.push(self.env.ctx.report(
                JOIN,
                &node.body[0],
                "Consider using str.join(sequence) for concatenating strings from an iterable",
            ));
        }
    }

    /// Whether `expr` infers to a list, dict or set node: a literal,
    /// `list()`, `dict()` and `set()`, `list(...)` or `set(...)` of a literal
    /// of constants, or a name that is assigned one of them.
    fn container(&self, expr: &'a Expr) -> Option<Kind> {
        let value = match expr {
            Expr::Name(name) => {
                let binding = self.env.binding(name)?;
                if binding.kind != BindingKind::Assignment {
                    return None;
                }
                *self.assigned.get(&binding.node?)?
            }
            other => other,
        };
        match value {
            Expr::List(_) => Some(Kind::List),
            Expr::Dict(_) => Some(Kind::Dict),
            Expr::Set(_) => Some(Kind::Set),
            Expr::Call(call)
                if matches!(&*call.func, Expr::Name(_)) && call.arguments.keywords.is_empty() =>
            {
                let builtin = self.env.qualified(&call.func)?;
                match (builtin.as_str(), &call.arguments.args[..]) {
                    ("list", []) => Some(Kind::List),
                    ("dict", []) => Some(Kind::Dict),
                    ("set", []) => Some(Kind::Set),
                    ("list", [only]) => self.constants(only).then_some(Kind::List),
                    ("set", [only]) => self.constants(only).then_some(Kind::Set),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether `expr` is a list, tuple or set of constants.
    fn constants(&self, expr: &'a Expr) -> bool {
        let value = match expr {
            Expr::Name(name) => self
                .env
                .binding(name)
                .and_then(|b| b.node)
                .and_then(|id| self.assigned.get(&id).copied())
                .unwrap_or(expr),
            other => other,
        };
        let (Expr::List(ruff_python_ast::ExprList { elts, .. })
        | Expr::Tuple(ruff_python_ast::ExprTuple { elts, .. })
        | Expr::Set(ruff_python_ast::ExprSet { elts, .. })) = value
        else {
            return false;
        };
        elts.iter().all(|e| {
            matches!(
                e,
                Expr::NumberLiteral(_)
                    | Expr::StringLiteral(_)
                    | Expr::BytesLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
                    | Expr::EllipsisLiteral(_)
            )
        })
    }

    /// W4701, E4702 and E4703 for a loop that iterates a name.
    fn modified(&mut self, node: &'a StmtFor) {
        let Expr::Name(iterated) = &*node.iter else {
            return;
        };
        let Some(binding) = self.env.binding(iterated) else {
            return;
        };
        let Some(kind) = self.container(&node.iter) else {
            return;
        };
        let mut modified = Modified {
            env: self.env,
            iterated: iterated.id.as_str(),
            binding: binding.id,
            target: &node.target,
            kind,
            found: Vec::new(),
        };
        modified.visit_body(&node.body);
        let found = modified.found;
        for (range, id, message) in found {
            if self.on(id) {
                self.out.push(self.env.ctx.report_at(id, range, message));
            }
        }
    }

    /// Whether `value` makes a one-shot iterator.
    fn makes_iterator(&self, value: &Expr) -> bool {
        match value {
            Expr::Generator(_) => true,
            Expr::Call(call) => {
                if let Expr::Name(name) = &*call.func
                    && let Some(func) = self.env.func_of(name)
                    && is_generator(func)
                {
                    return true;
                }
                self.env.qualified(&call.func).is_some_and(|q| {
                    matches!(
                        q.as_str(),
                        "map"
                            | "filter"
                            | "zip"
                            | "iter"
                            | "reversed"
                            | "enumerate"
                            | "itertools.chain"
                            | "itertools.islice"
                            | "itertools.groupby"
                            | "itertools.starmap"
                            | "itertools.accumulate"
                            | "itertools.product"
                            | "itertools.permutations"
                            | "itertools.combinations"
                            | "itertools.combinations_with_replacement"
                            | "itertools.compress"
                            | "itertools.dropwhile"
                            | "itertools.takewhile"
                            | "itertools.filterfalse"
                            | "itertools.zip_longest"
                            | "itertools.pairwise"
                            | "itertools.batched"
                    )
                })
            }
            _ => false,
        }
    }

    /// Pylint's `_check_variable_usage`.
    fn usage(&mut self, usage: &'a Expr) {
        let Expr::Name(name) = usage else { return };
        let iterator = name.id.as_str();
        let Some(Def::Iterator(defined)) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(iterator))
            .cloned()
        else {
            return;
        };
        // the loops around the usage, innermost first
        let around: Vec<&'a StmtFor> = self.loops.iter().rev().copied().collect();
        if around.len() < 2 {
            return;
        }
        // the loop that holds the definition refreshes it on every pass
        let boundary = defined
            .first()
            .and_then(|d| around.iter().position(|l| std::ptr::eq(*l, *d)))
            .unwrap_or(around.len());
        let repeating = &around[1..boundary.max(1)];
        if repeating.is_empty() {
            return;
        }
        let inner = around[0];
        let raises = |stmts: &[Stmt]| {
            stmts
                .iter()
                .any(|s| matches!(s, Stmt::Return(_) | Stmt::Break(_) | Stmt::Raise(_)))
        };
        if raises(&inner.orelse)
            || exits_early(&inner.body, &inner.orelse)
            || has_return(&inner.body)
            || has_return(&inner.orelse)
        {
            return;
        }
        let mut nested = inner;
        for repeating_loop in repeating {
            let Some(at) = repeating_loop
                .body
                .iter()
                .position(|s| matches!(s, Stmt::For(f) if std::ptr::eq(f, nested)))
            else {
                return;
            };
            let after = &repeating_loop.body[at + 1..];
            if after
                .iter()
                .any(|s| matches!(s, Stmt::Return(_) | Stmt::Raise(_)))
            {
                return;
            }
            if matches!(&*repeating_loop.iter, Expr::Name(n) if n.id.as_str() == iterator) {
                return;
            }
            if !after.iter().any(|s| matches!(s, Stmt::Break(_))) {
                self.out.push(self.env.ctx.report(
                    ITERATOR,
                    usage,
                    format!("Iterator '{iterator}' from an outer scope is reused or consumed in a nested loop."),
                ));
                return;
            }
            nested = repeating_loop;
        }
    }

    fn call(&mut self, call: &'a ExprCall) {
        let ctx = self.env.ctx;
        if self.on(POSITIONAL_ONLY)
            && let Expr::Name(name) = &*call.func
            && let Some(func) = self.env.func_of(name)
        {
            let p = &func.parameters;
            if !p.posonlyargs.is_empty() && p.kwarg.is_none() {
                let given: Vec<String> = call
                    .arguments
                    .keywords
                    .iter()
                    .filter_map(|k| k.arg.as_ref())
                    .filter(|arg| {
                        p.posonlyargs
                            .iter()
                            .any(|a| a.parameter.name.as_str() == arg.as_str())
                    })
                    .map(|arg| format!("'{arg}'"))
                    .collect();
                if !given.is_empty() {
                    self.out.push(ctx.report(
                        POSITIONAL_ONLY,
                        call,
                        format!(
                            "`{}()` got some positional-only arguments passed as keyword arguments: {}",
                            self.text(&call.func),
                            given.join(", ")
                        ),
                    ));
                }
            }
        }
        if self.on(JSON_RESPONSE)
            && super::django::plugin(ctx)
            && self.text(&call.func).ends_with("HttpResponse")
            && call.arguments.keywords.iter().any(|k| {
                k.arg.as_ref().is_some_and(|a| a.as_str() == "content_type")
                    && self
                        .text(&k.value)
                        .to_lowercase()
                        .contains("application/json")
            })
        {
            self.out.push(ctx.report(
                JSON_RESPONSE,
                call,
                "Instead of HttpResponse(content_type='application/json') use JsonResponse()",
            ));
        }
        if self.on(ITERATOR) && !matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "next") {
            for arg in &call.arguments.args {
                self.usage(arg);
            }
        }
    }
}

fn is_generator(func: &StmtFunctionDef) -> bool {
    struct Yields(bool);
    impl<'a> Visitor<'a> for Yields {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if !matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                visitor::walk_stmt(self, stmt);
            }
        }
        fn visit_expr(&mut self, expr: &'a Expr) {
            match expr {
                Expr::Yield(_) | Expr::YieldFrom(_) => self.0 = true,
                Expr::Lambda(_) => {}
                _ => visitor::walk_expr(self, expr),
            }
        }
    }
    let mut yields = Yields(false);
    yields.visit_body(&func.body);
    yields.0
}

/// Whether `return` is in `body`, outside nested functions and classes.
fn has_return(body: &[Stmt]) -> bool {
    struct Returns(bool);
    impl<'a> Visitor<'a> for Returns {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            match stmt {
                Stmt::Return(_) => self.0 = true,
                Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
                _ => visitor::walk_stmt(self, stmt),
            }
        }
        fn visit_expr(&mut self, _: &'a Expr) {}
    }
    let mut returns = Returns(false);
    returns.visit_body(body);
    returns.0
}

/// The names that assigning to `target` binds.
fn names_in(target: &Expr) -> Vec<String> {
    match target {
        Expr::Name(n) => vec![n.id.to_string()],
        Expr::Tuple(t) => t.elts.iter().flat_map(names_in).collect(),
        Expr::List(l) => l.elts.iter().flat_map(names_in).collect(),
        Expr::Starred(s) => names_in(&s.value),
        _ => Vec::new(),
    }
}

struct Subscripts<'x> {
    object: &'x Expr,
    target: &'x str,
    found: bool,
}
impl<'b> Visitor<'b> for Subscripts<'_> {
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        if !matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            visitor::walk_stmt(self, stmt);
        }
    }
    fn visit_expr(&mut self, expr: &'b Expr) {
        match expr {
            Expr::Lambda(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_) => {}
            Expr::Subscript(s) => {
                let same = match (self.object, &*s.value) {
                    (Expr::Name(a), Expr::Name(b)) => a.id == b.id,
                    (Expr::Attribute(a), Expr::Attribute(b)) => a.attr.as_str() == b.attr.as_str(),
                    _ => false,
                };
                if same && matches!(&*s.slice, Expr::Name(n) if n.id.as_str() == self.target) {
                    self.found = true;
                }
                visitor::walk_expr(self, expr);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

struct Modified<'x, 'a> {
    env: &'x Env<'a>,
    iterated: &'x str,
    binding: u32,
    target: &'x Expr,
    kind: Kind,
    found: Vec<(TextRange, &'static str, String)>,
}

/// What a loop iterates, for the rules about modifying it.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    List,
    Dict,
    Set,
}
impl Modified<'_, '_> {
    /// Whether `expr` names the iterated variable.
    fn same(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Name(n) if n.id.as_str() == self.iterated
            && self.env.binding(n).is_some_and(|b| b.id == self.binding))
    }
    fn report(&mut self, range: TextRange) {
        let name = self.iterated;
        let (id, message) = match self.kind {
            Kind::List => (
                LIST,
                format!(
                    "Iterated list '{name}' is being modified inside for loop body, consider iterating through a copy of it instead."
                ),
            ),
            Kind::Dict => (
                DICT,
                format!(
                    "Iterated dict '{name}' is being modified inside for loop body, iterate through a copy of it instead."
                ),
            ),
            Kind::Set => (
                SET,
                format!(
                    "Iterated set '{name}' is being modified inside for loop body, iterate through a copy of it instead."
                ),
            ),
        };
        self.found.push((range, id, message));
    }
}
impl<'b> Visitor<'b> for Modified<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::Delete(d)
                if d.targets.iter().any(|t| matches!(t, Expr::Name(n) if names_in(self.target).contains(&n.id.to_string()))) =>
            {
                self.report(stmt.range());
            }
            Stmt::Expr(e) if self.kind != Kind::Dict => {
                if let Expr::Call(call) = &*e.value
                    && let Expr::Attribute(a) = &*call.func
                    && self.same(&a.value)
                    && match self.kind {
                        Kind::List => matches!(a.attr.as_str(), "append" | "remove"),
                        _ => matches!(a.attr.as_str(), "add" | "clear" | "discard" | "pop" | "remove"),
                    }
                {
                    self.report(stmt.range());
                }
            }
            Stmt::Assign(a) if self.kind == Kind::Dict => {
                if let Some(Expr::Subscript(s)) = a.targets.first()
                    && self.same(&s.value)
                    // updating the key that the loop is on is fine
                    && !(matches!(self.target, Expr::Name(t) if matches!(&*s.slice, Expr::Name(k) if k.id == t.id)))
                {
                    self.report(stmt.range());
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
                // a class body ends the loops around it
                let loops = std::mem::take(&mut self.loops);
                let scope = std::mem::replace(&mut self.scope, Scope::Class);
                self.visit_body(&class.body);
                self.scope = scope;
                self.loops = loops;
            }
            Stmt::FunctionDef(func) => {
                self.frames.push(func.name.as_str());
                self.scopes.push(HashMap::new());
                let loops = std::mem::take(&mut self.loops);
                let scope = std::mem::replace(
                    &mut self.scope,
                    Scope::Function(func.name.as_str() == "__init__"),
                );
                visitor::walk_stmt(self, stmt);
                self.scope = scope;
                self.loops = loops;
                self.scopes.pop();
                self.frames.pop();
            }
            Stmt::With(node) => {
                if self.on(CONFUSING_WITH) {
                    self.with(node);
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Assign(assign)
                if self.on(STAR_TARGET)
                    && matches!(assign.targets.first(), Some(Expr::Starred(_))) =>
            {
                self.out.push(self.env.ctx.report(
                    STAR_TARGET,
                    stmt,
                    "Starred assignment target must be in a list or tuple",
                ));
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Assign(assign) if self.on(ITERATOR) => {
                let iterator = self.makes_iterator(&assign.value);
                let definition = Def::Iterator(self.loops.iter().rev().copied().collect());
                for target in &assign.targets {
                    if let Expr::Name(name) = target
                        && let Some(scope) = self.scopes.last_mut()
                    {
                        scope.insert(
                            name.id.to_string(),
                            if iterator {
                                definition.clone()
                            } else {
                                Def::Safe
                            },
                        );
                    }
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::For(node) => {
                if self.on(ENUMERATE) && !node.is_async {
                    self.enumerate(node);
                }
                if !node.is_async && (self.on(LIST) || self.on(DICT) || self.on(SET)) {
                    self.modified(node);
                }
                self.loops.push(node);
                if !node.is_async && self.on(ITERATOR) {
                    // the names of the target are the items, not iterators
                    for name in names_in(&node.target) {
                        if let Some(scope) = self.scopes.last_mut() {
                            scope.insert(name, Def::Safe);
                        }
                    }
                    self.scopes.push(HashMap::new());
                    self.usage(&node.iter);
                    visitor::walk_stmt(self, stmt);
                    self.scopes.pop();
                } else {
                    visitor::walk_stmt(self, stmt);
                }
                self.loops.pop();
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_body(&mut self, body: &'a [Stmt]) {
        self.statements(body, self.scope);
        visitor::walk_body(self, body);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            self.call(call);
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;
    use crate::native::flow::BATCH_C;
    use crate::native::lint_with;

    #[test]
    fn statements_and_loops() {
        let source = include_str!("fixtures/statements.py");
        let expected = include_str!("fixtures/statements.expected").trim_end();
        let options = [("load-plugins", "pylint_django")];
        assert_eq!(render(source, &options, BATCH_C), expected);
    }

    #[test]
    fn modified_iterating_set() {
        let source = include_str!("fixtures/modified_iterating_set.py");
        let expected = include_str!("fixtures/modified_iterating_set.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }

    #[test]
    fn superfluous_parens() {
        let source = include_str!("fixtures/superfluous_parens.py");
        let expected = include_str!("fixtures/superfluous_parens.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }

    #[test]
    fn too_many_lines_counts_like_wc() {
        let source = "x = 1\n".repeat(6);
        let ids = |found: Vec<(u32, String)>| {
            found
                .into_iter()
                .filter(|(_, c)| c == "C0302")
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(lint_with(&source, &[("max-module-lines", "5")])),
            [(1, "C0302".to_owned())]
        );
        assert_eq!(
            ids(lint_with(&source, &[("max-module-lines", "6")])),
            Vec::new()
        );
        assert_eq!(
            ids(lint_with("x = 1", &[("max-module-lines", "1")])),
            Vec::new()
        );
    }

    #[test]
    fn starred_target_alone() {
        let found = lint_with("*a = [1]\n*b, c = [1, 2]\n", &[]);
        let found: Vec<_> = found.into_iter().filter(|(_, c)| c == "E0113").collect();
        assert_eq!(found, [(1, "E0113".to_owned())]);
    }
}
