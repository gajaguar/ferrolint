//! Class rules: `E0211` no-method-argument, `C0203` bad-mcs-method-argument,
//! `C0204` bad-mcs-classmethod-argument, `W0213` implicit-flag-alias and
//! `R0904` too-many-public-methods. The helpers at the top also serve the
//! slots, special-method and call rules.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Decorator, Expr, ExprName, Number, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const NO_ARGUMENT: &str = "E0211";
const MCS_METHOD: &str = "C0203";
const MCS_CLASSMETHOD: &str = "C0204";
const FLAG_ALIAS: &str = "W0213";
const PUBLIC_METHODS: &str = "R0904";

/// The classes and functions of a file by node id, with the queries that
/// name what a class base or a callee stands for.
pub struct Env<'a> {
    pub ctx: &'a Ctx<'a>,
    classes: HashMap<u32, &'a StmtClassDef>,
    funcs: HashMap<u32, &'a StmtFunctionDef>,
}

struct Collect<'a, 'm> {
    classes: &'m mut HashMap<u32, &'a StmtClassDef>,
    funcs: &'m mut HashMap<u32, &'a StmtFunctionDef>,
}

impl<'a> Visitor<'a> for Collect<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                if let Some(id) = ruff_python_ast::HasNodeIndex::node_index(class)
                    .load()
                    .as_u32()
                {
                    self.classes.insert(id, class);
                }
            }
            Stmt::FunctionDef(func) => {
                if let Some(id) = ruff_python_ast::HasNodeIndex::node_index(func)
                    .load()
                    .as_u32()
                {
                    self.funcs.insert(id, func);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

impl<'a> Env<'a> {
    pub fn new(ctx: &'a Ctx<'a>) -> Self {
        let (mut classes, mut funcs) = (HashMap::new(), HashMap::new());
        Collect {
            classes: &mut classes,
            funcs: &mut funcs,
        }
        .visit_body(&ctx.parsed.syntax().body);
        Self {
            ctx,
            classes,
            funcs,
        }
    }

    pub fn binding(&self, name: &ExprName) -> Option<crate::wit::Binding> {
        let id = ruff_python_ast::HasNodeIndex::node_index(name)
            .load()
            .as_u32()?;
        self.ctx.model.resolve(self.ctx.flat, id)
    }

    /// The dotted name `expr` stands for, with imports followed to their
    /// origin (`enum.IntFlag` for `IntFlag` after `from enum import IntFlag`).
    pub fn qualified(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Name(name) => {
                let binding = self.binding(name)?;
                match binding.kind {
                    BindingKind::Builtin => Some(binding.name),
                    BindingKind::FromImport => binding.origin,
                    // `import a.b` binds `a`, which stands for the package.
                    BindingKind::Import => {
                        let origin = binding.origin?;
                        let first = origin.split('.').next().unwrap_or_default();
                        Some(if first == binding.name {
                            binding.name
                        } else {
                            origin
                        })
                    }
                    _ => None,
                }
            }
            Expr::Attribute(attribute) => Some(format!(
                "{}.{}",
                self.qualified(&attribute.value)?,
                attribute.attr
            )),
            _ => None,
        }
    }

    /// The class of this file that `expr` names, when it is bound only once.
    pub fn class_of(&self, expr: &Expr) -> Option<&'a StmtClassDef> {
        let Expr::Name(name) = expr else { return None };
        let binding = self.binding(name)?;
        (binding.kind == BindingKind::ClassDef && self.ctx.model.is_sole_binding(binding.id))
            .then(|| self.classes.get(&binding.node?).copied())
            .flatten()
    }

    /// The function of this file that `name` calls, when it is bound only once.
    pub fn func_of(&self, name: &ExprName) -> Option<&'a StmtFunctionDef> {
        let binding = self.binding(name)?;
        (binding.kind == BindingKind::FunctionDef && self.ctx.model.is_sole_binding(binding.id))
            .then(|| self.funcs.get(&binding.node?).copied())
            .flatten()
    }

    /// Whether `base`, or a class of this file it names, is one of the
    /// classes `pred` accepts by qualified name.
    pub fn base_is(&self, base: &Expr, pred: &dyn Fn(&str) -> bool) -> bool {
        self.base_is_at(base, pred, 0)
    }

    fn base_is_at(&self, base: &Expr, pred: &dyn Fn(&str) -> bool, depth: u8) -> bool {
        if self.qualified(base).is_some_and(|name| pred(&name)) {
            return true;
        }
        depth < 16
            && self.class_of(base).is_some_and(|class| {
                bases(class)
                    .iter()
                    .any(|b| self.base_is_at(b, pred, depth + 1))
            })
    }

    /// Whether the instances of `class` are known to derive from nothing
    /// but `object` and classes of this file.
    pub fn plain(&self, class: &StmtClassDef, depth: u8) -> bool {
        depth < 16
            && bases(class).iter().all(|base| {
                self.qualified(base).is_some_and(|name| name == "object")
                    || self
                        .class_of(base)
                        .is_some_and(|c| self.plain(c, depth + 1))
            })
    }

    /// Whether `class` or a class of this file it derives from binds `name`.
    pub fn defines(&self, class: &StmtClassDef, name: &str, depth: u8) -> bool {
        locals(&class.body).iter().any(|(local, _)| local == name)
            || (depth < 16
                && bases(class)
                    .iter()
                    .filter_map(|base| self.class_of(base))
                    .any(|base| self.defines(base, name, depth + 1)))
    }
}

