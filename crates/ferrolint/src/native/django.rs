//! The `pylint_django` messages that the syntax decides: `E5101`
//! model-unicode-not-callable, `E5141` hard-coded-auth-user, `E5142`
//! imported-auth-user, `R5101` http-response-with-json-dumps, `R5103`
//! redundant-content-type-for-json-response, `W5102` model-has-unicode,
//! `W5103` model-no-explicit-unicode, `W5104` modelform-uses-exclude,
//! `W5150` format-html-interpolation and `W5151` mark-safe-interpolation.
//! They report only when `load-plugins` names `pylint_django`; `R5102`
//! lives with the statement rules and shares the gate.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, Operator, Stmt, StmtClassDef};
use ruff_text_size::{Ranged, TextRange};

use super::classes::{Env, bases};
use super::{Checker, Ctx, rule};

const NOT_CALLABLE: &str = "E5101";
const AUTH_USER: &str = "E5141";
const IMPORTED_USER: &str = "E5142";
const JSON_DUMPS: &str = "R5101";
const REDUNDANT_TYPE: &str = "R5103";
const HAS_UNICODE: &str = "W5102";
const NO_EXPLICIT: &str = "W5103";
const EXCLUDE: &str = "W5104";
const FORMAT_HTML: &str = "W5150";
const MARK_SAFE: &str = "W5151";

const INTERPOLATION: &str = "Pass interpolated values as arguments to format_html() instead of building the format string; arguments are automatically HTML-escaped";

/// Whether the configuration loads the `pylint_django` plugin.
pub fn plugin(ctx: &Ctx<'_>) -> bool {
    ctx.option("load-plugins")
        .is_some_and(|list| list.split(',').any(|name| name.trim() == "pylint_django"))
}

pub struct Django;

impl Checker for Django {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                NOT_CALLABLE,
                "model-unicode-not-callable",
                "A model's `__unicode__` assigned something that is not callable.",
            ),
            rule(
                AUTH_USER,
                "hard-coded-auth-user",
                "The string `'auth.User'`.",
            ),
            rule(
                IMPORTED_USER,
                "imported-auth-user",
                "`User` imported from `django.contrib.auth.models`.",
            ),
            rule(
                JSON_DUMPS,
                "http-response-with-json-dumps",
                "`HttpResponse(json.dumps(data))`.",
            ),
            rule(
                REDUNDANT_TYPE,
                "redundant-content-type-for-json-response",
                "`JsonResponse(content_type=...)`.",
            ),
            rule(
                HAS_UNICODE,
                "model-has-unicode",
                "A model that defines `__unicode__`.",
            ),
            rule(
                NO_EXPLICIT,
                "model-no-explicit-unicode",
                "A model that only inherits `__unicode__`.",
            ),
            rule(
                EXCLUDE,
                "modelform-uses-exclude",
                "A `ModelForm` whose `Meta` sets `exclude`.",
            ),
            rule(
                FORMAT_HTML,
                "format-html-interpolation",
                "`format_html()` with an interpolated format string.",
            ),
            rule(
                MARK_SAFE,
                "mark-safe-interpolation",
                "`mark_safe()` of an interpolated string.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            NOT_CALLABLE,
            AUTH_USER,
            IMPORTED_USER,
            JSON_DUMPS,
            REDUNDANT_TYPE,
            HAS_UNICODE,
            NO_EXPLICIT,
            EXCLUDE,
            FORMAT_HTML,
            MARK_SAFE,
        ];
        if !plugin(ctx) || !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            docstrings: Vec::new(),
        };
        walk.docstring(&ctx.parsed.syntax().body);
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'c> {
    env: &'a Env<'c>,
    enabled: &'a BTreeSet<String>,
    out: &'a mut Vec<Diagnostic>,
    /// Ranges of the docstrings, which Pylint's `Const` visit does not see.
    docstrings: Vec<TextRange>,
}

/// The last dotted segment of `text`.
fn last(text: &str) -> &str {
    text.rsplit('.').next().unwrap_or(text)
}

fn assigned_name(stmt: &Stmt) -> Option<(&str, &Expr)> {
    let Stmt::Assign(assign) = stmt else {
        return None;
    };
    match assign.targets.first() {
        Some(Expr::Name(name)) => Some((name.id.as_str(), &assign.value)),
        _ => None,
    }
}

/// Whether a value is a literal, which no call can be made of.
fn literal(value: &Expr) -> bool {
    matches!(
        value,
        Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::FString(_)
            | Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Dict(_)
            | Expr::Set(_)
    )
}

