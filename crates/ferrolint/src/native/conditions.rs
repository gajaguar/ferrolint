//! Conditions and comparisons: `W0125` using-constant-test, `W0126`
//! missing-parentheses-for-call-in-test, `R1709` simplify-boolean-expression,
//! `R1726` simplifiable-condition, `R1727` condition-evals-to-constant,
//! `R1738` chained-comparison-all-equal, `R1739` impossible-comparison,
//! `W3601` bad-chained-comparison and `C0117` unnecessary-negation.
//!
//! shortcut: Pylint infers the value of a test. Here a test is a constant
//! when it is written as a literal, or is a name bound once to a function,
//! class, lambda or generator expression of this file; a condition folds
//! only literals, containers and `not`/`-`/`+` of them; `C0117` needs a
//! literal on both sides. Upgrade with an inference of values.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    BoolOp, CmpOp, Comprehension, Expr, ExprBoolOp, ExprCompare, ExprContext, HasNodeIndex, Number,
    Stmt, StmtClassDef, StmtFunctionDef, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange};

use super::astring::as_string;
use super::classes::{Env, is_static, py_repr};
use super::flow::{const_truth, own_returns};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const CONSTANT_TEST: &str = "W0125";
const MISSING_CALL: &str = "W0126";
const SIMPLIFY: &str = "R1709";
const SIMPLIFIABLE: &str = "R1726";
const TO_CONSTANT: &str = "R1727";
const ALL_EQUAL: &str = "R1738";
const IMPOSSIBLE: &str = "R1739";
const BAD_CHAIN: &str = "W3601";
const NEGATION: &str = "C0117";

pub struct Conditions;

impl Checker for Conditions {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                CONSTANT_TEST,
                "using-constant-test",
                "A condition that is a constant.",
            ),
            rule(
                MISSING_CALL,
                "missing-parentheses-for-call-in-test",
                "A condition that is a function, not its call.",
            ),
            rule(
                SIMPLIFY,
                "simplify-boolean-expression",
                "`cond and <falsy> or other`, which is `other`.",
            ),
            rule(
                SIMPLIFIABLE,
                "simplifiable-condition",
                "A condition with constant operands that can be dropped.",
            ),
            rule(
                TO_CONSTANT,
                "condition-evals-to-constant",
                "A condition that always has the same value.",
            ),
            rule(
                ALL_EQUAL,
                "chained-comparison-all-equal",
                "Comparisons that go in a circle of `<=` or `>=`.",
            ),
            rule(
                IMPOSSIBLE,
                "impossible-comparison",
                "Comparisons that cannot all be true.",
            ),
            rule(
                BAD_CHAIN,
                "bad-chained-comparison",
                "A chained comparison that mixes comparison, identity and membership operators.",
            ),
            rule(
                NEGATION,
                "unnecessary-negation",
                "`not not x`, or `not` over a comparison that has an inverse.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            CONSTANT_TEST,
            MISSING_CALL,
            SIMPLIFY,
            SIMPLIFIABLE,
            TO_CONSTANT,
            ALL_EQUAL,
            IMPOSSIBLE,
            BAD_CHAIN,
            NEGATION,
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
            tests: HashSet::new(),
            frames: Vec::new(),
            classes: Vec::new(),
        };
        walk.collect(&ctx.parsed.syntax().body);
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    /// The value assigned to each name written as an assignment target, by
    /// the node id of the target.
    assigned: HashMap<u32, &'a Expr>,
    /// The addresses of the boolean operations that are tested for truth.
    tests: HashSet<usize>,
    /// The names of the enclosing functions.
    frames: Vec<&'a StmtFunctionDef>,
    /// The enclosing classes.
    classes: Vec<&'a StmtClassDef>,
}

/// Whether the builtin `name` is a function or a class, not a constant or
/// an instance such as `exit`.
fn builtin_callable(name: &str) -> bool {
    !name.starts_with('_')
        && !matches!(
            name,
            "Ellipsis"
                | "NotImplemented"
                | "exit"
                | "quit"
                | "copyright"
                | "credits"
                | "license"
                | "help"
        )
}