/// The base expressions of `class`.
pub fn bases(class: &StmtClassDef) -> &[Expr] {
    class
        .arguments
        .as_ref()
        .map_or(&[], |arguments| &arguments.args[..])
}

/// The statement lists nested directly in a compound statement.
pub fn blocks(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::If(s) => std::iter::once(&s.body[..])
            .chain(s.elif_else_clauses.iter().map(|clause| &clause.body[..]))
            .collect(),
        Stmt::For(s) => vec![&s.body[..], &s.orelse[..]],
        Stmt::While(s) => vec![&s.body[..], &s.orelse[..]],
        Stmt::With(s) => vec![&s.body[..]],
        Stmt::Try(s) => {
            let mut all = vec![&s.body[..]];
            all.extend(s.handlers.iter().map(|handler| {
                let ruff_python_ast::ExceptHandler::ExceptHandler(h) = handler;
                &h.body[..]
            }));
            all.extend([&s.orelse[..], &s.finalbody[..]]);
            all
        }
        Stmt::Match(s) => s.cases.iter().map(|case| &case.body[..]).collect(),
        _ => Vec::new(),
    }
}

/// The functions defined in a class body, also under `if`, `try` and the
/// like: Pylint's methods.
pub fn methods(body: &[Stmt]) -> Vec<&StmtFunctionDef> {
    let mut found = Vec::new();
    for stmt in body {
        if let Stmt::FunctionDef(func) = stmt {
            found.push(func);
        }
        for block in blocks(stmt) {
            found.extend(methods(block));
        }
    }
    found
}

/// What binds a name in a class body.
#[derive(Clone, Copy, PartialEq)]
pub enum Local {
    Function,
    /// `name: int`, which gives the name no value.
    Bare,
    Other,
}

pub fn target_names(target: &Expr, out: &mut Vec<(String, Local)>) {
    match target {
        Expr::Name(name) => out.push((name.id.to_string(), Local::Other)),
        Expr::Tuple(t) => t.elts.iter().for_each(|e| target_names(e, out)),
        Expr::List(l) => l.elts.iter().for_each(|e| target_names(e, out)),
        Expr::Starred(s) => target_names(&s.value, out),
        _ => {}
    }
}

