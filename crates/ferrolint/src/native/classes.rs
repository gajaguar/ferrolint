//! Class rules: `E0211` no-method-argument, `E0213` no-self-argument, `C0203` bad-mcs-method-argument,
//! `C0204` bad-mcs-classmethod-argument, `W0213` implicit-flag-alias and
//! `R0904` too-many-public-methods. The helpers at the top also serve the
//! slots, special-method and call rules.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Decorator, Expr, ExprName, Number, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use super::flow::own_returns;
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const NO_ARGUMENT: &str = "E0211";
const NO_SELF: &str = "E0213";
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
    /// `Alias = value` by the node id of `Alias`.
    aliases: HashMap<u32, &'a Expr>,
    /// The parameters astroid cannot infer as anything but unknown: no default, not `*args`,
    /// `**kwargs` or the first of a method (an instance or the class).
    bare_params: HashSet<u32>,
}

struct Collect<'a, 'm> {
    classes: &'m mut HashMap<u32, &'a StmtClassDef>,
    funcs: &'m mut HashMap<u32, &'a StmtFunctionDef>,
    aliases: &'m mut HashMap<u32, &'a Expr>,
    bare_params: &'m mut HashSet<u32>,
    /// Whether the statement is written directly in a class body.
    in_class: bool,
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
                let first = usize::from(self.in_class);
                let ps = &func.parameters;
                for param in ps
                    .posonlyargs
                    .iter()
                    .chain(&ps.args)
                    .skip(first)
                    .chain(&ps.kwonlyargs)
                {
                    if param.default.is_none()
                        && let Some(id) =
                            ruff_python_ast::HasNodeIndex::node_index(&param.parameter)
                                .load()
                                .as_u32()
                    {
                        self.bare_params.insert(id);
                    }
                }
            }
            Stmt::Assign(assign) => {
                if let [Expr::Name(target)] = &assign.targets[..]
                    && let Some(id) = ruff_python_ast::HasNodeIndex::node_index(target)
                        .load()
                        .as_u32()
                {
                    self.aliases.insert(id, &assign.value);
                }
            }
            _ => {}
        }
        let saved = self.in_class;
        self.in_class =
            matches!(stmt, Stmt::ClassDef(_)) || (saved && !matches!(stmt, Stmt::FunctionDef(_)));
        visitor::walk_stmt(self, stmt);
        self.in_class = saved;
    }
}

impl<'a> Env<'a> {
    pub fn new(ctx: &'a Ctx<'a>) -> Self {
        let (mut classes, mut funcs, mut aliases) =
            (HashMap::new(), HashMap::new(), HashMap::new());
        let mut bare_params = HashSet::new();
        Collect {
            classes: &mut classes,
            funcs: &mut funcs,
            aliases: &mut aliases,
            bare_params: &mut bare_params,
            in_class: false,
        }
        .visit_body(&ctx.parsed.syntax().body);
        Self {
            ctx,
            classes,
            funcs,
            aliases,
            bare_params,
        }
    }

    /// Whether the binding is a parameter that astroid infers as unknown.
    pub fn is_bare_param(&self, binding: &crate::wit::Binding) -> bool {
        binding.kind == BindingKind::Parameter
            && binding
                .node
                .is_some_and(|id| self.bare_params.contains(&id))
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
        self.qualified_at(expr, 0)
    }