fn address(expr: &Expr) -> usize {
    std::ptr::from_ref(expr) as usize
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn text(&self, expr: &Expr) -> &'a str {
        &self.env.ctx.source[expr.range()]
    }

    /// The expression as astroid prints it.
    fn show(&self, expr: &'a Expr) -> String {
        let wrap = |e: &'a Expr| {
            let text = self.show(e);
            if matches!(
                e,
                Expr::If(_)
                    | Expr::Lambda(_)
                    | Expr::Named(_)
                    | Expr::Yield(_)
                    | Expr::YieldFrom(_)
                    | Expr::BoolOp(_)
            ) {
                format!("({text})")
            } else {
                text
            }
        };
        match expr {
            Expr::StringLiteral(s) => py_repr(s.value.to_str()),
            Expr::Compare(c) => {
                let mut out = wrap(&c.operands[0]);
                for (op, operand) in c.ops.iter().zip(&c.operands[1..]) {
                    let _ = write!(out, " {} {}", op.as_str(), wrap(operand));
                }
                out
            }
            Expr::UnaryOp(u) if u.op == UnaryOp::Not => format!("not {}", wrap(&u.operand)),
            Expr::BoolOp(_) => self.render(&Self::original(expr)),
            other => as_string(self.env.ctx.source, other),
        }
    }

    /// Record the value of every simple assignment, for the names it binds.
    fn collect(&mut self, body: &'a [Stmt]) {
        Assigned(&mut self.assigned).visit_body(body);
    }

    /// W0125 and W0126 for the test of an `if`, a conditional expression or
    /// a comprehension.
    fn constant_test(&mut self, test: &'a Expr) {
        if !self.on(CONSTANT_TEST) && !self.on(MISSING_CALL) {
            return;
        }
        let ctx = self.env.ctx;
        let report = |out: &mut Vec<Diagnostic>| {
            out.push(ctx.report(
                CONSTANT_TEST,
                test,
                "Using a conditional statement with a constant value",
            ));
        };
        if matches!(
            test,
            Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::StringLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::Dict(_)
                | Expr::Tuple(_)
                | Expr::Set(_)
                | Expr::List(_)
                | Expr::Lambda(_)
                | Expr::Generator(_)
        ) {
            if self.on(CONSTANT_TEST) {
                report(self.out);
            }
            return;
        }
        let Expr::Name(name) = test else { return };
        let Some(binding) = self.env.binding(name) else {
            return;
        };
        if binding.kind != BindingKind::Builtin && !self.env.ctx.model.is_sole_binding(binding.id) {
            return;
        }
        // what the name stands for, and whether calling it gives a result
        let calls = match binding.kind {
            // a module (if it can be imported) or a builtin function or class
            BindingKind::Builtin if !builtin_callable(&binding.name) => return,
            BindingKind::Import | BindingKind::Builtin | BindingKind::ClassDef => false,
            BindingKind::FunctionDef => {
                let Some(func) = self.env.func_of(name) else {
                    return;
                };
                if !func.decorator_list.is_empty() {
                    return;
                }
                true
            }
            BindingKind::Assignment => match binding.node.and_then(|id| self.assigned.get(&id)) {
                Some(Expr::Lambda(_)) => true,
                Some(Expr::Generator(_)) => false,
                _ => return,
            },
            _ => return,
        };
        if calls && self.on(MISSING_CALL) {
            self.out.push(ctx.report(
                MISSING_CALL,
                test,
                "Using a conditional statement with potentially wrong function or method call due to missing parentheses",
            ));
        }
        if self.on(CONSTANT_TEST) {
            report(self.out);
        }
    }

    /// W0125 for the call of a function of this file that only returns
    /// generator expressions.
    fn generator_call(&mut self, node: TextRange, test: &'a Expr) {
        let Expr::Call(call) = test else { return };
        let Expr::Name(name) = &*call.func else {
            return;
        };
        let Some(func) = self.env.func_of(name) else {
            return;
        };
        let returns = own_returns(&func.body);
        if !returns.is_empty()
            && returns
                .iter()
                .all(|r| matches!(r.value.as_deref(), Some(Expr::Generator(_))))
            && self.on(CONSTANT_TEST)
        {
            self.out.push(self.env.ctx.report_at(
                CONSTANT_TEST,
                node,
                "Using a conditional statement with a constant value",
            ));
        }
    }

    /// A test of an `if`, a conditional expression or a comprehension, in
    /// the statement or expression `node`.
    fn test(&mut self, node: TextRange, test: &'a Expr) {
        if matches!(test, Expr::Call(_)) {
            self.generator_call(node, test);
        } else if !matches!(
            test,
            Expr::BinOp(_) | Expr::BoolOp(_) | Expr::UnaryOp(_) | Expr::Subscript(_)
        ) {
            self.constant_test(test);
        }
        if matches!(test, Expr::BoolOp(_)) {
            self.tests.insert(address(test));
        }
    }

    // R1709 over the value of a `return` or an assignment
    fn and_or(&mut self, stmt: &'a Stmt, value: &'a Expr) {
        if !self.on(SIMPLIFY) {
            return;
        }
        // `cond and truth or false`, each part a plain value
        let Expr::BoolOp(ExprBoolOp {
            op: BoolOp::Or,
            values,
            ..
        }) = value
        else {
            return;
        };
        let [
            Expr::BoolOp(ExprBoolOp {
                op: BoolOp::And,
                values: inner,
                ..
            }),
            false_value,
        ] = &values[..]
        else {
            return;
        };
        let [_, truth] = &inner[..] else { return };
        if matches!(truth, Expr::BoolOp(_)) || matches!(false_value, Expr::BoolOp(_)) {
            return;
        }
        if matches!((truth, false_value), (Expr::Compare(_), Expr::Compare(_))) {
            return;
        }
        if inferred_truth(truth) == Some(false) {
            self.out.push(self.env.ctx.report(
                SIMPLIFY,
                stmt,
                format!(
                    "Boolean expression may be simplified to {}",
                    self.show(false_value)
                ),
            ));
        }
    }

    // R1726 and R1727 over a boolean operation that is tested for truth
    fn simplifiable(&mut self, expr: &'a Expr, op: &'a ExprBoolOp) {
        if !self.on(SIMPLIFIABLE) && !self.on(TO_CONSTANT) {
            return;
        }
        let mut changed = false;
        let simplified = Self::simplify(op, &mut changed);
        if !changed {
            return;
        }
        let original = self.render(&Node::Bool(
            op.op,
            op.values.iter().map(Self::original).collect(),
        ));
        let rendered = self.render(&simplified);
        let (id, message) = if has_name(&simplified) {
            (
                SIMPLIFIABLE,
                format!("Boolean condition \"{original}\" may be simplified to \"{rendered}\""),
            )
        } else {
            (
                TO_CONSTANT,
                format!("Boolean condition '{original}' will always evaluate to '{rendered}'"),
            )
        };
        if self.on(id) {
            self.out.push(self.env.ctx.report(id, expr, message));
        }
    }

    fn original(expr: &'a Expr) -> Node<'a> {
        match expr {
            Expr::BoolOp(op) => Node::Bool(op.op, op.values.iter().map(Self::original).collect()),
            other => Node::Expr(other),
        }
    }

    fn simplify(op: &'a ExprBoolOp, changed: &mut bool) -> Node<'a> {
        let children: Vec<Node<'a>> = op
            .values
            .iter()
            .map(|child| match child {
                Expr::BoolOp(inner) => Self::simplify(inner, changed),
                other => Node::Expr(other),
            })
            .collect();
        let count = children.len();
        let or = op.op == BoolOp::Or;
        let mut kept = Vec::new();
        let mut short = None;
        for child in children {
            let truth = if has_name(&child) {
                None
            } else {
                child.truth()
            };
            match truth {
                None => kept.push(child),
                Some(value) if value == or => {
                    short = Some(child);
                    break;
                }
                Some(_) => {}
            }
        }
        let result = match short {
            Some(child) => vec![child],
            None if kept.is_empty() => vec![Node::Const(!or)],
            None => kept,
        };
        if result.len() < count {
            *changed = true;
        }
        if result.len() == 1 {
            return result.into_iter().next().unwrap_or(Node::Const(!or));
        }
        Node::Bool(op.op, result)
    }

    fn render(&self, node: &Node<'a>) -> String {
        match node {
            Node::Const(value) => if *value { "True" } else { "False" }.to_owned(),
            Node::Expr(expr) => {
                let text = self.show(expr);
                if matches!(
                    expr,
                    Expr::If(_)
                        | Expr::Lambda(_)
                        | Expr::Named(_)
                        | Expr::Yield(_)
                        | Expr::YieldFrom(_)
                ) {
                    format!("({text})")
                } else {
                    text
                }
            }
            Node::Bool(op, values) => {
                let sep = if *op == BoolOp::And { " and " } else { " or " };
                values
                    .iter()
                    .map(|v| match v {
                        Node::Bool(BoolOp::Or, _) if *op == BoolOp::And => {
                            format!("({})", self.render(v))
                        }
                        _ => self.render(v),
                    })
                    .collect::<Vec<_>>()
                    .join(sep)
            }
        }
    }

    // R1738 and R1739
    fn comparisons(&mut self, expr: &'a Expr, op: &'a ExprBoolOp) {
        if op.op != BoolOp::And
            || op.values.len() < 2
            || (!self.on(ALL_EQUAL) && !self.on(IMPOSSIBLE))
        {
            return;
        }
        let Some(graphs) = self.graphs(op) else {
            return;
        };
        for graph in graphs {
            let cycles = graph.cycles();
            if cycles.is_empty() {
                continue;
            }
            let mut equal = Vec::new();
            for mut cycle in cycles {
                // start the cycle at a name, so that it reads `a == 5`
                let first = cycle
                    .iter()
                    .position(|&n| matches!(graph.nodes[n], Operand::Name(_)))
                    .unwrap_or(0);
                cycle.rotate_left(first);
                let all_ge = (0..cycle.len()).all(|i| {
                    graph.symbols.get(&(cycle[i], cycle[(i + 1) % cycle.len()])) == Some(&">=")
                });
                if all_ge {
                    equal.push(cycle);
                } else {
                    if self.on(IMPOSSIBLE) {
                        self.out.push(self.env.ctx.report(
                            IMPOSSIBLE,
                            expr,
                            "This comparison always evaluates to False",
                        ));
                    }
                    return;
                }
            }
            if self.on(ALL_EQUAL) {
                for cycle in equal {
                    let text: Vec<String> =
                        cycle.iter().map(|&n| graph.nodes[n].to_string()).collect();
                    self.out.push(self.env.ctx.report(
                        ALL_EQUAL,
                        expr,
                        format!("Simplify chained comparison cycle to equality between the operands: {}", text.join(" == ")),
                    ));
                }
            }
            return;
        }
    }

    /// Pylint's `_segment_comparisons`: the graphs of the runs of foldable
    /// comparisons, or none when the condition has nothing to chain.
    fn graphs(&self, op: &'a ExprBoolOp) -> Option<Vec<Graph>> {
        let usable = self.usable_texts(op);
        let mut graphs: Vec<Graph> = Vec::new();
        let mut current = Graph::default();
        for value in &op.values {
            if self.add_compare(value, &mut current, &usable) {
                continue;
            }
            if current.sources > 0 {
                graphs.push(std::mem::take(&mut current));
            }
            current = Graph::default();
        }
        if current.sources > 0 {
            graphs.push(current);
        }
        if graphs.is_empty()
            || graphs.iter().any(|g| g.nodes.len() > 64)
            || graphs
                .iter()
                .all(|g| g.symbols.values().all(|s| *s == "=="))
        {
            return None;
        }
        graphs.iter_mut().for_each(Graph::link_constants);
        Some(graphs)
    }

    fn usable_texts(&self, op: &ExprBoolOp) -> HashSet<String> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut impure: HashSet<&str> = HashSet::new();
        for value in &op.values {
            let Expr::Compare(compare) = value else {
                continue;
            };
            for operand in &compare.operands {
                if matches!(operand, Expr::Name(_)) || number(operand).is_some() {
                    continue;
                }
                let text = self.text(operand);
                *counts.entry(text).or_default() += 1;
                if has_impure(operand) {
                    impure.insert(text);
                }
            }
        }
        counts
            .into_iter()
            .filter(|(text, count)| *count == 1 || !impure.contains(text))
            .map(|(text, _)| text.to_owned())
            .collect()
    }

    /// Pylint's `_add_compare_to_graph`.
    fn add_compare(&self, value: &Expr, graph: &mut Graph, usable: &HashSet<String>) -> bool {
        let Expr::Compare(ExprCompare {
            ops, operands: all, ..
        }) = value
        else {
            return false;
        };
        let mut operands = Vec::new();
        for operand in all {
            let found = match operand {
                Expr::Name(n) => Operand::Name(n.id.to_string()),
                other => {
                    if let Some(n) = number(other) {
                        n
                    } else {
                        let text = self.text(other);
                        if usable.contains(text) && !is_identifier(text) {
                            Operand::Name(text.to_owned())
                        } else {
                            return false;
                        }
                    }
                }
            };
            operands.push(found);
        }
        if ops.iter().any(|op| {
            !matches!(
                op,
                CmpOp::Lt | CmpOp::Gt | CmpOp::Eq | CmpOp::LtE | CmpOp::GtE
            )
        }) {
            return false;
        }
        if operands
            .windows(2)
            .any(|pair| pair[0].key() == pair[1].key())
        {
            return false;
        }
        for (i, op) in ops.iter().enumerate() {
            let (mut larger, mut smaller) = (graph.id(&operands[i]), graph.id(&operands[i + 1]));
            let symbol = match op {
                CmpOp::Gt => ">",
                CmpOp::GtE => ">=",
                CmpOp::Eq => "==",
                CmpOp::Lt => {
                    std::mem::swap(&mut larger, &mut smaller);
                    ">"
                }
                _ => {
                    std::mem::swap(&mut larger, &mut smaller);
                    ">="
                }
            };
            graph.edges[larger].insert(smaller);
            graph.symbols.insert((larger, smaller), symbol);
        }
        for operand in &operands {
            if let Operand::Num(..) = operand {
                graph.consts.push(operand.value());
            }
        }
        graph.sources += 1;
        true
    }
}