/// The names a class body binds, in order, with what binds each time.
pub fn locals(body: &[Stmt]) -> Vec<(String, Local)> {
    let mut out = Vec::new();
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(f) => out.push((f.name.to_string(), Local::Function)),
            Stmt::ClassDef(c) => out.push((c.name.to_string(), Local::Other)),
            Stmt::Assign(a) => a.targets.iter().for_each(|t| target_names(t, &mut out)),
            Stmt::AugAssign(a) => target_names(&a.target, &mut out),
            Stmt::AnnAssign(a) => {
                if let Expr::Name(name) = &*a.target {
                    let kind = if a.value.is_none() {
                        Local::Bare
                    } else {
                        Local::Other
                    };
                    out.push((name.id.to_string(), kind));
                }
            }
            Stmt::For(f) => target_names(&f.target, &mut out),
            Stmt::With(w) => w
                .items
                .iter()
                .filter_map(|item| item.optional_vars.as_deref())
                .for_each(|t| target_names(t, &mut out)),
            Stmt::Try(t) => {
                for handler in &t.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(h) = handler;
                    if let Some(name) = &h.name {
                        out.push((name.to_string(), Local::Other));
                    }
                }
            }
            Stmt::Import(i) => {
                for alias in &i.names {
                    let name = alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_owned(),
                        ToString::to_string,
                    );
                    out.push((name, Local::Other));
                }
            }
            Stmt::ImportFrom(i) => {
                for alias in i.names.iter().filter(|alias| &alias.name != "*") {
                    let name = alias.asname.as_ref().unwrap_or(&alias.name);
                    out.push((name.to_string(), Local::Other));
                }
            }
            _ => {}
        }
        for block in blocks(stmt) {
            out.extend(locals(block));
        }
    }
    out
}

/// Python's `repr` of a string.
pub fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() && (c as u32) < 0x100 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python's `repr` of a constant expression, with `...` as `ellipsis`.
/// shortcut: a float prints as Rust does, so an exponent differs from
/// Python's; upgrade when a message needs one.
pub fn const_repr(expr: &Expr, ellipsis: &str) -> Option<String> {
    Some(match expr {
        Expr::NoneLiteral(_) => "None".to_owned(),
        Expr::BooleanLiteral(b) => if b.value { "True" } else { "False" }.to_owned(),
        Expr::EllipsisLiteral(_) => ellipsis.to_owned(),
        Expr::StringLiteral(s) => py_repr(s.value.to_str()),
        Expr::BytesLiteral(b) => {
            let text: String = b
                .value
                .bytes()
                .map(|byte| match byte {
                    b'\\' => "\\\\".to_owned(),
                    b'\'' => "\\'".to_owned(),
                    b'\n' => "\\n".to_owned(),
                    b'\r' => "\\r".to_owned(),
                    b'\t' => "\\t".to_owned(),
                    0x20..=0x7e => char::from(byte).to_string(),
                    _ => format!("\\x{byte:02x}"),
                })
                .collect();
            format!("b'{text}'")
        }
        Expr::NumberLiteral(n) => match &n.value {
            Number::Int(i) => i.to_string(),
            Number::Float(f) => format!("{f:?}"),
            Number::Complex { imag, .. } => format!("{imag}j"),
        },
        _ => return None,
    })
}

/// Whether the decorator is the builtin `name`, as `staticmethod` or
/// `builtins.staticmethod`.
fn is_builtin(decorator: &Decorator, name: &str) -> bool {
    match &decorator.expression {
        Expr::Name(n) => n.id.as_str() == name,
        Expr::Attribute(a) => {
            a.attr.as_str() == name
                && matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "builtins")
        }
        _ => false,
    }
}

/// `staticmethod` and `classmethod` as Pylint reads them from a method.
pub fn is_static(func: &StmtFunctionDef) -> bool {
    func.decorator_list
        .iter()
        .any(|d| is_builtin(d, "staticmethod"))
}

pub fn is_classmethod(func: &StmtFunctionDef) -> bool {
    matches!(
        func.name.as_str(),
        "__new__" | "__init_subclass__" | "__class_getitem__"
    ) || func
        .decorator_list
        .iter()
        .any(|d| is_builtin(d, "classmethod"))
}

pub struct Classes;

impl Checker for Classes {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                NO_ARGUMENT,
                "no-method-argument",
                "A method with no argument for the bound instance.",
            ),
            rule(
                MCS_METHOD,
                "bad-mcs-method-argument",
                "A metaclass method whose first argument is not in `valid-classmethod-first-arg`.",
            ),
            rule(
                MCS_CLASSMETHOD,
                "bad-mcs-classmethod-argument",
                "A metaclass class method whose first argument is not in `valid-metaclass-classmethod-first-arg`.",
            ),
            rule(
                FLAG_ALIAS,
                "implicit-flag-alias",
                "An `IntFlag` member that shares bit positions with another.",
            ),
            rule(
                PUBLIC_METHODS,
                "too-many-public-methods",
                "A class with more public methods than `max-public-methods`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            NO_ARGUMENT,
            MCS_METHOD,
            MCS_CLASSMETHOD,
            FLAG_ALIAS,
            PUBLIC_METHODS,
        ];
        if !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            nested: HashSet::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    /// The classes written directly in the body of another class, by address.
    nested: HashSet<usize>,
}

