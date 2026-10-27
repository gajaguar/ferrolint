//! `C0103` invalid-name and the type variable messages `C0105`
//! typevar-name-incorrect-variance, `C0131` typevar-double-variance and
//! `C0132` typevar-name-mismatch: Pylint's `NameChecker`, with its naming
//! styles, its `*-naming-style` and `*-rgx` options and the kind of each name.
//!
//! shortcut: Pylint infers the value of a module-level `name = value` to
//! tell a constant from a variable. Here a value is a constant when it is a
//! literal, a signed or folded literal, an f-string, `int`/`str`/`bool`/`len`
//! of literals, a name bound once to one, or a boolean/comparison/`if`
//! expression of them; a class, a function, a type alias or a `TypeVar` is read
//! from its syntax, and the instance of a class of the file, of a builtin
//! container or of an imported class (a call of a capitalized name) counts as
//! an inferable non-constant. Any other value (a call or an attribute of an
//! import, say) is taken as uninferable and is not checked, which is what
//! Pylint does when it fails to infer it. Pylint also infers across modules
//! the base classes whose methods a class overrides: here only the classes
//! of the file, the project and the table in `class_methods` are followed,
//! and a base named `...TestCase` is taken to define the `unittest` hooks.
//! `name-group` is not read, and a pattern in
//! `*-rgx` that Rust's regex engine rejects (a look-around) is not applied.
//! Upgrade with a cross-module class model.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::OnceLock;

use ferrolint_core::{Diagnostic, RuleInfo};
use regex::Regex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Arguments, CmpOp, Comprehension, ExceptHandler, Expr, ExprContext, ExprTuple, Number, Operator,
    Pattern, Stmt, StmtClassDef, StmtFunctionDef, StmtIf, StmtTry, TypeParam, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange};

use super::classes::{Env, Local, bases, locals, py_repr};
use super::flow::own_returns;
use super::misc::{Names as Lists, catches_import_error, csv, imported_names, regex};
use super::project::{ClassRef, Project};
use super::scopes::{Step, exclusive};
use super::statements::is_generator;
use super::{Checker, Ctx, TyValue, rule};
use crate::class_methods;
use crate::wit::BindingKind;

const INVALID: &str = "C0103";
const VARIANCE: &str = "C0105";
const DOUBLE: &str = "C0131";
const MISMATCH: &str = "C0132";

pub struct Naming;

impl Checker for Naming {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                INVALID,
                "invalid-name",
                "A name that does not conform to the naming rules of its kind.",
            ),
            rule(
                VARIANCE,
                "typevar-name-incorrect-variance",
                "A TypeVar name that does not reflect its variance.",
            ),
            rule(
                DOUBLE,
                "typevar-double-variance",
                "A TypeVar that is both covariant and contravariant.",
            ),
            rule(
                MISMATCH,
                "typevar-name-mismatch",
                "A TypeVar whose name differs from the variable it is assigned to.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![INVALID, VARIANCE, DOUBLE, MISMATCH]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let env = Env::new(ctx);
        let cfg = Cfg::new(ctx);
        let mut walk = Walk {
            env: &env,
            cfg: &cfg,
            enabled,
            out,
            binds: HashMap::new(),
            frames: vec![Frame::new(Kind::Module, None)],
            handlers: Vec::new(),
            path: Vec::new(),
            loops: 0,
            main: 0,
            project: Project::new(ctx.path, ctx.module, ctx.roots),
            cache: RefCell::default(),
            outside: std::cell::OnceCell::new(),
        };
        let module = ctx.module.rsplit('.').next().unwrap_or_default();
        if !module.is_empty() {
            walk.check("module", module, TextRange::default(), None);
        }
        let body = &ctx.parsed.syntax().body;
        let mut collector = Collector {
            ctx,
            path: Vec::new(),
            out: HashMap::new(),
        };
        collector.body(body);
        walk.binds = collector.out;
        foreign(ctx, body, &mut walk.binds);
        walk.visit_body(body);
    }
}

// ---------------------------------------------------------------------------
// Naming styles and options

/// The kinds of name, with Pylint's label and default style.
fn label(kind: &str) -> &'static str {
    match kind {
        "module" => "module",
        "const" => "constant",
        "class" => "class",
        "function" => "function",
        "method" => "method",
        "attr" => "attribute",
        "argument" => "argument",
        "variable" => "variable",
        "class_attribute" => "class attribute",
        "class_const" => "class constant",
        "inlinevar" => "inline iteration",
        "typevar" => "type variable",
        "paramspec" => "parameter specification variable",
        "typevartuple" => "type variable tuple",
        _ => "type alias",
    }
}

const KINDS: [&str; 15] = [
    "module",
    "const",
    "class",
    "function",
    "method",
    "attr",
    "argument",
    "variable",
    "class_attribute",
    "class_const",
    "inlinevar",
    "typevar",
    "paramspec",
    "typevartuple",
    "typealias",
];

fn default_style(kind: &str) -> &'static str {
    match kind {
        "const" | "class_const" => "UPPER_CASE",
        "class" => "PascalCase",
        "class_attribute" | "inlinevar" => "any",
        _ => "snake_case",
    }
}

/// The source of the regex a style gives a kind, as Pylint writes it.
fn style_source(style: &str, kind: &str) -> &'static str {
    // 0 module, 1 const, 2 class (and inline), 3 default, 4 class attribute
    let role = match kind {
        "module" => 0,
        "const" | "class_const" => 1,
        "class" | "inlinevar" => 2,
        "class_attribute" => 4,
        _ => 3,
    };
    let table: [&str; 5] = match style {
        "camelCase" => [
            r"([^\W\dA-Z][^\W_]*|__main__)$",
            r"([^\W\dA-Z][^\W_]*|__.*__)$",
            r"[^\W\dA-Z][^\W_]*$",
            r"(?:__)?([^\W\dA-Z][^\W_]*|__[^\W\dA-Z_]\w+__)$",
            r"([^\W\dA-Z][^\W_]*|__.*__)$",
        ],
        "PascalCase" => [
            r"([^\W\da-z][^\W_]*|__main__)$",
            r"([^\W\da-z][^\W_]*|__.*__)$",
            r"[^\W\da-z][^\W_]*$",
            r"([^\W\da-z][^\W_]*|__[^\W\dA-Z_]\w+__)$",
            r"[^\W\da-z][^\W_]*$",
        ],
        "UPPER_CASE" => [
            r"([^\W\da-z][^\Wa-z]*|__main__)$",
            r"([^\W\da-z][^\Wa-z]*|__.*__)$",
            r"[^\W\da-z][^\Wa-z]*$",
            r"([^\W\da-z][^\Wa-z]*|__[^\W\dA-Z_]\w+__)$",
            r"[^\W\da-z][^\Wa-z]*$",
        ],
        "any" => [".*"; 5],
        _ => [
            r"[^\W\dA-Z][^\WA-Z]*$",
            r"([^\W\dA-Z][^\WA-Z]*|__.*__)$",
            r"[^\W\dA-Z][^\WA-Z]*$",
            r"([^\W\dA-Z][^\WA-Z]*|_[^\WA-Z]*|__[^\WA-Z\d_][^\WA-Z]+__)$",
            r"([^\W\dA-Z][^\WA-Z]*|__.*__)$",
        ],
    };
    table[role]
}

fn predefined_source(kind: &str) -> &'static str {
    match kind {
        "typevar" => {
            r"^_{0,2}(?!T[A-Z])(?:[A-Z]+|(?:[A-Z]+[a-z0-9]+)+(?:T)?(?<!Type))(?:_co(?:ntra)?)?$"
        }
        "paramspec" => r"^_{0,2}(?:[A-Z]+|(?:[A-Z]+[a-z0-9]+)+(?:P)?(?<!Type))$",
        "typevartuple" => r"^_{0,2}(?:[A-Z]+|(?:[A-Z]+[a-z0-9]+)+(?:Ts)?(?<!Type))$",
        _ => r"^_{0,2}(?!T[A-Z]|Type)[A-Z]+[a-z0-9]+(?:[A-Z][a-z0-9]+)*$",
    }
}

/// Words of capitals and then lower case or digits, as the predefined
/// patterns spell them, with `extra` allowed after the last one.
fn words(text: &str, extra: &str) -> bool {
    static PLAIN: OnceLock<Regex> = OnceLock::new();
    let plain = PLAIN.get_or_init(|| {
        Regex::new(r"^(?:[A-Z]+[a-z0-9]+)+$").unwrap_or_else(|_| unreachable!("a static regex"))
    });
    plain.is_match(text)
        || text
            .strip_suffix(extra)
            .is_some_and(|core| plain.is_match(core))
}

/// A name without its (at most two) leading underscores.
fn strip_underscores(name: &str) -> Option<&str> {
    let rest = name.trim_start_matches('_');
    (name.len() - rest.len() <= 2).then_some(rest)
}

fn all_caps(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_uppercase())
}

/// The predefined pattern of a kind that has no style, by hand: Rust's regex
/// has no look-around.
fn predefined(kind: &str, name: &str) -> bool {
    let Some(rest) = strip_underscores(name) else {
        return false;
    };
    let starts_t = |text: &str| {
        let mut chars = text.chars();
        chars.next() == Some('T') && chars.next().is_some_and(|c| c.is_ascii_uppercase())
    };
    match kind {
        "typevar" => {
            !starts_t(rest)
                && ["", "_contra", "_co"].iter().any(|suffix| {
                    rest.strip_suffix(suffix).is_some_and(|core| {
                        all_caps(core) || (words(core, "T") && !core.ends_with("Type"))
                    })
                })
        }
        "paramspec" | "typevartuple" => {
            let extra = if kind == "paramspec" { "P" } else { "Ts" };
            all_caps(rest) || (words(rest, extra) && !rest.ends_with("Type"))
        }
        _ => !starts_t(rest) && !rest.starts_with("Type") && words(rest, ""),
    }
}

struct Matcher {
    /// None when the pattern is a predefined one or Rust rejects it.
    rx: Option<Regex>,
    predefined: bool,
    source: String,
    hint: String,
}

impl Matcher {
    fn matches(&self, kind: &str, name: &str) -> bool {
        if self.predefined {
            predefined(kind, name)
        } else {
            self.rx.as_ref().is_none_or(|rx| rx.is_match(name))
        }
    }
}

struct Cfg {
    rules: HashMap<&'static str, Matcher>,
    lists: Lists,
    include_hint: bool,
    property_classes: Vec<String>,
    property_names: Vec<String>,
}

