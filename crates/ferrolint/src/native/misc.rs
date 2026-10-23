//! `C0104` disallowed-name and `W3101` missing-timeout.
//!
//! shortcut: `C0104` checks a module-level name only when it gets a literal
//! constant, which is all Pylint checks for the default `bad-names`: for any
//! other value its `variable` naming regex excuses a name that follows the
//! default style. It reads an overridden method from the classes of the file.
//! `W3101` knows `requests.get` for `requests.api.get`, the one re-export a
//! default list needs. Upgrade both with an inference of calls and imports.

use std::collections::{BTreeSet, HashMap, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use regex::Regex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Comprehension, ExceptHandler, Expr, ExprContext, Operator, Stmt, StmtClassDef, StmtFunctionDef,
    UnaryOp,
};
use ruff_text_size::{Ranged, TextRange};

use super::classes::{Env, bases, is_classmethod, is_static, locals, methods};
use super::{Checker, Ctx, rule};

const DISALLOWED: &str = "C0104";
const TIMEOUT: &str = "W3101";

pub struct Misc;

impl Checker for Misc {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                DISALLOWED,
                "disallowed-name",
                "A name that `bad-names` refuses.",
            ),
            rule(
                TIMEOUT,
                "missing-timeout",
                "A call of a `timeout-methods` function without a `timeout`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let (names, timeout) = (enabled.contains(DISALLOWED), enabled.contains(TIMEOUT));
        if !names && !timeout {
            return;
        }
        let env = Env::new(ctx);
        let mut collect = Collect::default();
        collect.visit_body(&ctx.parsed.syntax().body);
        let mut walk = Walk {
            env: &env,
            names: names.then(|| Names::new(ctx)),
            timeout: timeout.then(|| timeout_methods(ctx)),
            out,
            scopes: Vec::new(),
            fallbacks: HashSet::new(),
            classes: collect.0,
        };
        // Pylint checks the module name too, at the top of the file.
        let module = ctx.module.rsplit('.').next().unwrap_or_default();
        walk.report_name(module, TextRange::default());
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// An option of Pylint's `csv` type.
pub(super) fn csv(ctx: &Ctx<'_>, key: &str, default: &str) -> Vec<String> {
    ctx.option(key)
        .unwrap_or(default)
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Pylint's `regexp_csv`: patterns split at the commas outside brackets.
pub(super) fn regexps(text: &str) -> Vec<Regex> {
    let (mut items, mut depth, mut current) = (Vec::new(), 0_i32, String::new());
    for c in text.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth <= 0 => {
                items.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    items.push(current);
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .filter_map(|item| Regex::new(&format!("^(?:{item})")).ok())
        .collect()
}

/// Python's `re.match(pattern, name)`; a pattern Rust rejects never matches.
pub(super) fn py_match(pattern: &str, name: &str) -> bool {
    regexps(pattern).iter().any(|re| re.is_match(name))
}

pub(super) struct Names {
    good: Vec<String>,
    good_rx: Vec<Regex>,
    bad: Vec<String>,
    bad_rx: Vec<Regex>,
}

impl Names {
    pub(super) fn new(ctx: &Ctx<'_>) -> Self {
        Self {
            good: csv(ctx, "good-names", "i,j,k,ex,Run,_"),
            good_rx: regexps(ctx.option("good-names-rgxs").unwrap_or_default()),
            bad: csv(ctx, "bad-names", "foo,bar,baz,toto,tutu,tata"),
            bad_rx: regexps(ctx.option("bad-names-rgxs").unwrap_or_default()),
        }
    }

    /// Whether `good-names` or `good-names-rgxs` accept `name` outright.
    pub(super) fn allowed(&self, name: &str) -> bool {
        self.good.iter().any(|n| n == name) || self.good_rx.iter().any(|r| r.is_match(name))
    }

    pub(super) fn refuses(&self, name: &str) -> bool {
        let allowed = self.allowed(name);
        !allowed
            && (self.bad.iter().any(|n| n == name) || self.bad_rx.iter().any(|r| r.is_match(name)))
    }
}

fn timeout_methods(ctx: &Ctx<'_>) -> Vec<String> {
    let default = [
        "delete", "get", "head", "options", "patch", "post", "put", "request",
    ]
    .map(|verb| format!("requests.api.{verb}"))
    .join(",");
    csv(ctx, "timeout-methods", &default)
}

#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Module,
    Function,
    Class,
    Lambda,
}

/// What the walk keeps about one frame.
struct Scope<'a> {
    frame: Frame,
    /// The parameter names, for a function.
    params: HashSet<&'a str>,
    /// The name of its `**` parameter.
    kwarg: Option<&'a str>,
    class: Option<&'a StmtClassDef>,
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    names: Option<Names>,
    timeout: Option<Vec<String>>,
    out: &'e mut Vec<Diagnostic>,
    scopes: Vec<Scope<'a>>,
    /// The names an `except ImportError` handler may rebind: the imports of
    /// its `try` body.
    fallbacks: HashSet<String>,
    /// The classes of the file by name: a base is looked up by name only,
    /// as Pylint infers it among all the bindings of the name.
    classes: HashMap<&'a str, Vec<&'a StmtClassDef>>,
}

#[derive(Default)]
struct Collect<'a>(HashMap<&'a str, Vec<&'a StmtClassDef>>);

impl<'a> Visitor<'a> for Collect<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            self.0.entry(class.name.as_str()).or_default().push(class);
        }
        visitor::walk_stmt(self, stmt);
    }
}

