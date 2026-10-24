//! The messages of `pylint.extensions.docparams` (`W9005`, `W9006`, `W9008`,
//! `W9010` to `W9021` here): a function's
//! parameters, exceptions, return and yields against the Sphinx, Epytext,
//! Google or `NumPy` docstring that documents them. They report only when
//! `load-plugins` names `pylint.extensions.docparams`, and follow the
//! options `accept-no-param-doc`, `accept-no-raise-doc`,
//! `accept-no-return-doc`, `accept-no-yields-doc`, `default-docstring-type`,
//! `no-docstring-rgx`, `docstring-min-length` and `ignored-argument-names`.
//! The docstring is read by [`super::docstring`].
//!
//! shortcut: a raised exception is the class a name or call names directly
//! (a builtin, a class of this file or of the project), not an instance held
//! in a variable nor a class a function returns; upgrade with type inference.
//!
//! A function's type comment (`# type: (int) -> str`) and the ones on its
//! parameters are read from the source, since the parser drops comments.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use regex::Regex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ExceptHandler, ExceptHandlerExceptHandler, Expr, ExprLambda, ExprYield, ExprYieldFrom,
    Identifier, Parameters, Stmt, StmtClassDef, StmtFunctionDef, StmtRaise, StmtReturn, StmtTry,
    StmtWith,
};
use ruff_text_size::Ranged;

use super::abstract_methods::raises_not_implemented;
use super::classes::{Env, bases, methods};
use super::docstring::Docstring;
use super::extensions::loaded;
use super::flow::{builtin_ancestors, builtin_exception};
use super::hierarchy::{Class, Hierarchy};
use super::misc::regex;
use super::overrides::is_overload;
use super::statements::is_generator;
use super::{Checker, Ctx, rule};