impl Cfg {
    fn new(ctx: &Ctx<'_>) -> Self {
        let mut rules = HashMap::new();
        for kind in KINDS {
            let stem = kind.replace('_', "-");
            let custom = ctx
                .option(&format!("{stem}-rgx"))
                .map(str::trim)
                .filter(|text| !text.is_empty());
            let styled = !matches!(kind, "typevar" | "paramspec" | "typevartuple" | "typealias");
            let style = ctx
                .option(&format!("{stem}-naming-style"))
                .map(str::trim)
                .filter(|style| {
                    ["snake_case", "camelCase", "PascalCase", "UPPER_CASE", "any"].contains(style)
                })
                .unwrap_or_else(|| default_style(kind));
            let (source, hint, predefined) = match (custom, styled) {
                (Some(text), _) => (text.to_owned(), format!("{} pattern", py_repr(text)), false),
                (None, true) => (
                    style_source(style, kind).to_owned(),
                    format!("{style} naming style"),
                    false,
                ),
                (None, false) => (
                    predefined_source(kind).to_owned(),
                    "predefined naming style".to_owned(),
                    true,
                ),
            };
            let rx = (!predefined)
                .then(|| regex(&format!(r"\A(?:{source})")))
                .flatten();
            rules.insert(
                kind,
                Matcher {
                    rx,
                    predefined,
                    source,
                    hint,
                },
            );
        }
        let property_classes = csv(ctx, "property-classes", "abc.abstractproperty");
        let property_names = property_classes
            .iter()
            .map(|class| class.rsplit('.').next().unwrap_or_default().to_owned())
            .collect();
        Self {
            rules,
            lists: Lists::new(ctx),
            include_hint: ctx.option("include-naming-hint").is_some_and(|v| {
                matches!(v.trim().to_lowercase().as_str(), "y" | "yes" | "true" | "1")
            }),
            property_classes,
            property_names,
        }
    }
}

// ---------------------------------------------------------------------------
// The module's bindings, found before the walk

#[derive(Clone, Copy, PartialEq)]
enum Bk {
    /// An `AssignName` of the module scope.
    Name,
    /// The target of an augmented assignment, which Pylint can infer.
    Aug,
    Def,
    Class,
    Import,
    FromImport,
    /// Assigned by a function that declares it `global`.
    Foreign,
}

struct ModBind<'a> {
    line: u32,
    path: Vec<Step<'a>>,
    kind: Bk,
    /// The value of a plain `name = value`.
    value: Option<&'a Expr>,
}

type Binds<'a> = HashMap<&'a str, Vec<ModBind<'a>>>;

fn line_of(ctx: &Ctx<'_>, range: TextRange) -> u32 {
    ctx.index
        .range(ctx.source, range.start().to_u32(), range.start().to_u32())
        .start
        .line
}

fn target_names<'a>(target: &'a Expr, direct: bool, out: &mut Vec<(&'a str, TextRange, bool)>) {
    match target {
        Expr::Name(name) => out.push((name.id.as_str(), name.range, direct)),
        Expr::Tuple(t) => t.elts.iter().for_each(|e| target_names(e, false, out)),
        Expr::List(l) => l.elts.iter().for_each(|e| target_names(e, false, out)),
        Expr::Starred(s) => target_names(&s.value, false, out),
        _ => {}
    }
}

/// Finds the bindings of the module's own scope, with the branch each is in.
struct Collector<'a, 'c, 'x> {
    ctx: &'c Ctx<'x>,
    path: Vec<Step<'a>>,
    out: Binds<'a>,
}

impl<'a> Collector<'a, '_, '_> {
    fn add(&mut self, name: &'a str, range: TextRange, kind: Bk, value: Option<&'a Expr>) {
        self.out.entry(name).or_default().push(ModBind {
            line: line_of(self.ctx, range),
            path: self.path.clone(),
            kind,
            value,
        });
    }

    /// The names of a target; `value` is kept for a plain name.
    fn target(&mut self, target: &'a Expr, value: Option<&'a Expr>) {
        let mut names = Vec::new();
        target_names(target, true, &mut names);
        for (name, range, direct) in names {
            self.add(name, range, Bk::Name, value.filter(|_| direct));
        }
    }

    fn branch(&mut self, stmt: &'a Stmt, slot: usize, body: &'a [Stmt]) {
        self.path.push(Step { stmt, slot });
        self.body(body);
        self.path.pop();
    }

    fn body(&mut self, body: &'a [Stmt]) {
        for stmt in body {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(f) => self.add(f.name.as_str(), f.name.range(), Bk::Def, None),
            Stmt::ClassDef(c) => self.add(c.name.as_str(), c.name.range(), Bk::Class, None),
            Stmt::Assign(a) => a
                .targets
                .iter()
                .for_each(|t| self.target(t, Some(&a.value))),
            Stmt::AnnAssign(a) => self.target(&a.target, a.value.as_deref()),
            Stmt::AugAssign(a) => {
                let mut names = Vec::new();
                target_names(&a.target, true, &mut names);
                for (name, range, _) in names {
                    self.add(name, range, Bk::Aug, Some(&a.value));
                }
            }
            Stmt::Import(i) => {
                for alias in &i.names {
                    let first = alias.name.split('.').next().unwrap_or_default();
                    let name = alias.asname.as_ref().map_or(first, |a| a.as_str());
                    self.add(name, alias.range, Bk::Import, None);
                }
            }
            Stmt::ImportFrom(i) => {
                for alias in i.names.iter().filter(|alias| &alias.name != "*") {
                    let name = alias.asname.as_ref().unwrap_or(&alias.name);
                    self.add(name.as_str(), alias.range, Bk::FromImport, None);
                }
            }
            Stmt::For(f) => {
                self.target(&f.target, None);
                self.body(&f.body);
                self.body(&f.orelse);
            }
            Stmt::While(w) => {
                self.body(&w.body);
                self.body(&w.orelse);
            }
            Stmt::With(w) => {
                w.items
                    .iter()
                    .filter_map(|item| item.optional_vars.as_deref())
                    .for_each(|vars| self.target(vars, None));
                self.body(&w.body);
            }
            Stmt::If(s) => {
                for (slot, body) in if_bodies(s).into_iter().enumerate() {
                    self.branch(stmt, slot, body);
                }
            }
            Stmt::Try(t) => self.try_stmt(stmt, t),
            Stmt::Match(m) => {
                for case in &m.cases {
                    let mut names = Vec::new();
                    pattern_names(&case.pattern, &mut names);
                    for (name, range) in names {
                        self.add(name, range, Bk::Name, None);
                    }
                    self.body(&case.body);
                }
            }
            _ => {}
        }
    }

    fn try_stmt(&mut self, stmt: &'a Stmt, t: &'a StmtTry) {
        self.branch(stmt, 0, &t.body);
        for (i, handler) in t.handlers.iter().enumerate() {
            let ExceptHandler::ExceptHandler(h) = handler;
            self.path.push(Step { stmt, slot: i + 1 });
            if let Some(name) = &h.name {
                self.add(name.as_str(), name.range(), Bk::Name, None);
            }
            self.body(&h.body);
            self.path.pop();
        }
        self.branch(stmt, 1000, &t.orelse);
        self.branch(stmt, 1001, &t.finalbody);
    }
}

fn if_bodies(s: &StmtIf) -> Vec<&[Stmt]> {
    std::iter::once(&s.body[..])
        .chain(s.elif_else_clauses.iter().map(|clause| &clause.body[..]))
        .collect()
}

fn pattern_names<'a>(pattern: &'a Pattern, out: &mut Vec<(&'a str, TextRange)>) {
    struct Names<'a, 'o>(&'o mut Vec<(&'a str, TextRange)>);
    impl<'a> Visitor<'a> for Names<'a, '_> {
        fn visit_pattern(&mut self, pattern: &'a Pattern) {
            let name = match pattern {
                Pattern::MatchAs(p) => p.name.as_ref(),
                Pattern::MatchStar(p) => p.name.as_ref(),
                Pattern::MatchMapping(p) => p.rest.as_ref(),
                _ => None,
            };
            if let Some(name) = name {
                self.0.push((name.as_str(), name.range()));
            }
            visitor::walk_pattern(self, pattern);
        }
    }
    Names(out).visit_pattern(pattern);
}

/// The module names that a function declares `global` and assigns: Pylint
/// binds them in the module, where no branch excludes them.
fn foreign<'a>(ctx: &Ctx<'_>, body: &'a [Stmt], out: &mut Binds<'a>) {
    #[derive(Default)]
    struct Scan<'a> {
        globals: HashSet<&'a str>,
        stores: Vec<(&'a str, TextRange)>,
        /// The value of each plain `name = value`, by the start of the name.
        values: HashMap<u32, &'a Expr>,
    }
    impl<'a> Visitor<'a> for Scan<'a> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            match stmt {
                Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
                Stmt::Assign(a) => {
                    for target in &a.targets {
                        if let Expr::Name(name) = target {
                            self.values.insert(name.range.start().to_u32(), &a.value);
                        }
                    }
                    visitor::walk_stmt(self, stmt);
                }
                Stmt::Global(g) => self
                    .globals
                    .extend(g.names.iter().map(ruff_python_ast::Identifier::as_str)),
                _ => visitor::walk_stmt(self, stmt),
            }
        }
        fn visit_expr(&mut self, expr: &'a Expr) {
            if let Expr::Name(name) = expr
                && name.ctx == ExprContext::Store
            {
                self.stores.push((name.id.as_str(), name.range));
            }
            visitor::walk_expr(self, expr);
        }
    }
    struct Functions<'a, 'o, 'c, 'x> {
        ctx: &'c Ctx<'x>,
        out: &'o mut Binds<'a>,
    }
    impl<'a> Visitor<'a> for Functions<'a, '_, '_, '_> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if let Stmt::FunctionDef(f) = stmt {
                let mut scan = Scan::default();
                scan.visit_body(&f.body);
                for (name, range) in scan.stores {
                    if scan.globals.contains(name) {
                        self.out.entry(name).or_default().push(ModBind {
                            line: line_of(self.ctx, range),
                            path: Vec::new(),
                            kind: Bk::Foreign,
                            value: scan.values.get(&range.start().to_u32()).copied(),
                        });
                    }
                }
            }
            visitor::walk_stmt(self, stmt);
        }
    }
    Functions { ctx, out }.visit_body(body);
}

// ---------------------------------------------------------------------------
// The walk

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Module,
    Function,
    Class,
    Lambda,
}

struct Frame<'a> {
    kind: Kind,
    class: Option<&'a StmtClassDef>,
    params: HashSet<&'a str>,
    globals: HashSet<&'a str>,
}

