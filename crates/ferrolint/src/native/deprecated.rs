//! `W4901` to `W4906`: the uses of deprecated modules, functions, arguments,
//! classes, decorators and attributes of the standard library, against the
//! tables Pylint ships (`pylint_deprecated.rs`, dumped by `make
//! pylint-deprecated`).
//!
//! shortcut: Pylint infers what each name stands for; here a name is what the
//! imports say it is, spelled through the table's `ALIASES`. A callee or an
//! attribute owner that is a local variable, an instance or a call result is
//! not followed; build the type model first, then drop this limit.

use std::collections::{BTreeSet, HashMap, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt};
use ruff_text_size::Ranged;

use super::classes::{Env, py_repr};
use super::project::Project;
use super::{Checker, Ctx, rule};
use crate::pylint_deprecated as table;
use crate::wit::BindingKind;

const MODULE: &str = "W4901";
const METHOD: &str = "W4902";
const CLASS: &str = "W4904";

pub struct Deprecated;

impl Checker for Deprecated {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                MODULE,
                "deprecated-module",
                "A module marked as deprecated is imported.",
            ),
            rule(
                METHOD,
                "deprecated-method",
                "A deprecated function or method is called.",
            ),
            rule(
                CLASS,
                "deprecated-class",
                "A deprecated class is used or imported.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![MODULE, METHOD, CLASS]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let mut walk = Walk {
            ctx,
            env: Env::new(ctx),
            out,
            enabled,
            tables: Tables::new(ctx),
            project: Project::new(ctx.path, ctx.module),
            guarded: HashSet::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// The entries of Pylint's tables that apply to the target version.
struct Tables {
    modules: BTreeSet<String>,
    methods: HashSet<&'static str>,
    /// Per module, the deprecated classes of the newest version that lists it.
    classes: HashMap<&'static str, &'static [&'static str]>,
}

impl Tables {
    fn new(ctx: &Ctx<'_>) -> Self {
        // Pylint reads the version of the interpreter it runs on; without a
        // `py-version` the reference interpreter of the tables stands in.
        let target = ctx.py_version.unwrap_or(table::REFERENCE);
        let mut modules: BTreeSet<String> = ctx
            .option("deprecated-modules")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        let mut methods = HashSet::new();
        let mut classes = HashMap::new();
        for (since, names) in table::MODULES {
            if *since <= target {
                modules.extend(names.iter().map(|name| (*name).to_owned()));
            }
        }
        for (since, names) in table::METHODS {
            if *since <= target {
                methods.extend(names.iter().copied());
            }
        }
        for (since, entries) in table::CLASSES {
            if *since <= target {
                classes.extend(entries.iter().copied());
            }
        }
        Self {
            modules,
            methods,
            classes,
        }
    }
}

/// The qualified names the table knows `spelling` by.
fn aliases(spelling: &str) -> impl Iterator<Item = &'static str> + '_ {
    let start = table::ALIASES.partition_point(|(public, _)| *public < spelling);
    table::ALIASES[start..]
        .iter()
        .take_while(move |(public, _)| *public == spelling)
        .map(|(_, qualified)| *qualified)
}

/// `if sys.version_info ...` and `if six.PY2`: Pylint's version guards.
fn is_sys_guard(test: &Expr) -> bool {
    let dotted = |expr: &Expr, owner: &str, attr: &[&str]| {
        matches!(expr, Expr::Attribute(a)
            if attr.contains(&a.attr.as_str())
                && matches!(&*a.value, Expr::Name(n) if n.id.as_str() == owner))
    };
    match test {
        Expr::Compare(compare) => {
            let left = match compare.operands.first() {
                Some(Expr::Subscript(subscript)) => Some(&*subscript.value),
                other => other,
            };
            left.is_some_and(|left| dotted(left, "sys", &["version_info"]))
        }
        other => dotted(other, "six", &["PY2", "PY3"]),
    }
}

struct Walk<'a, 'c> {
    ctx: &'a Ctx<'c>,
    env: Env<'a>,
    out: &'a mut Vec<Diagnostic>,
    enabled: &'a BTreeSet<String>,
    tables: Tables,
    /// Resolves a relative import to the module it names.
    project: Project,
    /// The `from` imports directly under a version guard, by offset.
    guarded: HashSet<u32>,
}

impl Walk<'_, '_> {
    fn module(&mut self, node: &impl Ranged, path: &str) {
        if !self.enabled.contains(MODULE) {
            return;
        }
        for name in &self.tables.modules {
            if path == name
                || path
                    .strip_prefix(name.as_str())
                    .is_some_and(|r| r.starts_with('.'))
            {
                let message = format!("Deprecated module {}", py_repr(path));
                self.out.push(self.ctx.report(MODULE, node, message));
            }
        }
    }

    fn class(&mut self, node: &impl Ranged, module: &str, names: &[&str]) {
        if !self.enabled.contains(CLASS) {
            return;
        }
        let Some(deprecated) = self.tables.classes.get(module) else {
            return;
        };
        for name in names.iter().filter(|name| deprecated.contains(name)) {
            let message = format!("Using deprecated class {name} of module {module}");
            self.out.push(self.ctx.report(CLASS, node, message));
        }
    }

    fn call(&mut self, call: &ExprCall) {
        let name = match &*call.func {
            Expr::Attribute(attribute) => {
                // Pylint reads the owner's spelling, not what it is bound to.
                if let Expr::Name(owner) = &*attribute.value {
                    self.class(call, owner.id.as_str(), &[attribute.attr.as_str()]);
                }
                attribute.attr.as_str()
            }
            Expr::Name(name) => name.id.as_str(),
            _ => return,
        };
        if self.enabled.contains(MODULE)
            && name == "__import__"
            && self.env.qualified(&call.func).as_deref() == Some("__import__")
            && let [Expr::StringLiteral(module)] = &*call.arguments.args
        {
            self.module(call, module.value.to_str());
        }
        if !self.enabled.contains(METHOD) {
            return;
        }
        let qualified = self.env.qualified(&call.func);
        let callees = qualified
            .as_deref()
            .map(|spelling| aliases(spelling).collect::<Vec<_>>())
            .unwrap_or_default();
        if callees
            .iter()
            .any(|qname| self.tables.methods.contains(qname))
            || self.tables.methods.contains(name) && self.is_defined(&call.func)
        {
            let message = format!("Using deprecated method {name}()");
            self.out.push(self.ctx.report(METHOD, call, message));
        }
    }

    /// Whether `func` names a function of this file or a builtin, which
    /// Pylint matches by its bare name.
    fn is_defined(&self, func: &Expr) -> bool {
        matches!(func, Expr::Name(name)
            if self.env.func_of(name).is_some()
                || self.env.binding(name).is_some_and(|b| b.kind == BindingKind::Builtin))
    }

    fn guard(&mut self, guarded: bool, body: &[Stmt]) {
        if guarded {
            let from = body
                .iter()
                .filter(|stmt| matches!(stmt, Stmt::ImportFrom(_)));
            self.guarded.extend(from.map(|stmt| stmt.start().to_u32()));
        }
    }
}

impl<'a> Visitor<'a> for Walk<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let name = alias.name.as_str();
                    self.module(stmt, name);
                    if let Some((module, class)) = name.split_once('.') {
                        self.class(stmt, module, &[class]);
                    }
                }
            }
            Stmt::ImportFrom(from) => {
                let relative = ".".repeat(from.level as usize);
                let module = from.module.as_ref().map_or("", |m| m.as_str());
                let module = self.project.absolute(&format!("{relative}{module}"));
                if module.is_empty() {
                    return visitor::walk_stmt(self, stmt);
                }
                self.module(stmt, &module);
                if !self.guarded.contains(&stmt.start().to_u32()) {
                    let names: Vec<&str> =
                        from.names.iter().map(|alias| alias.name.as_str()).collect();
                    self.class(stmt, &module, &names);
                }
            }
            Stmt::If(branch) => {
                // astroid nests an `elif` in the `else` of the `if` before it,
                // so an `else` body answers to the test above it
                let mut guard = is_sys_guard(&branch.test);
                self.guard(guard, &branch.body);
                for clause in &branch.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        guard = is_sys_guard(test);
                    }
                    self.guard(guard, &clause.body);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
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
    use super::*;
    use ferrolint_core::LineIndex;

    fn render(source: &str, version: Option<(u32, u32)>, options: &[(&str, &str)]) -> String {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = LineIndex::new(source);
        let flat = crate::ast::flatten(source, &index, parsed.syntax());
        let model =
            crate::semantic::Model::build(parsed.syntax(), &flat.nodes, source, &index, None);
        let options: Vec<(String, String)> = options
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        let ctx = Ctx {
            source,
            index: &index,
            parsed: &parsed,
            flat: &flat,
            model: &model,
            options: &options,
            py_version: version,
            module: "mod",
            path: std::path::Path::new("mod.py"),
        };
        let enabled = Deprecated.rules().into_iter().map(|r| r.code).collect();
        let mut out = Vec::new();
        Deprecated.check(&ctx, &enabled, &mut out);
        let mut found: Vec<_> = out
            .iter()
            .map(|d| (d.range.start.line, d.code.clone(), d.message.clone()))
            .collect();
        found.sort();
        found
            .iter()
            .map(|(line, code, message)| format!("{line}:{code}:{message}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn modules_methods_and_classes_match_pylint() {
        let source = include_str!("fixtures/deprecated_part1.py");
        let expected = include_str!("fixtures/deprecated_part1.expected").trim_end();
        assert_eq!(render(source, Some((3, 14)), &[]), expected);
    }

    #[test]
    fn version_picks_the_entries() {
        let source = "import imp\nimport profile\nimport cgi\nfrom ast import Num, AST\n";
        assert_eq!(render(source, Some((3, 3)), &[]), "");
        assert_eq!(
            render(source, Some((3, 4)), &[]),
            "1:W4901:Deprecated module 'imp'"
        );
        assert_eq!(
            render(source, Some((3, 15)), &[]),
            "1:W4901:Deprecated module 'imp'\n2:W4901:Deprecated module 'profile'\n\
             3:W4901:Deprecated module 'cgi'\n\
             4:W4904:Using deprecated class AST of module ast"
        );
    }

    #[test]
    fn deprecated_modules_option_adds_to_the_table() {
        let source = "import json\nimport json.tool\nimport jsonschema\n";
        let options = [("deprecated-modules", "json, other")];
        assert_eq!(
            render(source, Some((3, 14)), &options),
            "1:W4901:Deprecated module 'json'\n2:W4901:Deprecated module 'json.tool'"
        );
    }
}
