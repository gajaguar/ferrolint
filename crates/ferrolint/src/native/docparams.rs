//! The messages of `pylint.extensions.docparams` (`W9005`, `W9015` to
//! `W9018` here): a function's parameters against the Sphinx, Epytext,
//! Google or `NumPy` docstring that documents them. They report only when
//! `load-plugins` names `pylint.extensions.docparams`, and follow the
//! options `accept-no-param-doc`, `default-docstring-type`,
//! `no-docstring-rgx`, `docstring-min-length` and `ignored-argument-names`.
//! The docstring is read by [`super::docstring`].
//!
//! shortcut: a type comment (`# type: (int) -> str`) is not read, so a
//! parameter typed only by one counts as untyped; upgrade when the parser
//! exposes type comments.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use regex::Regex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Identifier, Parameters, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::Ranged;

use super::docstring::Docstring;
use super::extensions::loaded;
use super::overrides::is_overload;
use super::{Checker, Ctx, rule};

const MULTIPLE_CONSTRUCTOR: &str = "W9005";
const MISSING_PARAM: &str = "W9015";
const MISSING_TYPE: &str = "W9016";
const DIFFERING_PARAM: &str = "W9017";
const DIFFERING_TYPE: &str = "W9018";
const ALL: [&str; 5] = [
    MULTIPLE_CONSTRUCTOR,
    MISSING_PARAM,
    MISSING_TYPE,
    DIFFERING_PARAM,
    DIFFERING_TYPE,
];

const CONSTRUCTORS: [&str; 2] = ["__init__", "__new__"];
const NOT_NEEDED: [&str; 2] = ["self", "cls"];

pub struct DocParams;