impl<'a> Frame<'a> {
    fn new(kind: Kind, class: Option<&'a StmtClassDef>) -> Self {
        Self {
            kind,
            class,
            params: HashSet::new(),
            globals: HashSet::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SiteKind {
    Assign,
    AnnAssign,
    Other,
}

#[derive(Clone, Copy)]
struct Site<'a> {
    kind: SiteKind,
    value: Option<&'a Expr>,
    annotation: Option<&'a Expr>,
}

const OTHER: Site<'static> = Site {
    kind: SiteKind::Other,
    value: None,
    annotation: None,
};

/// What holds the name in an assignment target.
#[derive(Clone, Copy)]
enum Parent<'a> {
    /// The statement itself.
    Direct,
    Tuple(usize, &'a ExprTuple),
    Other,
}

/// What Pylint infers a value to be, as far as the names need.
#[derive(Clone, Copy, PartialEq)]
enum Val {
    Const,
    Class,
    /// The functional `Enum(...)`: a class for a module name, an instance for a variable.
    Enum,
    Func,
    Other,
    Unknown,
}

struct Walk<'a, 'c> {
    env: &'c Env<'a>,
    cfg: &'c Cfg,
    enabled: &'c BTreeSet<String>,
    out: &'c mut Vec<Diagnostic>,
    binds: Binds<'a>,
    frames: Vec<Frame<'a>>,
    /// For each enclosing `except` clause, the names the `try` body imports
    /// when the clause catches `ImportError`.
    handlers: Vec<Option<HashSet<String>>>,
    path: Vec<Step<'a>>,
    /// The `for` and `while` statements around, in the module frame.
    loops: usize,
    /// The `if __name__ == "__main__":` bodies around.
    main: usize,
    project: Project,
    /// The known ancestors of each class, by the start of its name.
    cache: RefCell<HashMap<u32, Rc<Vec<Anc<'a>>>>>,
    /// The attributes that code outside a class gives its instances, by class address.
    outside: std::cell::OnceCell<HashMap<usize, Vec<(&'a str, TextRange)>>>,
}

/// An ancestor of a class.
enum Anc<'a> {
    /// A class of the file.
    Local(&'a StmtClassDef),
    /// A class of another module of the project.
    Remote(ClassRef),
    /// A class the project does not hold, by its qualified name.
    External(String),
}

/// The methods and attributes of `unittest.TestCase`, which the test
/// cases of Django and Django REST framework derive from.
const TEST_CASE: [&str; 61] = [
    "addAsyncCleanup",
    "addClassCleanup",
    "addCleanup",
    "addTypeEqualityFunc",
    "assertAlmostEqual",
    "assertCountEqual",
    "assertDictEqual",
    "assertEndsWith",
    "assertEqual",
    "assertFalse",
    "assertGreater",
    "assertGreaterEqual",
    "assertHasAttr",
    "assertIn",
    "assertIs",
    "assertIsInstance",
    "assertIsNone",
    "assertIsNot",
    "assertIsNotNone",
    "assertIsSubclass",
    "assertLess",
    "assertLessEqual",
    "assertListEqual",
    "assertLogs",
    "assertMultiLineEqual",
    "assertNoLogs",
    "assertNotAlmostEqual",
    "assertNotEndsWith",
    "assertNotEqual",
    "assertNotHasAttr",
    "assertNotIn",
    "assertNotIsInstance",
    "assertNotIsSubclass",
    "assertNotRegex",
    "assertNotStartsWith",
    "assertRaises",
    "assertRaisesRegex",
    "assertRegex",
    "assertSequenceEqual",
    "assertSetEqual",
    "assertStartsWith",
    "assertTrue",
    "assertTupleEqual",
    "assertWarns",
    "assertWarnsRegex",
    "asyncSetUp",
    "asyncTearDown",
    "countTestCases",
    "defaultTestResult",
    "doClassCleanups",
    "doCleanups",
    "enterAsyncContext",
    "enterClassContext",
    "enterContext",
    "setUp",
    "setUpClass",
    "shortDescription",
    "skipTest",
    "subTest",
    "tearDown",
    "tearDownClass",
];

/// What Django's test cases add to `unittest.TestCase`.
const DJANGO_TEST_CASE: [&str; 17] = [
    "assertContains",
    "assertFieldOutput",
    "assertFormError",
    "assertFormSetError",
    "assertHTMLEqual",
    "assertHTMLNotEqual",
    "assertInHTML",
    "assertJSONEqual",
    "assertJSONNotEqual",
    "assertNotContains",
    "assertNumQueries",
    "assertQuerySetEqual",
    "assertRaisesMessage",
    "assertRedirects",
    "assertTemplateNotUsed",
    "assertTemplateUsed",
    "assertWarnsMessage",
];

/// Whether a class outside the project and the file defines `name`. The
/// methods of the classes in `class_methods` are known; of the rest only
/// test cases are: a class whose name ends in `TestCase` is taken to derive
/// from `unittest.TestCase`, and from Django's `TestCase`, which owns
/// `setUpTestData`, unless it is a unittest, transaction, simple or live
/// server one.
fn external_defines(qualified: &str, name: &str, method: bool) -> bool {
    if method && let Some(found) = class_methods::methods(qualified) {
        return found.binary_search(&name).is_ok();
    }
    if !qualified.ends_with("TestCase") {
        return false;
    }
    let unittest = qualified.starts_with("unittest.");
    let atomic = !unittest
        && !["Transaction", "Simple", "LiveServer"]
            .iter()
            .any(|kind| qualified.contains(kind));
    TEST_CASE.contains(&name)
        || (!unittest && DJANGO_TEST_CASE.contains(&name))
        || (atomic && name == "setUpTestData")
        || (!method && matches!(name, "maxDiff" | "longMessage" | "failureException"))
}

fn is_main_test(test: &Expr) -> bool {
    let Expr::Compare(c) = test else { return false };
    if c.ops.len() != 1 || c.ops[0] != CmpOp::Eq {
        return false;
    }
    let operands = [&c.operands[0], &c.operands[1]];
    operands
        .iter()
        .any(|e| matches!(e, Expr::Name(n) if n.id.as_str() == "__name__"))
        && operands
            .iter()
            .any(|e| matches!(e, Expr::StringLiteral(s) if s.value.to_str() == "__main__"))
}

/// An annotation spelled `Final` or `x.Final`, also under a subscript.
fn names_final(annotation: &Expr) -> bool {
    let annotation = match annotation {
        Expr::Subscript(s) => &*s.value,
        other => other,
    };
    last_name(annotation) == Some("Final")
}

fn last_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.as_str()),
        _ => None,
    }
}

/// `ClassVar[Final]`, `ClassVar[Final[int]]`.
fn class_var_final(annotation: &Expr) -> bool {
    let Expr::Subscript(outer) = annotation else {
        return false;
    };
    if last_name(&outer.value) != Some("ClassVar") {
        return false;
    }
    let inner = match &*outer.slice {
        Expr::Subscript(s) => &*s.value,
        other => other,
    };
    last_name(inner) == Some("Final")
}

/// A name that is not a dunder or a sunder, which an enum does not take as a member.
fn plain_member(name: &str) -> bool {
    !(name.starts_with('_') && name.ends_with('_') && name.len() > 1)
}