impl<'a> Walk<'a, '_> {
    fn ctx(&self) -> &Ctx<'a> {
        self.env.ctx
    }

    fn frame(&self) -> Frame {
        self.scopes
            .last()
            .map_or(Frame::Module, |scope| scope.frame)
    }

    fn report_name(&mut self, name: &str, range: TextRange) {
        if self.names.as_ref().is_some_and(|names| names.refuses(name)) {
            let message = format!("Disallowed name \"{name}\"");
            self.out
                .push(self.ctx().report_at(DISALLOWED, range, message));
        }
    }

    /// Whether an ancestor of `class` in this file satisfies `found`.
    fn ancestor(
        &self,
        class: &StmtClassDef,
        found: &dyn Fn(&StmtClassDef) -> bool,
        depth: u8,
    ) -> bool {
        depth < 16
            && bases(class)
                .iter()
                .filter_map(|base| match base {
                    Expr::Name(name) => self.classes.get(name.id.as_str()),
                    _ => None,
                })
                .flatten()
                .any(|base| found(base) || self.ancestor(base, found, depth + 1))
    }

    /// A name bound outside an assignment of a module constant: a variable
    /// of a function, an attribute of a class, nothing at module level.
    fn bind_name(&mut self, name: &str, range: TextRange) {
        let allowed = match self.frame() {
            Frame::Function => {
                !self.scopes.last().is_some_and(|s| s.params.contains(name))
                    && !self.fallbacks.contains(name)
            }
            Frame::Class => self
                .scopes
                .last()
                .and_then(|scope| scope.class)
                .is_none_or(|class| {
                    !self.ancestor(
                        class,
                        &|base| locals(&base.body).iter().any(|(l, _)| l == name),
                        0,
                    )
                }),
            Frame::Module | Frame::Lambda => false,
        };
        if allowed {
            self.report_name(name, range);
        }
    }

    /// The names an assignment `target = value` binds.
    fn bind(&mut self, target: &'a Expr, value: Option<&'a Expr>) {
        let mut found = Vec::new();
        target_names(target, &mut found);
        for (name, range) in found {
            // Only a constant reaches Pylint's check at module level.
            if self.frame() == Frame::Module {
                if value.is_some_and(constant) {
                    self.report_name(name, range);
                }
            } else {
                self.bind_name(name, range);
            }
        }
    }

    fn function(&mut self, func: &'a StmtFunctionDef) {
        let class = (self.frame() == Frame::Class)
            .then(|| self.scopes.last().and_then(|scope| scope.class))
            .flatten();
        let name = func.name.as_str();
        let overrides = class.is_some_and(|class| {
            self.ancestor(
                class,
                &|base| methods(&base.body).iter().any(|m| m.name.as_str() == name),
                0,
            )
        });
        if !overrides {
            self.report_name(name, func.name.range());
            for arg in &func.parameters.args {
                self.report_name(arg.parameter.name.as_str(), arg.parameter.name.range());
            }
        }
        let params = func.parameters.iter().map(|p| p.name().as_str()).collect();
        let kwarg = func.parameters.kwarg.as_ref().map(|p| p.name.as_str());
        self.scopes.push(Scope {
            frame: Frame::Function,
            params,
            kwarg,
            class: None,
        });
        let fallbacks = std::mem::take(&mut self.fallbacks);
        self.visit_body(&func.body);
        self.fallbacks = fallbacks;
        self.scopes.pop();
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        self.report_name(class.name.as_str(), class.name.range());
        if self.names.is_some() {
            for (attr, range) in instance_attrs(class) {
                let has =
                    |base: &StmtClassDef| instance_attrs(base).iter().any(|(n, _)| *n == attr);
                if !self.ancestor(class, &has, 0) {
                    self.report_name(&attr, range);
                }
            }
        }
        self.scopes.push(Scope {
            frame: Frame::Class,
            params: HashSet::new(),
            kwarg: None,
            class: Some(class),
        });
        self.visit_body(&class.body);
        self.scopes.pop();
    }

    fn comprehensions(&mut self, generators: &'a [Comprehension], elements: &[&'a Expr]) {
        for generator in generators {
            self.visit_expr(&generator.iter);
            let mut found = Vec::new();
            target_names(&generator.target, &mut found);
            for (name, range) in found {
                self.report_name(name, range);
            }
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        for element in elements {
            self.visit_expr(element);
        }
    }

    /// Whether `name` is the `**` parameter of the function that binds it.
    fn is_kwarg(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .filter(|scope| scope.frame != Frame::Class)
            .find(|scope| scope.params.contains(name))
            .is_some_and(|scope| scope.kwarg == Some(name))
    }

    fn call(&mut self, call: &'a ruff_python_ast::ExprCall) {
        let Some(methods) = &self.timeout else { return };
        // A function of this file is listed under the module's own name.
        let qualified = match (&*call.func, self.env.qualified(&call.func)) {
            (_, Some(qualified)) => qualified,
            (Expr::Name(name), None) if self.env.func_of(name).is_some() => {
                format!("{}.{}", self.ctx().module, name.id)
            }
            _ => return,
        };
        // `requests.get` re-exports `requests.api.get`.
        let listed = methods.iter().any(|m| {
            *m == qualified
                || (m.starts_with("requests.api.")
                    && qualified.strip_prefix("requests.") == m.strip_prefix("requests.api."))
        });
        if !listed {
            return;
        }
        let mut keys: Vec<&str> = Vec::new();
        for keyword in &call.arguments.keywords {
            match &keyword.arg {
                Some(arg) => keys.push(arg.as_str()),
                // A `**` of anything but a dict of strings is unknown; Pylint
                // infers the `**` parameter of a function as an empty dict.
                None => match &keyword.value {
                    Expr::Name(name) if self.is_kwarg(name.id.as_str()) => {}
                    Expr::Dict(dict) => {
                        for item in &dict.items {
                            match item.key.as_ref() {
                                Some(Expr::StringLiteral(key)) => keys.push(key.value.to_str()),
                                _ => return,
                            }
                        }
                    }
                    _ => return,
                },
            }
        }
        if !keys.contains(&"timeout") {
            let shown = &self.ctx().source[call.func.range()];
            let message = format!(
                "Missing timeout argument for method '{shown}' can cause your program to hang indefinitely"
            );
            self.out.push(self.ctx().report(TIMEOUT, call, message));
        }
    }
}