    fn qualified_at(&self, expr: &Expr, depth: u8) -> Option<String> {
        match expr {
            Expr::Name(name) => {
                let mut binding = self.binding(name)?;
                // `try: import m` / `except ImportError: m = None`: Pylint
                // infers the module among the values of `m`.
                if binding.kind == BindingKind::Assignment {
                    let imported =
                        self.ctx.model.same_name(binding.id).into_iter().find(|b| {
                            matches!(b.kind, BindingKind::Import | BindingKind::FromImport)
                        });
                    let Some(imported) = imported else {
                        // `Structure = ctypes.Structure`, the only binding of the name.
                        return match self.aliases.get(&binding.node?)? {
                            value @ (Expr::Name(_) | Expr::Attribute(_))
                                if depth < 8 && self.ctx.model.is_sole_binding(binding.id) =>
                            {
                                self.qualified_at(value, depth + 1)
                            }
                            _ => None,
                        };
                    };
                    binding = imported;
                }
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
                self.qualified_at(&attribute.value, depth)?,
                attribute.attr
            )),
            _ => None,
        }
    }

    /// Every dotted name `expr` may stand for. A name bound in several branches
    /// (`X = int` under `if`, `X = long` under `else`; an import under `try` and
    /// another under `except`) stands for each value, as Pylint infers them.
    // shortcut: takes every binding of the name, also one a later binding shadows;
    // upgrade when a shadowed base shows up.
    pub fn qualified_all(&self, expr: &Expr) -> Vec<String> {
        let Expr::Name(name) = expr else {
            return self.qualified(expr).into_iter().collect();
        };
        let Some(binding) = self.binding(name) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for bound in self.ctx.model.same_name(binding.id) {
            let found = match bound.kind {
                BindingKind::FromImport => bound.origin,
                BindingKind::Assignment => bound
                    .node
                    .and_then(|node| self.aliases.get(&node))
                    .and_then(|value| match value {
                        // `Alias = Cls[T]` is `Cls`.
                        Expr::Subscript(s) => self.qualified(&s.value),
                        other => self.qualified(other),
                    }),
                _ => None,
            };
            out.extend(found.filter(|q| !out.contains(q)));
        }
        if out.is_empty() {
            out.extend(self.qualified(expr));
        }
        out
    }

    /// The value of the `name = value` that binds the name at node `node`.
    pub fn assigned(&self, node: u32) -> Option<&'a Expr> {
        self.aliases.get(&node).copied()
    }

    /// The class of this file that `expr` names, when it is bound only once,
    /// directly or through `Alias = Class`.
    pub fn class_of(&self, expr: &Expr) -> Option<&'a StmtClassDef> {
        self.class_of_at(expr, 0)
    }

    fn class_of_at(&self, expr: &Expr, depth: u8) -> Option<&'a StmtClassDef> {
        // `Outer.Inner`: a class that the body of a class of this file binds once.
        if let Expr::Attribute(attribute) = expr {
            let outer = self.class_of_at(&attribute.value, depth + 1)?;
            let mut bound = locals(&outer.body)
                .into_iter()
                .filter(|(name, _)| name == attribute.attr.as_str());
            bound.next()?;
            if bound.next().is_some() {
                return None;
            }
            return outer.body.iter().find_map(|stmt| match stmt {
                Stmt::ClassDef(c) if c.name.as_str() == attribute.attr.as_str() => Some(c),
                _ => None,
            });
        }
        let Expr::Name(name) = expr else { return None };
        let binding = self.binding(name)?;
        if !self.ctx.model.is_sole_binding(binding.id) {
            return None;
        }
        match binding.kind {
            BindingKind::ClassDef => self.classes.get(&binding.node?).copied(),
            // `A = B` and `B = A` would loop.
            BindingKind::Assignment if depth < 8 => {
                let value = self.aliases.get(&binding.node?)?;
                if !value.is_name_expr() {
                    return None;
                }
                self.class_of_at(value, depth + 1)
            }
            _ => None,
        }
    }

    /// The number of elements of the list or tuple literal that `expr` names, when
    /// it is bound only once: where Pylint unpacks `*expr` into that many arguments.
    pub fn literal_len(&self, expr: &Expr) -> Option<usize> {
        let Expr::Name(name) = expr else { return None };
        let binding = self.binding(name)?;
        if binding.kind != BindingKind::Assignment || !self.ctx.model.is_sole_binding(binding.id) {
            return None;
        }
        match self.aliases.get(&binding.node?)? {
            Expr::List(list) => Some(list.elts.len()),
            Expr::Tuple(tuple) => Some(tuple.elts.len()),
            _ => None,
        }
    }

    /// The class of this file that `name = make(...)` makes, when `make` is a
    /// plain function of this file whose every `return` names that one class
    /// (the factory of a mixin).
    pub fn made_class(&self, expr: &Expr) -> Option<&'a StmtClassDef> {
        let Expr::Name(name) = expr else { return None };
        let binding = self.binding(name)?;
        if binding.kind != BindingKind::Assignment || !self.ctx.model.is_sole_binding(binding.id) {
            return None;
        }
        let Expr::Call(call) = self.aliases.get(&binding.node?)? else {
            return None;
        };
        let Expr::Name(callee) = &*call.func else {
            return None;
        };
        let func = self.func_of(callee)?;
        if func.is_async || !func.decorator_list.is_empty() {
            return None;
        }
        let mut classes = own_returns(&func.body)
            .into_iter()
            .map(|r| r.value.as_deref().and_then(|value| self.class_of(value)));
        let first = classes.next()??;
        classes
            .all(|class| class.is_some_and(|class| std::ptr::eq(class, first)))
            .then_some(first)
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