impl<'a> Walk<'a, '_> {
    fn ctx(&self) -> &Ctx<'a> {
        self.env.ctx
    }

    fn frame(&self) -> &Frame<'a> {
        self.frames
            .last()
            .unwrap_or_else(|| unreachable!("the module frame"))
    }

    fn emit(&mut self, id: &str, range: TextRange, message: String) {
        if self.enabled.contains(id) {
            let diagnostic = self.ctx().report_at(id, range, message);
            self.out.push(diagnostic);
        }
    }

    /// Pylint's `_check_name`: a name that the good or bad lists settle is
    /// not judged; the rest must match the pattern of its kind.
    fn check(&mut self, kind: &str, name: &str, range: TextRange, typevar: Option<&'a Expr>) {
        if self.cfg.lists.allowed(name) || self.cfg.lists.refuses(name) {
            return;
        }
        let cfg = self.cfg;
        let rule = &cfg.rules[kind];
        if !rule.matches(kind, name) {
            let mut hint = rule.hint.clone();
            if cfg.include_hint {
                let _ = write!(hint, " ({} pattern)", py_repr(&rule.source));
            }
            let mut label = label(kind).to_owned();
            label[..1].make_ascii_uppercase();
            self.emit(
                INVALID,
                range,
                format!("{label} name \"{name}\" doesn't conform to {hint}"),
            );
        }
        if kind == "typevar" {
            self.typevar(name, range, typevar);
        }
    }

    /// `_check_typevar`: the variance suffix and the name argument.
    fn typevar(&mut self, name: &str, range: TextRange, call: Option<&'a Expr>) {
        let Some(Expr::Call(call)) = call else { return };
        let (mut covariant, mut contravariant, mut double) = (false, false, false);
        let mut name_arg: Option<String> = None;
        let truthy = |e: &Expr| match e {
            Expr::BooleanLiteral(b) => Some(b.value),
            Expr::NumberLiteral(_) => Some(true),
            Expr::NoneLiteral(_) => Some(false),
            Expr::StringLiteral(s) => Some(!s.value.to_str().is_empty()),
            _ => None,
        };
        for keyword in &call.arguments.keywords {
            let Some(arg) = &keyword.arg else { continue };
            let Some(value) = truthy(&keyword.value) else {
                continue;
            };
            if double {
                // nothing more to learn
            } else if arg.as_str() == "covariant" && value {
                (covariant, double) = (!contravariant, contravariant);
            } else if arg.as_str() == "contravariant" && value {
                (contravariant, double) = (!covariant, covariant);
            }
            if arg.as_str() == "name"
                && let Expr::StringLiteral(s) = &keyword.value
            {
                name_arg = Some(s.value.to_str().to_owned());
            }
        }
        if name_arg.is_none()
            && let Some(Expr::StringLiteral(s)) = call.arguments.args.first()
        {
            name_arg = Some(s.value.to_str().to_owned());
        }
        let strip = |suffixes: &[&str]| {
            suffixes
                .iter()
                .find_map(|suffix| name.strip_suffix(suffix))
                .unwrap_or(name)
                .to_owned()
        };
        let incorrect = |what: &str, suggest: String| {
            format!(
                "Type variable name does not reflect variance. \"{name}\" is {what}, use \"{suggest}\" instead"
            )
        };
        if double {
            self.emit(
                DOUBLE,
                range,
                "TypeVar cannot be both covariant and contravariant".to_owned(),
            );
            self.emit(
                VARIANCE,
                range,
                "Type variable name does not reflect variance".to_owned(),
            );
        } else if covariant && !name.ends_with("_co") {
            let suggest = format!("{}_co", strip(&["_contra"]));
            self.emit(VARIANCE, range, incorrect("covariant", suggest));
        } else if contravariant && !name.ends_with("_contra") {
            let suggest = format!("{}_contra", strip(&["_co"]));
            self.emit(VARIANCE, range, incorrect("contravariant", suggest));
        } else if !covariant
            && !contravariant
            && (name.ends_with("_co") || name.ends_with("_contra"))
        {
            let suggest = strip(&["_contra", "_co"]);
            self.emit(VARIANCE, range, incorrect("invariant", suggest));
        }
        if let Some(arg) = name_arg
            && arg != name
        {
            self.emit(
                MISMATCH,
                range,
                format!("TypeVar name \"{arg}\" does not match assigned variable name \"{name}\""),
            );
        }
    }

    // -- what a value is ---------------------------------------------------

    /// The first segment of `expr`'s qualified name, or `None`.
    fn qualified(&self, expr: &Expr) -> Option<String> {
        self.env.qualified(expr)
    }

    fn typevar_kind(&self, value: &Expr) -> Option<&'static str> {
        let Expr::Call(call) = value else { return None };
        match self.qualified(&call.func)?.as_str() {
            "typing.TypeVar" | "typing_extensions.TypeVar" => Some("typevar"),
            "typing.ParamSpec" | "typing_extensions.ParamSpec" => Some("paramspec"),
            "typing.TypeVarTuple" | "typing_extensions.TypeVarTuple" => Some("typevartuple"),
            _ => None,
        }
    }

    /// A type that a `|` or `Union[...]` can join.
    fn type_like(&self, expr: &Expr) -> bool {
        match expr {
            Expr::NoneLiteral(_) => true,
            Expr::BinOp(b) if b.op == Operator::BitOr => {
                self.type_like(&b.left) && self.type_like(&b.right)
            }
            Expr::Subscript(s) => self.qualified(&s.value).is_some_and(|q| {
                q.starts_with("typing.") || (!q.contains('.') && builtin_class(&q))
            }),
            Expr::Name(_) | Expr::Attribute(_) => {
                self.env.class_of(expr).is_some()
                    || self.qualified(expr).is_some_and(|q| {
                        q.starts_with("typing.") || (!q.contains('.') && builtin_class(&q))
                    })
            }
            _ => false,
        }
    }

    fn assigns_typealias(&self, value: &Expr, depth: u8) -> bool {
        match value {
            Expr::Subscript(s) => matches!(
                self.qualified(&s.value).as_deref(),
                Some("typing.Union" | "typing_extensions.Union")
            ),
            Expr::BinOp(b) if b.op == Operator::BitOr => self.type_like(value),
            // An alias of an alias: astroid infers the name's only value.
            Expr::Name(name) if depth < 6 => {
                self.env
                    .binding(name)
                    .is_some_and(|b| b.kind == BindingKind::Assignment && b.scope == 0)
                    && matches!(
                        self.binds.get(name.id.as_str()).map(Vec::as_slice),
                        Some([only]) if only.kind == Bk::Name
                            && only.value.is_some_and(|v| self.assigns_typealias(v, depth + 1))
                    )
            }
            _ => false,
        }
    }

    fn typealias_annotation(&self, annotation: &Expr) -> bool {
        matches!(
            self.qualified(annotation).as_deref(),
            Some("typing.TypeAlias" | "typing_extensions.TypeAlias")
        )
    }

    /// What the value of a binding is: `val`, and for a name, attribute or call that it
    /// cannot follow, the value ty infers when the build has it.
    fn val_at(&self, value: &Expr) -> Val {
        let val = self.val(value, 0);
        if val != Val::Unknown
            || !matches!(value, Expr::Name(_) | Expr::Attribute(_) | Expr::Call(_))
        {
            return val;
        }
        match self
            .ctx()
            .types
            .and_then(|types| types.value(value.range()))
        {
            // Only a class: ty's function and literal are wider than Pylint's `Const` (an enum
            // member is a literal to ty and an instance to Pylint).
            Some(TyValue::Class(_)) => Val::Class,
            _ => Val::Unknown,
        }
    }

    fn val(&self, value: &Expr, depth: u8) -> Val {
        if depth > 6 {
            return Val::Unknown;
        }
        let all = |values: &[&Expr]| values.iter().all(|e| self.val(e, depth + 1) == Val::Const);
        match value {
            Expr::NoneLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::FString(_) => Val::Const,
            Expr::UnaryOp(u)
                if matches!(
                    u.op,
                    UnaryOp::USub | UnaryOp::UAdd | UnaryOp::Invert | UnaryOp::Not
                ) =>
            {
                if all(&[&*u.operand]) {
                    Val::Const
                } else {
                    Val::Unknown
                }
            }
            Expr::BinOp(b) if b.op != Operator::MatMult => {
                if all(&[&*b.left, &*b.right]) {
                    Val::Const
                } else if b.op == Operator::BitOr && self.type_like(value) {
                    Val::Class
                } else {
                    Val::Unknown
                }
            }
            Expr::BoolOp(b) if all(&b.values.iter().collect::<Vec<_>>()) => Val::Const,
            // astroid infers no `is` comparison.
            Expr::Compare(c)
                if !c
                    .ops
                    .iter()
                    .any(|op| matches!(op, CmpOp::Is | CmpOp::IsNot))
                    && all(&c.operands.iter().collect::<Vec<_>>()) =>
            {
                Val::Const
            }
            // astroid joins the two arms only when they have one Python type.
            // shortcut: a test that astroid infers (`True`, `sys.platform`) picks one arm there;
            // upgrade with a value model.
            Expr::If(i) if all(&[&*i.body, &*i.orelse]) => {
                match (self.const_kind(&i.body, 0), self.const_kind(&i.orelse, 0)) {
                    (Some(a), Some(b)) if a != b => Val::Unknown,
                    _ => Val::Const,
                }
            }
            // astroid cannot infer a dict that unpacks what it does not know.
            Expr::Dict(d)
                if d.items
                    .iter()
                    .any(|i| i.key.is_none() && !i.value.is_dict_expr()) =>
            {
                Val::Unknown
            }
            Expr::List(_) | Expr::Tuple(_) | Expr::Set(_) | Expr::Dict(_) => Val::Other,
            // astroid has no inference for a comprehension.
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                Val::Unknown
            }
            Expr::Lambda(_) => Val::Func,
            Expr::Name(name) => self.name_val(name, value, depth),
            Expr::Attribute(_) => {
                let qualified = self.qualified(value);
                if typing_class(qualified.as_deref())
                    || self.is_class(qualified.as_deref())
                    || self.env.class_of(value).is_some()
                {
                    Val::Class
                } else if qualified.is_some_and(|q| HOST_CONSTS.contains(&q.as_str())) {
                    Val::Const
                } else {
                    Val::Unknown
                }
            }
            Expr::Subscript(s) => match self.qualified(&s.value).as_deref() {
                Some("typing.Union" | "typing_extensions.Union") => Val::Other,
                Some(q)
                    if q.starts_with("typing.")
                        || q.starts_with("collections.abc.")
                        || (!stdlib_class(q) && self.is_class(Some(q))) =>
                {
                    Val::Class
                }
                Some(q) if !q.contains('.') && builtin_class(q) => Val::Class,
                _ => Val::Unknown,
            },
            Expr::Call(call) => self.call_val(call, depth),
            _ => Val::Unknown,
        }
    }

    /// The Python type of a literal, or of a module name bound once to one.
    fn const_kind(&self, expr: &Expr, depth: u8) -> Option<&'static str> {
        Some(match expr {
            Expr::NoneLiteral(_) => "None",
            Expr::BooleanLiteral(_) => "bool",
            Expr::StringLiteral(_) | Expr::FString(_) => "str",
            Expr::BytesLiteral(_) => "bytes",
            Expr::NumberLiteral(n) => match n.value {
                Number::Int(_) => "int",
                Number::Float(_) => "float",
                Number::Complex { .. } => "complex",
            },
            Expr::Name(name)
                if depth < 6 && self.env.binding(name).is_some_and(|b| b.scope == 0) =>
            {
                match self.binds.get(name.id.as_str()).map(Vec::as_slice) {
                    Some([only]) if only.kind == Bk::Name => {
                        return self.const_kind(only.value?, depth + 1);
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    fn name_val(&self, name: &ruff_python_ast::ExprName, value: &Expr, depth: u8) -> Val {
        if self.env.class_of(value).is_some() {
            return Val::Class;
        }
        let Some(binding) = self.env.binding(name) else {
            return Val::Unknown;
        };
        match binding.kind {
            BindingKind::Builtin => {
                if builtin_class(&binding.name) {
                    Val::Class
                } else {
                    Val::Func
                }
            }
            BindingKind::FunctionDef if self.env.func_of(name).is_some() => Val::Func,
            BindingKind::Import => Val::Other,
            BindingKind::FromImport => {
                let origin = binding.origin.as_deref();
                let last = origin
                    .and_then(|o| o.rsplit('.').next())
                    .unwrap_or_default();
                if typing_class(origin) || self.is_class(origin) {
                    Val::Class
                } else if origin.is_some_and(|o| HOST_CONSTS.contains(&o)) {
                    Val::Const
                } else if last.starts_with(|c: char| c.is_ascii_lowercase()) {
                    // A lower-case import is most often a function.
                    Val::Func
                } else {
                    Val::Unknown
                }
            }
            // A variable of a function that is bound once, as `d` in `d = {}; return d`.
            BindingKind::Assignment
                if binding.scope != 0 && self.ctx().model.is_sole_binding(binding.id) =>
            {
                binding
                    .node
                    .and_then(|node| self.env.assigned(node))
                    .map_or(Val::Unknown, |v| self.val(v, depth + 1))
            }
            BindingKind::Assignment if binding.scope == 0 => {
                match self.binds.get(name.id.as_str()).map(Vec::as_slice) {
                    Some([only]) if only.kind == Bk::Name => {
                        only.value.map_or(Val::Unknown, |v| self.val(v, depth + 1))
                    }
                    _ => Val::Unknown,
                }
            }
            _ => Val::Unknown,
        }
    }

    /// Whether `qualified` names a class that the project defines, once.
    fn is_class(&self, qualified: Option<&str>) -> bool {
        qualified.is_some_and(|q| {
            stdlib_class(q)
                || self
                    .project
                    .class(&self.project.absolute(q))
                    .is_some_and(|class| class.is_sole())
        })
    }

    /// Whether `expr` names a module class that is the only binding of its
    /// name above it, as `Maker` in `Maker = Maker()`, which astroid infers
    /// though the name is bound twice.
    fn class_above(&self, expr: &Expr) -> bool {
        let Expr::Name(name) = expr else { return false };
        let line = line_of(self.ctx(), name.range);
        let above: Vec<&ModBind<'_>> = self
            .binds
            .get(name.id.as_str())
            .into_iter()
            .flatten()
            .filter(|b| b.line < line)
            .collect();
        matches!(above[..], [only] if only.kind == Bk::Class)
            && self
                .env
                .binding(name)
                .is_some_and(|b| b.kind == BindingKind::ClassDef && b.scope == 0)
    }

    /// Whether a function of the file returns, wherever it returns, a class.
    fn returns_class(&self, func: &StmtFunctionDef) -> bool {
        let returns = own_returns(&func.body);
        !returns.is_empty()
            && returns.iter().all(|r| {
                r.value
                    .as_deref()
                    .is_some_and(|e| self.env.class_of(e).is_some())
            })
    }

    /// What a call of a function of the file infers to: classes when every return is one, and
    /// the value of its only return otherwise, as astroid joins values of one Python type only.
    // shortcut: a decorated, async or generator function, and several returns of values
    // that are not classes, are not judged; upgrade with the inferred Python type.
    fn returns_val(&self, func: &StmtFunctionDef, depth: u8) -> Val {
        if self.returns_class(func) {
            return Val::Class;
        }
        let returns = own_returns(&func.body);
        let [only] = returns[..] else {
            return Val::Unknown;
        };
        if !func.decorator_list.is_empty() || func.is_async || is_generator(func) {
            return Val::Unknown;
        }
        match only.value.as_deref().map(|e| self.val(e, depth + 1)) {
            Some(val @ (Val::Const | Val::Other | Val::Func)) => val,
            _ => Val::Unknown,
        }
    }

    /// Whether `value` calls a container type on something that is not a
    /// display, which astroid infers to an instance and not to a container.
    fn container_instance(&self, value: &Expr) -> bool {
        let Expr::Call(call) = value else {
            return false;
        };
        matches!(
            self.qualified(&call.func).as_deref(),
            Some("list" | "tuple" | "set" | "frozenset" | "dict")
        ) && call.arguments.args.first().is_some_and(|a| {
            !matches!(
                a,
                Expr::List(_) | Expr::Tuple(_) | Expr::Set(_) | Expr::Dict(_)
            )
        })
    }

    /// astroid evaluates `"...".format(...)` itself, so it is a constant only when every argument is
    /// one and the template's fields are all supplied; `**` arguments supply nothing.
    // shortcut: only a literal template, not a named constant one, upgrade when one shows up.
    fn format_is_const(
        &self,
        func: &ruff_python_ast::ExprAttribute,
        call: &ruff_python_ast::ExprCall,
        depth: u8,
    ) -> bool {
        let Expr::StringLiteral(template) = &*func.value else {
            return false;
        };
        let args = &call.arguments;
        let keywords: Vec<_> = args
            .keywords
            .iter()
            .filter_map(|k| k.arg.as_ref().map(|a| (a.as_str(), &k.value)))
            .collect();
        if !args
            .args
            .iter()
            .chain(keywords.iter().map(|(_, v)| *v))
            .all(|e| self.val(e, depth + 1) == Val::Const)
        {
            return false;
        }
        let mut next = 0;
        let text = template.value.to_str().replace("{{", "").replace("}}", "");
        text.split('{').skip(1).all(|field| {
            let name = field
                .split(['}', '!', ':', '.', '['])
                .next()
                .unwrap_or_default();
            match name.parse::<usize>() {
                Ok(i) => i < args.args.len(),
                Err(_) if name.is_empty() => {
                    next += 1;
                    next <= args.args.len()
                }
                Err(_) => keywords.iter().any(|(k, _)| *k == name),
            }
        })
    }

    fn call_val(&self, call: &ruff_python_ast::ExprCall, depth: u8) -> Val {
        let all = |values: &[&Expr]| values.iter().all(|e| self.val(e, depth + 1) == Val::Const);
        // The `str` and `bytes` methods that astroid infers to a constant, whatever the arguments.
        if let Expr::Attribute(a) = &*call.func
            && STR_METHODS.contains(&a.attr.as_str())
            && all(&[&*a.value])
            && (a.attr.as_str() != "format" || self.format_is_const(a, call, depth))
        {
            return Val::Const;
        }
        let Some(callee) = self.qualified(&call.func) else {
            if let Some(class) = self.env.class_of(&call.func) {
                // A metaclass called with a name, bases and a namespace builds a class.
                return if call.arguments.args.len() == 3 && self.is_metaclass(class) {
                    Val::Class
                } else if self.is_enum(class) {
                    Val::Unknown
                } else {
                    Val::Other
                };
            }
            if self.class_above(&call.func) {
                return Val::Other;
            }
            return match &*call.func {
                Expr::Name(name) => self
                    .env
                    .func_of(name)
                    .map_or(Val::Unknown, |f| self.returns_val(f, depth)),
                _ => Val::Unknown,
            };
        };
        let args: Vec<&Expr> = call.arguments.args.iter().collect();
        match callee.as_str() {
            "type" if args.len() == 3 => Val::Class,
            // astroid cannot tell the type of a call it cannot infer.
            "type"
                if args.len() == 1
                    && (!args[0].is_call_expr()
                        || self.val(args[0], depth + 1) != Val::Unknown) =>
            {
                Val::Class
            }
            "int" | "str" | "bool" | "len" if !args.is_empty() && all(&args) => Val::Const,
            // astroid builds a `str` constant whatever the argument is, and an
            // instance for the other builtin types when it cannot fold it.
            "str" | "int" | "float" | "bytes"
                if !args.is_empty()
                    && call.arguments.keywords.is_empty()
                    && !args.iter().any(|a| a.is_starred_expr()) =>
            {
                if callee == "str" {
                    Val::Const
                } else {
                    Val::Other
                }
            }
            "typing.NewType"
            | "typing.NamedTuple"
            | "typing.TypedDict"
            | "collections.namedtuple"
            | "typing_extensions.NewType"
            | "typing_extensions.TypedDict" => Val::Class,
            "enum.Enum" => Val::Enum,
            // astroid infers the other enums' functional calls to nothing.
            "enum.IntEnum"
            | "enum.StrEnum"
            | "enum.Flag"
            | "enum.IntFlag"
            | "typing.TypeVar"
            | "typing.ParamSpec"
            | "typing.TypeVarTuple"
            | "typing.TypeAliasType"
            | "typing_extensions.TypeAliasType" => Val::Unknown,
            "dict" | "list" | "set" | "frozenset" | "tuple" | "object" => Val::Other,
            "hasattr" if self.has_module_attr(&call.arguments) => Val::Const,
            q if self.project.returns_class(&self.project.absolute(q)) => Val::Class,
            q => {
                let last = q.rsplit('.').next().unwrap_or_default();
                if q.contains('.') && last.starts_with(|c: char| c.is_ascii_uppercase()) {
                    Val::Other
                } else {
                    Val::Unknown
                }
            }
        }
    }

    /// Whether `args` is `(module, "name")` for a module imported once and found: astroid
    /// answers `hasattr` of it with `True` or `False`.
    fn has_module_attr(&self, args: &Arguments) -> bool {
        let [Expr::Name(name), Expr::StringLiteral(_)] = &args.args[..] else {
            return false;
        };
        args.keywords.is_empty()
            && self.env.binding(name).is_some_and(|b| {
                b.kind == BindingKind::Import
                    && self.ctx().model.is_sole_binding(b.id)
                    && b.origin
                        .as_deref()
                        .is_some_and(|module| self.project.spec(module).is_some())
            })
    }

    // -- storing a name ----------------------------------------------------

    fn redefines_import(&self, name: &str) -> bool {
        self.handlers
            .last()
            .is_some_and(|imports| imports.as_ref().is_some_and(|set| set.contains(name)))
    }

    fn store(&mut self, name: &'a str, range: TextRange, site: Site<'a>, parent: Parent<'a>) {
        match self.frame().kind {
            Kind::Lambda => {}
            Kind::Module => {
                if site.kind != SiteKind::Other {
                    self.module_store(name, range, site, parent);
                }
            }
            Kind::Function => {
                let frame = self.frame();
                if frame.globals.contains(name)
                    || frame.params.contains(name)
                    || self.redefines_import(name)
                {
                    return;
                }
                if site.kind == SiteKind::AnnAssign
                    && site
                        .annotation
                        .is_some_and(|a| self.typealias_annotation(a))
                {
                    self.check("typealias", name, range, None);
                } else if !(matches!(parent, Parent::Direct)
                    && site.value.is_some_and(|v| {
                        self.val(v, 0) == Val::Class || self.typevar_kind(v).is_some()
                    }))
                {
                    self.check("variable", name, range, None);
                }
            }
            Kind::Class => self.class_store(name, range, site),
        }
    }

    fn module_store(
        &mut self,
        name: &'a str,
        range: TextRange,
        site: Site<'a>,
        parent: Parent<'a>,
    ) {
        if site.kind == SiteKind::AnnAssign
            && site
                .annotation
                .is_some_and(|a| self.typealias_annotation(a))
        {
            self.check("typealias", name, range, None);
            return;
        }
        if site.kind == SiteKind::Assign
            && let (Parent::Direct, Some(value)) = (parent, site.value)
        {
            if let Some(kind) = self.typevar_kind(value) {
                self.check(kind, name, range, Some(value));
                return;
            }
            if self.assigns_typealias(value, 0) {
                self.check("typealias", name, range, None);
                return;
            }
        }
        if let (Parent::Tuple(index, _), Some(Expr::Tuple(values))) = (parent, site.value)
            && site.kind == SiteKind::Assign
            && index < values.elts.len()
        {
            let assigner = &values.elts[index];
            if let Some(kind) = self.typevar_kind(assigner) {
                self.check(kind, name, range, Some(assigner));
            } else if self.assigns_typealias(assigner, 0) {
                self.check("typealias", name, range, None);
            }
            return;
        }
        let Some(value) = site.value else { return };
        let val = self.val_at(value);
        match val {
            Val::Unknown => return,
            Val::Class | Val::Enum => return self.check("class", name, range, None),
            _ => {}
        }
        let line = line_of(self.ctx(), range);
        let redefines = self.redefines_import(name);
        let binds = self.binds.get(name).map_or(&[][..], Vec::as_slice);
        let reassigned = |before: bool| {
            binds.iter().any(|b| {
                matches!(b.kind, Bk::Name | Bk::Aug | Bk::Def | Bk::Class)
                    && if before { b.line < line } else { b.line > line }
            })
        };
        let plain = !redefines
            && val != Val::Func
            && !reassigned(true)
            && !reassigned(false)
            && self.loops == 0;
        // astroid cannot infer `+=` on the instance that `list(x)` builds.
        let instance = binds
            .iter()
            .any(|b| b.kind == Bk::Name && b.value.is_some_and(|v| self.container_instance(v)));
        // Pylint's inference of every binding of the name: an uninferable binding.
        let unknown = binds.iter().any(|b| match b.kind {
            Bk::Name | Bk::Foreign => b.value.is_none_or(|v| self.val_at(v) == Val::Unknown),
            Bk::FromImport => true,
            // astroid cannot infer `+=` of a display that holds a `*starred`.
            Bk::Aug => {
                instance
                    || b.value.is_some_and(|v| match v {
                        Expr::Tuple(t) => t.elts.iter().any(Expr::is_starred_expr),
                        Expr::List(l) => l.elts.iter().any(Expr::is_starred_expr),
                        _ => false,
                    })
            }
            Bk::Def | Bk::Class | Bk::Import => false,
        });
        let all_exclusive = binds.len() > 1
            && binds
                .iter()
                .enumerate()
                .all(|(i, a)| binds[i + 1..].iter().all(|b| exclusive(&a.path, &b.path)));
        let cfg = self.cfg;
        let const_ok = cfg.rules["const"].matches("const", name);
        let exempt = val != Val::Const && cfg.rules["variable"].matches("variable", name);
        let in_main = self.main > 0;
        let main_type = if const_ok { "const" } else { "variable" };
        if plain {
            if !exempt {
                self.check(if in_main { main_type } else { "const" }, name, range, None);
            }
        } else if !(exempt || redefines || unknown && const_ok) {
            let kind = if all_exclusive {
                if in_main { main_type } else { "const" }
            } else {
                "variable"
            };
            self.check(kind, name, range, None);
        }
    }

    fn class_store(&mut self, name: &'a str, range: TextRange, site: Site<'a>) {
        let Some(class) = self.frame().class else {
            return;
        };
        if self
            .ancestors(class)
            .iter()
            .any(|anc| Self::anc_defines(anc, name, false))
        {
            return;
        }
        let annotation = site.annotation.filter(|_| site.kind == SiteKind::AnnAssign);
        let kind = if annotation.is_some_and(class_var_final) {
            "class_const"
        } else if annotation.is_some_and(names_final) {
            if self.is_dataclass(class) {
                "class_attribute"
            } else {
                "class_const"
            }
        } else if site.kind != SiteKind::Other
            && site.value.is_some()
            && plain_member(name)
            && self.is_enum(class)
        {
            "class_const"
        } else {
            "class_attribute"
        };
        self.check(kind, name, range, None);
    }

    // -- classes -------------------------------------------------------------

    /// The ancestors of `class` that are known: the classes of the file and
    /// of the project, and the qualified names of the rest.
    fn ancestors(&self, class: &'a StmtClassDef) -> Rc<Vec<Anc<'a>>> {
        let key = class.name.range().start().to_u32();
        if let Some(found) = self.cache.borrow().get(&key) {
            return found.clone();
        }
        let mut out = Vec::new();
        self.local_ancestors(class, &mut out, 0);
        let out = Rc::new(out);
        self.cache.borrow_mut().insert(key, out.clone());
        out
    }

    fn local_ancestors(&self, class: &'a StmtClassDef, out: &mut Vec<Anc<'a>>, depth: u8) {
        if depth >= 16 {
            return;
        }
        for base in bases(class) {
            // `Base[int]` is `Base` to astroid.
            let base = match base {
                Expr::Subscript(s) => &*s.value,
                other => other,
            };
            if let Some(local) = self.env.class_of(base) {
                out.push(Anc::Local(local));
                self.local_ancestors(local, out, depth + 1);
            } else {
                for qualified in self.env.qualified_all(base) {
                    let qualified = self.project.absolute(&qualified);
                    match self.project.class(&qualified) {
                        Some(remote) => self.remote_ancestors(&remote, out, depth + 1),
                        None => self.external(qualified, out),
                    }
                }
            }
        }
    }

    fn remote_ancestors(&self, class: &ClassRef, out: &mut Vec<Anc<'a>>, depth: u8) {
        out.push(Anc::Remote(class.clone()));
        if depth >= 16 {
            return;
        }
        for base in class.bases_all(&self.project) {
            match base {
                Ok(remote) => self.remote_ancestors(&remote, out, depth + 1),
                Err(qualified) => self.external(qualified, out),
            }
        }
    }

    /// A class the project does not hold, or what a name of the project that aliases one stands for.
    fn external(&self, qualified: String, out: &mut Vec<Anc<'a>>) {
        let aliased = self.project.unalias(&qualified);
        if aliased.is_empty() {
            out.push(Anc::External(qualified));
        } else {
            out.extend(aliased.into_iter().map(Anc::External));
        }
    }

    /// Whether the ancestor binds `name` (as a method when `method`).
    fn anc_defines(anc: &Anc<'_>, name: &str, method: bool) -> bool {
        let first = |class: &StmtClassDef| {
            let mut found = locals(&class.body).into_iter().filter(|(l, _)| l == name);
            if method {
                found
                    .next()
                    .is_some_and(|(_, kind)| kind == Local::Function)
            } else {
                found.next().is_some()
            }
        };
        match anc {
            Anc::Local(class) => first(class),
            Anc::Remote(class) => class.class().is_some_and(first),
            Anc::External(qualified) => external_defines(qualified, name, method),
        }
    }

    fn is_enum(&self, class: &'a StmtClassDef) -> bool {
        let enum_class = |name: &str| {
            matches!(
                name,
                "enum.Enum"
                    | "enum.IntEnum"
                    | "enum.StrEnum"
                    | "enum.Flag"
                    | "enum.IntFlag"
                    | "enum.ReprEnum"
            ) || name.starts_with("django.db.models.") && name.ends_with("Choices")
        };
        self.ancestors(class)
            .iter()
            .any(|anc| matches!(anc, Anc::External(q) if enum_class(q)))
    }

    fn is_metaclass(&self, class: &'a StmtClassDef) -> bool {
        self.ancestors(class)
            .iter()
            .any(|anc| matches!(anc, Anc::External(q) if q == "type" || q == "abc.ABCMeta"))
    }

    fn is_dataclass(&self, class: &StmtClassDef) -> bool {
        class.decorator_list.iter().any(|d| {
            let expr = match &d.expression {
                Expr::Call(c) => &*c.func,
                other => other,
            };
            self.qualified(expr).as_deref() == Some("dataclasses.dataclass")
        })
    }

    /// Whether `name` overrides a method of an ancestor of `class`.
    fn overrides(&self, class: &'a StmtClassDef, name: &str) -> bool {
        self.ancestors(class)
            .iter()
            .any(|anc| Self::anc_defines(anc, name, true))
    }

    fn function_kind(&self, f: &StmtFunctionDef, is_method: bool) -> &'static str {
        if !is_method {
            return "function";
        }
        let attr_name = |d: &Expr| matches!(d, Expr::Attribute(a) if matches!(a.attr.as_str(), "setter" | "deleter"));
        if f.decorator_list.iter().any(|d| attr_name(&d.expression)) {
            return "attr";
        }
        for decorator in &f.decorator_list {
            let expr = &decorator.expression;
            let candidate = match expr {
                Expr::Name(_) => true,
                Expr::Attribute(a) => self.cfg.property_names.iter().any(|n| n == a.attr.as_str()),
                _ => false,
            };
            if candidate
                && let Some(qualified) = self.qualified(expr)
                && (qualified == "property" || self.cfg.property_classes.contains(&qualified))
            {
                return "attr";
            }
        }
        "method"
    }

    fn function(&mut self, f: &'a StmtFunctionDef) {
        for decorator in &f.decorator_list {
            self.visit_decorator(decorator);
        }
        self.visit_parameters(&f.parameters);
        if let Some(returns) = &f.returns {
            self.visit_expr(returns);
        }
        self.type_params(f.type_params.as_deref());
        let class = (self.frame().kind == Kind::Class)
            .then(|| self.frame().class)
            .flatten();
        let is_method = self.frame().kind == Kind::Class;
        let skip = class.is_some_and(|class| self.overrides(class, f.name.as_str()));
        if !skip {
            let kind = self.function_kind(f, is_method);
            self.check(kind, f.name.as_str(), f.name.range(), None);
            for arg in &f.parameters.args {
                self.check(
                    "argument",
                    arg.parameter.name.as_str(),
                    arg.parameter.name.range(),
                    None,
                );
            }
        }
        let mut frame = Frame::new(Kind::Function, None);
        frame.params = f.parameters.iter().map(|p| p.name().as_str()).collect();
        self.enter(frame, |walk| walk.visit_body(&f.body));
    }

    fn type_params(&mut self, params: Option<&'a ruff_python_ast::TypeParams>) {
        for param in params.iter().flat_map(|p| &p.type_params) {
            let (kind, name) = match param {
                TypeParam::TypeVar(p) => ("typevar", &p.name),
                TypeParam::ParamSpec(p) => ("paramspec", &p.name),
                TypeParam::TypeVarTuple(p) => ("typevartuple", &p.name),
            };
            self.check(kind, name.as_str(), name.range(), None);
        }
    }

    fn enter(&mut self, frame: Frame<'a>, body: impl FnOnce(&mut Self)) {
        let outer = (
            std::mem::take(&mut self.loops),
            std::mem::take(&mut self.main),
        );
        let path = std::mem::take(&mut self.path);
        self.frames.push(frame);
        body(self);
        self.frames.pop();
        (self.loops, self.main) = outer;
        self.path = path;
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        for decorator in &class.decorator_list {
            self.visit_decorator(decorator);
        }
        if let Some(arguments) = &class.arguments {
            self.visit_arguments(arguments);
        }
        self.type_params(class.type_params.as_deref());
        self.check("class", class.name.as_str(), class.name.range(), None);
        for attr in self.instance_attrs(class) {
            let has = |anc: &Anc<'_>| match anc {
                Anc::Local(base) => self
                    .instance_attrs(base)
                    .iter()
                    .any(|a| a.name == attr.name),
                Anc::Remote(base) => base
                    .class()
                    .is_some_and(|c| self.instance_attrs(c).iter().any(|a| a.name == attr.name)),
                Anc::External(_) => false,
            };
            if !attr.is_final && !self.ancestors(class).iter().any(has) {
                self.check("attr", &attr.name, attr.range, None);
            }
        }
        self.enter(Frame::new(Kind::Class, Some(class)), |walk| {
            walk.visit_body(&class.body);
        });
    }

    /// The attributes `self.x = ...` that the methods assign, and the
    /// annotated fields of a dataclass, with the first place of each.
    fn instance_attrs(&self, class: &StmtClassDef) -> Vec<Attr> {
        struct Find<'o> {
            me: &'o str,
            found: &'o mut Vec<Attr>,
        }
        impl<'a> Visitor<'a> for Find<'_> {
            fn visit_stmt(&mut self, stmt: &'a Stmt) {
                if let Stmt::AnnAssign(a) = stmt
                    && let Expr::Attribute(attr) = &*a.target
                    && matches!(&*attr.value, Expr::Name(n) if n.id.as_str() == self.me)
                {
                    self.add(attr.attr.as_str(), attr.range, names_final(&a.annotation));
                }
                visitor::walk_stmt(self, stmt);
            }
            fn visit_expr(&mut self, expr: &'a Expr) {
                if let Expr::Attribute(attr) = expr
                    && attr.ctx == ExprContext::Store
                    && matches!(&*attr.value, Expr::Name(n) if n.id.as_str() == self.me)
                {
                    self.add(attr.attr.as_str(), attr.range, false);
                }
                visitor::walk_expr(self, expr);
            }
        }
        impl Find<'_> {
            fn add(&mut self, name: &str, range: TextRange, is_final: bool) {
                if !self.found.iter().any(|a| a.name == name) {
                    self.found.push(Attr {
                        name: name.to_owned(),
                        range,
                        is_final,
                    });
                }
            }
        }
        let mut found: Vec<Attr> = Vec::new();
        // astroid adds the fields of a dataclass before what the methods assign.
        if self.is_dataclass(class) {
            for stmt in &class.body {
                if let Stmt::AnnAssign(a) = stmt
                    && let Expr::Name(target) = &*a.target
                    && !is_class_var(&a.annotation)
                    && !found.iter().any(|f| f.name == target.id.as_str())
                {
                    found.push(Attr {
                        name: target.id.to_string(),
                        range: target.range,
                        is_final: names_final(&a.annotation),
                    });
                }
            }
        }
        for method in super::classes::methods(&class.body) {
            if super::classes::is_static(method) || super::classes::is_classmethod(method) {
                continue;
            }
            let first = method
                .parameters
                .posonlyargs
                .iter()
                .chain(&method.parameters.args)
                .next();
            if let Some(first) = first {
                Find {
                    me: first.parameter.name.as_str(),
                    found: &mut found,
                }
                .visit_body(&method.body);
            }
        }
        // astroid adds what code outside the class assigns to its instances last, and a
        // `__slots__` keeps it out.
        // shortcut: a class with `__slots__` is not given them, whatever the slots.
        let slotted = super::classes::locals(&class.body)
            .iter()
            .any(|(name, _)| name == "__slots__");
        let outside = self
            .outside
            .get_or_init(|| super::instance_attrs::outside_attrs(self.env));
        for &(name, range) in outside
            .get(&std::ptr::from_ref(class).addr())
            .filter(|_| !slotted)
            .into_iter()
            .flatten()
        {
            if !found.iter().any(|a| a.name == name) {
                found.push(Attr {
                    name: name.to_owned(),
                    range,
                    is_final: false,
                });
            }
        }
        found
    }

    // -- targets ---------------------------------------------------------------

    fn bind_target(&mut self, target: &'a Expr, site: Site<'a>, parent: Parent<'a>) {
        match target {
            Expr::Name(name) => self.store(name.id.as_str(), name.range, site, parent),
            Expr::Tuple(t) => {
                for (index, element) in t.elts.iter().enumerate() {
                    self.bind_target(element, site, Parent::Tuple(index, t));
                }
            }
            Expr::List(l) => l
                .elts
                .iter()
                .for_each(|e| self.bind_target(e, site, Parent::Other)),
            Expr::Starred(s) => self.bind_target(&s.value, site, Parent::Other),
            other => self.visit_expr(other),
        }
    }

    fn comprehension(&mut self, generators: &'a [Comprehension], elements: &[&'a Expr]) {
        for generator in generators {
            self.visit_expr(&generator.iter);
            let mut names = Vec::new();
            target_names(&generator.target, true, &mut names);
            for (name, range, _) in names {
                self.check("inlinevar", name, range, None);
            }
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        for element in elements {
            self.visit_expr(element);
        }
    }

    fn pattern(&mut self, pattern: &'a Pattern) {
        let mut names = Vec::new();
        pattern_names(pattern, &mut names);
        for (name, range) in names {
            self.store(name, range, OTHER, Parent::Other);
        }
    }

    fn for_stmt(&mut self, f: &'a ruff_python_ast::StmtFor) {
        self.visit_expr(&f.iter);
        self.loops += 1;
        // A loop over classes binds a class, which a variable name excuses.
        let elements = match &*f.iter {
            Expr::Tuple(t) => &t.elts[..],
            Expr::List(l) => &l.elts[..],
            _ => &[],
        };
        let site = Site {
            kind: SiteKind::Other,
            value: elements
                .first()
                .filter(|_| elements.iter().all(|e| self.val(e, 0) == Val::Class)),
            annotation: None,
        };
        self.bind_target(&f.target, site, Parent::Direct);
        self.visit_body(&f.body);
        self.visit_body(&f.orelse);
        self.loops -= 1;
    }

    fn try_stmt(&mut self, stmt: &'a Stmt, t: &'a StmtTry) {
        self.branch(stmt, 0, &t.body);
        for (index, handler) in t.handlers.iter().enumerate() {
            let ExceptHandler::ExceptHandler(h) = handler;
            self.path.push(Step {
                stmt,
                slot: index + 1,
            });
            if let Some(type_) = &h.type_ {
                self.visit_expr(type_);
            }
            self.handlers
                .push(catches_import_error(h).then(|| imported_names(std::slice::from_ref(stmt))));
            if let Some(name) = &h.name {
                self.store(name.as_str(), name.range(), OTHER, Parent::Direct);
            }
            self.visit_body(&h.body);
            self.handlers.pop();
            self.path.pop();
        }
        self.branch(stmt, 1000, &t.orelse);
        self.branch(stmt, 1001, &t.finalbody);
    }

    fn branch(&mut self, stmt: &'a Stmt, slot: usize, body: &'a [Stmt]) {
        self.path.push(Step { stmt, slot });
        self.visit_body(body);
        self.path.pop();
    }
}