/// An operand of a comparison: a name, or the text of an expression that
/// reads like one value, or a number.
#[derive(Clone)]
enum Operand {
    Name(String),
    Num(f64, String),
}

impl Operand {
    fn key(&self) -> (bool, String) {
        match self {
            Operand::Name(name) => (false, name.clone()),
            Operand::Num(value, _) => (true, format!("{}", *value + 0.0)),
        }
    }

    fn value(&self) -> f64 {
        match self {
            Operand::Num(value, _) => *value,
            Operand::Name(_) => 0.0,
        }
    }
}

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Name(name) => f.write_str(name),
            Operand::Num(_, text) => f.write_str(text),
        }
    }
}

/// The number that `expr` is written as, with a sign.
fn number(expr: &Expr) -> Option<Operand> {
    match expr {
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub | UnaryOp::UAdd) => {
            let Operand::Num(value, text) = number(&u.operand)? else {
                return None;
            };
            Some(if u.op == UnaryOp::USub {
                Operand::Num(-value, format!("-{text}"))
            } else {
                Operand::Num(value, text)
            })
        }
        Expr::NumberLiteral(n) => match &n.value {
            Number::Int(i) => {
                let text = i.to_string();
                Some(Operand::Num(text.parse().ok()?, text))
            }
            Number::Float(f) => Some(Operand::Num(*f, format!("{f:?}"))),
            Number::Complex { .. } => None,
        },
        _ => None,
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn has_impure(expr: &Expr) -> bool {
    struct Impure(bool);
    impl<'a> Visitor<'a> for Impure {
        fn visit_expr(&mut self, expr: &'a Expr) {
            if matches!(
                expr,
                Expr::Await(_)
                    | Expr::Call(_)
                    | Expr::Named(_)
                    | Expr::Yield(_)
                    | Expr::YieldFrom(_)
            ) {
                self.0 = true;
            }
            visitor::walk_expr(self, expr);
        }
    }
    let mut impure = Impure(false);
    impure.visit_expr(expr);
    impure.0
}

/// Pylint's `_ComparisonGraph`, with the operands as ids: the edge from
/// the larger operand to the smaller, and its symbol.
#[derive(Default)]
struct Graph {
    nodes: Vec<Operand>,
    index: HashMap<(bool, String), usize>,
    edges: Vec<BTreeSet<usize>>,
    symbols: HashMap<(usize, usize), &'static str>,
    consts: Vec<f64>,
    sources: usize,
}

impl Graph {
    fn id(&mut self, operand: &Operand) -> usize {
        let key = operand.key();
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        self.nodes.push(operand.clone());
        self.edges.push(BTreeSet::new());
        self.index.insert(key, self.nodes.len() - 1);
        self.nodes.len() - 1
    }

    /// Pylint's `_link_constants`: a `>` edge between the numbers of one
    /// component of the graph.
    fn link_constants(&mut self) {
        // the component of each operand, whichever way the edges point
        let mut component: Vec<Option<usize>> = vec![None; self.nodes.len()];
        for start in 0..self.nodes.len() {
            if component[start].is_some() {
                continue;
            }
            let mut todo = vec![start];
            while let Some(node) = todo.pop() {
                if component[node].is_some() {
                    continue;
                }
                component[node] = Some(start);
                todo.extend((0..self.nodes.len()).filter(|&other| {
                    self.edges[node].contains(&other) || self.edges[other].contains(&node)
                }));
            }
        }
        let by_value = |graph: &Graph, value: f64| {
            graph
                .index
                .get(&Operand::Num(value, String::new()).key())
                .copied()
        };
        let mut sorted = self.consts.clone();
        sorted.sort_by(f64::total_cmp);
        while let Some(largest) = sorted.pop() {
            let mut distinct: Vec<f64> = sorted.clone();
            distinct.dedup();
            for smaller in distinct {
                if smaller >= largest {
                    continue;
                }
                let (Some(big), Some(small)) = (by_value(self, largest), by_value(self, smaller))
                else {
                    continue;
                };
                if component[big] != component[small] {
                    continue;
                }
                self.symbols.insert((big, small), ">");
                self.edges[big].insert(small);
                let names: Vec<usize> = self.edges[small]
                    .iter()
                    .copied()
                    .filter(|&n| matches!(self.nodes[n], Operand::Name(_)))
                    .collect();
                for name in names {
                    self.edges[big].remove(&name);
                }
            }
        }
    }

    /// Pylint's `get_cycles`, over the operands ordered by their text.
    fn cycles(&self) -> Vec<Vec<usize>> {
        let order = |n: usize| self.nodes[n].to_string();
        let mut result: Vec<Vec<usize>> = Vec::new();
        for start in 0..self.nodes.len() {
            let mut path = Vec::new();
            let mut visited = HashSet::new();
            self.walk(start, &mut path, &mut visited, &mut result, &order);
        }
        result
    }

    fn walk(
        &self,
        node: usize,
        path: &mut Vec<usize>,
        visited: &mut HashSet<usize>,
        result: &mut Vec<Vec<usize>>,
        order: &dyn Fn(usize) -> String,
    ) {
        if let Some(at) = path.iter().position(|&n| n == node) {
            let mut cycle = path[at..].to_vec();
            // the canonical form starts at the smallest operand
            if let Some(smallest) = (0..cycle.len()).min_by_key(|&i| order(cycle[i])) {
                cycle.rotate_left(smallest);
            }
            if !result.contains(&cycle) {
                result.push(cycle);
            }
            return;
        }
        path.push(node);
        let mut next: Vec<usize> = self.edges[node].iter().copied().collect();
        next.sort_by_key(|&n| order(n));
        for n in next {
            if !visited.contains(&n) {
                self.walk(n, path, visited, result, order);
                visited.insert(n);
            }
        }
        path.pop();
    }
}

/// A condition during its simplification.
enum Node<'a> {
    Expr(&'a Expr),
    Const(bool),
    Bool(BoolOp, Vec<Node<'a>>),
}

impl Node<'_> {
    fn truth(&self) -> Option<bool> {
        match self {
            Node::Const(value) => Some(*value),
            Node::Expr(expr) => inferred_truth(expr),
            Node::Bool(..) => None,
        }
    }
}

fn has_name(node: &Node<'_>) -> bool {
    struct Names(bool);
    impl<'a> Visitor<'a> for Names {
        fn visit_expr(&mut self, expr: &'a Expr) {
            self.0 |= matches!(expr, Expr::Name(_));
            visitor::walk_expr(self, expr);
        }
    }
    match node {
        Node::Const(_) => false,
        Node::Expr(expr) => {
            let mut names = Names(false);
            names.visit_expr(expr);
            names.0
        }
        Node::Bool(_, values) => values.iter().any(has_name),
    }
}

/// The truth of an expression that holds no name: a literal, a container,
/// or `not`, `-` and `+` of one.
fn inferred_truth(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::UnaryOp(u) => match u.op {
            UnaryOp::Not => inferred_truth(&u.operand).map(|t| !t),
            UnaryOp::USub | UnaryOp::UAdd => number(expr).map(|n| n.value() != 0.0),
            UnaryOp::Invert => None,
        },
        other => const_truth(other),
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::If(s) => {
                self.test(s.range(), &s.test);
                for clause in &s.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.test(clause.range(), test);
                    }
                }
            }
            Stmt::While(s) => {
                if matches!(s.test.as_ref(), Expr::BoolOp(_)) {
                    self.tests.insert(address(&s.test));
                }
            }
            Stmt::Assert(s) => {
                if matches!(s.test.as_ref(), Expr::BoolOp(_)) {
                    self.tests.insert(address(&s.test));
                }
            }
            Stmt::Return(s) => {
                if let Some(value) = &s.value {
                    self.and_or(stmt, value);
                }
            }
            Stmt::Assign(s) => self.and_or(stmt, &s.value),
            Stmt::FunctionDef(func) => {
                self.frames.push(func);
                visitor::walk_stmt(self, stmt);
                self.frames.pop();
                return;
            }
            Stmt::ClassDef(class) => {
                self.classes.push(class);
                visitor::walk_stmt(self, stmt);
                self.classes.pop();
                return;
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        for test in &comprehension.ifs {
            self.test(comprehension.range(), test);
        }
        visitor::walk_comprehension(self, comprehension);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::If(e) => self.test(expr.range(), &e.test),
            Expr::Call(call) => {
                // `bool(a and b)` tests its argument
                if matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "bool") {
                    for arg in &call.arguments.args {
                        if matches!(arg, Expr::BoolOp(_)) {
                            self.tests.insert(address(arg));
                        }
                    }
                }
            }
            Expr::BoolOp(op) => {
                if self.tests.contains(&address(expr)) {
                    self.simplifiable(expr, op);
                }
                self.comparisons(expr, op);
            }
            Expr::Compare(compare) if self.on(BAD_CHAIN) => {
                let mut ops: Vec<&str> = compare.ops.iter().map(CmpOp::as_str).collect();
                ops.sort_unstable();
                ops.dedup();
                let group = |op: &str| match op {
                    "<" | "<=" | ">" | ">=" | "!=" | "==" => 0,
                    "is" | "is not" => 1,
                    _ => 2,
                };
                if ops.iter().any(|op| group(op) != group(ops[0])) {
                    let (last, rest) = ops.split_last().unwrap_or((&"", &[]));
                    let quoted: Vec<String> = rest.iter().map(|o| format!("'{o}'")).collect();
                    self.out.push(self.env.ctx.report(
                        BAD_CHAIN,
                        expr,
                        format!(
                            "Suspicious {}-part chained comparison using semantically incompatible operators ({} and '{last}')",
                            compare.ops.len(),
                            quoted.join(", ")
                        ),
                    ));
                }
            }
            Expr::UnaryOp(u) if u.op == UnaryOp::Not && self.on(NEGATION) => {
                self.negation(expr, &u.operand);
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

impl<'a> Walk<'a, '_> {
    // C0117
    fn negation(&mut self, expr: &'a Expr, operand: &'a Expr) {
        let ctx = self.env.ctx;
        match operand {
            Expr::UnaryOp(inner) if inner.op == UnaryOp::Not => {
                self.out.push(ctx.report(
                    NEGATION,
                    expr,
                    format!(
                        "Consider changing \"{}\" to \"{}\"",
                        self.show(expr),
                        self.show(&inner.operand)
                    ),
                ));
            }
            Expr::Compare(compare) if compare.ops.len() == 1 => {
                let reverse = match compare.ops[0] {
                    CmpOp::Lt => ">=",
                    CmpOp::LtE => ">",
                    CmpOp::Gt => "<=",
                    CmpOp::GtE => "<",
                    CmpOp::Eq => "!=",
                    CmpOp::NotEq => "==",
                    CmpOp::In => "not in",
                    CmpOp::NotIn => "in",
                    CmpOp::Is => "is not",
                    CmpOp::IsNot => "is",
                };
                let (left, right) = (&compare.operands[0], &compare.operands[1]);
                if self
                    .frames
                    .last()
                    .is_some_and(|f| f.name.as_str() == "__ne__")
                    && compare.ops[0] == CmpOp::Eq
                {
                    return;
                }
                if !(self.typed(left) && self.typed(right)) {
                    return;
                }
                self.out.push(ctx.report(
                    NEGATION,
                    expr,
                    format!(
                        "Consider changing \"{}\" to \"{} {reverse} {}\"",
                        self.show(expr),
                        self.show(left),
                        self.show(right)
                    ),
                ));
            }
            _ => {}
        }
    }
}

