//! Pylint's operation rules, answered by ty: a value that does not support
//! subscripting, calling, iterating, `in` or an operator.
//!
//! A rule starts from the diagnostic ty gives for a node, found by its range
//! and its shape, and keeps it only when the operand has a single concrete
//! type (see [`criterion::single`]): ty also reports a union that fails in one
//! member, and Pylint does not.

use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{
    CmpOp, Comprehension, Expr, ExprContext, ExprRef, Stmt, StmtAugAssign, StmtFor,
};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::ProgramFile;
use ty_python_semantic::types::Type;
use ty_python_semantic::{Db, HasType, ProgramEnvironment, SemanticModel, check_file};

use super::{Types, criterion};
use crate::native::{self, Ctx, astring::as_string};

/// The rules of this module: Pylint's msg-id, symbol and summary.
const RULES: &[(&str, &str, &str)] = &[
    (
        "E0633",
        "unpacking-non-sequence",
        "Unpacking a value that is not a sequence.",
    ),
    (
        "E1102",
        "not-callable",
        "A call of a value that cannot be called.",
    ),
    (
        "E1130",
        "invalid-unary-operand-type",
        "A unary operator on a type that does not support it.",
    ),
    (
        "E1131",
        "unsupported-binary-operation",
        "A binary operator on types that do not support it.",
    ),
    (
        "E1133",
        "not-an-iterable",
        "Iterating over a value that is not iterable.",
    ),
    (
        "E1135",
        "unsupported-membership-test",
        "`in` on a value that does not support membership tests.",
    ),
    (
        "E1136",
        "unsubscriptable-object",
        "Subscripting a value that does not support it.",
    ),
    (
        "E1137",
        "unsupported-assignment-operation",
        "Assigning to an item of a value that does not support it.",
    ),
    (
        "E1138",
        "unsupported-delete-operation",
        "Deleting an item of a value that does not support it.",
    ),
];

pub fn rules() -> Vec<RuleInfo> {
    RULES
        .iter()
        .map(|(id, symbol, summary)| native::rule(id, symbol, summary))
        .collect()
}

/// Whether any of these rules is enabled.
pub fn wanted(enabled: &BTreeSet<String>) -> bool {
    RULES.iter().any(|(id, ..)| enabled.contains(*id))
}

/// The diagnostics of the enabled rules on the file at `ctx.path`.
pub fn check(types: &Types, ctx: &Ctx<'_>, enabled: &BTreeSet<String>) -> Vec<Diagnostic> {
    types
        .with_file(ctx.path, ctx.source, |db, file| {
            scan(db, file, ctx, enabled)
        })
        .unwrap_or_default()
}

fn scan(
    db: &dyn Db,
    file: ProgramFile<'_>,
    ctx: &Ctx<'_>,
    enabled: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let Ok(diagnostics) = check_file(db, file) else {
        return Vec::new();
    };
    let found: HashSet<(&str, TextRange)> = diagnostics
        .iter()
        .filter_map(|d| Some((d.id().as_str(), d.primary_span()?.range()?)))
        .collect();
    if found.is_empty() {
        return Vec::new();
    }
    let module = parsed_module(db, file.python_file(db)).load(db);
    let model = SemanticModel::new(db, file);
    let env = model.program_environment();
    let mut scan = Scan {
        ctx,
        enabled,
        found: &found,
        db,
        env: &env,
        model: &model,
        out: Vec::new(),
    };
    scan.visit_body(module.suite());
    scan.out
}

struct Scan<'a, 'db> {
    ctx: &'a Ctx<'a>,
    enabled: &'a BTreeSet<String>,
    found: &'a HashSet<(&'a str, TextRange)>,
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    model: &'a SemanticModel<'db>,
    out: Vec<Diagnostic>,
}