struct Attr {
    name: String,
    range: TextRange,
    is_final: bool,
}

fn is_class_var(annotation: &Expr) -> bool {
    let annotation = match annotation {
        Expr::Subscript(s) => &*s.value,
        other => other,
    };
    last_name(annotation) == Some("ClassVar")
}

/// The attributes of `sys` and `os` that astroid infers to a constant, whatever the platform
/// (`os.name` is left out: a comparison of it is not inferred).
const HOST_CONSTS: [&str; 24] = [
    "sys.maxsize",
    "sys.maxunicode",
    "sys.byteorder",
    "sys.hexversion",
    "sys.platform",
    "sys.version",
    "sys.prefix",
    "sys.exec_prefix",
    "sys.base_prefix",
    "sys.executable",
    "sys.api_version",
    "sys.copyright",
    "sys.pycache_prefix",
    "sys.dont_write_bytecode",
    "sys.float_repr_style",
    "os.sep",
    "os.linesep",
    "os.pathsep",
    "os.curdir",
    "os.pardir",
    "os.extsep",
    "os.altsep",
    "os.devnull",
    "os.defpath",
];

const STR_METHODS: [&str; 19] = [
    "capitalize",
    "center",
    "count",
    "decode",
    "encode",
    "find",
    "format",
    "index",
    "join",
    "ljust",
    "lower",
    "lstrip",
    "replace",
    "rjust",
    "rstrip",
    "strip",
    "swapcase",
    "title",
    "upper",
];