/// An option of Pylint's `csv` type.
fn csv(ctx: &Ctx<'_>, key: &str, default: &str) -> Vec<String> {
    ctx.option(key)
        .unwrap_or(default)
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Pylint's wording of the accepted names: `'a'` or `'a', 'b' or 'c'`.
fn spell(names: &[String]) -> String {
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            let rest: Vec<String> = rest.iter().map(|n| py_repr(n)).collect();
            format!("{} or {}", rest.join(", "), py_repr(last))
        }
        Some((only, _)) => py_repr(only),
        None => String::new(),
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn name_range(func: &StmtFunctionDef) -> TextRange {
        func.name.range()
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        for stmt in &class.body {
            if let Stmt::ClassDef(inner) = stmt {
                self.nested.insert(std::ptr::from_ref(inner).addr());
            }
        }
        if [NO_ARGUMENT, MCS_METHOD, MCS_CLASSMETHOD]
            .iter()
            .any(|id| self.on(id))
        {
            for method in methods(&class.body) {
                self.first_argument(class, method);
            }
        }
        if self.on(PUBLIC_METHODS) {
            self.public_methods(class);
        }
        if self.on(FLAG_ALIAS) {
            for base in bases(class) {
                if self.env.base_is(base, &|name| name == "enum.IntFlag") {
                    self.flag_alias(class);
                }
            }
        }
    }

    fn is_metaclass(&self, class: &StmtClassDef) -> bool {
        // shortcut: a metaclass is `type`, the abc and enum metaclasses or a
        // class of this file derived from one; Pylint infers any base.
        class.name.as_str() == "type"
            || bases(class).iter().any(|base| {
                self.env.base_is(base, &|name| {
                    matches!(
                        name,
                        "type" | "abc.ABCMeta" | "enum.EnumMeta" | "enum.EnumType"
                    )
                })
            })
    }

    fn first_argument(&mut self, class: &StmtClassDef, method: &StmtFunctionDef) {
        if is_static(method) {
            return;
        }
        let p = &method.parameters;
        let ctx = self.env.ctx;
        let name = method.name.as_str();
        if p.posonlyargs.is_empty() && p.args.is_empty() && p.vararg.is_none() && p.kwarg.is_none()
        {
            if self.on(NO_ARGUMENT) {
                self.out.push(ctx.report_at(
                    NO_ARGUMENT,
                    Self::name_range(method),
                    format!("Method {} has no argument", py_repr(name)),
                ));
            }
            return;
        }
        if !self.is_metaclass(class) {
            return;
        }
        let first = p
            .posonlyargs
            .first()
            .or(p.args.first())
            .map(|arg| arg.parameter.name.as_str());
        let (id, key, default, what) = if is_classmethod(method) {
            (
                MCS_CLASSMETHOD,
                "valid-metaclass-classmethod-first-arg",
                "mcs",
                "Metaclass class method",
            )
        } else {
            (
                MCS_METHOD,
                "valid-classmethod-first-arg",
                "cls",
                "Metaclass method",
            )
        };
        let valid = csv(ctx, key, default);
        if self.on(id) && !first.is_some_and(|f| valid.iter().any(|v| v == f)) {
            self.out.push(ctx.report_at(
                id,
                Self::name_range(method),
                format!(
                    "{what} {name} should have {} as first argument",
                    spell(&valid)
                ),
            ));
        }
    }

    /// `pylint_django` does not count the methods of a Django `TestCase`
    /// named `...Test` or nested in a class, nor of a `ModelAdmin` named
    /// `...Admin`. shortcut: an imported base is a test case when its name
    /// ends in `TestCase` (but not the simple, transaction, live-server or
    /// `unittest` ones), since its ancestry is in another file; upgrade with
    /// a cross-file class lookup.
    fn django_exempt(&self, class: &StmtClassDef) -> bool {
        let plugin = self
            .env
            .ctx
            .option("load-plugins")
            .is_some_and(|list| list.split(',').any(|name| name.trim() == "pylint_django"));
        let nested = self.nested.contains(&std::ptr::from_ref(class).addr());
        let name = class.name.as_str();
        let (test_case, admin) = (
            name.ends_with("Test") || nested,
            name.ends_with("Admin") && !nested,
        );
        plugin
            && bases(class).iter().any(|base| {
                self.env.base_is(base, &|qualified| {
                    let last = qualified.rsplit('.').next().unwrap_or(qualified);
                    (admin && qualified.ends_with(".ModelAdmin"))
                        || (test_case
                            && last.ends_with("TestCase")
                            && !qualified.starts_with("unittest.")
                            && !["Simple", "Transaction", "LiveServer", "IsolatedAsyncio"]
                                .iter()
                                .any(|kind| last.contains(kind)))
                })
            })
    }

    fn public_methods(&mut self, class: &StmtClassDef) {
        let ctx = self.env.ctx;
        if self.django_exempt(class) {
            return;
        }
        let max = ctx
            .option("max-public-methods")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(20);
        // Pylint counts the first binding of each name in the class.
        let mut seen = HashSet::new();
        let count = locals(&class.body)
            .into_iter()
            .filter(|(name, _)| seen.insert(name.clone()))
            .filter(|(name, kind)| *kind == Local::Function && !name.starts_with('_'))
            .count();
        if count > max {
            self.out.push(ctx.report_at(
                PUBLIC_METHODS,
                class.name.range(),
                format!("Too many public methods ({count}/{max})"),
            ));
        }
    }

    /// Pylint's `_check_enum_base` for an `IntFlag`: every integer assigned
    /// anywhere under the class, against the bits of the ones before.
    fn flag_alias(&mut self, class: &StmtClassDef) {
        let mut found = Assigned(Vec::new());
        found.visit_body(&class.body);
        let mut by_value: Vec<(u64, Vec<(&str, TextRange)>)> = Vec::new();
        for (name, range, value) in found.0 {
            match by_value.iter_mut().find(|(v, _)| *v == value) {
                Some((_, names)) => names.push((name, range)),
                None => by_value.push((value, vec![(name, range)])),
            }
        }
        // The flags that set each bit, bits in order of first appearance.
        let mut bit_flags: Vec<(u32, BTreeSet<u64>)> = Vec::new();
        for (flag, _) in &by_value {
            for bit in (0..64).filter(|bit| flag >> bit & 1 == 1) {
                match bit_flags.iter_mut().find(|(b, _)| *b == bit) {
                    Some((_, flags)) => {
                        flags.insert(*flag);
                    }
                    None => bit_flags.push((bit, BTreeSet::from([*flag]))),
                }
            }
        }
        let mut overlaps: Vec<(u64, Vec<u64>)> = Vec::new();
        for (_, flags) in &bit_flags {
            let mut flags = flags.iter();
            let Some(&source) = flags.next() else {
                continue;
            };
            for &conflict in flags {
                match overlaps.iter_mut().find(|(c, _)| *c == conflict) {
                    Some((_, sources)) => sources.push(source),
                    None => overlaps.push((conflict, vec![source])),
                }
            }
        }
        let class_name = class.name.as_str();
        let first_name = |value: u64| {
            by_value
                .iter()
                .find(|(v, _)| *v == value)
                .map_or("", |(_, names)| names[0].0)
        };
        for (overlap, sources) in &overlaps {
            let sources: Vec<String> = sources
                .iter()
                .map(|s| {
                    format!(
                        "<{class_name}.{}: {s}> ({overlap} & {s} = {})",
                        first_name(*s),
                        overlap & s
                    )
                })
                .collect();
            let names = by_value
                .iter()
                .find(|(v, _)| v == overlap)
                .map_or(&[][..], |(_, n)| &n[..]);
            for (name, range) in names {
                self.out.push(self.env.ctx.report_at(
                    FLAG_ALIAS,
                    *range,
                    format!(
                        "Flag member <{class_name}.{name}: {overlap}> shares bit positions with {}",
                        sources.join(", ")
                    ),
                ));
            }
        }
    }
}