/// The enum classes of the standard library and of Django's `models`.
fn is_enum_class(qualified: &str) -> bool {
    matches!(
        qualified,
        "enum.Enum"
            | "enum.IntEnum"
            | "enum.StrEnum"
            | "enum.Flag"
            | "enum.IntFlag"
            | "enum.ReprEnum"
            | "django.db.models.Choices"
            | "django.db.models.TextChoices"
            | "django.db.models.IntegerChoices"
            | "django.db.models.enums.Choices"
            | "django.db.models.enums.TextChoices"
            | "django.db.models.enums.IntegerChoices"
    )
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
                NO_SELF,
                "no-self-argument",
                "A method whose first argument is not `self`.",
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
            NO_SELF,
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
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
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
        if [NO_ARGUMENT, NO_SELF, MCS_METHOD, MCS_CLASSMETHOD]
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
        let first = p
            .posonlyargs
            .first()
            .or(p.args.first())
            .map(|arg| arg.parameter.name.as_str());
        if !self.is_metaclass(class) {
            // A class method's first argument is `bad-classmethod-argument`'s.
            if self.on(NO_SELF) && !is_classmethod(method) && first != Some("self") {
                self.out.push(ctx.report_at(
                    NO_SELF,
                    Self::name_range(method),
                    format!(
                        "Method {} should have \"self\" as first argument",
                        py_repr(name)
                    ),
                ));
            }
            return;
        }
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

    fn public_methods(&mut self, class: &StmtClassDef) {
        let ctx = self.env.ctx;
        let max = ctx
            .option("max-public-methods")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(20);
        // Pylint counts the first binding of each name in the class.
        let mut seen = HashSet::new();
        let own = locals(&class.body)
            .into_iter()
            .filter(|(name, _)| seen.insert(name.clone()))
            .filter(|(name, kind)| *kind == Local::Function && !name.starts_with('_'))
            .count();
        // astroid gives every enum class one public method of its own.
        let enum_method = bases(class)
            .iter()
            .any(|base| self.env.base_is(base, &is_enum_class));
        let count = own + usize::from(enum_method);
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

    let parsed = crate::checks::parse(
        source,
        ruff_python_parser::ParseOptions::from(ruff_python_ast::PySourceType::Python),
    );
    assert_eq!(parsed.errors(), []);
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
        roots: &crate::native::Project::test_roots(path, module),
        types: None,
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
    fn method_without_self() {
        let source = include_str!("fixtures/method_without_self.py");
        let expected = include_str!("fixtures/method_without_self.expected").trim_end();
        assert_eq!(render(source, &[], &["E0213"]), expected);
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
}