/// A builtin name that is a class.
fn builtin_class(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        || matches!(
            name,
            "int"
                | "str"
                | "float"
                | "bool"
                | "bytes"
                | "bytearray"
                | "complex"
                | "list"
                | "dict"
                | "set"
                | "frozenset"
                | "tuple"
                | "object"
                | "type"
                | "range"
                | "slice"
                | "property"
                | "super"
                | "memoryview"
                | "staticmethod"
                | "classmethod"
        )
}

/// A standard library class that astroid infers to a class. Not the ones the
/// table lists without methods (`Enum` and `Flag`, which its enum brain
/// rewrites; `ABC`, left out too) and not `Element`, rebound by its C module.
fn stdlib_class(qualified: &str) -> bool {
    class_methods::methods(qualified).is_some_and(|m| !m.is_empty())
        && qualified != "xml.etree.ElementTree.Element"
}

/// A `typing` name that is a class or a special form, not a function.
fn typing_class(qualified: Option<&str>) -> bool {
    qualified
        .and_then(|q| {
            q.strip_prefix("typing.")
                .or_else(|| q.strip_prefix("typing_extensions."))
        })
        .is_some_and(|name| {
            name.starts_with(|c: char| c.is_ascii_uppercase()) && name != "TypeAlias"
        })
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(f) => self.function(f),
            Stmt::ClassDef(c) => self.class(c),
            Stmt::Assign(a) => {
                self.visit_expr(&a.value);
                let site = Site {
                    kind: SiteKind::Assign,
                    value: Some(&a.value),
                    annotation: None,
                };
                for target in &a.targets {
                    self.bind_target(target, site, Parent::Direct);
                }
            }
            Stmt::AnnAssign(a) => {
                self.visit_expr(&a.annotation);
                if let Some(value) = &a.value {
                    self.visit_expr(value);
                }
                let site = Site {
                    kind: SiteKind::AnnAssign,
                    value: a.value.as_deref(),
                    annotation: Some(&a.annotation),
                };
                self.bind_target(&a.target, site, Parent::Direct);
            }
            Stmt::AugAssign(a) => {
                self.visit_expr(&a.value);
                self.bind_target(&a.target, OTHER, Parent::Direct);
            }
            Stmt::For(f) => self.for_stmt(f),
            Stmt::While(w) => {
                self.visit_expr(&w.test);
                self.loops += 1;
                self.visit_body(&w.body);
                self.visit_body(&w.orelse);
                self.loops -= 1;
            }
            Stmt::With(w) => {
                for item in &w.items {
                    self.visit_expr(&item.context_expr);
                    if let Some(vars) = &item.optional_vars {
                        self.bind_target(vars, OTHER, Parent::Direct);
                    }
                }
                self.visit_body(&w.body);
            }
            Stmt::If(s) => {
                let main = is_main_test(&s.test);
                self.visit_expr(&s.test);
                for (slot, body) in if_bodies(s).into_iter().enumerate() {
                    if let Some(clause) =
                        slot.checked_sub(1).and_then(|i| s.elif_else_clauses.get(i))
                        && let Some(test) = &clause.test
                    {
                        self.visit_expr(test);
                    }
                    if slot == 0 && main {
                        self.main += 1;
                    }
                    self.branch(stmt, slot, body);
                    if slot == 0 && main {
                        self.main -= 1;
                    }
                }
            }
            Stmt::Try(t) => self.try_stmt(stmt, t),
            Stmt::Match(m) => {
                self.visit_expr(&m.subject);
                for case in &m.cases {
                    self.pattern(&case.pattern);
                    if let Some(guard) = &case.guard {
                        self.visit_expr(guard);
                    }
                    self.visit_body(&case.body);
                }
            }
            Stmt::TypeAlias(t) => {
                if let Expr::Name(name) = &*t.name {
                    self.check("typealias", name.id.as_str(), name.range, None);
                }
                self.type_params(t.type_params.as_deref());
                self.visit_expr(&t.value);
            }
            Stmt::Global(g) => {
                if let Some(frame) = self.frames.last_mut() {
                    frame
                        .globals
                        .extend(g.names.iter().map(ruff_python_ast::Identifier::as_str));
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = &*named.target {
                    self.store(target.id.as_str(), target.range, OTHER, Parent::Direct);
                }
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
                self.enter(Frame::new(Kind::Lambda, None), |walk| {
                    walk.visit_expr(&lambda.body);
                });
            }
            Expr::ListComp(c) => self.comprehension(&c.generators, &[&c.elt]),
            Expr::SetComp(c) => self.comprehension(&c.generators, &[&c.elt]),
            Expr::Generator(c) => self.comprehension(&c.generators, &[&c.elt]),
            Expr::DictComp(c) => {
                let elements: Vec<&Expr> = c
                    .key
                    .as_deref()
                    .into_iter()
                    .chain([c.value.as_ref()])
                    .collect();
                self.comprehension(&c.generators, &elements);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    const IDS: [&str; 4] = ["C0103", "C0105", "C0131", "C0132"];

    macro_rules! fixture {
        ($name:ident, $file:literal, $options:expr) => {
            #[test]
            fn $name() {
                let source = include_str!(concat!("fixtures/", $file, ".py"));
                let expected = include_str!(concat!("fixtures/", $file, ".expected")).trim_end();
                assert_eq!(render(source, &$options, &IDS), expected);
            }
        };
    }

    /// Lint `source` as the module `module` of the file `path`.
    fn lint_at(source: &str, path: &std::path::Path, module: &str) -> Vec<(u32, String)> {
        lint_with(source, path, module, None)
    }

    fn lint_with(
        source: &str,
        path: &std::path::Path,
        module: &str,
        types: Option<&dyn super::super::TypeAt>,
    ) -> Vec<(u32, String)> {
        use ferrolint_core::LineIndex;

        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = LineIndex::new(source);
        let flat = crate::ast::flatten(source, &index, parsed.syntax());
        let model =
            crate::semantic::Model::build(parsed.syntax(), &flat.nodes, source, &index, None);
        let ctx = super::Ctx {
            source,
            index: &index,
            parsed: &parsed,
            flat: &flat,
            model: &model,
            options: &[],
            py_version: None,
            module,
            path,
            roots: &crate::native::Project::test_roots(path, module),
            types,
        };
        let enabled = IDS.iter().map(|id| (*id).to_owned()).collect();
        let mut out = Vec::new();
        super::Checker::check(&super::Naming, &ctx, &enabled, &mut out);
        out.into_iter()
            .map(|d| (d.range.start.line, d.message))
            .collect()
    }

    /// A class the rule cannot follow, named by ty; what is not one stays silent.
    #[cfg(feature = "ty")]
    #[test]
    fn ty_names_a_class_the_rule_cannot_follow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("use.py");
        let source = "import io\nimport queue\n\norig_queue = queue.LifoQueue\nwriter = io.StringIO.write\nbuffer = queue.Queue()\n";
        std::fs::write(&path, source).unwrap();
        let types = crate::ty::Types::new(dir.path()).unwrap();
        let with = types
            .with_file(&path, source, |db, file| {
                let values = crate::ty::values::Values::new(db, file);
                lint_with(source, &path, "use", Some(&values))
            })
            .unwrap();
        assert_eq!(lint_at(source, &path, "use"), []);
        assert_eq!(
            with,
            [(
                4,
                "Class name \"orig_queue\" doesn't conform to PascalCase naming style".to_owned()
            )]
        );
    }

    #[test]
    fn overrides_found_in_other_modules_of_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("__init__.py"), "from .base import Base\n").unwrap();
        std::fs::write(
            package.join("base.py"),
            "import unittest\n\nclass Root(unittest.TestCase):\n    attr = 1\n    def getName(self): pass\n\n\nclass Base(Root):\n    def getTitle(self): pass\n\n\nclass Mapped(dict):\n    pass\n\n\nclass Twice:\n    pass\n\n\ntry:\n    import x\nexcept ImportError:\n    Twice = None\n",
        )
        .unwrap();
        let source = "from . import base\nfrom pkg import Base\nfrom .base import Root\nfrom unknown import Other\n\n\nclass A(Base):\n    attr = 2\n    def getName(self): pass\n    def getTitle(self): pass\n    def getOther(self): pass\n    def setUp(self): pass\n    def setUpTestData(cls): pass\n\n\nclass B(base.Root):\n    def getName(self): pass\n\n\nclass C(Other):\n    def getName(self): pass\n\n\nclass D:\n    def setUp(self): pass\n\n\nclass E(base.Mapped):\n    def setdefault(self, Key): pass\n    def setDefault(self, Key): pass\n\n\ndef make():\n    Alias = base.Root\n    Other = Root\n    Twice = base.Twice\n    return Alias, Other, Twice\n";
        let path = package.join("child.py");
        let found = lint_at(source, &path, "pkg.child");
        let names: Vec<_> = found
            .iter()
            .map(|(line, message)| (*line, message.split('"').nth(1).unwrap().to_owned()))
            .collect();
        assert_eq!(
            names,
            [
                (11, "getOther".to_owned()),
                (13, "setUpTestData".to_owned()),
                (21, "getName".to_owned()),
                (25, "setUp".to_owned()),
                (30, "setDefault".to_owned()),
                (30, "Key".to_owned()),
                (36, "Twice".to_owned()),
            ],
            "{found:?}"
        );
    }

    #[test]
    fn classes_found_through_functions_and_handlers_of_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(
            package.join("__init__.py"),
            "from . import base\nAlias = base.Cls\n",
        )
        .unwrap();
        std::fs::write(
            package.join("base.py"),
            "class Cls:\n    def run(self):\n        pass\n\n\ntry:\n    import x\nexcept ImportError:\n    class Fallback(dict):\n        pass\n",
        )
        .unwrap();
        std::fs::write(
            package.join("utils.py"),
            "from functools import cache\n\n\n@cache\ndef get_cls():\n    from pkg.base import Cls\n\n    return Cls\n\n\ndef get_obj():\n    from pkg.base import Cls\n\n    return Cls()\n",
        )
        .unwrap();
        let source = "from pkg import Alias\nfrom pkg.base import Fallback\nfrom pkg.utils import get_cls, get_obj\n\n\nclass F(Fallback):\n    def setdefault(self, Key): pass\n\n\ndef make():\n    Klass = get_cls()\n    Obj = get_obj()\n    return Klass, Obj\n\n\nclass G(Alias):\n    def run(self, Key): pass\n";
        let found = lint_at(source, &package.join("child.py"), "pkg.child");
        let lines: Vec<_> = found.iter().map(|(line, _)| *line).collect();
        assert_eq!(lines, [12], "{found:?}");
    }

    #[test]
    fn a_class_a_function_of_the_project_defines_and_returns() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("__init__.py"), "").unwrap();
        std::fs::write(
            package.join("maker.py"),
            "def make(base):\n    class Made(base):\n        pass\n\n    return Made\n",
        )
        .unwrap();
        let source = "from pkg.maker import make\n\n\ndef build(base):\n    Klass = make(base)\n    return Klass\n";
        let found = lint_at(source, &package.join("child.py"), "pkg.child");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn bases_bound_in_several_branches_of_another_module() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("__init__.py"), "").unwrap();
        std::fs::write(
            package.join("compat.py"),
            "import sys\n\nif sys.version_info[0] == 3:\n    number = int\nelse:\n    number = long\n\ntry:\n    from ordered import Odd\nexcept ImportError:\n    from collections import OrderedDict as Odd\n\n\nclass Ordered(Odd):\n    pass\n",
        )
        .unwrap();
        let source = "from pkg.compat import number, Ordered\n\n\nclass A(number):\n    def __new__(Cls, Value): pass\n    def ownA(self): pass\n\n\nclass B(Ordered):\n    def popitem(self, Last): pass\n    def ownB(self): pass\n";
        let found = lint_at(source, &package.join("child.py"), "pkg.child");
        let lines: Vec<_> = found.iter().map(|(line, _)| *line).collect();
        assert_eq!(lines, [6, 11], "{found:?}");
    }

    fixture!(module_level_names, "names_module", []);
    fixture!(typing_values, "names_typing", []);
    fixture!(inferred_values, "names_values", []);
    fixture!(flow_around_module_names, "names_flow", []);
    fixture!(function_class_and_comprehension_names, "names_scopes", []);
    fixture!(class_members_and_properties, "names_classes", []);
    fixture!(type_variables, "names_typevars", []);
    fixture!(
        naming_styles_good_names_and_hints,
        "names_options",
        [
            ("argument-naming-style", "camelCase"),
            ("attr-naming-style", "PascalCase"),
            ("class-attribute-naming-style", "UPPER_CASE"),
            ("class-const-naming-style", "snake_case"),
            ("class-naming-style", "snake_case"),
            ("const-naming-style", "camelCase"),
            ("function-naming-style", "PascalCase"),
            ("inlinevar-naming-style", "UPPER_CASE"),
            ("method-naming-style", "UPPER_CASE"),
            ("module-naming-style", "any"),
            ("variable-naming-style", "UPPER_CASE"),
            ("good-names", "i,j,ok_name,Good"),
            ("good-names-rgxs", "^good_.*$,^_tmp"),
            ("bad-names", "foo,bar,baz,notgood"),
            ("include-naming-hint", "yes"),
        ]
    );
    fixture!(
        regular_expressions_and_property_classes,
        "names_options_rgx",
        [
            ("const-rgx", "[a-z]+_[a-z]+$"),
            ("class-rgx", "^[a-z]+\\d$"),
            ("function-rgx", "^fn_[a-z]+$"),
            ("method-rgx", "^m_[a-z]+$"),
            ("argument-rgx", "^a_[a-z]+$"),
            ("variable-rgx", "^v_[a-z]+$"),
            ("attr-rgx", "^at_[a-z]+$"),
            ("class-attribute-rgx", "^ca_[a-z]+$"),
            ("class-const-rgx", "^cc_[a-z]+$"),
            ("inlinevar-rgx", "^iv$"),
            ("module-rgx", "^zzz$"),
            ("typevar-rgx", "^TV$"),
            ("typealias-rgx", "^TA$"),
            ("paramspec-rgx", "^PS$"),
            (
                "property-classes",
                "functools.cached_property,abc.abstractproperty"
            ),
        ]
    );
}