impl Walk<'_, '_> {
    /// Whether Pylint gives the expression a type that is not a set: a
    /// literal other than `None` (which `node_type` skips), a call of a
    /// builtin class, `*args` or `**kwargs`, or a name
    /// bound once to a literal or to such a call.
    fn typed(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Name(name) => {
                let Some(binding) = self.env.binding(name) else {
                    return false;
                };
                match binding.kind {
                    BindingKind::Parameter => self.frames.iter().any(|f| {
                        let p = &f.parameters;
                        p.vararg
                            .iter()
                            .chain(&p.kwarg)
                            .any(|v| v.name.as_str() == name.id.as_str())
                    }),
                    BindingKind::Assignment if self.env.ctx.model.is_sole_binding(binding.id) => {
                        binding
                            .node
                            .and_then(|id| self.assigned.get(&id))
                            .is_some_and(|value| {
                                matches!(value, Expr::Call(_)) && self.typed(value)
                                    || literal_type(value)
                            })
                    }
                    _ => false,
                }
            }
            Expr::Attribute(attr) => self.typed_attribute(&attr.value, attr.attr.as_str()),
            // calling a builtin class makes an instance of it; `len` of a
            // known value is a number
            Expr::Call(call) => {
                self.env
                    .qualified(&call.func)
                    .is_some_and(|name| match name.as_str() {
                        "str" | "int" | "float" | "bool" | "bytes" | "dict" | "list" | "tuple"
                        | "map" | "filter" | "zip" | "enumerate" | "reversed" | "range" => true,
                        "len" => matches!(&call.arguments.args[..], [only] if self.typed(only)),
                        _ => false,
                    })
            }
            other => literal_type(other),
        }
    }
}