impl Checker for DocParams {
    fn rules(&self) -> Vec<RuleInfo> {
        let needs = "; needs `pylint.extensions.docparams`.";
        vec![
            rule(
                MULTIPLE_CONSTRUCTOR,
                "multiple-constructor-doc",
                &format!(
                    "Constructor parameters documented in both the class and the constructor docstring{needs}"
                ),
            ),
            rule(
                MISSING_PARAM,
                "missing-param-doc",
                &format!("A parameter the docstring's parameter section leaves out{needs}"),
            ),
            rule(
                MISSING_TYPE,
                "missing-type-doc",
                &format!("A parameter with neither a documented nor an annotated type{needs}"),
            ),
            rule(
                DIFFERING_PARAM,
                "differing-param-doc",
                &format!("A documented parameter the signature does not have{needs}"),
            ),
            rule(
                DIFFERING_TYPE,
                "differing-type-doc",
                &format!(
                    "A documented parameter type for a name the signature does not have{needs}"
                ),
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !ALL.iter().any(|id| enabled.contains(*id)) || !loaded(ctx, "docparams") {
            return;
        }
        let mut walk = Walk {
            ctx,
            enabled,
            out,
            options: Options::new(ctx),
            frames: Vec::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Options {
    accept_no_param_doc: bool,
    default_type: String,
    no_docstring: Option<Regex>,
    min_length: i64,
    ignored_arguments: Option<Regex>,
}

/// An `yn` option of Pylint.
fn yn(ctx: &Ctx<'_>, key: &str, default: bool) -> bool {
    ctx.option(key).map_or(default, |value| {
        matches!(value.trim().to_lowercase().as_str(), "y" | "yes" | "true")
    })
}

/// A regular expression option, matched from the start like `re.match`.
fn pattern(ctx: &Ctx<'_>, key: &str, default: Option<&str>) -> Option<Regex> {
    let text = ctx
        .option(key)
        .or(default)
        .filter(|text| !text.is_empty())?;
    Regex::new(&format!("^(?:{text})")).ok()
}

impl Options {
    fn new(ctx: &Ctx<'_>) -> Self {
        Self {
            accept_no_param_doc: yn(ctx, "accept-no-param-doc", true),
            default_type: ctx
                .option("default-docstring-type")
                .unwrap_or("default")
                .trim()
                .to_owned(),
            no_docstring: pattern(ctx, "no-docstring-rgx", Some("^_")),
            min_length: ctx
                .option("docstring-min-length")
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(-1),
            ignored_arguments: pattern(
                ctx,
                "ignored-argument-names",
                Some("_.*|^ignored_|^unused_"),
            ),
        }
    }
}

enum Frame<'a> {
    Function(&'a StmtFunctionDef),
    Class(&'a StmtClassDef),
}

struct Walk<'a, 'c> {
    ctx: &'c Ctx<'a>,
    enabled: &'c BTreeSet<String>,
    out: &'c mut Vec<Diagnostic>,
    options: Options,
    /// The functions and classes that enclose the node visited, the innermost last.
    frames: Vec<Frame<'a>>,
}

/// The docstring of a body: its first statement, when that is a string.
fn docstring(body: &[Stmt]) -> Option<&str> {
    match body.first()? {
        Stmt::Expr(expr) => match &*expr.value {
            Expr::StringLiteral(text) => Some(text.value.to_str()),
            _ => None,
        },
        _ => None,
    }
}

/// The parameter names a signature expects: plain ones, then `*args` and `**kwargs`.
struct Signature {
    names: BTreeSet<String>,
    ignored: BTreeSet<String>,
    not_needed_type: BTreeSet<String>,
}

impl Signature {
    fn new(parameters: &Parameters, ignored: Option<&Regex>) -> Self {
        let mut names: BTreeSet<String> = parameters
            .iter_non_variadic_params()
            .map(|p| p.parameter.name.to_string())
            .collect();
        let ignored = names
            .iter()
            .filter(|name| ignored.is_some_and(|rx| rx.is_match(name)))
            .cloned()
            .collect();
        let mut not_needed_type: BTreeSet<String> =
            NOT_NEEDED.iter().map(|name| (*name).to_owned()).collect();
        if let Some(vararg) = &parameters.vararg {
            names.insert(format!("*{}", vararg.name));
            not_needed_type.insert(format!("*{}", vararg.name));
        }
        if let Some(kwarg) = &parameters.kwarg {
            names.insert(format!("**{}", kwarg.name));
            not_needed_type.insert(format!("**{}", kwarg.name));
        }
        Self {
            names,
            ignored,
            not_needed_type,
        }
    }
}

/// `args_with_annotation`: the parameters Pylint counts as annotated. Like
/// Pylint, it reads the annotations of the plain parameters by position, so
/// a positional-only parameter with none borrows the first ones.
fn args_with_annotation(parameters: &Parameters) -> BTreeSet<String> {
    let flag = |annotation: &Option<Box<Expr>>, skip_ellipsis: bool| {
        annotation
            .as_deref()
            .is_some_and(|expr| !(skip_ellipsis && expr.is_ellipsis_literal_expr()))
    };
    let annotations: Vec<bool> = parameters
        .args
        .iter()
        .map(|p| flag(&p.parameter.annotation, true))
        .collect();
    let slice = |offset: usize, count: usize| -> Vec<bool> {
        (offset..offset + count)
            .map(|i| annotations.get(i).copied().unwrap_or(false))
            .collect()
    };
    let mut offset = 0;
    let mut result = BTreeSet::new();
    let mut add = |names: &[&str], flags: &[bool]| {
        for (name, flag) in names.iter().zip(flags) {
            if *flag {
                result.insert((*name).to_owned());
            }
        }
    };
    if !parameters.posonlyargs.is_empty() {
        let names: Vec<&str> = parameters
            .posonlyargs
            .iter()
            .map(|p| p.parameter.name.as_str())
            .collect();
        let mut flags: Vec<bool> = parameters
            .posonlyargs
            .iter()
            .map(|p| flag(&p.parameter.annotation, false))
            .collect();
        if !flags.iter().any(|f| *f) {
            flags = slice(offset, names.len());
            offset += names.len();
        }
        add(&names, &flags);
    }
    if !parameters.args.is_empty() {
        let names: Vec<&str> = parameters
            .args
            .iter()
            .map(|p| p.parameter.name.as_str())
            .collect();
        add(&names, &slice(offset, names.len()));
        offset += names.len();
    }
    if let Some(vararg) = &parameters.vararg {
        if vararg.annotation.is_some() {
            add(&[vararg.name.as_str()], &[true]);
        } else if annotations.get(offset).copied().unwrap_or(false) {
            add(&[vararg.name.as_str()], &[true]);
            offset += 1;
        }
    }
    if !parameters.kwonlyargs.is_empty() {
        let names: Vec<&str> = parameters
            .kwonlyargs
            .iter()
            .map(|p| p.parameter.name.as_str())
            .collect();
        let mut flags: Vec<bool> = parameters
            .kwonlyargs
            .iter()
            .map(|p| flag(&p.parameter.annotation, false))
            .collect();
        if !flags.iter().any(|f| *f) {
            flags = slice(offset, names.len());
        }
        add(&names, &flags);
    }
    if let Some(kwarg) = &parameters.kwarg
        && kwarg.annotation.is_some()
    {
        add(&[kwarg.name.as_str()], &[true]);
    }
    result
}

fn join(names: &BTreeSet<String>) -> String {
    names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    /// The class a method belongs to: the nearest class around the function
    /// being checked, which is not in `frames` yet.
    fn class_of_method(&self) -> Option<&'a StmtClassDef> {
        self.frames.iter().rev().find_map(|frame| match frame {
            Frame::Class(class) => Some(*class),
            Frame::Function(_) => None,
        })
    }

    fn function(&mut self, func: &'a StmtFunctionDef) {
        if is_overload(func) {
            return;
        }
        let doc = Docstring::parse(docstring(&func.body), &self.options.default_type);
        self.constructor_documented_twice(func, &doc);
        if self
            .options
            .no_docstring
            .as_ref()
            .is_some_and(|rx| rx.is_match(func.name.as_str()))
        {
            return;
        }
        if self.shorter_than_min_length(func) {
            return;
        }
        self.params(func, &doc);
    }

    fn constructor_documented_twice(&mut self, func: &StmtFunctionDef, doc: &Docstring) {
        if !self.on(MULTIPLE_CONSTRUCTOR) || !CONSTRUCTORS.contains(&func.name.as_str()) {
            return;
        }
        let Some(class) = self.class_of_method() else {
            return;
        };
        let class_doc = Docstring::parse(docstring(&class.body), &self.options.default_type);
        if class_doc.has_params() && doc.has_params() {
            self.out.push(self.ctx.report(
                MULTIPLE_CONSTRUCTOR,
                &class.name,
                format!(
                    "\"{}\" has constructor parameters documented in class and {}",
                    class.name, func.name
                ),
            ));
        }
    }

    /// `_is_shorter_than_min_length`: lines from the first decorator (or the
    /// `def`) to the line of the last nested statement.
    fn shorter_than_min_length(&self, func: &StmtFunctionDef) -> bool {
        let line =
            |offset: ruff_text_size::TextSize| i64::from(self.ctx.index.line(offset.to_u32()));
        let count = last_line(&func.body, func, self.ctx) - line(func.start());
        -1 < count && count < self.options.min_length
    }

    fn params(&mut self, func: &'a StmtFunctionDef, doc: &Docstring) {
        let mut allow_no_param = None;
        if CONSTRUCTORS.contains(&func.name.as_str())
            && let Some(class) = self.class_of_method()
        {
            let class_doc = Docstring::parse(docstring(&class.body), &self.options.default_type);
            allow_no_param =
                (class_doc.has_params() || class_doc.params_documented_elsewhere()).then_some(true);
            let class_allow =
                (doc.has_params() || doc.params_documented_elsewhere()).then_some(true);
            self.arguments_in_docstring(&class_doc, &func.parameters, &class.name, class_allow);
        }
        self.arguments_in_docstring(doc, &func.parameters, &func.name, allow_no_param);
    }

    /// `check_arguments_in_docstring`.
    fn arguments_in_docstring(
        &mut self,
        doc: &Docstring,
        parameters: &Parameters,
        warning: &Identifier,
        accept_no_param_doc: Option<bool>,
    ) {
        if doc.text().is_empty() {
            return;
        }
        let accept = accept_no_param_doc.unwrap_or(self.options.accept_no_param_doc);
        let mut tolerate_missing = doc.params_documented_elsewhere();
        let signature = Signature::new(parameters, self.options.ignored_arguments.as_ref());
        let (with_doc, mut with_type) = doc.match_param_docs();
        if with_doc.is_empty() && with_type.is_empty() && accept {
            tolerate_missing = true;
        }
        with_type.extend(args_with_annotation(parameters));
        let not_needed: BTreeSet<String> = NOT_NEEDED.iter().map(|n| (*n).to_owned()).collect();
        let with_ignored = |base: &BTreeSet<String>| -> BTreeSet<String> {
            base.union(&signature.ignored).cloned().collect()
        };
        if !tolerate_missing {
            let missing_doc: BTreeSet<_> = signature
                .names
                .difference(&with_doc)
                .filter(|n| !with_ignored(&not_needed).contains(*n))
                .cloned()
                .collect();
            let missing_type: BTreeSet<_> = signature
                .names
                .difference(&with_type)
                .filter(|n| !with_ignored(&signature.not_needed_type).contains(*n))
                .cloned()
                .collect();
            // Nothing documented at all is `missing-any-param-doc`, not these.
            let nothing = missing_doc == signature.names
                && signature.names == missing_type
                && !signature.names.is_empty();
            if !nothing {
                self.missing(
                    &with_doc,
                    MISSING_PARAM,
                    &with_ignored(&not_needed),
                    &signature,
                    warning,
                );
                self.missing(
                    &with_type,
                    MISSING_TYPE,
                    &with_ignored(&signature.not_needed_type),
                    &signature,
                    warning,
                );
            }
        }
        self.differing(&with_doc, DIFFERING_PARAM, &not_needed, &signature, warning);
        self.differing(
            &with_type,
            DIFFERING_TYPE,
            &signature.not_needed_type,
            &signature,
            warning,
        );
    }

    /// `_compare_missing_args`.
    fn missing(
        &mut self,
        found: &BTreeSet<String>,
        id: &str,
        not_needed: &BTreeSet<String>,
        signature: &Signature,
        warning: &Identifier,
    ) {
        if !self.on(id) {
            return;
        }
        // `*args` and `**kwargs` may be documented without their asterisks.
        let missing: BTreeSet<String> = signature
            .names
            .difference(found)
            .filter(|name| !not_needed.contains(*name))
            .filter(|name| !found.contains(&name.replace('*', "")))
            .cloned()
            .collect();
        if !missing.is_empty() {
            self.out.push(self.ctx.report(
                id,
                warning,
                format!("\"{}\" {}", join(&missing), tail(id)),
            ));
        }
    }

    /// `_compare_different_args`.
    fn differing(
        &mut self,
        found: &BTreeSet<String>,
        id: &str,
        not_needed: &BTreeSet<String>,
        signature: &Signature,
        warning: &Identifier,
    ) {
        if !self.on(id) {
            return;
        }
        let modified: BTreeSet<String> = signature
            .names
            .iter()
            .map(|name| {
                let bare = name.replace('*', "");
                if found.contains(&bare) {
                    bare
                } else {
                    name.clone()
                }
            })
            .collect();
        let differing: BTreeSet<String> = modified
            .symmetric_difference(found)
            .filter(|name| !not_needed.contains(*name) && !signature.names.contains(*name))
            .cloned()
            .collect();
        if !differing.is_empty() {
            self.out.push(self.ctx.report(
                id,
                warning,
                format!("\"{}\" {}", join(&differing), tail(id)),
            ));
        }
    }
}

/// What follows the names in the message of `id`.
fn tail(id: &str) -> &'static str {
    match id {
        MISSING_PARAM => "missing in parameter documentation",
        MISSING_TYPE => "missing in parameter type documentation",
        DIFFERING_PARAM => "differing in parameter documentation",
        _ => "differing in parameter type documentation",
    }
}

/// The last block of a compound statement, as `get_node_last_lineno` descends.
fn nested<'s>(stmt: &'s Stmt) -> Option<&'s [Stmt]> {
    use ruff_python_ast::ExceptHandler;
    let tail = |blocks: &[&'s [Stmt]]| blocks.iter().find(|b| !b.is_empty()).copied();
    match stmt {
        Stmt::If(s) => tail(&[
            s.elif_else_clauses.last().map_or(&[][..], |c| &c.body[..]),
            &s.body[..],
        ]),
        Stmt::For(s) => tail(&[&s.orelse[..], &s.body[..]]),
        Stmt::While(s) => tail(&[&s.orelse[..], &s.body[..]]),
        Stmt::With(s) => tail(&[&s.body[..]]),
        Stmt::Try(s) => {
            let handler = s.handlers.last().map_or(&[][..], |h| {
                let ExceptHandler::ExceptHandler(h) = h;
                &h.body[..]
            });
            tail(&[&s.finalbody[..], &s.orelse[..], handler, &s.body[..]])
        }
        Stmt::FunctionDef(s) => tail(&[&s.body[..]]),
        Stmt::ClassDef(s) => tail(&[&s.body[..]]),
        _ => None,
    }
}

/// `get_node_last_lineno`: the line where the last statement nested at the
/// end of `body` starts (the function's own line without statements).
fn last_line(body: &[Stmt], func: &StmtFunctionDef, ctx: &Ctx<'_>) -> i64 {
    let line = |stmt: &Stmt| i64::from(ctx.index.line(stmt.start().to_u32()));
    // astroid keeps the docstring out of the body.
    let body = if docstring(body).is_some() {
        &body[1..]
    } else {
        body
    };
    let Some(last) = body.last() else {
        return i64::from(ctx.index.line(func.start().to_u32()));
    };
    let mut stmt = last;
    while let Some(inner) = nested(stmt) {
        // A nested function's docstring is not a statement either.
        let inner = if matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_))
            && docstring(inner).is_some()
        {
            &inner[1..]
        } else {
            inner
        };
        match inner.last() {
            Some(next) => stmt = next,
            None => break,
        }
    }
    line(stmt)
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(func) => {
                self.function(func);
                self.frames.push(Frame::Function(func));
                visitor::walk_stmt(self, stmt);
                self.frames.pop();
            }
            Stmt::ClassDef(class) => {
                self.frames.push(Frame::Class(class));
                visitor::walk_stmt(self, stmt);
                self.frames.pop();
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    const PLUGIN: (&str, &str) = ("load-plugins", "pylint.extensions.docparams");
    const IDS: [&str; 5] = ["W9005", "W9015", "W9016", "W9017", "W9018"];

    #[test]
    fn parameters() {
        let source = include_str!("fixtures/docparams_parameters.py");
        let expected = include_str!("fixtures/docparams_parameters.expected").trim_end();
        assert_eq!(render(source, &[PLUGIN], &IDS), expected);
    }

    #[test]
    fn parameters_strict() {
        let source = include_str!("fixtures/docparams_parameters.py");
        let expected = include_str!("fixtures/docparams_parameters_strict.expected").trim_end();
        let options = [
            PLUGIN,
            ("accept-no-param-doc", "no"),
            ("no-docstring-rgx", "^$"),
        ];
        assert_eq!(render(source, &options, &IDS), expected);
    }

    #[test]
    fn constructors() {
        let source = include_str!("fixtures/docparams_constructors.py");
        let expected = include_str!("fixtures/docparams_constructors.expected").trim_end();
        assert_eq!(
            render(source, &[PLUGIN, ("no-docstring-rgx", "^$")], &IDS),
            expected
        );
    }

    #[test]
    fn min_length_and_default_type() {
        let source = include_str!("fixtures/docparams_min_length.py");
        let expected = include_str!("fixtures/docparams_min_length.expected").trim_end();
        let options = [
            PLUGIN,
            ("docstring-min-length", "6"),
            ("default-docstring-type", "google"),
        ];
        assert_eq!(render(source, &options, &IDS), expected);
    }

    #[test]
    fn needs_the_extension() {
        let source = include_str!("fixtures/docparams_parameters.py");
        assert_eq!(render(source, &[], &IDS), "");
    }
}