fn interpolated(expr: &Expr) -> bool {
    match expr {
        Expr::FString(f) => f
            .value
            .f_strings()
            .any(|part| part.elements.interpolations().next().is_some()),
        Expr::BinOp(op) => match op.op {
            Operator::Mod => true,
            Operator::Add => {
                let (left, right) = (&*op.left, &*op.right);
                let constant = |e: &Expr| {
                    matches!(
                        e,
                        Expr::StringLiteral(_)
                            | Expr::BytesLiteral(_)
                            | Expr::NumberLiteral(_)
                            | Expr::BooleanLiteral(_)
                            | Expr::NoneLiteral(_)
                            | Expr::EllipsisLiteral(_)
                    )
                };
                (matches!(left, Expr::StringLiteral(_)) || matches!(right, Expr::StringLiteral(_)))
                    && (!constant(left) || !constant(right))
            }
            _ => false,
        },
        Expr::Call(call) => {
            matches!(&*call.func, Expr::Attribute(a) if a.attr.as_str() == "format")
        }
        _ => false,
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn text(&self, expr: &Expr) -> &'a str {
        &self.env.ctx.source[expr.range()]
    }

    fn docstring(&mut self, body: &[Stmt]) {
        if let Some(Stmt::Expr(first)) = body.first()
            && matches!(&*first.value, Expr::StringLiteral(_))
        {
            self.docstrings.push(first.value.range());
        }
    }

    /// Whether `class` derives from a base named one of `names`, through
    /// the classes of this file. shortcut: a base is a Django class by its
    /// name, since the real ancestry is in other files; upgrade with a
    /// cross-file class lookup.
    fn derives(&self, class: &StmtClassDef, names: &[&str], depth: u8) -> bool {
        depth < 16
            && bases(class).iter().any(|base| {
                names.contains(&last(self.text(base)))
                    || self
                        .env
                        .class_of(base)
                        .is_some_and(|c| self.derives(c, names, depth + 1))
            })
    }

    fn ancestors(&self, class: &'a StmtClassDef, found: &mut Vec<&'a StmtClassDef>, depth: u8) {
        if depth >= 16 {
            return;
        }
        for base in bases(class) {
            if let Some(c) = self.env.class_of(base)
                && !found.iter().any(|f| std::ptr::eq(*f, c))
            {
                found.push(c);
                self.ancestors(c, found, depth + 1);
            }
        }
    }

    fn model(&mut self, class: &'a StmtClassDef) {
        if !self.derives(class, &["Model"], 0) {
            return;
        }
        let name = class.name.as_str();
        for stmt in &class.body {
            if let Stmt::ClassDef(meta) = stmt
                && meta.name.as_str() == "Meta"
                && meta.body.iter().any(|s| {
                    assigned_name(s).is_some_and(|(n, v)| {
                        n == "abstract"
                            && match v {
                                Expr::BooleanLiteral(b) => b.value,
                                Expr::StringLiteral(s) => !s.value.to_str().is_empty(),
                                _ => false,
                            }
                    })
                })
            {
                return;
            }
            if let Some(("__unicode__", value)) = assigned_name(stmt) {
                if self.on(NOT_CALLABLE) && literal(value) {
                    self.out.push(self.env.ctx.report_at(
                        NOT_CALLABLE,
                        class.name.range(),
                        format!("__unicode__ on a model must be callable ({name})"),
                    ));
                }
                return;
            }
            if let Stmt::FunctionDef(f) = stmt
                && f.name.as_str() == "__unicode__"
            {
                if self.on(HAS_UNICODE) {
                    self.out.push(self.env.ctx.report_at(
                        HAS_UNICODE,
                        class.name.range(),
                        format!(
                            "Found __unicode__ method on model ({name}). Python3 uses __str__."
                        ),
                    ));
                }
                return;
            }
        }
        if !self.on(NO_EXPLICIT) {
            return;
        }
        let methods = |c: &StmtClassDef| -> Vec<String> {
            c.body
                .iter()
                .filter_map(|s| match s {
                    Stmt::FunctionDef(f) => Some(f.name.to_string()),
                    _ => None,
                })
                .collect()
        };
        let mut seen = methods(class);
        let mut found = Vec::new();
        self.ancestors(class, &mut found, 0);
        for ancestor in found {
            let compat = ancestor.decorator_list.iter().any(
                |d| matches!(&d.expression, Expr::Name(n) if n.id.as_str() == "python_2_unicode_compatible"),
            );
            for method in methods(ancestor) {
                if seen.contains(&method) {
                    continue;
                }
                if method == "__unicode__" || (method == "__str__" && compat) {
                    self.out.push(self.env.ctx.report_at(
                        NO_EXPLICIT,
                        class.name.range(),
                        format!("Model does not explicitly define __unicode__ ({name})"),
                    ));
                    return;
                }
                seen.push(method);
            }
        }
    }

    fn model_form(&mut self, class: &StmtClassDef) {
        if !self.on(EXCLUDE) || !self.derives(class, &["ModelForm"], 0) {
            return;
        }
        let meta = class.body.iter().find_map(|s| match s {
            Stmt::ClassDef(c) if c.name.as_str() == "Meta" => Some(c),
            _ => None,
        });
        if let Some(meta) = meta
            && let Some(stmt) = meta
                .body
                .iter()
                .find(|s| assigned_name(s).is_some_and(|(n, _)| n == "exclude"))
        {
            self.out.push(self.env.ctx.report(
                EXCLUDE,
                stmt,
                "Use explicit fields instead of exclude in ModelForm",
            ));
        }
    }

    fn call(&mut self, call: &ExprCall) {
        let ctx = self.env.ctx;
        let func = self.text(&call.func);
        let args = &call.arguments.args;
        let keyword = |name: &str| {
            call.arguments
                .keywords
                .iter()
                .any(|k| k.arg.as_ref().is_some_and(|a| a.as_str() == name))
        };
        if self.on(JSON_DUMPS)
            && func.ends_with("HttpResponse")
            && matches!(args.first(), Some(Expr::Call(inner)) if self.text(&inner.func) == "json.dumps")
        {
            self.out.push(ctx.report(
                JSON_DUMPS,
                call,
                "Instead of HttpResponse(json.dumps(data)) use JsonResponse(data)",
            ));
        }
        if self.on(REDUNDANT_TYPE) && func.ends_with("JsonResponse") && keyword("content_type") {
            self.out.push(ctx.report(
                REDUNDANT_TYPE,
                call,
                "Redundant content_type parameter for JsonResponse()",
            ));
        }
        let (candidate, id) = if func.ends_with("format_html_join") && args.len() >= 2 {
            (&args[1], FORMAT_HTML)
        } else if func.ends_with("format_html") && !args.is_empty() {
            (&args[0], FORMAT_HTML)
        } else if func.ends_with("mark_safe") && !args.is_empty() {
            (&args[0], MARK_SAFE)
        } else {
            return;
        };
        if self.on(id) && interpolated(candidate) {
            let message = if id == FORMAT_HTML {
                INTERPOLATION
            } else {
                "Avoid mark_safe() on a dynamically-built string; use format_html() with arguments so values are HTML-escaped"
            };
            self.out.push(ctx.report(id, call, message));
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                self.docstring(&class.body);
                self.model(class);
                self.model_form(class);
            }
            Stmt::FunctionDef(func) => self.docstring(&func.body),
            Stmt::ImportFrom(import)
                if self.on(IMPORTED_USER)
                    && import
                        .module
                        .as_ref()
                        .is_some_and(|m| m.as_str() == "django.contrib.auth.models")
                    && import
                        .names
                        .iter()
                        .any(|n| matches!(n.name.as_str(), "*" | "User")) =>
            {
                self.out.push(self.env.ctx.report(
                    IMPORTED_USER,
                    import,
                    "User model imported from django.contrib.auth.models",
                ));
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        let auth = |text: &str, range: TextRange, this: &mut Self| {
            if this.on(AUTH_USER) && text == "auth.User" && !this.docstrings.contains(&range) {
                this.out.push(
                    this.env
                        .ctx
                        .report_at(AUTH_USER, range, "Hard-coded 'auth.User'"),
                );
            }
        };
        match expr {
            Expr::StringLiteral(s) => auth(s.value.to_str(), expr.range(), self),
            Expr::FString(f) => {
                for part in f.value.f_strings() {
                    for literal in part.elements.literals() {
                        auth(&literal.value, literal.range, self);
                    }
                }
            }
            Expr::Call(call) => self.call(call),
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    const IDS: &[&str] = &[
        "E5101", "E5141", "E5142", "R5101", "R5102", "R5103", "W5102", "W5103", "W5104", "W5150",
        "W5151",
    ];

    #[test]
    fn django_messages_match_pylint() {
        let source = include_str!("fixtures/django_plugin.py");
        let expected = include_str!("fixtures/django_plugin.expected").trim_end();
        let options = [("load-plugins", "pylint_django")];
        assert_eq!(render(source, &options, IDS), expected);
    }

    #[test]
    fn silent_without_the_plugin() {
        let source = include_str!("fixtures/django_plugin.py");
        assert_eq!(render(source, &[], IDS), "");
        assert_eq!(render(source, &[("load-plugins", "other")], IDS), "");
    }

    #[test]
    fn plugin_may_be_one_of_a_list() {
        let source = "from django.contrib.auth.models import User\n";
        let options = [("load-plugins", "a, pylint_django")];
        assert_eq!(
            render(source, &options, IDS),
            "1:E5142:User model imported from django.contrib.auth.models"
        );
    }

    #[test]
    fn f_string_and_docstring_cases() {
        let source = include_str!("fixtures/django_auth_user_strings.py");
        let expected = include_str!("fixtures/django_auth_user_strings.expected").trim_end();
        let options = [("load-plugins", "pylint_django")];
        assert_eq!(render(source, &options, IDS), expected);
    }
}