impl Walk<'_, '_> {
    /// `self.__dict__`, or `self.name` / `cls.name` where the enclosing class
    /// assigns `name` once, to a literal.
    ///
    /// shortcut: a base class that assigns `name` too is not seen, upgrade
    /// with the inference of the bases.
    fn typed_attribute(&self, base: &Expr, name: &str) -> bool {
        let (Expr::Name(base), Some(func), Some(class)) =
            (base, self.frames.last(), self.classes.last())
        else {
            return false;
        };
        let first = func
            .parameters
            .posonlyargs
            .iter()
            .chain(&func.parameters.args)
            .next();
        if first.is_none_or(|p| p.name().as_str() != base.id.as_str()) {
            return false;
        }
        if is_static(func) {
            return false;
        }
        if name == "__dict__" {
            return true;
        }
        let mut found = AttrAssigns {
            name,
            values: Vec::new(),
            stores: 0,
            in_fn: false,
        };
        for stmt in &class.body {
            if let Stmt::Assign(a) = stmt {
                for target in &a.targets {
                    if matches!(target, Expr::Name(n) if n.id.as_str() == name) {
                        found.values.push(&a.value);
                    }
                }
            }
        }
        found.visit_body(&class.body);
        // any other store (`+=`, unpacking, `for`, `with`, a `def`) types it differently
        matches!(found.values[..], [only] if literal_type(only)) && found.stores == 1
    }
}