const MULTIPLE_CONSTRUCTOR: &str = "W9005";
const MISSING_RAISES: &str = "W9006";
const REDUNDANT_RETURNS: &str = "W9008";
const REDUNDANT_YIELDS: &str = "W9010";
const MISSING_RETURN: &str = "W9011";
const MISSING_RETURN_TYPE: &str = "W9012";
const MISSING_YIELD: &str = "W9013";
const MISSING_YIELD_TYPE: &str = "W9014";
const MISSING_PARAM: &str = "W9015";
const MISSING_TYPE: &str = "W9016";
const DIFFERING_PARAM: &str = "W9017";
const DIFFERING_TYPE: &str = "W9018";
const USELESS_PARAM: &str = "W9019";
const USELESS_TYPE: &str = "W9020";
const MISSING_ANY: &str = "W9021";
const ALL: [&str; 15] = [
    MULTIPLE_CONSTRUCTOR,
    MISSING_RAISES,
    REDUNDANT_RETURNS,
    REDUNDANT_YIELDS,
    MISSING_RETURN,
    MISSING_RETURN_TYPE,
    MISSING_YIELD,
    MISSING_YIELD_TYPE,
    MISSING_PARAM,
    MISSING_TYPE,
    DIFFERING_PARAM,
    DIFFERING_TYPE,
    USELESS_PARAM,
    USELESS_TYPE,
    MISSING_ANY,
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
                MISSING_RAISES,
                "missing-raises-doc",
                &format!("A raised exception the docstring does not document{needs}"),
            ),
            rule(
                REDUNDANT_RETURNS,
                "redundant-returns-doc",
                &format!("A documented return in a function that returns nothing{needs}"),
            ),
            rule(
                REDUNDANT_YIELDS,
                "redundant-yields-doc",
                &format!("Documented yields in a function that is not a generator{needs}"),
            ),
            rule(
                MISSING_RETURN,
                "missing-return-doc",
                &format!("A function that returns a value without documenting it{needs}"),
            ),
            rule(
                MISSING_RETURN_TYPE,
                "missing-return-type-doc",
                &format!("A function that returns a value without documenting its type{needs}"),
            ),
            rule(
                MISSING_YIELD,
                "missing-yield-doc",
                &format!("A generator that yields without documenting it{needs}"),
            ),
            rule(
                MISSING_YIELD_TYPE,
                "missing-yield-type-doc",
                &format!("A generator that yields without documenting the type{needs}"),
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
            rule(
                USELESS_PARAM,
                "useless-param-doc",
                &format!("A documented parameter that `ignored-argument-names` ignores{needs}"),
            ),
            rule(
                USELESS_TYPE,
                "useless-type-doc",
                &format!(
                    "A documented type of a parameter that `ignored-argument-names` ignores{needs}"
                ),
            ),
            rule(
                MISSING_ANY,
                "missing-any-param-doc",
                &format!(
                    "A function whose parameters have neither a description nor a type{needs}"
                ),
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !ALL.iter().any(|id| enabled.contains(*id)) || !loaded(ctx, "docparams") {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            ctx,
            env: &env,
            hierarchy: Hierarchy::new(&env),
            enabled,
            out,
            options: Options::new(ctx),
            frames: Vec::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

#[allow(clippy::struct_excessive_bools)] // one flag per Pylint option
struct Options {
    accept_no_param_doc: bool,
    accept_no_raise_doc: bool,
    accept_no_return_doc: bool,
    accept_no_yields_doc: bool,
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
    regex(&format!("^(?:{text})"))
}

impl Options {
    fn new(ctx: &Ctx<'_>) -> Self {
        Self {
            accept_no_param_doc: yn(ctx, "accept-no-param-doc", true),
            accept_no_raise_doc: yn(ctx, "accept-no-raise-doc", true),
            accept_no_return_doc: yn(ctx, "accept-no-return-doc", true),
            accept_no_yields_doc: yn(ctx, "accept-no-yields-doc", true),
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

/// What encloses the node visited.
enum Frame<'a> {
    Function(&'a StmtFunctionDef),
    Class(&'a StmtClassDef),
    Lambda,
    /// A `try` statement, for the statements of its body, `else` and `finally`.
    Try(&'a StmtTry),
    Handler(&'a ExceptHandlerExceptHandler),
    With(&'a StmtWith),
}

struct Walk<'a, 'e> {
    ctx: &'e Ctx<'a>,
    env: &'e Env<'a>,
    hierarchy: Hierarchy<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    options: Options,
    /// The nodes that enclose the one visited, the innermost last.
    frames: Vec<Frame<'a>>,
}

/// An exception class a `raise` names: its own name and its ancestors'.
struct Exc {
    name: String,
    ancestors: BTreeSet<String>,
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
fn args_with_annotation(parameters: &Parameters, commented: &[bool]) -> BTreeSet<String> {
    let flag = |annotation: &Option<Box<Expr>>, skip_ellipsis: bool| {
        annotation
            .as_deref()
            .is_some_and(|expr| !(skip_ellipsis && expr.is_ellipsis_literal_expr()))
    };
    // Pylint merges the annotations with the types of the type comments, by position.
    let count = parameters.args.len().max(commented.len());
    let annotations: Vec<bool> = (0..count)
        .map(|i| {
            parameters
                .args
                .get(i)
                .is_some_and(|p| flag(&p.parameter.annotation, true))
                || commented.get(i) == Some(&true)
        })
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

/// A function's type comment, `# type: (A, B) -> R`, as astroid reads it.
struct TypeComment {
    /// Whether each argument type is a type, not the `...` that Pylint skips.
    args: Vec<bool>,
}

/// The positions that a type comment types: the function's own and the ones
/// after the posonly, plain and keyword-only parameters, merged like Pylint.
fn commented(source: &str, func: &StmtFunctionDef) -> Vec<bool> {
    let own = type_comment(source, func)
        .map(|c| c.args)
        .unwrap_or_default();
    let p = &func.parameters;
    let each: Vec<bool> = p
        .posonlyargs
        .iter()
        .chain(&p.args)
        .chain(&p.kwonlyargs)
        .map(|arg| parameter_comment(source, arg.end().to_usize()))
        .collect();
    (0..own.len().max(each.len()))
        .map(|i| own.get(i) == Some(&true) || each.get(i) == Some(&true))
        .collect()
}

/// Whether a type comment follows the parameter that ends at `end`, after its
/// comma and on its line, and holds a type.
fn parameter_comment(source: &str, end: usize) -> bool {
    let Some(line) = source.get(end..).and_then(|rest| rest.split('\n').next()) else {
        return false;
    };
    let line = line.trim_start();
    let line = line.strip_prefix(',').unwrap_or(line).trim_start();
    line.strip_prefix('#')
        .and_then(|comment| comment.trim_start().strip_prefix("type:"))
        .map(|text| text.split('#').next().unwrap_or_default().trim())
        .is_some_and(|text| !text.is_empty() && !text.starts_with("ignore") && text != "...")
}

/// The type comment of `func`: after the `:` of its header on the same line,
/// or alone on the first line of the body.
fn type_comment(source: &str, func: &StmtFunctionDef) -> Option<TypeComment> {
    let from = func
        .returns
        .as_ref()
        .map_or(func.parameters.end(), |returns| returns.end());
    let rest = source.get(from.to_usize()..)?;
    let rest = &rest[rest.find(':')? + 1..];
    let (first, more) = rest.split_once('\n').unwrap_or((rest, ""));
    let comment = if first.trim().is_empty() {
        more.lines().map(str::trim).find(|line| !line.is_empty())?
    } else {
        first.trim()
    };
    let text = comment
        .strip_prefix('#')?
        .trim_start()
        .strip_prefix("type:")?;
    let text = text.trim_start();
    if text.starts_with("ignore") {
        return None;
    }
    parse_func_type(text)
}

/// `(A, B) -> R`: the argument types and a return type.
fn parse_func_type(text: &str) -> Option<TypeComment> {
    let inner = text.strip_prefix('(')?;
    let (mut depth, mut quote) = (1_u32, None);
    let (mut items, mut item, mut end) = (Vec::new(), String::new(), None);
    for (at, c) in inner.char_indices() {
        if let Some(open) = quote {
            quote = (c != open).then_some(open);
        } else {
            match c {
                '\'' | '"' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 1 => {
                    items.push(std::mem::take(&mut item));
                    continue;
                }
                _ => {}
            }
            if depth == 0 {
                end = Some(at);
                break;
            }
        }
        item.push(c);
    }
    items.push(item);
    let returns = inner[end? + 1..].trim_start().strip_prefix("->")?;
    if returns.split('#').next()?.trim().is_empty() {
        return None;
    }
    Some(TypeComment {
        args: items
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item != "...")
            .collect(),
    })
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
            _ => None,
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
        self.redundant(func, &doc);
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
            self.arguments_in_docstring(&class_doc, func, &class.name, class_allow);
        }
        self.arguments_in_docstring(doc, func, &func.name, allow_no_param);
    }

    /// `check_arguments_in_docstring`.
    fn arguments_in_docstring(
        &mut self,
        doc: &Docstring,
        func: &StmtFunctionDef,
        warning: &Identifier,
        accept_no_param_doc: Option<bool>,
    ) {
        let parameters = &func.parameters;
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
        // Only the types the docstring documents are useless, not the annotated ones.
        self.useless(&with_type, USELESS_TYPE, &signature, warning);
        with_type.extend(args_with_annotation(
            parameters,
            &commented(self.ctx.source, func),
        ));
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
            let nothing = missing_doc == signature.names
                && signature.names == missing_type
                && !signature.names.is_empty();
            if nothing {
                if self.on(MISSING_ANY) {
                    self.out.push(self.ctx.report(
                        MISSING_ANY,
                        warning,
                        format!("Missing any documentation in \"{}\"", warning.as_str()),
                    ));
                }
            } else {
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
        self.useless(&with_doc, USELESS_PARAM, &signature, warning);
    }

    /// `_compare_ignored_args`.
    fn useless(
        &mut self,
        found: &BTreeSet<String>,
        id: &str,
        signature: &Signature,
        warning: &Identifier,
    ) {
        if !self.on(id) {
            return;
        }
        let useless: BTreeSet<_> = signature.ignored.intersection(found).cloned().collect();
        if !useless.is_empty() {
            let what = if id == USELESS_PARAM { "" } else { " type" };
            self.out.push(self.ctx.report(
                id,
                warning,
                format!(
                    "\"{}\" useless ignored parameter{what} documentation",
                    join(&useless)
                ),
            ));
        }
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

/// The identifiers `expr` names anywhere inside, as `_get_name_nodes`.
fn names_in(expr: &Expr) -> BTreeSet<String> {
    struct Names(BTreeSet<String>);
    impl Visitor<'_> for Names {
        fn visit_expr(&mut self, expr: &Expr) {
            if let Expr::Name(name) = expr {
                self.0.insert(name.id.to_string());
            }
            visitor::walk_expr(self, expr);
        }
    }
    let mut names = Names(BTreeSet::new());
    names.visit_expr(expr);
    names.0
}

/// A name that `bases` of a class leave out the subscript of (`Base[T]`).
fn unsubscript(expr: &Expr) -> &Expr {
    match expr {
        Expr::Subscript(s) => &s.value,
        other => other,
    }
}

impl<'a> Walk<'a, '_> {
    /// The function, class or lambda the node visited belongs to (`frame()`).
    fn frame(&self) -> Option<&Frame<'a>> {
        self.frames
            .iter()
            .rev()
            .find(|f| matches!(f, Frame::Function(_) | Frame::Class(_) | Frame::Lambda))
    }

    /// The function the node visited belongs to, when its frame is one.
    fn function_frame(&self) -> Option<&'a StmtFunctionDef> {
        match self.frame()? {
            Frame::Function(func) => Some(*func),
            _ => None,
        }
    }

    fn skipped_by_name(&self, func: &StmtFunctionDef) -> bool {
        self.options
            .no_docstring
            .as_ref()
            .is_some_and(|rx| rx.is_match(func.name.as_str()))
    }

    fn doc_of(&self, func: &StmtFunctionDef) -> Docstring {
        Docstring::parse(docstring(&func.body), &self.options.default_type)
    }

    /// `check_functiondef_returns` and `check_functiondef_yields`.
    fn redundant(&mut self, func: &'a StmtFunctionDef, doc: &Docstring) {
        let abstract_ = self.is_abstract(func);
        let generator = is_generator(func);
        if self.on(REDUNDANT_RETURNS)
            && !(!doc.supports_yields() && generator || abstract_)
            && (doc.has_returns() || doc.has_rtype())
            && !any_return_with_value(&func.body)
        {
            self.out.push(self.ctx.report(
                REDUNDANT_RETURNS,
                &func.name,
                "Redundant returns documentation",
            ));
        }
        if self.on(REDUNDANT_YIELDS)
            && doc.supports_yields()
            && !abstract_
            && (doc.has_yields() || doc.has_yields_type())
            && !generator
        {
            self.out.push(self.ctx.report(
                REDUNDANT_YIELDS,
                &func.name,
                "Redundant yields documentation",
            ));
        }
    }

    /// astroid's `is_abstract(pass_is_abstract=True)`: an `abc` decorator, or a
    /// body that is empty, `pass`, or raises `NotImplementedError` first.
    fn is_abstract(&self, func: &StmtFunctionDef) -> bool {
        let abc = func.decorator_list.iter().any(|decorator| {
            self.env
                .qualified(&decorator.expression)
                .is_some_and(|name| name == "abc.abstractmethod" || name == "abc.abstractproperty")
        });
        let rest = if docstring(&func.body).is_some() {
            &func.body[1..]
        } else {
            &func.body[..]
        };
        abc || match rest.first() {
            None | Some(Stmt::Pass(_)) => true,
            Some(Stmt::Raise(_)) => raises_not_implemented(func),
            Some(_) => false,
        }
    }

    /// `decorated_with_property`: a `property`, a `cached_property` or a class
    /// that derives from `property`.
    fn is_property(&self, func: &StmtFunctionDef) -> bool {
        func.decorator_list.iter().any(|decorator| {
            self.env.base_is(&decorator.expression, &|name| {
                matches!(
                    name,
                    "property"
                        | "builtins.property"
                        | "functools.cached_property"
                        | "abc.abstractproperty"
                )
            })
        })
    }

    /// `get_setters_property`: the property a setter belongs to.
    fn setters_property(&self, func: &StmtFunctionDef) -> Option<&'a StmtFunctionDef> {
        let is_setter = func.decorator_list.iter().any(|decorator| {
            matches!(&decorator.expression, Expr::Attribute(a)
                if a.attr.as_str() == "setter" && a.value.is_name_expr())
        });
        let class = self.class_of_method()?;
        is_setter
            .then(|| {
                methods(&class.body)
                    .into_iter()
                    .find(|f| f.name.as_str() == func.name.as_str() && self.is_property(f))
            })
            .flatten()
    }

    fn return_(&mut self, node: &StmtReturn) {
        if !(self.on(MISSING_RETURN) || self.on(MISSING_RETURN_TYPE))
            || node.value.as_deref().is_none_or(Expr::is_none_literal_expr)
            || self.options.accept_no_return_doc
        {
            return;
        }
        let Some(func) = self.function_frame() else {
            return;
        };
        if self.skipped_by_name(func) {
            return;
        }
        let doc = self.doc_of(func);
        let property = self.is_property(func);
        if self.on(MISSING_RETURN) && !(doc.has_returns() || doc.has_property_returns() && property)
        {
            self.out.push(self.ctx.report(
                MISSING_RETURN,
                &func.name,
                "Missing return documentation",
            ));
        }
        // An annotation or a type comment gives the type.
        if self.on(MISSING_RETURN_TYPE)
            && func.returns.is_none()
            && type_comment(self.ctx.source, func).is_none()
            && !(doc.has_rtype() || doc.has_property_type() && property)
        {
            self.out.push(self.ctx.report(
                MISSING_RETURN_TYPE,
                &func.name,
                "Missing return type documentation",
            ));
        }
    }

    fn yield_(&mut self) {
        // Pylint measures a yield's own one line against the minimum, which
        // any positive minimum skips.
        if !(self.on(MISSING_YIELD) || self.on(MISSING_YIELD_TYPE))
            || self.options.accept_no_yields_doc
            || self.options.min_length > 0
        {
            return;
        }
        let Some(func) = self.function_frame() else {
            return;
        };
        if self.skipped_by_name(func) {
            return;
        }
        let doc = self.doc_of(func);
        let (documented, typed) = if doc.supports_yields() {
            (doc.has_yields(), doc.has_yields_type())
        } else {
            (doc.has_returns(), doc.has_rtype())
        };
        if self.on(MISSING_YIELD) && !documented {
            self.out.push(self.ctx.report(
                MISSING_YIELD,
                &func.name,
                "Missing yield documentation",
            ));
        }
        if self.on(MISSING_YIELD_TYPE)
            && !(typed || func.returns.is_some() || type_comment(self.ctx.source, func).is_some())
        {
            self.out.push(self.ctx.report(
                MISSING_YIELD_TYPE,
                &func.name,
                "Missing yield type documentation",
            ));
        }
    }

    fn raise(&mut self, node: &StmtRaise) {
        if !self.on(MISSING_RAISES) || self.options.min_length > 0 {
            return;
        }
        let Some(func) = self.function_frame() else {
            return;
        };
        if self.skipped_by_name(func) {
            return;
        }
        let expected = self.raised(node);
        if expected.is_empty() {
            return;
        }
        // A setter documents nothing: its property does.
        let func = if docstring(&func.body).is_none() {
            self.setters_property(func).unwrap_or(func)
        } else {
            func
        };
        let doc = self.doc_of(func);
        let found = doc.exceptions();
        if self.options.accept_no_raise_doc && found.is_empty() {
            return;
        }
        let mut missing: BTreeSet<String> = BTreeSet::new();
        if doc.matching_sections() == 0 {
            if doc.text().is_empty() {
                return;
            }
            missing.extend(expected.iter().map(|exc| exc.name.clone()));
        } else {
            let found: Vec<&str> = found
                .iter()
                .map(|name| name.rsplit('.').next().unwrap_or_default())
                .collect();
            for exc in &expected {
                let documented = found.iter().any(|name| {
                    *name == exc.name
                        // `re.error` is `re.PatternError` since Python 3.13.
                        || (*name == "error" && exc.name == "PatternError")
                        || exc.ancestors.contains(*name)
                });
                if !documented {
                    missing.insert(exc.name.clone());
                }
            }
        }
        if self.is_abstract(func) {
            missing.remove("NotImplementedError");
        }
        if !missing.is_empty() {
            self.out.push(self.ctx.report(
                MISSING_RAISES,
                &func.name,
                format!("\"{}\" not documented as being raised", join(&missing)),
            ));
        }
    }

    /// `possible_exc_types`, less the exceptions the surroundings handle.
    fn raised(&self, node: &StmtRaise) -> Vec<Exc> {
        let mut found = Vec::new();
        match node.exc.as_deref() {
            Some(exc @ Expr::Name(_)) => found.extend(self.exception(exc)),
            Some(Expr::Call(call)) if matches!(&*call.func, Expr::Name(_) | Expr::Attribute(_)) => {
                found.extend(self.exception(&call.func));
            }
            Some(_) => {}
            None => {
                let handler = self.frames.iter().rev().find_map(|frame| match frame {
                    Frame::Handler(handler) => Some(*handler),
                    _ => None,
                });
                if let Some(kind) = handler.and_then(|h| h.type_.as_deref()) {
                    match kind {
                        Expr::Tuple(tuple) => {
                            found.extend(tuple.elts.iter().filter_map(|e| self.exception(e)));
                        }
                        other => found.extend(self.exception(other)),
                    }
                }
            }
        }
        found.retain(|exc| !self.ignores(&exc.name));
        found
    }

    /// The exception class `expr` names: a builtin, a class of this file or of
    /// the project, or a class attribute reached through `self`.
    fn exception(&self, expr: &Expr) -> Option<Exc> {
        self.named_exception(expr)
            .or_else(|| self.class_attribute(expr))
    }

    /// `self.Error`: a class nested in the class of the method, or assigned in
    /// its body to a class (`Error = errors.Error`).
    fn class_attribute(&self, expr: &Expr) -> Option<Exc> {
        let Expr::Attribute(attribute) = expr else {
            return None;
        };
        let Expr::Name(base) = &*attribute.value else {
            return None;
        };
        let parameters = &self.function_frame()?.parameters;
        let first = parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .next()?;
        if first.parameter.name.as_str() != base.id.as_str() {
            return None;
        }
        let attr = attribute.attr.as_str();
        self.class_of_method()?.body.iter().find_map(|stmt| match stmt {
            Stmt::ClassDef(nested) if nested.name.as_str() == attr => Some(Exc {
                name: attr.to_owned(),
                ancestors: self.ancestors(&Class::Local(nested), 0),
            }),
            Stmt::Assign(assign)
                if matches!(&assign.targets[..], [Expr::Name(n)] if n.id.as_str() == attr) =>
            {
                self.named_exception(&assign.value)
            }
            _ => None,
        })
    }

    fn named_exception(&self, expr: &Expr) -> Option<Exc> {
        if let Some(class) = self.hierarchy.resolve_here(expr) {
            let name = class.def()?.name.to_string();
            return Some(Exc {
                name,
                ancestors: self.ancestors(&class, 0),
            });
        }
        let builtin = builtin_exception(&self.env.qualified(expr)?)?;
        Some(Exc {
            name: builtin.to_owned(),
            ancestors: builtin_ancestors(builtin)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        })
    }

    /// The names of the classes `class` derives from.
    fn ancestors(&self, class: &Class<'a>, depth: u8) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        if depth > 16 {
            return out;
        }
        let builtin = |name: &str, out: &mut BTreeSet<String>| {
            out.extend(builtin_ancestors(name).into_iter().map(str::to_owned));
        };
        match class {
            Class::Local(def) => {
                for base in bases(def) {
                    let base = unsubscript(base);
                    if let Some(parent) = self.hierarchy.resolve_here(base) {
                        out.extend(parent.def().map(|d| d.name.to_string()));
                        out.extend(self.ancestors(&parent, depth + 1));
                    } else if let Some(name) = self.env.qualified(base) {
                        builtin(&name, &mut out);
                    }
                }
            }
            Class::Remote(remote) => {
                // `bases` leaves out a builtin, which no import names.
                for base in remote.class().map_or(&[][..], bases) {
                    if let Expr::Name(name) = unsubscript(base)
                        && remote.qualify(base).is_none()
                    {
                        builtin(name.id.as_str(), &mut out);
                    }
                }
                for base in remote.bases(&self.hierarchy.project) {
                    match base {
                        Ok(parent) => {
                            let parent = Class::Remote(parent);
                            out.extend(parent.def().map(|d| d.name.to_string()));
                            out.extend(self.ancestors(&parent, depth + 1));
                        }
                        Err(name) => builtin(&name, &mut out),
                    }
                }
            }
        }
        out
    }

    /// `node_ignores_exception`: the nearest `try` around the node has an
    /// `except` that names `name`, or a `contextlib.suppress` around it does.
    fn ignores(&self, name: &str) -> bool {
        let handled = match self
            .frames
            .iter()
            .rev()
            .find(|f| matches!(f, Frame::Try(_) | Frame::Handler(_)))
        {
            Some(Frame::Try(statement)) => statement.handlers.iter().any(|handler| {
                let ExceptHandler::ExceptHandler(handler) = handler;
                handler
                    .type_
                    .as_deref()
                    .is_some_and(|kind| names_in(kind).contains(name))
            }),
            _ => false,
        };
        handled || self.suppressed(name)
    }

    fn suppressed(&self, name: &str) -> bool {
        let is = |expr: &Expr| self.exception(expr).is_some_and(|exc| exc.name == name);
        self.frames.iter().any(|frame| {
            let Frame::With(with) = frame else {
                return false;
            };
            // astroid's `AsyncWith` is a `With`.
            with.items.iter().any(|item| {
                let Expr::Call(call) = &item.context_expr else {
                    return false;
                };
                self.env.qualified(&call.func).as_deref() == Some("contextlib.suppress")
                    && call.arguments.args.iter().any(|arg| match arg {
                        Expr::Tuple(tuple) => tuple.elts.iter().any(is),
                        other => is(other),
                    })
            })
        })
    }
}

/// Whether a `return` that gives a value is in `body`, nested functions and
/// classes included, as `nodes_of_class(Return)` finds it.
fn any_return_with_value(body: &[Stmt]) -> bool {
    struct Returns(bool);
    impl<'a> Visitor<'a> for Returns {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if let Stmt::Return(ret) = stmt
                && ret
                    .value
                    .as_deref()
                    .is_some_and(|v| !v.is_none_literal_expr())
            {
                self.0 = true;
            }
            visitor::walk_stmt(self, stmt);
        }
    }
    let mut returns = Returns(false);
    returns.visit_body(body);
    returns.0
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
            Stmt::Raise(raise) => {
                self.raise(raise);
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Return(ret) => {
                self.return_(ret);
                visitor::walk_stmt(self, stmt);
            }
            Stmt::With(with) => {
                self.frames.push(Frame::With(with));
                visitor::walk_stmt(self, stmt);
                self.frames.pop();
            }
            Stmt::Try(statement) => {
                // astroid's `TryStar` is not a `Try`, which the lookup wants.
                let plain = !statement.is_star;
                if plain {
                    self.frames.push(Frame::Try(statement));
                }
                self.visit_body(&statement.body);
                for handler in &statement.handlers {
                    let ExceptHandler::ExceptHandler(inner) = handler;
                    self.frames.push(Frame::Handler(inner));
                    visitor::walk_except_handler(self, handler);
                    self.frames.pop();
                }
                self.visit_body(&statement.orelse);
                self.visit_body(&statement.finalbody);
                if plain {
                    self.frames.pop();
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ExprLambda { .. }) => {
                self.frames.push(Frame::Lambda);
                visitor::walk_expr(self, expr);
                self.frames.pop();
            }
            Expr::Yield(ExprYield { .. }) | Expr::YieldFrom(ExprYieldFrom { .. }) => {
                self.yield_();
                visitor::walk_expr(self, expr);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    const PLUGIN: (&str, &str) = ("load-plugins", "pylint.extensions.docparams");
    const IDS: [&str; 15] = [
        "W9005", "W9006", "W9008", "W9010", "W9011", "W9012", "W9013", "W9014", "W9015", "W9016",
        "W9017", "W9018", "W9019", "W9020", "W9021",
    ];

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

    const RETURN_IDS: [&str; 6] = ["W9008", "W9010", "W9011", "W9012", "W9013", "W9014"];

    #[test]
    fn returns_and_yields() {
        let source = include_str!("fixtures/docparams_returns.py");
        let expected = include_str!("fixtures/docparams_returns.expected").trim_end();
        assert_eq!(render(source, &[PLUGIN], &RETURN_IDS), expected);
    }

    #[test]
    fn returns_and_yields_strict() {
        let source = include_str!("fixtures/docparams_returns.py");
        let expected = include_str!("fixtures/docparams_returns_strict.expected").trim_end();
        let options = [
            PLUGIN,
            ("accept-no-return-doc", "no"),
            ("accept-no-yields-doc", "no"),
            ("no-docstring-rgx", "^$"),
        ];
        assert_eq!(render(source, &options, &RETURN_IDS), expected);
    }

    #[test]
    fn raises() {
        let source = include_str!("fixtures/docparams_raises.py");
        let expected = include_str!("fixtures/docparams_raises.expected").trim_end();
        assert_eq!(render(source, &[PLUGIN], &["W9006"]), expected);
    }

    #[test]
    fn raises_strict() {
        let source = include_str!("fixtures/docparams_raises.py");
        let expected = include_str!("fixtures/docparams_raises_strict.expected").trim_end();
        let options = [
            PLUGIN,
            ("accept-no-raise-doc", "no"),
            ("no-docstring-rgx", "^$"),
        ];
        assert_eq!(render(source, &options, &["W9006"]), expected);
    }

    /// A raise is one line long, so any positive minimum skips it.
    #[test]
    fn raises_min_length() {
        let source = include_str!("fixtures/docparams_raises.py");
        let options = [PLUGIN, ("docstring-min-length", "2")];
        assert_eq!(render(source, &options, &["W9006"]), "");
    }

    #[test]
    fn type_comments() {
        let source = include_str!("fixtures/docparams_type_comments.py");
        let expected = include_str!("fixtures/docparams_type_comments.expected").trim_end();
        assert_eq!(render(source, &[PLUGIN], &IDS), expected);
    }

    #[test]
    fn ignored_parameters() {
        let source = include_str!("fixtures/docparams_ignored.py");
        let expected = include_str!("fixtures/docparams_ignored.expected").trim_end();
        let options = [PLUGIN, ("no-docstring-rgx", "^$")];
        assert_eq!(render(source, &options, &IDS), expected);
    }

    #[test]
    fn ignored_parameters_strict() {
        let source = include_str!("fixtures/docparams_ignored.py");
        let expected = include_str!("fixtures/docparams_ignored_strict.expected").trim_end();
        let options = [
            PLUGIN,
            ("accept-no-param-doc", "no"),
            ("no-docstring-rgx", "^$"),
        ];
        assert_eq!(render(source, &options, &IDS), expected);
    }

    #[test]
    fn ignored_parameters_custom() {
        let source = include_str!("fixtures/docparams_ignored.py");
        let expected = include_str!("fixtures/docparams_ignored_custom.expected").trim_end();
        let options = [
            PLUGIN,
            ("accept-no-param-doc", "no"),
            ("ignored-argument-names", "^(_|b$)"),
            ("no-docstring-rgx", "^$"),
        ];
        assert_eq!(render(source, &options, &IDS), expected);
    }

    #[test]
    fn needs_the_extension() {
        let source = include_str!("fixtures/docparams_parameters.py");
        assert_eq!(render(source, &[], &IDS), "");
    }
}