impl<'db> Scan<'_, 'db> {
    /// The type of `expr` when it is one concrete type.
    fn single(&self, expr: &Expr) -> Option<Type<'db>> {
        let ty = ExprRef::from(expr).inferred_type(self.model)?;
        criterion::single(self.db, self.env, ty).then_some(ty)
    }

    /// Whether `rule` is enabled and ty reported `lint` over exactly `range`.
    fn reported(&self, rule: &str, lint: &str, range: TextRange) -> bool {
        self.enabled.contains(rule) && self.found.contains(&(lint, range))
    }

    fn report(&mut self, rule: &str, range: TextRange, message: String) {
        self.out.push(self.ctx.report_at(rule, range, message));
    }

    fn text(&self, expr: &Expr) -> String {
        as_string(self.ctx.source, expr)
    }

    fn for_iter(&mut self, iter: &Expr) {
        if self.reported("E1133", "not-iterable", iter.range()) && self.single(iter).is_some() {
            let message = format!(
                "Non-iterable value {} is used in an iterating context",
                self.text(iter)
            );
            self.report("E1133", iter.range(), message);
        }
    }

    fn aug_assign(&mut self, stmt: &StmtAugAssign) {
        let range = stmt.range();
        if self.reported("E1131", "unsupported-operator", range)
            && self.single(&stmt.target).is_some()
            && self.single(&stmt.value).is_some()
        {
            let message = format!("unsupported operand type(s) for {}=", stmt.op.as_str());
            self.report("E1131", range, message);
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for Scan<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::For(StmtFor { iter, .. }) => self.for_iter(iter),
            Stmt::Assign(assign)
                if assign
                    .targets
                    .iter()
                    .any(|target| matches!(target, Expr::Tuple(_) | Expr::List(_)))
                    && self.reported("E0633", "not-iterable", assign.value.range())
                    && self.single(&assign.value).is_some() =>
            {
                self.report(
                    "E0633",
                    assign.range(),
                    "Attempting to unpack a non-sequence".into(),
                );
            }
            Stmt::AugAssign(aug) => self.aug_assign(aug),
            _ => {}
        }
        source_order::walk_stmt(self, stmt);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        self.for_iter(&comprehension.iter);
        source_order::walk_comprehension(self, comprehension);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        let range = expr.range();
        match expr {
            Expr::Subscript(subscript) => {
                let (rule, lint, message) = match subscript.ctx {
                    ExprContext::Load => (
                        "E1136",
                        "not-subscriptable",
                        "Value '{}' is unsubscriptable",
                    ),
                    ExprContext::Del => (
                        "E1138",
                        "not-subscriptable",
                        "'{}' does not support item deletion",
                    ),
                    ExprContext::Store | ExprContext::Invalid => (
                        "E1137",
                        "invalid-assignment",
                        "'{}' does not support item assignment",
                    ),
                };
                if self.reported(rule, lint, range) && self.single(&subscript.value).is_some() {
                    let message = message.replace("{}", &self.text(&subscript.value));
                    self.report(rule, range, message);
                }
            }
            Expr::Call(call) => {
                if self.reported("E1102", "call-non-callable", range)
                    && self.single(&call.func).is_some()
                {
                    let message = format!("{} is not callable", self.text(&call.func));
                    self.report("E1102", range, message);
                }
            }
            Expr::Compare(compare) => {
                if let ([CmpOp::In | CmpOp::NotIn], [right]) =
                    (&*compare.ops, compare.comparators())
                    && self.reported("E1135", "unsupported-operator", range)
                    && self.single(right).is_some()
                {
                    let message = format!(
                        "Value '{}' doesn't support membership test",
                        self.text(right)
                    );
                    self.report("E1135", range, message);
                }
            }
            Expr::UnaryOp(unary) => {
                if self.reported("E1130", "unsupported-operator", range)
                    && let Some(ty) = self.single(&unary.operand)
                {
                    let message = format!(
                        "bad operand type for unary {}: {}",
                        unary.op.as_str(),
                        ty.display(self.db, self.env)
                    );
                    self.report("E1130", range, message);
                }
            }
            Expr::BinOp(binary)
                if self.reported("E1131", "unsupported-operator", range)
                    && self.single(&binary.left).is_some()
                    && self.single(&binary.right).is_some() =>
            {
                let message = format!("unsupported operand type(s) for {}", binary.op.as_str());
                self.report("E1131", range, message);
            }
            _ => {}
        }
        source_order::walk_expr(self, expr);
    }
}