/// The names a target binds, with their ranges.
fn target_names<'a>(target: &'a Expr, out: &mut Vec<(&'a str, TextRange)>) {
    match target {
        Expr::Name(name) => out.push((name.id.as_str(), name.range)),
        Expr::Tuple(t) => t.elts.iter().for_each(|e| target_names(e, out)),
        Expr::List(l) => l.elts.iter().for_each(|e| target_names(e, out)),
        Expr::Starred(s) => target_names(&s.value, out),
        _ => {}
    }
}

/// A value Pylint infers as a `Const`: a literal, possibly signed.
fn constant(value: &Expr) -> bool {
    match value {
        Expr::NoneLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_) => true,
        Expr::UnaryOp(u) => {
            matches!(
                u.op,
                UnaryOp::USub | UnaryOp::UAdd | UnaryOp::Invert | UnaryOp::Not
            ) && constant(&u.operand)
        }
        Expr::BinOp(b) => {
            !matches!(b.op, Operator::MatMult) && constant(&b.left) && constant(&b.right)
        }
        _ => false,
    }
}

/// The `self.x` stores of a method body, by attribute name.
struct Attrs<'a>(&'a str, &'a mut Vec<(String, TextRange)>);
impl<'a> Visitor<'a> for Attrs<'_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Attribute(attr) = expr
            && attr.ctx == ExprContext::Store
            && matches!(&*attr.value, Expr::Name(n) if n.id.as_str() == self.0)
            && !self.1.iter().any(|(name, _)| name == attr.attr.as_str())
        {
            self.1.push((attr.attr.to_string(), attr.range));
        }
        visitor::walk_expr(self, expr);
    }
}