/// The values assigned to `.name` on any object in a class, and how many
/// times `name` is bound at all (as an attribute anywhere, as a name or a
/// `def` in the class body).
struct AttrAssigns<'a> {
    name: &'a str,
    values: Vec<&'a Expr>,
    stores: usize,
    in_fn: bool,
}

impl<'a> Visitor<'a> for AttrAssigns<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Assign(assign) = stmt {
            for target in &assign.targets {
                if matches!(target, Expr::Attribute(a) if a.attr.as_str() == self.name) {
                    self.values.push(&assign.value);
                }
            }
        }
        let outer = self.in_fn;
        if let Stmt::FunctionDef(f) = stmt {
            self.stores += usize::from(!outer && f.name.as_str() == self.name);
            self.in_fn = true;
        }
        visitor::walk_stmt(self, stmt);
        self.in_fn = outer;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        let stored = match expr {
            Expr::Attribute(a) => a.attr.as_str() == self.name && a.ctx == ExprContext::Store,
            Expr::Name(n) => {
                !self.in_fn && n.id.as_str() == self.name && n.ctx == ExprContext::Store
            }
            _ => false,
        };
        self.stores += usize::from(stored);
        visitor::walk_expr(self, expr);
    }
}

/// The value of every simple assignment, by the node id of each name it binds.
pub(super) struct Assigned<'a, 'm>(pub &'m mut HashMap<u32, &'a Expr>);

impl<'a> Visitor<'a> for Assigned<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Assign(assign) = stmt {
            for target in &assign.targets {
                if let Expr::Name(name) = target
                    && let Some(id) = name.node_index().load().as_u32()
                {
                    self.0.insert(id, &assign.value);
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

fn literal_type(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::BooleanLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Dict(_)
    )
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;
    use crate::native::flow::BATCH_C;

    #[test]
    fn conditions_and_comparisons() {
        let source = include_str!("fixtures/conditions.py");
        let expected = include_str!("fixtures/conditions.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }
}