/// Every `name = <integer>` under a class, nested scopes included.
struct Assigned<'a>(Vec<(&'a str, TextRange, u64)>);

impl<'a> Visitor<'a> for Assigned<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Assign(assign) = stmt {
            let value = match &*assign.value {
                Expr::NumberLiteral(n) => match &n.value {
                    Number::Int(i) => i.to_string().parse::<u64>().ok(),
                    _ => None,
                },
                Expr::BooleanLiteral(b) => Some(u64::from(b.value)),
                _ => None,
            };
            if let Some(value) = value {
                for target in &assign.targets {
                    if let Expr::Name(name) = target {
                        self.0.push((name.id.as_str(), name.range(), value));
                    }
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            self.class(class);
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
pub(super) fn render(source: &str, options: &[(&str, &str)], ids: &[&str]) -> String {
    render_at(source, std::path::Path::new("mod.py"), "mod", options, ids)
}

#[cfg(test)]
pub(super) fn render_at(
    source: &str,
    path: &std::path::Path,
    module: &str,
    options: &[(&str, &str)],
    ids: &[&str],
) -> String {
    use ferrolint_core::LineIndex;

    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let index = LineIndex::new(source);
    let flat = crate::ast::flatten(source, &index, parsed.syntax());
    let model = crate::semantic::Model::build(parsed.syntax(), &flat.nodes, source, &index, None);
    let options: Vec<(String, String)> = options
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let ctx = Ctx {
        source,
        index: &index,
        parsed: &parsed,
        flat: &flat,
        model: &model,
        options: &options,
        py_version: None,
        module,
        path,
    };
    let enabled = super::rules().into_iter().map(|rule| rule.code).collect();
    let mut found: Vec<(u32, String, String)> = super::check(&ctx, &enabled)
        .into_iter()
        .filter(|d| ids.contains(&d.code.as_str()))
        .map(|d| (d.range.start.line, d.code, d.message))
        .collect();
    found.sort();
    found
        .iter()
        .map(|(line, code, message)| format!("{line}:{code}:{message}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn method_without_argument() {
        let source = include_str!("fixtures/method_without_argument.py");
        let expected = include_str!("fixtures/method_without_argument.expected").trim_end();
        assert_eq!(render(source, &[], &["E0211"]), expected);
    }

    #[test]
    fn metaclass_arguments() {
        let source = include_str!("fixtures/metaclass_arguments.py");
        let expected = include_str!("fixtures/metaclass_arguments.expected").trim_end();
        assert_eq!(render(source, &[], &["C0203", "C0204"]), expected);
    }

    #[test]
    fn metaclass_argument_options() {
        let source = include_str!("fixtures/metaclass_argument_options.py");
        let expected = include_str!("fixtures/metaclass_argument_options.expected").trim_end();
        assert_eq!(
            render(
                source,
                &[
                    ("valid-classmethod-first-arg", "cls,klass"),
                    ("valid-metaclass-classmethod-first-arg", "mcs,meta,cls")
                ],
                &["C0203", "C0204"]
            ),
            expected
        );
    }

    #[test]
    fn metaclass_argument_single_option() {
        let source = include_str!("fixtures/metaclass_argument_single_option.py");
        let expected =
            include_str!("fixtures/metaclass_argument_single_option.expected").trim_end();
        assert_eq!(
            render(
                source,
                &[
                    ("valid-classmethod-first-arg", "self"),
                    ("valid-metaclass-classmethod-first-arg", "meta")
                ],
                &["C0203", "C0204"]
            ),
            expected
        );
    }

    #[test]
    fn implicit_flag_alias() {
        let source = include_str!("fixtures/implicit_flag_alias.py");
        let expected = include_str!("fixtures/implicit_flag_alias.expected").trim_end();
        assert_eq!(render(source, &[], &["W0213"]), expected);
    }

    #[test]
    fn too_many_public_methods() {
        let source = include_str!("fixtures/too_many_public_methods.py");
        let expected = include_str!("fixtures/too_many_public_methods.expected").trim_end();
        assert_eq!(
            render(source, &[("max-public-methods", "2")], &["R0904"]),
            expected
        );
    }

    #[test]
    fn pylint_django_exempts_test_cases_and_admins() {
        let source = include_str!("fixtures/pylint_django_exempts_test_cases_and_admins.py");
        let expected =
            include_str!("fixtures/pylint_django_exempts_test_cases_and_admins.expected")
                .trim_end();
        assert_eq!(
            render(
                source,
                &[
                    ("max-public-methods", "2"),
                    ("load-plugins", "pylint_django")
                ],
                &["R0904"]
            ),
            expected
        );
    }
}