/// The attributes `self.x = ...` that the methods of `class` assign, with
/// the place of the first assignment.
pub(super) fn instance_attrs(class: &StmtClassDef) -> Vec<(String, TextRange)> {
    let mut found: Vec<(String, TextRange)> = Vec::new();
    for method in methods(&class.body) {
        if is_static(method) || is_classmethod(method) {
            continue;
        }
        let Some(first) = method
            .parameters
            .posonlyargs
            .iter()
            .chain(&method.parameters.args)
            .next()
        else {
            continue;
        };
        Attrs(first.parameter.name.as_str(), &mut found).visit_body(&method.body);
    }
    found
}

/// Whether the handler catches `ImportError`.
pub(super) fn catches_import_error(handler: &ruff_python_ast::ExceptHandlerExceptHandler) -> bool {
    handler.type_.as_ref().is_some_and(|type_| {
        struct Find(bool);
        impl<'a> Visitor<'a> for Find {
            fn visit_expr(&mut self, expr: &'a Expr) {
                if matches!(expr, Expr::Name(n) if n.id.as_str() == "ImportError") {
                    self.0 = true;
                }
                visitor::walk_expr(self, expr);
            }
        }
        let mut find = Find(false);
        find.visit_expr(type_);
        find.0
    })
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(func) => {
                for decorator in &func.decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(&func.parameters);
                if let Some(returns) = &func.returns {
                    self.visit_expr(returns);
                }
                self.function(func);
            }
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
                self.class(class);
            }
            Stmt::Assign(assign) => {
                self.visit_expr(&assign.value);
                for target in &assign.targets {
                    self.bind(target, Some(&assign.value));
                    self.visit_expr(target);
                }
            }
            Stmt::AnnAssign(assign) => {
                self.visit_expr(&assign.annotation);
                if let Some(value) = &assign.value {
                    self.visit_expr(value);
                }
                self.bind(&assign.target, assign.value.as_deref());
                self.visit_expr(&assign.target);
            }
            Stmt::AugAssign(assign) => {
                self.visit_expr(&assign.value);
                self.bind(&assign.target, None);
                self.visit_expr(&assign.target);
            }
            Stmt::For(f) => {
                self.visit_expr(&f.iter);
                self.bind(&f.target, None);
                self.visit_body(&f.body);
                self.visit_body(&f.orelse);
            }
            Stmt::With(w) => {
                for item in &w.items {
                    self.visit_expr(&item.context_expr);
                    if let Some(vars) = &item.optional_vars {
                        self.bind(vars, None);
                    }
                }
                self.visit_body(&w.body);
            }
            Stmt::Try(t) => {
                self.visit_body(&t.body);
                for handler in &t.handlers {
                    let ExceptHandler::ExceptHandler(h) = handler;
                    if let Some(type_) = &h.type_ {
                        self.visit_expr(type_);
                    }
                    let outer = std::mem::replace(
                        &mut self.fallbacks,
                        if catches_import_error(h) {
                            imported_names(&t.body)
                        } else {
                            HashSet::new()
                        },
                    );
                    if let Some(name) = &h.name {
                        self.bind_name(name.as_str(), name.range());
                    }
                    self.visit_body(&h.body);
                    self.fallbacks = outer;
                }
                self.visit_body(&t.orelse);
                self.visit_body(&t.finalbody);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(call) => {
                self.call(call);
                visitor::walk_expr(self, expr);
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
                self.scopes.push(Scope {
                    frame: Frame::Lambda,
                    params: HashSet::new(),
                    kwarg: None,
                    class: None,
                });
                self.visit_expr(&lambda.body);
                self.scopes.pop();
            }
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(name) = &*named.target {
                    self.bind_name(name.id.as_str(), name.range);
                }
            }
            Expr::ListComp(c) => self.comprehensions(&c.generators, &[&c.elt]),
            Expr::SetComp(c) => self.comprehensions(&c.generators, &[&c.elt]),
            Expr::Generator(c) => self.comprehensions(&c.generators, &[&c.elt]),
            Expr::DictComp(c) => {
                let elements: Vec<&Expr> = c
                    .key
                    .as_deref()
                    .into_iter()
                    .chain([c.value.as_ref()])
                    .collect();
                self.comprehensions(&c.generators, &elements);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// The names the statements import, with their `as` names.
pub(super) fn imported_names(body: &[Stmt]) -> HashSet<String> {
    struct Imports(HashSet<String>);
    impl<'a> Visitor<'a> for Imports {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            let aliases = match stmt {
                Stmt::Import(s) => &s.names,
                Stmt::ImportFrom(s) => &s.names,
                _ => return visitor::walk_stmt(self, stmt),
            };
            for alias in aliases {
                self.0.insert(
                    alias
                        .asname
                        .as_ref()
                        .map_or_else(|| alias.name.to_string(), ToString::to_string),
                );
            }
        }
    }
    let mut imports = Imports(HashSet::new());
    imports.visit_body(body);
    imports.0
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::classes::{render, render_at};

    #[test]
    fn disallowed_names() {
        let source = include_str!("fixtures/disallowed_names.py");
        let expected = include_str!("fixtures/disallowed_names.expected").trim_end();
        assert_eq!(render(source, &[], &["C0104"]), expected);
    }

    #[test]
    fn disallowed_module_name() {
        // the unit tests lint a module named `mod`
        assert_eq!(
            render("x = 1\n", &[("bad-names", "mod")], &["C0104"]),
            "1:C0104:Disallowed name \"mod\""
        );
    }

    #[test]
    fn disallowed_names_options() {
        let source = include_str!("fixtures/disallowed_names_options.py");
        let expected = include_str!("fixtures/disallowed_names_options.expected").trim_end();
        let options = [
            ("bad-names", "qux,Quux"),
            ("bad-names-rgxs", "tmp.*,data\\d"),
            ("good-names", "qux2"),
            ("good-names-rgxs", "tmp_ok"),
        ];
        assert_eq!(render(source, &options, &["C0104"]), expected);
    }

    #[test]
    fn missing_timeout() {
        let source = include_str!("fixtures/missing_timeout.py");
        let expected = include_str!("fixtures/missing_timeout.expected").trim_end();
        assert_eq!(render(source, &[], &["W3101"]), expected);
    }

    #[test]
    fn a_function_of_the_listed_module() {
        let source = "def request(method, url, **kwargs):\n    pass\n\n\ndef get(url, **kwargs):\n    return request(\"get\", url, **kwargs)\n\n\ndef put(url):\n    return request(\"put\", url)\n";
        let at = |module| render_at(source, Path::new("api.py"), module, &[], &["W3101"]);
        assert_eq!(
            at("requests.api"),
            "6:W3101:Missing timeout argument for method 'request' can cause your program to hang indefinitely\n10:W3101:Missing timeout argument for method 'request' can cause your program to hang indefinitely"
        );
        assert_eq!(at("other"), "");
    }

    #[test]
    fn missing_timeout_methods_option() {
        let source = include_str!("fixtures/missing_timeout_option.py");
        let expected = include_str!("fixtures/missing_timeout_option.expected").trim_end();
        let options = [("timeout-methods", "urllib.request.urlopen,requests.api.get")];
        assert_eq!(render(source, &options, &["W3101"]), expected);
    }
}
